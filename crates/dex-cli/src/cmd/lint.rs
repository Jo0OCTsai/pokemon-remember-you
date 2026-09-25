//! `dex lint`（FR-6.11 / §5.6）：结构体检，四层检查集（结构/准入/文件/流转）。
//!
//! 冻结检查集（§5.6 全量）：
//! - 结构层：顶层白名单 = 八大 scope 目录 + `.git/ .cache/ .obsidian/ .gitignore
//!   .dex-ignore .dex/`（名单外报错；`.DS_Store`/`Thumbs.db`/`desktop.ini` 固定忽略）；
//!   目录命名 kebab-case（文件名允许 CJK）；domains/ 深度 ≤2、apps/projects 一级
//! - 准入层：无空的内容子目录（domains/apps/projects 及以下；顶层八大豁免）；
//!   domains/ 一级软预算 ≤8（超提示）
//! - 文件层：单文件条目数/字数软上限（提示拆分）；H1 与文件名一致性（弱提示）；
//!   scope 内 frontmatter 残留
//! - 流转层：inbox 树无 >7 天未裁决文件（递归；`inbox/staging/` 豁免；v0 遗留
//!   `inbox/bootstrap/` 同受检并提示迁移）；inbox 命名与必填字段；archive 镜像路径一致；
//!   src/superseded-by/keep-until 注释格式与两级作用域位置（keep-until 过期残留提示）；
//!   superseded-by 目标存在性；可疑密钥模式；config 悬空 scope 引用
//! - 信封特例（§8.4）：error 级发现 → 退出码 1 且 `--json` 信封 **ok=true**、问题走
//!   `data.findings`（warning 级同时进 `warnings[]`；仅 warning 级 → 退出码 0）
//!
//! Finding 结构（json `data.findings[]`）：`{level: "error"|"warning", check, path?, line?, message}`。
//!
//! 实现注（D 组）：
//! - 检查集落在 [`run_checks`]（pub 供 `dex review` 段①复用）；输出顺序稳定
//!   （check 名 → path → line 字典序）
//! - h1-filename 弱提示的实现口径：缺 H1 → 提示；拉丁字面（topic 与文件名均为 ASCII）
//!   归一化后不一致 → 提示；CJK 主题与 ASCII 文件名不做字面比对（跨文字系统无一致
//!   语义，避免常态误报）
//! - warning 级发现进 `warnings[]` 时借用 `W_CONFIG_NOTICE` 承载（§8.4 六码无 lint 专用
//!   警告码，与 config.rs 既有先例同口径）

use crate::cmd::CommonOpts;
use crate::config::Config;
use crate::guard_runtime::{today_local, Ctx};
use chrono::NaiveDate;
use clap::Args;
use dex_core::entry::{comment_line, parse_file, parse_keep_until, trailing_comment};
use dex_core::guard::scan_secrets;
use dex_core::proposal::{parse_filename_date_source, parse_proposal_file};
use dex_core::scope::{scope_of_path, FOUR_LAYERS};
use dex_core::{DexError, Warning};
use dex_store::fs::top_level_entries;
use serde::Serialize;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

#[derive(Args, Debug)]
pub struct LintArgs {
    #[command(flatten)]
    pub common: CommonOpts,
    /// 限定检查的 scope 前缀（逗号分隔）
    #[arg(long, value_delimiter = ',')]
    pub scope: Option<Vec<String>>,
}

/// 体检发现
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Finding {
    /// error（违反硬规则）/ warning（软预算/提示）
    pub level: Level,
    /// 检查项名（如 top-level-whitelist、inbox-stale、frontmatter-residue）
    pub check: &'static str,
    pub path: Option<String>,
    pub line: Option<usize>,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Error,
    Warning,
}

// ---------- 冻结常量（§5.6 / FR-2.8/2.9） ----------

/// 八大 scope 目录（FR-1.2）
const TOP_SCOPES: [&str; 8] = [
    "person", "domains", "apps", "projects", "journal", "inbox", "archive", "index",
];
/// 顶层固定合法项（§5.6 结构层白名单）
const TOP_EXTRA: [&str; 6] = [
    ".git",
    ".cache",
    ".obsidian",
    ".gitignore",
    ".dex-ignore",
    ".dex",
];
/// 系统杂项：固定忽略
const SYSTEM_JUNK: [&str; 3] = [".DS_Store", "Thumbs.db", "desktop.ini"];
/// 「先内容后结构」约束对象（FR-2.9 空目录/软预算）
const CONTENT_TREES: [&str; 3] = ["domains", "apps", "projects"];
/// inbox 滞留阈值（天，FR-2.8 周清空机检）
const INBOX_STALE_DAYS: i64 = 7;
/// 单文件软上限：条目数 / 字符数
const FILE_ENTRY_SOFT_LIMIT: usize = 50;
const FILE_CHAR_SOFT_LIMIT: usize = 5000;
/// domains/ 一级软预算
const DOMAINS_BUDGET: usize = 8;
/// 两级作用域注释键（§2.2）
const COMMENT_KEYS: [&str; 3] = ["src", "superseded-by", "keep-until"];

// ---------- 检查集 ----------

/// 全量体检（FR-6.11 检查集；pub 供 `dex review` 段①/⑦复用）。
/// 输出顺序稳定：check 名 → path → line 字典序。
pub fn run_checks(root: &Path, config: &Config) -> Vec<Finding> {
    let today = today_local();
    let mut out: Vec<Finding> = Vec::new();

    check_top_level(root, &mut out);

    let tree = collect_tree(root);
    check_dir_naming(&tree, &mut out);
    check_depth(&tree, &mut out);
    check_empty_content_dirs(&tree, &mut out);
    check_domains_budget(&tree, &mut out);
    check_archive_mirror(&tree, &mut out);
    for rel in &tree.files {
        if is_four_layer(rel) {
            check_scope_file(root, rel, &today, &mut out);
        }
    }
    check_inbox(root, &tree, &today, &mut out);
    check_config_scope_dangling(root, config, &mut out);

    out.sort_by(|a, b| {
        a.check
            .cmp(b.check)
            .then_with(|| a.path.cmp(&b.path))
            .then_with(|| a.line.unwrap_or(0).cmp(&b.line.unwrap_or(0)))
    });
    out
}

pub fn run(args: &LintArgs, ctx: &Ctx) -> Result<i32, DexError> {
    let mut findings = run_checks(&ctx.root, &ctx.config);
    // --scope：前缀过滤 findings 的 path（段级前缀匹配）
    if let Some(scopes) = &args.scope {
        findings.retain(|f| match f.path.as_deref() {
            None => true,
            Some(p) => scopes.iter().any(|s| path_matches(p, s)),
        });
    }
    let has_error = findings.iter().any(|f| f.level == Level::Error);
    let code = i32::from(has_error);

    // 信封特例（§8.4）：ok=true、问题走 data.findings；warning 级同时进 warnings[]
    let extra: Vec<Warning> = findings
        .iter()
        .filter(|f| f.level == Level::Warning)
        .map(|f| Warning::ConfigNotice {
            detail: format!("lint: {}", text_finding_line(f)),
        })
        .collect();
    let data = json!({ "findings": &findings });

    let mut text = String::new();
    for f in &findings {
        text.push_str(&text_finding_line(f));
        text.push('\n');
    }
    Ok(ctx.emit(code, extra, data, text))
}

/// text 行格式（冻结）：`[error] check path:line message`
pub(crate) fn text_finding_line(f: &Finding) -> String {
    let level = match f.level {
        Level::Error => "error",
        Level::Warning => "warning",
    };
    let loc = match (&f.path, f.line) {
        (Some(p), Some(l)) => format!(" {p}:{l}"),
        (Some(p), None) => format!(" {p}"),
        (None, _) => String::new(),
    };
    format!("[{level}] {}{}: {}", f.check, loc, f.message)
}

// ---------- 结构层 ----------

fn check_top_level(root: &Path, out: &mut Vec<Finding>) {
    for name in top_level_entries(root) {
        if SYSTEM_JUNK.contains(&name.as_str()) {
            continue;
        }
        if TOP_SCOPES.contains(&name.as_str()) || TOP_EXTRA.contains(&name.as_str()) {
            continue;
        }
        out.push(Finding {
            level: Level::Error,
            check: "top-level-whitelist",
            path: Some(name.clone()),
            line: None,
            message: format!(
                "顶层白名单外条目 {name:?}（合法集 = 八大 scope 目录 + .git/.cache/.obsidian/.gitignore/.dex-ignore/.dex，§5.6）"
            ),
        });
    }
}

fn check_dir_naming(tree: &Tree, out: &mut Vec<Finding>) {
    for dir in &tree.dirs {
        if let Some(bad) = dir.split('/').skip(1).find(|seg| !kebab_ok(seg)) {
            out.push(Finding {
                level: Level::Error,
                check: "dir-naming",
                path: Some(dir.clone()),
                line: None,
                message: format!(
                    "目录段 {bad:?} 不合 kebab-case（^[a-z0-9][a-z0-9-]*$；文件名允许 CJK，FR-2.8）"
                ),
            });
        }
    }
}

fn check_depth(tree: &Tree, out: &mut Vec<Finding>) {
    let mut domains_bad: BTreeSet<String> = BTreeSet::new();
    let mut apps_bad: BTreeSet<String> = BTreeSet::new();
    for dir in &tree.dirs {
        let segs: Vec<&str> = dir.split('/').collect();
        if segs.len() > 2 {
            match segs[0] {
                "domains" => {
                    domains_bad.insert(dir.clone());
                }
                "apps" | "projects" => {
                    apps_bad.insert(dir.clone());
                }
                _ => {}
            }
        }
    }
    // 只报最浅违例（深层违例由其父目录违例蕴含）
    for (set, check, msg) in [
        (&domains_bad, "domains-depth", "domains/ 深度 ≤2（FR-2.8）"),
        (
            &apps_bad,
            "apps-projects-depth",
            "apps/ 与 projects/ 仅一级（FR-2.8）",
        ),
    ] {
        for dir in set {
            let parent_violates = parent_of(dir).is_some_and(|p| set.contains(p));
            if !parent_violates {
                out.push(Finding {
                    level: Level::Error,
                    check,
                    path: Some(dir.clone()),
                    line: None,
                    message: format!("目录超深：{dir}——{msg}"),
                });
            }
        }
    }
}

// ---------- 准入层 ----------

fn check_empty_content_dirs(tree: &Tree, out: &mut Vec<Finding>) {
    for dir in &tree.dirs {
        if !CONTENT_TREES.contains(&rel_top(dir)) {
            continue;
        }
        let prefix = format!("{dir}/");
        if !tree.files.iter().any(|f| f.starts_with(&prefix)) {
            out.push(Finding {
                level: Level::Warning,
                check: "empty-content-dir",
                path: Some(dir.clone()),
                line: None,
                message:
                    "空内容目录（先内容后结构，FR-2.9；同类条目 ≥10 且连续两周进入回顾清单才建域）"
                        .into(),
            });
        }
    }
}

fn check_domains_budget(tree: &Tree, out: &mut Vec<Finding>) {
    let first_level = tree
        .dirs
        .iter()
        .filter(|d| d.starts_with("domains/") && d.matches('/').count() == 1)
        .count();
    if first_level > DOMAINS_BUDGET {
        out.push(Finding {
            level: Level::Warning,
            check: "domains-budget",
            path: Some("domains".into()),
            line: None,
            message: format!(
                "domains/ 一级子域 {first_level} 个 > 软预算 {DOMAINS_BUDGET}——提示合并候选（FR-2.9）"
            ),
        });
    }
}

// ---------- 文件层 + 流转层（scope 文件） ----------

fn check_scope_file(root: &Path, rel: &str, today: &str, out: &mut Vec<Finding>) {
    let Ok(content) = std::fs::read_to_string(root.join(rel)) else {
        return;
    };
    // frontmatter 残留（FR-2.3：归位须剥提案元数据）
    if content.lines().next().is_some_and(|l| l.trim() == "---") {
        out.push(Finding {
            level: Level::Error,
            check: "frontmatter-residue",
            path: Some(rel.to_string()),
            line: Some(1),
            message: "scope 文件残留 frontmatter（归位动作必须剥掉提案元数据，FR-2.3）".into(),
        });
    }

    let entries = parse_file(rel, &content);

    // 单文件软上限
    let n = entries.len();
    let chars = content.chars().count();
    if n > FILE_ENTRY_SOFT_LIMIT || chars > FILE_CHAR_SOFT_LIMIT {
        out.push(Finding {
            level: Level::Warning,
            check: "file-soft-limit",
            path: Some(rel.to_string()),
            line: None,
            message: format!(
                "单文件 {n} 条目 / {chars} 字超软上限（{FILE_ENTRY_SOFT_LIMIT} 条 / {FILE_CHAR_SOFT_LIMIT} 字）——建议拆分"
            ),
        });
    }

    // H1 与文件名一致性（弱提示；口径见模块实现注）
    if let Some(first) = entries.first() {
        let stem = Path::new(rel)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        if first.topic.is_empty() {
            out.push(Finding {
                level: Level::Warning,
                check: "h1-filename",
                path: Some(rel.to_string()),
                line: None,
                message: "缺 H1 主题（H1 与文件名一致性无法校验，弱提示）".into(),
            });
        } else if first.topic.is_ascii()
            && stem.is_ascii()
            && norm_word(&first.topic) != norm_word(stem)
        {
            out.push(Finding {
                level: Level::Warning,
                check: "h1-filename",
                path: Some(rel.to_string()),
                line: None,
                message: format!("H1 {:?} 与文件名 {stem:?} 不一致（弱提示）", first.topic),
            });
        }
    }

    // 注释格式与两级位置（FR-2.2/2.10）
    check_comment_format(rel, &content, today, out);

    // 可疑密钥模式（§5.5-8 复用）
    if let Some(pattern) = scan_secrets(&content).first() {
        out.push(Finding {
            level: Level::Error,
            check: "secret-pattern",
            path: Some(rel.to_string()),
            line: None,
            message: format!("疑似密钥命中（{pattern}）——gitleaks 类模式（§5.5-8）"),
        });
    }

    // superseded-by 目标存在性（目标去重后逐一校验）
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for e in &entries {
        if let Some(target) = &e.superseded_by {
            if seen.insert(target.as_str()) {
                check_superseded_target(root, rel, e.line, target, out);
            }
        }
    }
}

/// src/superseded-by/keep-until 注释格式：两级作用域位置 + keep-until 日期形状/日历/过期残留。
/// 位置判定与 core::entry 的吸收规则同构：紧随 H1（文件级）/ 紧随条目行（条目级）；
/// 其余（空行后、标题后、正文游离）→ 位置非法。
fn check_comment_format(rel: &str, content: &str, today: &str, out: &mut Vec<Finding>) {
    let lines: Vec<&str> = content.lines().collect();
    let n = lines.len();
    let mut start = 0;
    if n > 0 && lines[0].trim() == "---" {
        let mut close = 1;
        while close < n && lines[close].trim() != "---" {
            close += 1;
        }
        start = if close < n { close + 1 } else { n };
    }
    let h1 = (start..n).find(|&i| is_h1(lines[i]));

    for (idx, &line) in lines.iter().enumerate().skip(start) {
        if is_standalone(line) {
            for key in COMMENT_KEYS {
                if let Some(v) = comment_line(line, key) {
                    validate_comment_value(rel, idx + 1, key, &v, today, out);
                    if !standalone_anchored(&lines, start, h1, idx) {
                        out.push(Finding {
                            level: Level::Error,
                            check: "comment-format",
                            path: Some(rel.to_string()),
                            line: Some(idx + 1),
                            message: format!(
                                "{key} 注释位置非法：游离注释（须紧随 H1 = 文件级，或紧随条目行 = 条目级，FR-2.2/2.10）"
                            ),
                        });
                    }
                    break;
                }
            }
        } else if is_content_line(line) {
            for key in COMMENT_KEYS {
                if let Some(v) = trailing_comment(line, key) {
                    validate_comment_value(rel, idx + 1, key, &v, today, out);
                    break;
                }
            }
        } else {
            // 标题/空白行携带行尾已知注释 → 位置非法
            for key in COMMENT_KEYS {
                if trailing_comment(line, key).is_some() {
                    out.push(Finding {
                        level: Level::Error,
                        check: "comment-format",
                        path: Some(rel.to_string()),
                        line: Some(idx + 1),
                        message: format!("{key} 行尾注释挂在非条目行（标题/空行不承载条目注释）"),
                    });
                    break;
                }
            }
        }
    }
}

fn validate_comment_value(
    rel: &str,
    line_no: usize,
    key: &str,
    v: &str,
    today: &str,
    out: &mut Vec<Finding>,
) {
    if v.trim().is_empty() {
        out.push(Finding {
            level: Level::Error,
            check: "comment-format",
            path: Some(rel.to_string()),
            line: Some(line_no),
            message: format!("注释值不能为空（<!-- {key}: … -->）"),
        });
        return;
    }
    if key != "keep-until" {
        return;
    }
    let Some(ku) = parse_keep_until(v) else {
        out.push(Finding {
            level: Level::Error,
            check: "comment-format",
            path: Some(rel.to_string()),
            line: Some(line_no),
            message: "keep-until 日期形状非法（须 YYYY-MM-DD 原因，FR-2.10）".into(),
        });
        return;
    };
    match (
        NaiveDate::parse_from_str(&ku.until, "%Y-%m-%d"),
        NaiveDate::parse_from_str(today, "%Y-%m-%d"),
    ) {
        (Ok(until), Ok(t)) if until >= t => {}
        (Ok(_), Ok(_)) => {
            out.push(Finding {
                level: Level::Warning,
                check: "comment-format",
                path: Some(rel.to_string()),
                line: Some(line_no),
                message: format!(
                    "keep-until 已于 {} 过期（原因：{}）——复审后清理或续期（FR-2.10 残留提示）",
                    ku.until, ku.reason
                ),
            });
        }
        _ => {
            out.push(Finding {
                level: Level::Error,
                check: "comment-format",
                path: Some(rel.to_string()),
                line: Some(line_no),
                message: format!("keep-until 日历日期非法（{} 不存在）", ku.until),
            });
        }
    }
}

fn check_superseded_target(
    root: &Path,
    rel: &str,
    line: usize,
    target: &str,
    out: &mut Vec<Finding>,
) {
    let (file, anchor) = match target.split_once('#') {
        Some((f, a)) => (f, if a.is_empty() { None } else { Some(a) }),
        None => (target, None),
    };
    let bad = |message: String| Finding {
        level: Level::Warning,
        check: "superseded-target",
        path: Some(rel.to_string()),
        line: Some(line),
        message,
    };
    if file.is_empty() {
        out.push(bad("superseded-by 目标为空".into()));
        return;
    }
    let abs = root.join(file);
    if !abs.is_file() {
        out.push(bad(format!(
            "superseded-by 目标文件 {file} 不存在（悬空引用，FR-2.5）"
        )));
        return;
    }
    if let Some(anchor) = anchor {
        if let Ok(content) = std::fs::read_to_string(&abs) {
            let hit = parse_file(file, &content)
                .iter()
                .any(|e| e.topic == anchor || e.content.contains(anchor));
            if !hit {
                out.push(bad(format!(
                    "superseded-by 锚点 {anchor:?} 未在 {file} 命中（悬空引用，FR-2.5）"
                )));
            }
        }
    }
}

// ---------- 流转层（inbox / archive / config） ----------

fn check_inbox(root: &Path, tree: &Tree, today: &str, out: &mut Vec<Finding>) {
    for rel in &tree.files {
        // inbox/staging/** 豁免（FR-6.14 收割暂存）
        if !rel.starts_with("inbox/") || rel.starts_with("inbox/staging/") {
            continue;
        }
        let name = rel.rsplit('/').next().unwrap_or(rel);
        // 滞留判龄：文件名日期优先，缺则 mtime（FR-2.8）
        if let Some(days) = file_age_days(root, rel, today) {
            if days > INBOX_STALE_DAYS {
                out.push(Finding {
                    level: Level::Error,
                    check: "inbox-stale",
                    path: Some(rel.clone()),
                    line: None,
                    message: format!(
                        "inbox 滞留 {days} 天未裁决（> {INBOX_STALE_DAYS} 天，FR-4.4 周清空约束）"
                    ),
                });
            }
        }
        // 顶层文件：命名式 + 必填 frontmatter 字段
        if rel.matches('/').count() == 1 {
            if parse_filename_date_source(name).is_none() {
                out.push(Finding {
                    level: Level::Error,
                    check: "inbox-naming",
                    path: Some(rel.clone()),
                    line: None,
                    message: format!(
                        "inbox 顶层命名不合 {{YYYY-MM-DD}}-{{source}}-{{shortid}}.md 式：{name:?}（FR-4.1）"
                    ),
                });
            }
            if let Ok(raw) = std::fs::read_to_string(root.join(rel)) {
                if let Err(e) = parse_proposal_file(&raw) {
                    out.push(Finding {
                        level: Level::Error,
                        check: "inbox-naming",
                        path: Some(rel.clone()),
                        line: Some(1),
                        message: format!("提案 frontmatter 缺必填字段或非法：{e}"),
                    });
                }
            }
        }
    }
    // v0 遗留 bootstrap：迁移提示（US-13）
    if let Some(n) = dex_store::inbox::Inbox::new(root).legacy_bootstrap() {
        if n > 0 {
            out.push(Finding {
                level: Level::Warning,
                check: "inbox-stale",
                path: Some("inbox/bootstrap".into()),
                line: None,
                message: format!(
                    "v0 遗留 bootstrap 草稿 {n} 份——建议迁移至 inbox 顶层（v1 起 bootstrap 模式统一走 dex propose，US-13）"
                ),
            });
        }
    }
}

fn check_archive_mirror(tree: &Tree, out: &mut Vec<Finding>) {
    for rel in &tree.files {
        let Some(stripped) = rel.strip_prefix("archive/") else {
            continue;
        };
        let ok = scope_of_path(Path::new(stripped)).is_some_and(|s| s.kind().is_four_layer());
        if !ok {
            out.push(Finding {
                level: Level::Warning,
                check: "archive-mirror",
                path: Some(rel.clone()),
                line: None,
                message:
                    "archive 路径未镜像四层结构（须为 archive/{person|domains/<d>|apps/<a>|projects/<p>}/…，FR-6.11）"
                        .into(),
            });
        }
    }
}

fn check_config_scope_dangling(root: &Path, config: &Config, out: &mut Vec<Finding>) {
    let mut missing: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (id, conf) in &config.clients {
        for s in &conf.scopes {
            if !root.join(s.dir_prefix()).is_dir() {
                missing
                    .entry(s.as_str().to_string())
                    .or_default()
                    .push(id.clone());
            }
        }
    }
    for (scope, ids) in missing {
        out.push(Finding {
            level: Level::Warning,
            check: "config-scope-dangling",
            path: Some(scope.clone()),
            line: None,
            message: format!(
                "config clients [{}] 引用的 scope 目录 {scope:?} 不存在（悬空引用，FR-6.11）",
                ids.join(", ")
            ),
        });
    }
}

// ---------- 遍历与小工具 ----------

/// 八大 scope 目录树（结构检查输入；.git/.cache/.obsidian 等白名单非 scope 项不进入）
struct Tree {
    dirs: Vec<String>,
    files: Vec<String>,
}

fn collect_tree(root: &Path) -> Tree {
    let mut dirs = Vec::new();
    let mut files = Vec::new();
    for top in TOP_SCOPES {
        let base = root.join(top);
        if base.is_dir() {
            walk_rec(&base, top.to_string(), &mut dirs, &mut files);
        }
    }
    dirs.sort();
    files.sort();
    Tree { dirs, files }
}

fn walk_rec(abs: &Path, rel: String, dirs: &mut Vec<String>, files: &mut Vec<String>) {
    let Ok(rd) = std::fs::read_dir(abs) else {
        return;
    };
    for e in rd.flatten() {
        let Ok(name) = e.file_name().into_string() else {
            continue;
        };
        let child = format!("{rel}/{name}");
        if e.file_type().is_ok_and(|t| t.is_dir()) {
            dirs.push(child.clone());
            walk_rec(&e.path(), child, dirs, files);
        } else {
            files.push(child);
        }
    }
}

fn rel_top(rel: &str) -> &str {
    rel.split('/').next().unwrap_or(rel)
}

fn is_four_layer(rel: &str) -> bool {
    FOUR_LAYERS.contains(&rel_top(rel))
}

fn parent_of(p: &str) -> Option<&str> {
    p.rsplit_once('/').map(|(a, _)| a)
}

/// kebab-case：^[a-z0-9][a-z0-9-]*$（目录段；文件名允许 CJK 不在此判）
fn kebab_ok(seg: &str) -> bool {
    let b = seg.as_bytes();
    if b.is_empty() {
        return false;
    }
    let first = b[0].is_ascii_lowercase() || b[0].is_ascii_digit();
    first
        && b[1..]
            .iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-')
}

/// 归一化（h1-filename 弱提示用）：仅保留字母数字并小写
fn norm_word(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

fn is_standalone(line: &str) -> bool {
    let t = line.trim();
    t.len() >= 7 && t.starts_with("<!--") && t.ends_with("-->")
}

fn heading_hashes(line: &str) -> usize {
    let t = line.trim_start();
    let hashes = t.chars().take_while(|c| *c == '#').count();
    if hashes > 0 && (t[hashes..].is_empty() || t[hashes..].starts_with(' ')) {
        hashes
    } else {
        0
    }
}

fn is_h1(line: &str) -> bool {
    heading_hashes(line) == 1
}

fn is_content_line(line: &str) -> bool {
    !line.trim().is_empty() && heading_hashes(line) == 0 && !is_standalone(line)
}

/// 独立注释行是否挂靠合法（文件级紧随 H1 / 条目级紧随条目行）
fn standalone_anchored(lines: &[&str], start: usize, h1: Option<usize>, idx: usize) -> bool {
    let mut j = idx;
    while j > start && is_standalone(lines[j - 1]) {
        j -= 1;
    }
    if j == start {
        return false;
    }
    if h1 == Some(j - 1) {
        return true;
    }
    is_content_line(lines[j - 1])
}

fn mtime_date(path: &Path) -> Option<String> {
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    let dt: chrono::DateTime<chrono::Local> = modified.into();
    Some(dt.format("%Y-%m-%d").to_string())
}

/// inbox 文件判龄（天）：文件名日期优先，缺则 mtime；无法判定 → None（不报）
fn file_age_days(root: &Path, rel: &str, today: &str) -> Option<i64> {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    let date = parse_filename_date_source(name)
        .map(|(d, _)| d)
        .or_else(|| mtime_date(&root.join(rel)))?;
    let d = NaiveDate::parse_from_str(&date, "%Y-%m-%d").ok()?;
    let t = NaiveDate::parse_from_str(today, "%Y-%m-%d").ok()?;
    Some((t - d).num_days())
}

/// 段级前缀匹配：`domains/work` 匹配 `domains/work/x.md`，不匹配 `domains/workx`
fn path_matches(path: &str, prefix: &str) -> bool {
    let p: Vec<&str> = path.split('/').collect();
    let s: Vec<&str> = prefix.split('/').filter(|x| !x.is_empty()).collect();
    !s.is_empty() && s.len() <= p.len() && s.iter().zip(&p).all(|(a, b)| a == b)
}
