//! `dex init`（FR-6.9 / §8.1）：八大目录骨架 + .gitignore + git init 与首次提交。
//!
//! 冻结语义：
//! - 目标路径：`--path`（缺省 = root 解析结果：DEX_ROOT → 本机层 config → ~/dex）
//! - 建 `person/ domains/ apps/ projects/ journal/ inbox/ archive/ index/` +
//!   `.gitignore`（内容：`.cache/` 与 `.obsidian/`）；幂等补齐缺失项、**不覆盖已有文件**
//! - 目标尚非 git 仓库 → `git init` + 空骨架首次提交（提交信息 `init: dex skeleton`）；
//!   已在 git 仓库内 → 幂等补齐骨架（不重复 init）；git 缺失 → 骨架照建 + W_GIT_UNAVAILABLE 不阻断
//! - 已完整初始化（八目录 + .gitignore + git 仓库齐）→ E_REPO_STATE · 8
//!
//! 实现注：init 是仓库前置命令（run 无 Ctx），json 信封自行经 [`crate::output::print_ok`]
//! 输出（`--format` 优先于 `--json`，与 main::normalize 同口径）；Err 交回 main 统一打印。

use crate::cmd::CommonOpts;
use clap::Args;
use dex_core::{DexError, Warning};
use dex_store::git::CommitOutcome;

#[derive(Args, Debug)]
pub struct InitArgs {
    #[command(flatten)]
    pub common: CommonOpts,
    /// 目标路径（缺省 root 解析：DEX_ROOT → 本机层 config → ~/dex）
    #[arg(long)]
    pub path: Option<std::path::PathBuf>,
}

/// 八大骨架目录（§2.1 顶层结构；US-01 与 `dex init` 一次建齐合法，FR-2.9 豁免空目录约束）
const SKELETON_DIRS: [&str; 8] = [
    "person", "domains", "apps", "projects", "journal", "inbox", "archive", "index",
];
/// `.gitignore` 骨架内容（忽略本地缓存与 Obsidian 工作区文件）
const GITIGNORE: &str = ".cache/\n.obsidian/\n";

/// 仓库不存在、config 未建时的管理命令入口（init/skills）：TTY human 直行；
/// 非交互须完整凭证 + human（FR-10.4）。main 调用。
pub fn run(args: &InitArgs) -> Result<i32, DexError> {
    let json = wants_json(&args.common);
    let root = match &args.path {
        Some(p) => crate::config::expand_tilde(&p.to_string_lossy()),
        // 复用 config 的 root 解析（DEX_ROOT → 本机层 → ~/dex）；仓库层此时缺席，load 安全。
        // load 失败（非法 config 等）→ 退 ~/dex（init 本就允许零配置起步）
        None => crate::config::load()
            .map(|(c, _)| c.root)
            .unwrap_or_else(|_| crate::config::home_dir().join("dex")),
    };

    // 已完整初始化（八目录 + .gitignore + git 仓库齐且无新增）→ E_REPO_STATE · 8
    let gitignore = root.join(".gitignore");
    if SKELETON_DIRS.iter().all(|d| root.join(d).is_dir())
        && gitignore.is_file()
        && dex_store::git::is_repo(&root)
    {
        return Err(DexError::RepoState {
            message: format!(
                "{} 已完整初始化（八大目录 + .gitignore + git 仓库齐备，FR-6.9）",
                root.display()
            ),
        });
    }

    // 只建缺失项：目录已存在不重创，已有文件不覆盖
    let mut created: Vec<String> = Vec::new();
    for d in SKELETON_DIRS {
        let p = root.join(d);
        if !p.is_dir() {
            std::fs::create_dir_all(&p).map_err(|e| io_err("建骨架目录失败", e))?;
            created.push(d.to_string());
        }
    }
    if !gitignore.exists() {
        std::fs::write(&gitignore, GITIGNORE).map_err(|e| io_err("写 .gitignore 失败", e))?;
        created.push(".gitignore".to_string());
    }

    // git：缺失 → W_GIT_UNAVAILABLE 降级不阻断；非仓库 → init + 首提；已在仓库 → 补齐提交
    let mut warnings: Vec<Warning> = Vec::new();
    let mut git_initialized = false;
    let git_note = if !dex_store::git::available() {
        warnings.push(git_unavailable("跳过版本控制初始化（骨架照建）"));
        GitNote::Degraded
    } else if !dex_store::git::is_repo(&root) {
        match dex_store::git::init_and_first_commit(&root, "init: dex skeleton") {
            Ok(_) => {
                git_initialized = true;
                GitNote::Initialized
            }
            Err(e) if dex_store::git::git_missing(&e) => {
                warnings.push(git_unavailable("跳过版本控制初始化（骨架照建）"));
                GitNote::Degraded
            }
            Err(e) => return Err(e),
        }
    } else if !created.is_empty() {
        match dex_store::git::commit_all(&root, "init: 补齐骨架") {
            Ok(CommitOutcome::Committed) => GitNote::Refilled,
            // 补齐项 git 不可见（如空目录）→ NothingToCommit 容忍
            Ok(CommitOutcome::NothingToCommit) => GitNote::NothingNew,
            Err(e) if dex_store::git::git_missing(&e) => {
                warnings.push(git_unavailable("跳过骨架补齐提交"));
                GitNote::Degraded
            }
            Err(e) => return Err(e),
        }
    } else {
        GitNote::None
    };

    let created_text = created.clone();
    let data = serde_json::json!({
        "root": root.display().to_string(),
        "created": created,
        "git_initialized": git_initialized,
    });
    if json {
        crate::output::print_ok(&warnings, data);
    } else {
        println!("dex init → {}", root.display());
        if created_text.is_empty() {
            println!("  （无缺失项）");
        } else {
            for c in &created_text {
                println!("  + {c}");
            }
        }
        match git_note {
            GitNote::Initialized => {
                println!("git: 已初始化仓库并创建首次提交「init: dex skeleton」");
            }
            GitNote::Refilled => println!("git: 仓库已存在，补齐提交「init: 补齐骨架」"),
            GitNote::NothingNew => {
                println!("git: 仓库已存在（本次补齐项 git 不可见，无新提交）");
            }
            GitNote::None => println!("git: 仓库已存在"),
            GitNote::Degraded => println!("git: 不可用，版本控制初始化已降级（见警告）"),
        }
        for w in &warnings {
            eprintln!("⚠ {} {}", w.code(), w.message());
        }
    }
    Ok(0)
}

/// git 动作结果（text 输出用）
enum GitNote {
    Initialized,
    Refilled,
    NothingNew,
    None,
    Degraded,
}

fn git_unavailable(detail: &str) -> Warning {
    Warning::GitUnavailable {
        detail: detail.to_string(),
    }
}

/// json 开关（`--format` 优先于 `--json`；非法值已由 main::normalize 前置拦截）
fn wants_json(common: &CommonOpts) -> bool {
    match common.format.as_deref() {
        Some("json") => true,
        Some("text") => false,
        _ => common.json,
    }
}

fn io_err(what: &str, e: std::io::Error) -> DexError {
    DexError::RepoState {
        message: format!("{what}：{e}"),
    }
}
