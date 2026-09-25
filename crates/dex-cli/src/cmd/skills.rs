//! `dex skills install|uninstall`（FR-11.6 / §8.1）：内嵌技能物化 + symlink 三工具目录。
//!
//! 冻结语义：
//! - 安装源：缺省 = 发行物内嵌技能（构建时自本仓 `skills/` 打包，include_dir）；
//!   `--from <path>` = 本仓工作副本（开发/dogfood 快路径，可信自担，FR-11.7）
//! - 物化目录（本实现冻结）：`~/.local/share/dex/skills/`（debt.md 待定参数，已定值回填）；
//!   每次安装**重建**受管五项（dex-bootstrap/dex-propose/dex-review/repo-knowledge/connectors——版本漂移重物化），
//!   并清除 .DS_Store 等杂项
//! - symlink 目标：`claude → ~/.claude/skills/`、`pi → ~/.pi/agent/skills/`、
//!   `zcode → ~/.zcode/skills/`（缺省全部；`--tool` 限定子集；未知 id → E_BAD_ARGS · 2）
//! - 幂等：指向一致跳过；死链自动重建；目标已有**非本仓**同名条目 → E_RENDER_REFUSE · 10，
//!   **遇冲突立即整体失败**（原子性从简，冻结决策）
//! - uninstall：只移除指向本仓物化路径的 symlink（他物留置并在结果标注 conflict，不报错）；
//!   不动物化目录；`dex skills` 无子命令 → 状态清单
//!
//! 实现注：skills 是仓库前置命令（run 无 Ctx，只依赖 HOME、不依赖 DEX_ROOT），
//! json 信封自行经 [`crate::output::print_ok`] 输出；Err 交回 main 统一打印。

use crate::cmd::CommonOpts;
use clap::{Args, Subcommand};
use dex_core::DexError;
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

/// 发行物内嵌技能单一源（构建时打包本仓 skills/，FR-11.6）
pub static EMBEDDED_SKILLS: include_dir::Dir<'_> =
    include_dir::include_dir!("$CARGO_MANIFEST_DIR/../../skills");

/// 工具 id → 技能目录（§7 支持范围：claude / pi / zcode）
pub fn tool_skill_dir(tool: &str, home: &std::path::Path) -> Option<std::path::PathBuf> {
    match tool {
        "claude" => Some(home.join(".claude").join("skills")),
        "pi" => Some(home.join(".pi").join("agent").join("skills")),
        "zcode" => Some(home.join(".zcode").join("skills")),
        _ => None,
    }
}

/// 物化目录（冻结定值：`~/.local/share/dex/skills/`）
pub fn materialize_dir(home: &std::path::Path) -> std::path::PathBuf {
    home.join(".local").join("share").join("dex").join("skills")
}

/// 四个内管技能（symlink 单元）
const SKILLS: [&str; 4] = [
    "dex-bootstrap",
    "dex-propose",
    "dex-review",
    "repo-knowledge",
];
/// 支持工具（缺省全装；顺序即处理序）
const TOOLS: [&str; 3] = ["claude", "pi", "zcode"];
/// 物化目录中受重建管理的五项（四技能 + 连接器页目录）
const MANAGED_ENTRIES: [&str; 5] = [
    "dex-bootstrap",
    "dex-propose",
    "dex-review",
    "repo-knowledge",
    "connectors",
];
/// 物化时清除的杂项文件
const JUNK_FILES: [&str; 3] = [".DS_Store", "Thumbs.db", "desktop.ini"];

#[derive(Args, Debug)]
pub struct SkillsArgs {
    #[command(flatten)]
    pub common: CommonOpts,
    #[command(subcommand)]
    pub action: Option<Action>,
}

#[derive(Subcommand, Debug)]
pub enum Action {
    /// 物化内嵌技能并 symlink 各工具技能目录
    Install {
        /// 限定工具子集（claude/pi/zcode，可多次；缺省全部）
        #[arg(long)]
        tool: Vec<String>,
        /// 从本仓工作副本安装（开发/dogfood；默认内嵌发行物）
        #[arg(long)]
        from: Option<std::path::PathBuf>,
    },
    /// 移除各工具技能目录中的本仓 symlink
    Uninstall {
        #[arg(long)]
        tool: Vec<String>,
    },
}

pub fn run(args: &SkillsArgs) -> Result<i32, DexError> {
    let json = wants_json(&args.common);
    let home = crate::config::home_dir();
    let mat = materialize_dir(&home);
    match &args.action {
        None => Ok(status(json, &home, &mat)),
        Some(Action::Install { tool, from }) => install(json, &home, &mat, tool, from),
        Some(Action::Uninstall { tool }) => uninstall(json, &home, &mat, tool),
    }
}

/// `dex skills`（无子命令）→ 三工具 × 四技能 symlink 状态清单
fn status(json: bool, home: &Path, mat: &Path) -> i32 {
    let mut tools: Map<String, Value> = Map::new();
    let mut lines: Vec<String> = Vec::new();
    for tool in TOOLS {
        let dir = tool_skill_dir(tool, home).expect("TOOLS 与 tool_skill_dir 同源");
        let mut per: Map<String, Value> = Map::new();
        let mut parts: Vec<String> = Vec::new();
        for skill in SKILLS {
            let state = skill_state(&dir, skill, mat);
            per.insert(skill.to_string(), json!(state));
            parts.push(format!("{skill}={state}"));
        }
        tools.insert(tool.to_string(), Value::Object(per));
        lines.push(format!("{tool}: {}", parts.join("  ")));
    }
    let data = json!({
        "materialize_dir": mat.display().to_string(),
        "tools": tools,
    });
    if json {
        crate::output::print_ok(&[], data);
    } else {
        println!("dex skills 状态（物化目录：{}）", mat.display());
        for l in lines {
            println!("{l}");
        }
    }
    0
}

fn install(
    json: bool,
    home: &Path,
    mat: &Path,
    tool_filter: &[String],
    from: &Option<PathBuf>,
) -> Result<i32, DexError> {
    let tools = select_tools(tool_filter)?;

    // 1. 安装源：--from 工作副本（须含 dex-*/SKILL.md）否则内嵌发行物
    let src = from
        .as_ref()
        .map(|p| crate::config::expand_tilde(&p.to_string_lossy()));
    if let Some(s) = &src {
        if !has_dex_skill(s) {
            return Err(DexError::BadArgs {
                message: format!(
                    "--from 目录 {} 不存在或未发现 dex-*/SKILL.md（须为本仓工作副本；不可信副本自担，FR-11.7）",
                    s.display()
                ),
            });
        }
    }

    // 2. 物化：重建受管五项（版本漂移重物化）+ 清杂项
    rebuild_materialize(mat, src.as_deref())?;
    let from_label = src
        .as_ref()
        .map(|s| s.display().to_string())
        .unwrap_or_else(|| "embedded".to_string());

    // 3+4. 每工具 symlink：幂等 skip / 死链重建 / 非本仓条目 → 整体失败
    let mut per_tool: Vec<(&'static str, Vec<&'static str>, Vec<&'static str>)> = Vec::new();
    for tool in tools {
        let dir = tool_skill_dir(tool, home).expect("select_tools 已校验");
        std::fs::create_dir_all(&dir).map_err(|e| io_err("建工具技能目录失败", e))?;
        let mut installed: Vec<&'static str> = Vec::new();
        let mut skipped: Vec<&'static str> = Vec::new();
        for skill in SKILLS {
            let target = mat.join(skill);
            if !target.is_dir() {
                continue; // --from 部分副本缺该技能：无可装即不装
            }
            let link = dir.join(skill);
            match std::fs::symlink_metadata(&link) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    make_symlink(&target, &link)?;
                    installed.push(skill);
                }
                Err(e) => return Err(io_err("读取技能目录条目失败", e)),
                Ok(meta) if meta.file_type().is_symlink() => {
                    let cur = std::fs::read_link(&link)
                        .map_err(|e| io_err("读取 symlink 指向失败", e))?;
                    let cur = absolutize(&cur, &link);
                    if !cur.exists() {
                        // 死链 → 删了重建
                        std::fs::remove_file(&link).map_err(|e| io_err("清除死链失败", e))?;
                        make_symlink(&target, &link)?;
                        installed.push(skill);
                    } else if points_to_ours(&cur, &target) {
                        skipped.push(skill); // 幂等：指向一致
                    } else {
                        return Err(refuse(&link, "既有 symlink 指向别处"));
                    }
                }
                Ok(_) => return Err(refuse(&link, "已有非 symlink 条目（目录/文件）")),
            }
        }
        per_tool.push((tool, installed, skipped));
    }

    // 5. 输出：data={materialize_dir, installed:{tool:[skills]}, skipped, from}
    let installed_map: Map<String, Value> = per_tool
        .iter()
        .map(|(t, ins, _)| (t.to_string(), json!(ins)))
        .collect();
    let skipped_map: Map<String, Value> = per_tool
        .iter()
        .map(|(t, _, skip)| (t.to_string(), json!(skip)))
        .collect();
    let data = json!({
        "materialize_dir": mat.display().to_string(),
        "installed": installed_map,
        "skipped": skipped_map,
        "from": from_label,
    });
    if json {
        crate::output::print_ok(&[], data);
    } else {
        println!("dex skills install（源：{from_label}）");
        println!("物化目录：{}", mat.display());
        for (tool, ins, skip) in &per_tool {
            if !ins.is_empty() {
                println!("{tool}: install {}", ins.join(", "));
            }
            if !skip.is_empty() {
                println!("{tool}: skip（已指向本仓）{}", skip.join(", "));
            }
            if ins.is_empty() && skip.is_empty() {
                println!("{tool}: 无物化技能（--from 副本缺 dex-*）");
            }
        }
    }
    Ok(0)
}

/// uninstall 结果行：(tool, removed, skipped, conflicts)
type UninstallRow = (
    &'static str,
    Vec<&'static str>,
    Vec<&'static str>,
    Vec<&'static str>,
);

fn uninstall(json: bool, home: &Path, mat: &Path, tool_filter: &[String]) -> Result<i32, DexError> {
    let tools = select_tools(tool_filter)?;

    // (tool, removed, skipped, conflicts)
    let mut per_tool: Vec<UninstallRow> = Vec::new();
    for tool in tools {
        let Some(dir) = tool_skill_dir(tool, home) else {
            continue;
        };
        let mut removed: Vec<&'static str> = Vec::new();
        let mut skipped: Vec<&'static str> = Vec::new();
        let mut conflicts: Vec<&'static str> = Vec::new();
        for skill in SKILLS {
            let link = dir.join(skill);
            match std::fs::symlink_metadata(&link) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => skipped.push(skill),
                Err(e) => return Err(io_err("读取技能目录条目失败", e)),
                Ok(meta) if meta.file_type().is_symlink() => {
                    let cur = std::fs::read_link(&link)
                        .map_err(|e| io_err("读取 symlink 指向失败", e))?;
                    let cur = absolutize(&cur, &link);
                    if points_to_ours(&cur, &mat.join(skill)) {
                        std::fs::remove_file(&link)
                            .map_err(|e| io_err("移除本仓 symlink 失败", e))?;
                        removed.push(skill);
                    } else {
                        // 非本仓 symlink → 留置标注，不报错
                        conflicts.push(skill);
                    }
                }
                Ok(_) => conflicts.push(skill), // 实体条目 → 留置标注，不报错
            }
        }
        per_tool.push((tool, removed, skipped, conflicts));
    }

    let removed_map: Map<String, Value> = per_tool
        .iter()
        .map(|(t, r, _, _)| (t.to_string(), json!(r)))
        .collect();
    let skipped_map: Map<String, Value> = per_tool
        .iter()
        .map(|(t, _, s, _)| (t.to_string(), json!(s)))
        .collect();
    let conflicts_map: Map<String, Value> = per_tool
        .iter()
        .map(|(t, _, _, c)| (t.to_string(), json!(c)))
        .collect();
    let data = json!({
        "materialize_dir": mat.display().to_string(),
        "removed": removed_map,
        "skipped": skipped_map,
        "conflicts": conflicts_map,
    });
    if json {
        crate::output::print_ok(&[], data);
    } else {
        println!("dex skills uninstall（物化目录保留不动）");
        for (tool, rem, skip, conf) in &per_tool {
            if !rem.is_empty() {
                println!("{tool}: removed {}", rem.join(", "));
            }
            if !skip.is_empty() {
                println!("{tool}: 无本仓条目 {}", skip.join(", "));
            }
            if !conf.is_empty() {
                println!("{tool}: conflict 留置（非本仓条目）{}", conf.join(", "));
            }
        }
    }
    Ok(0)
}

// ---- 内部 helper ----

/// 单技能 symlink 状态：installed（指向本仓物化路径）/ missing / conflict（非本仓条目）
fn skill_state(dir: &Path, skill: &str, mat: &Path) -> &'static str {
    let link = dir.join(skill);
    match std::fs::symlink_metadata(&link) {
        Err(_) => "missing",
        Ok(meta) if meta.file_type().is_symlink() => match std::fs::read_link(&link) {
            Ok(cur) => {
                let cur = absolutize(&cur, &link);
                if points_to_ours(&cur, &mat.join(skill)) {
                    "installed"
                } else {
                    "conflict"
                }
            }
            Err(_) => "conflict",
        },
        Ok(_) => "conflict",
    }
}

/// `--tool` 过滤 → 工具子集（缺省全部；未知 id → E_BAD_ARGS · 2；去重保序）
fn select_tools(filter: &[String]) -> Result<Vec<&'static str>, DexError> {
    if filter.is_empty() {
        return Ok(TOOLS.to_vec());
    }
    let mut selected: Vec<&'static str> = Vec::new();
    for id in filter {
        let Some(t) = TOOLS.into_iter().find(|t| *t == id.as_str()) else {
            return Err(DexError::BadArgs {
                message: format!("未知工具 id {id:?}（支持 claude|pi|zcode，§7 支持范围）"),
            });
        };
        if !selected.contains(&t) {
            selected.push(t);
        }
    }
    Ok(selected)
}

/// --from 目录是否含 `dex-*/SKILL.md`（工作副本的最小合法判据）
fn has_dex_skill(src: &Path) -> bool {
    let Ok(rd) = std::fs::read_dir(src) else {
        return false;
    };
    rd.filter_map(|e| e.ok()).any(|e| {
        e.file_name().to_string_lossy().starts_with("dex-") && e.path().join("SKILL.md").is_file()
    })
}

/// 物化重建：删旧受管五项 → 全量写入（内嵌 extract / 工作副本复制）→ 清杂项
fn rebuild_materialize(mat: &Path, src: Option<&Path>) -> Result<(), DexError> {
    for name in MANAGED_ENTRIES {
        let p = mat.join(name);
        if let Ok(meta) = std::fs::symlink_metadata(&p) {
            if meta.is_dir() {
                std::fs::remove_dir_all(&p).map_err(|e| io_err("清除旧物化目录失败", e))?;
            } else {
                std::fs::remove_file(&p).map_err(|e| io_err("清除旧物化条目失败", e))?;
            }
        }
    }
    std::fs::create_dir_all(mat).map_err(|e| io_err("建物化目录失败", e))?;
    match src {
        None => EMBEDDED_SKILLS
            .extract(mat)
            .map_err(|e| io_err("物化内嵌技能失败", e))?,
        Some(s) => copy_tree(s, mat).map_err(|e| io_err("物化工作副本技能失败", e))?,
    }
    sweep_junk(mat).map_err(|e| io_err("清除物化目录杂项失败", e))?;
    Ok(())
}

/// 递归复制（跳过杂项文件；源内 symlink 等异类条目不跟随）
fn copy_tree(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let name = entry.file_name();
        if JUNK_FILES.contains(&name.to_string_lossy().as_ref()) {
            continue;
        }
        let ft = entry.file_type()?;
        if ft.is_dir() {
            copy_tree(&entry.path(), &dst.join(&name))?;
        } else if ft.is_file() {
            std::fs::copy(entry.path(), dst.join(&name))?;
        }
    }
    Ok(())
}

/// 递归清除杂项文件（.DS_Store 等）
fn sweep_junk(dir: &Path) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let ft = entry.file_type()?;
        if ft.is_dir() {
            sweep_junk(&entry.path())?;
        } else if JUNK_FILES.contains(&entry.file_name().to_string_lossy().as_ref()) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
    Ok(())
}

/// 目录 symlink（目标 = 物化路径，绝对指向）
#[cfg(unix)]
fn make_symlink(target: &Path, link: &Path) -> Result<(), DexError> {
    std::os::unix::fs::symlink(target, link).map_err(|e| io_err("创建 symlink 失败", e))
}

#[cfg(not(unix))]
fn make_symlink(_target: &Path, _link: &Path) -> Result<(), DexError> {
    Err(DexError::RepoState {
        message: "目录 symlink 仅支持 unix 平台（§7 支持范围）".into(),
    })
}

/// symlink 目标相对写法 → 以 link 所在目录为基准的绝对路径
fn absolutize(target: &Path, link: &Path) -> PathBuf {
    if target.is_absolute() {
        target.to_path_buf()
    } else {
        link.parent().unwrap_or(Path::new("/")).join(target)
    }
}

/// 是否指向本仓物化路径（先词面比较；双活时 canonicalize 消解 /var ↔ /private/var 等别名）
fn points_to_ours(cur: &Path, expected: &Path) -> bool {
    if cur == expected {
        return true;
    }
    match (cur.canonicalize(), expected.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

fn refuse(link: &Path, reason: &str) -> DexError {
    DexError::RenderRefuse {
        target: link.display().to_string(),
        reason: format!("目标已有非本仓条目（{reason}；拒不覆盖，FR-11.6）"),
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
