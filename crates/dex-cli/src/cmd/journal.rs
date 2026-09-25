//! `dex journal --source S [--date D] [msg|-]`（FR-5.4/6.13 / §8.1）：journal 供稿唯一通道。
//!
//! 冻结语义：
//! - 守卫子集 0/1/5/6/8（§5.5 末行：限流 journal_per_day 缺省 60，按当日该 source
//!   已供稿条数计数）；正文为位置参数（`-` = stdin）
//! - 追加至 `journal/{date}.md` 的 `## 供稿 · {source}` 小节尾部（core::proposal::append_journal；
//!   小节不存在则创建、缺页建页；不跨小节覆盖）
//! - git 自动提交 `journal: append from <source>`；git 缺失 → W_GIT_UNAVAILABLE + 0
//! - 退出码 0 / 2 元数据或绑定错 / 5 正文超限或限流 / 9 密钥
//!
//! **D 组任务**：实现 run + 集成测试（多来源小节隔离、同日重复供稿追加尾部、限流、密钥）。

use crate::cmd::propose::{auto_commit, read_body};
use crate::cmd::CommonOpts;
use crate::guard_runtime::{check_source_binding, today_local, Ctx};
use chrono::NaiveDate;
use clap::Args;
use dex_core::guard::{check_journal, scan_secrets};
use dex_core::proposal::{append_journal, journal_page_path, journal_section_title};
use dex_core::{DexError, Warning};
use serde_json::json;

#[derive(Args, Debug)]
pub struct JournalArgs {
    #[command(flatten)]
    pub common: CommonOpts,
    #[arg(long)]
    pub source: String,
    /// 供稿日期 `YYYY-MM-DD`（缺省今日，本地时区）
    #[arg(long)]
    pub date: Option<String>,
    /// 供稿正文（`-` = stdin）
    pub msg: Option<String>,
}

/// 解析供稿日期：`YYYY-MM-DD` 且为合法日历日（往返一致，拒绝 `2026-9-5` 形态）。
fn parse_date(d: &str) -> Result<String, DexError> {
    let bad = || DexError::BadArgs {
        message: format!("--date 非法：{d:?}（须为 YYYY-MM-DD 日历日）"),
    };
    let parsed = NaiveDate::parse_from_str(d, "%Y-%m-%d").map_err(|_| bad())?;
    if parsed.format("%Y-%m-%d").to_string() == d {
        Ok(d.to_string())
    } else {
        Err(bad())
    }
}

/// 当日该 source 已供稿条数：journal 页 `## 供稿 · <source>` 小节内 `- ` 开头行数
/// （小节止于下一个 `## `；缺页/缺小节 = 0，§5.5-6）。
fn count_today_entries(page: &str, source: &str) -> usize {
    let title = journal_section_title(source);
    let mut in_section = false;
    let mut count = 0;
    for line in page.lines() {
        if line.starts_with("## ") {
            in_section = line == title;
        } else if in_section && line.starts_with("- ") {
            count += 1;
        }
    }
    count
}

pub fn run(args: &JournalArgs, ctx: &Ctx) -> Result<i32, DexError> {
    let date = match &args.date {
        Some(d) => parse_date(d)?,
        None => today_local(),
    };
    let text = read_body(&args.msg)?;
    // 步骤 0：source 与客户端身份绑定（FR-4.7）——失败写 audit
    if let Err(e) = check_source_binding(&ctx.client, &args.source) {
        ctx.audit(&args.source, e.code());
        return Err(e);
    }
    // 当日计数 + 守卫子集 1/5/6/8（core 纯函数）；协议面拒绝写 audit
    let page_rel = journal_page_path(&date);
    let page_path = ctx.root.join(&page_rel);
    let page = std::fs::read_to_string(&page_path).unwrap_or_default();
    let today_count = count_today_entries(&page, &args.source);
    let limits = ctx.limits(None);
    if let Err(e) = check_journal(&args.source, &text, &limits, today_count) {
        if e.is_denial() {
            ctx.audit(&args.source, e.code());
        }
        return Err(e);
    }
    let mut warnings = Vec::new();
    // advisory 密钥命中 → W_SECRET 放行（check_journal 静默通过，这里补警告面）
    if limits.secret_advisory {
        if let Some(pattern) = scan_secrets(&text).first() {
            warnings.push(Warning::Secret {
                pattern: pattern.to_string(),
            });
        }
    }
    // 小节级追加（不跨小节覆盖；缺页建页）→ 写回
    let updated = append_journal(&page, &date, &args.source, &text);
    if let Some(parent) = page_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| DexError::RepoState {
            message: format!("journal 目录创建失败：{e}"),
        })?;
    }
    std::fs::write(&page_path, updated).map_err(|e| DexError::RepoState {
        message: format!("journal 页写回失败（{page_rel}）：{e}"),
    })?;
    // git 自动提交（§2.3 模板；git 缺失/失败 → W_GIT_UNAVAILABLE，数据不丢）
    let (committed, commit_warnings) =
        auto_commit(ctx, &format!("journal: append from {}", args.source));
    warnings.extend(commit_warnings);
    let title = journal_section_title(&args.source);
    let section = title.strip_prefix("## ").unwrap_or(&title).to_string();
    let data = json!({
        "page": page_rel,
        "section": section,
        "committed": committed,
    });
    Ok(ctx.emit(0, warnings, data, format!("{page_rel}\n")))
}
