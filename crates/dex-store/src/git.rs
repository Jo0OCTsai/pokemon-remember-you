//! git 适配（C2）：子进程调用 ≥2.20（NFR-5 / DESIGN §9）。
//!
//! 冻结契约：
//! - [`last_touch_snapshot`]：**单遍全树** `git log --name-only --date=short`（一次进程调用），
//!   取每文件最后触及 commit 日期（新→旧遍历首见即定格；不做 diff 级过滤——v1 保守近似，§5.4；
//!   **禁止**逐文件 `git log --follow` 全历史扫描）。返回 文件路径 → `YYYY-MM-DD`。
//! - [`commit_all`]：`add -A` + commit；`index.lock` 占用 → 退避 50ms×≤10 次重试，
//!   超限 E_REPO_STATE·8（§5.5）；无变更 → Ok(CommitOutcome::NothingToCommit)；
//!   git 缺失 → Err(DexError) 由调用方降级为 W_GIT_UNAVAILABLE（退出码 0）。
//! - 其余：available/is_repo/init_and_first_commit/status_dirty。
//!
//! 实现决策（C 组冻结）：每次 commit 统一带 `-c user.name=dex -c user.email=dex@local`
//! （临时仓库友好、不污染全局配置；真实仓库已有身份时仅本调用覆盖——审计面 source 信息
//! 在提交信息里，可接受）。
//!
//! **C 组任务**：实现 + 集成测试（tempfile 临时 git 仓库 fixture，配置 user.name/email）；
//! 公共 API 冻结。

use dex_core::DexError;
use std::collections::HashMap;
use std::path::Path;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread::sleep;
use std::time::Duration;

/// index.lock 退避重试次数上限（50ms × ≤10 次，§5.5）。
pub const LOCK_RETRY_MAX: usize = 10;
/// index.lock 退避间隔毫秒数（缺省 50；`AtomicU64` 暴露为测试钩子——
/// 集成测试可 `store(1)` 缩短等待，不影响生产语义）。
pub static LOCK_BACKOFF_MS: AtomicU64 = AtomicU64::new(50);

/// commit 身份（见模块级实现决策）
const GIT_IDENTITY: [&str; 4] = ["-c", "user.name=dex", "-c", "user.email=dex@local"];
/// git 缺失标记（spawn Err(kind=NotFound) 的映射；[`git_missing`] 按此识别）
const GIT_MISSING_MARK: &str = "git 不可用";

/// git 可用性（PATH 上有 git 且 `git --version` ≥ 2.20）。
pub fn available() -> bool {
    let out = match Command::new("git").arg("--version").output() {
        Ok(o) => o,
        Err(_) => return false,
    };
    if !out.status.success() {
        return false;
    }
    match parse_version(String::from_utf8_lossy(&out.stdout).trim()) {
        Some((maj, min)) => (maj, min) >= (2, 20),
        None => false,
    }
}

/// 解析 "git version 2.55.0" → (2, 55)
fn parse_version(s: &str) -> Option<(u32, u32)> {
    let rest = s.strip_prefix("git version ")?;
    let mut it = rest.split('.');
    let maj = it.next()?.parse().ok()?;
    let min = it.next()?.parse().ok()?;
    Some((maj, min))
}

/// root 是否 git 仓库（存在 .git 且 git rev-parse --is-inside-work-tree 成功）。
pub fn is_repo(root: &Path) -> bool {
    if !root.join(".git").exists() {
        return false;
    }
    match run(root, &["rev-parse", "--is-inside-work-tree"]) {
        Ok(out) => out.status.success() && String::from_utf8_lossy(&out.stdout).trim() == "true",
        Err(_) => false,
    }
}

/// 单遍全树快照：每文件最后触及 commit 日期（§5.4 实现口径）。无提交 → 空表。
pub fn last_touch_snapshot(root: &Path) -> Result<HashMap<String, String>, DexError> {
    let out = run(
        root,
        &[
            // 中文/emoji 等非 ASCII 路径不转义引号形式，直接给 UTF-8 原文
            "-c",
            "core.quotepath=false",
            "log",
            "--name-only",
            "--date=short",
            "--pretty=format:%x00%cd",
        ],
    )?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        // 空仓库（HEAD 未生）→ 空表（冻结契约）；其余失败（如非仓库 exit 128）→ E_REPO_STATE
        if stderr.contains("does not have any commits yet") {
            return Ok(HashMap::new());
        }
        return Err(DexError::RepoState {
            message: format!(
                "git log 失败（exit {}）：{}",
                exit_code(&out),
                summarize(&stderr)
            ),
        });
    }
    let text = decode(&out.stdout)?;
    let mut map: HashMap<String, String> = HashMap::new();
    // 记录间以 \0 分隔；每记录首非空行 = 日期，其后每行 = 文件路径（空行跳过）。
    // git log 新→旧输出 ⇒ 每文件首见（最新触及）即定格。
    for record in text.split('\0') {
        let mut lines = record.lines().filter(|l| !l.trim().is_empty());
        let Some(date) = lines.next() else { continue };
        for path in lines {
            map.entry(path.to_string())
                .or_insert_with(|| date.to_string());
        }
    }
    Ok(map)
}

/// add -A + commit（index.lock 退避 50ms×≤10，超限 E_REPO_STATE）。
pub fn commit_all(root: &Path, message: &str) -> Result<CommitOutcome, DexError> {
    let st = run(root, &["status", "--porcelain"])?;
    if !st.status.success() {
        return Err(repo_state("git status 失败", &st.stderr));
    }
    if !st.stdout.iter().any(|b| !b.is_ascii_whitespace()) {
        return Ok(CommitOutcome::NothingToCommit);
    }
    add_all_with_backoff(root)?;
    if commit_with_backoff(root, message, false)? {
        Ok(CommitOutcome::Committed)
    } else {
        Ok(CommitOutcome::NothingToCommit)
    }
}

/// git init（若尚非仓库）+ 首次提交（空骨架 commit；`dex init` 用，FR-6.9）。
/// 返回是否新建了仓库。
pub fn init_and_first_commit(root: &Path, message: &str) -> Result<bool, DexError> {
    if is_repo(root) {
        return Ok(false);
    }
    std::fs::create_dir_all(root).map_err(|e| DexError::RepoState {
        message: format!("init 目录创建失败：{e}"),
    })?;
    let out = run(root, &["init", "-q"])?;
    if !out.status.success() {
        return Err(repo_state("git init 失败", &out.stderr));
    }
    add_all_with_backoff(root)?;
    commit_with_backoff(root, message, true)?;
    Ok(true)
}

/// 工作区是否有未提交变更（`status --porcelain` 非空）。
pub fn status_dirty(root: &Path) -> bool {
    match run(root, &["status", "--porcelain"]) {
        Ok(out) => out.status.success() && out.stdout.iter().any(|b| !b.is_ascii_whitespace()),
        Err(_) => false,
    }
}

/// 是否为「git 缺失」类错误（spawn NotFound → E_REPO_STATE{message 含 "git 不可用"}）。
/// 调用方据此降级为 W_GIT_UNAVAILABLE（FR-4.5/NFR-5：降级成功，退出码 0）。
pub fn git_missing(e: &DexError) -> bool {
    match e {
        DexError::RepoState { message } => message.contains(GIT_MISSING_MARK),
        _ => false,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommitOutcome {
    Committed,
    NothingToCommit,
}

// ---- 内部 helper ----

fn spawn_err(e: std::io::Error) -> DexError {
    if e.kind() == std::io::ErrorKind::NotFound {
        DexError::RepoState {
            message: format!("{GIT_MISSING_MARK}（PATH 未找到）"),
        }
    } else {
        DexError::RepoState {
            message: format!("git 调用失败：{e}"),
        }
    }
}

fn run(root: &Path, args: &[&str]) -> Result<Output, DexError> {
    Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .map_err(spawn_err)
}

/// UTF-8 解码（数据性输出——git log 路径表；失败 → E_REPO_STATE）
fn decode(data: &[u8]) -> Result<String, DexError> {
    std::str::from_utf8(data)
        .map(str::to_string)
        .map_err(|_| DexError::RepoState {
            message: "git 输出 UTF-8 解码失败".into(),
        })
}

fn exit_code(out: &Output) -> i32 {
    out.status.code().unwrap_or(-1)
}

/// 诊断摘要（stderr 头部，防错误信息失控）
fn summarize(s: &str) -> String {
    s.trim().chars().take(200).collect()
}

fn repo_state(what: &str, stderr: &[u8]) -> DexError {
    let msg = String::from_utf8_lossy(stderr);
    DexError::RepoState {
        message: format!("{what}：{}", summarize(&msg)),
    }
}

fn backoff_ms() -> u64 {
    LOCK_BACKOFF_MS.load(Ordering::SeqCst).max(1)
}

/// `git add -A`（index.lock 冲突 → 退避重试 ≤[`LOCK_RETRY_MAX`] 次；其他失败即报）
fn add_all_with_backoff(root: &Path) -> Result<(), DexError> {
    for attempt in 0..=LOCK_RETRY_MAX {
        if attempt > 0 {
            sleep(Duration::from_millis(backoff_ms()));
        }
        let out = run(root, &["add", "-A"])?;
        if out.status.success() {
            return Ok(());
        }
        let stderr = String::from_utf8_lossy(&out.stderr);
        if !stderr.contains("index.lock") {
            return Err(repo_state("git add 失败", &out.stderr));
        }
    }
    Err(DexError::RepoState {
        message: "index.lock 冲突重试超限".into(),
    })
}

/// commit（统一带 dex 身份；index.lock 冲突退避；竞态下 nothing to commit → Ok(false)）
fn commit_with_backoff(root: &Path, message: &str, allow_empty: bool) -> Result<bool, DexError> {
    let mut args: Vec<&str> = GIT_IDENTITY.to_vec();
    args.push("commit");
    if allow_empty {
        args.push("--allow-empty");
    }
    args.push("-m");
    args.push(message);
    for attempt in 0..=LOCK_RETRY_MAX {
        if attempt > 0 {
            sleep(Duration::from_millis(backoff_ms()));
        }
        let out = run(root, &args)?;
        if out.status.success() {
            return Ok(true);
        }
        let stderr = String::from_utf8_lossy(&out.stderr);
        if stderr.contains("index.lock") {
            continue;
        }
        if stderr.contains("nothing to commit") {
            return Ok(false);
        }
        return Err(repo_state("git commit 失败", &out.stderr));
    }
    Err(DexError::RepoState {
        message: "index.lock 冲突重试超限".into(),
    })
}
