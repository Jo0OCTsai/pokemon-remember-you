//! `dex stale`（FR-6.5 / §5.4）：衰减候选清单。
//!
//! 冻结语义：
//! - v1 单遍全树 `git log --name-only` 快照（stale 与注入比较器共用口径；复活 git mv
//!   即重置计时）；**git 缺失 → 回退 mtime + W_GIT_UNAVAILABLE（降级成功，退出码 0）**
//! - `--days N`：覆盖缺省窗口（per-scope 分档仍生效，前缀最长匹配）
//! - `--scope`：过滤输出（逗号分隔，前缀匹配）
//! - 输出：文件、条目、最后实质变更、建议动作（archive/rewrite/keep-until 流转）
//! - 退出码 0（空清单也 0）
//!
//! 实现注（D 组）：
//! - 快照 miss 的未跟踪文件回退 mtime 兜底；git 非「缺失」类失败（如日志炸裂）同样
//!   降级 mtime + W_GIT_UNAVAILABLE（detail 区分），维持降级成功口径
//! - `collect_stale` 为 pub（供 `dex review` 段④复用）；referenced 恒空——周报引用
//!   豁免是 review 的「Spoke 使用周报粘贴区」人工步骤（v1 无协议通道）

use crate::cmd::CommonOpts;
use crate::config::Config;
use crate::guard_runtime::{today_local, Ctx};
use clap::Args;
use dex_core::decay::{compute_candidates, suggestion_for, StaleCandidate, Suggestion};
use dex_core::entry::{parse_file, Entry};
use dex_core::scope::{scope_of_path, FOUR_LAYERS};
use dex_core::{DexError, Warning};
use serde_json::json;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Args, Debug)]
pub struct StaleArgs {
    #[command(flatten)]
    pub common: CommonOpts,
    /// 覆盖缺省窗口天数（per-scope 分档不受影响）
    #[arg(long)]
    pub days: Option<i64>,
    /// 过滤 scope 前缀（逗号分隔）
    #[arg(long, value_delimiter = ',')]
    pub scope: Option<Vec<String>>,
}

/// 衰减候选收集（pub 供 review 段④复用）。
/// `days_override` 仅覆盖 `StalePolicy.default_days`（per-scope 分档仍生效）。
pub fn collect_stale(
    root: &Path,
    config: &Config,
    days_override: Option<i64>,
) -> (Vec<StaleCandidate>, Vec<Warning>) {
    let mut warnings = Vec::new();
    let entries = four_layer_entries(root);

    // 单遍 git log 快照；缺失/失败 → 全体 mtime 回退 + W_GIT_UNAVAILABLE
    let snapshot = dex_store::git::last_touch_snapshot(root);
    let (map, git_ok) = match snapshot {
        Ok(m) => (m, true),
        Err(e) => {
            let detail = if dex_store::git::git_missing(&e) {
                "PATH 未找到 git，stale 回退 mtime".to_string()
            } else {
                format!("git log 快照失败，stale 回退 mtime：{e}")
            };
            warnings.push(Warning::GitUnavailable { detail });
            (HashMap::new(), false)
        }
    };

    let owned_root = root.to_path_buf();
    let last_change = |p: &str| -> Option<String> {
        if git_ok {
            if let Some(d) = map.get(p) {
                return Some(d.clone());
            }
            // 快照 miss（未跟踪新文件）→ mtime 兜底
        }
        mtime_date(&owned_root.join(p))
    };
    let scope_of = |p: &str| scope_of_path(Path::new(p)).map(|s| s.as_str().to_string());

    let mut policy = config.stale.clone();
    if let Some(d) = days_override {
        policy.default_days = d;
    }
    let today = today_local();
    let candidates = compute_candidates(&entries, &scope_of, &last_change, &today, &policy, &[]);
    (candidates, warnings)
}

pub fn run(args: &StaleArgs, ctx: &Ctx) -> Result<i32, DexError> {
    let (mut candidates, warnings) = collect_stale(&ctx.root, &ctx.config, args.days);
    // --scope：前缀过滤输出（段级前缀匹配）
    if let Some(scopes) = &args.scope {
        candidates.retain(|c| scopes.iter().any(|s| path_matches(&c.path, s)));
    }

    let mut text = String::new();
    for c in &candidates {
        text.push_str(&format!(
            "{}:{} | {} | 最后变更 {} | 建议 {}{}\n",
            c.path,
            c.line,
            c.scope,
            c.last_change,
            suggestion_str(c),
            if c.force_review { " !复审" } else { "" }
        ));
    }
    let data = json!({ "candidates": &candidates, "count": candidates.len() });
    Ok(ctx.emit(0, warnings, data, text))
}

/// 四层 scope 树内全部条目（stale 与 review ⑤/⑥ 共用收集口径）
pub(crate) fn four_layer_entries(root: &Path) -> Vec<Entry> {
    let dirs: Vec<PathBuf> = FOUR_LAYERS.iter().map(PathBuf::from).collect();
    let mut out = Vec::new();
    for rel in dex_store::fs::walk_markdown(root, &dirs) {
        if let Ok(content) = std::fs::read_to_string(root.join(&rel)) {
            out.extend(parse_file(&rel, &content));
        }
    }
    out
}

fn suggestion_str(c: &StaleCandidate) -> &'static str {
    match suggestion_for(c) {
        Suggestion::Archive => "archive",
        Suggestion::Rewrite => "rewrite",
        Suggestion::Keep => "keep",
    }
}

fn mtime_date(path: &Path) -> Option<String> {
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    let dt: chrono::DateTime<chrono::Local> = modified.into();
    Some(dt.format("%Y-%m-%d").to_string())
}

/// 段级前缀匹配：`domains/work` 匹配 `domains/work/x.md`，不匹配 `domains/workx`
fn path_matches(path: &str, prefix: &str) -> bool {
    let p: Vec<&str> = path.split('/').collect();
    let s: Vec<&str> = prefix.split('/').filter(|x| !x.is_empty()).collect();
    !s.is_empty() && s.len() <= p.len() && s.iter().zip(&p).all(|(a, b)| a == b)
}
