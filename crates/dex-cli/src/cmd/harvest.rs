//! `dex harvest`（FR-6.14 / §8.1）：收割会话便利封装（**不蒸馏**）。
//!
//! 冻结语义：
//! - `--from <source>`：须已注册于 `[harvest.sources]`（未注册 → E_BAD_ARGS）；
//!   客户端 = human 或 `harvest-<source>` 收割客户端（FR-10.4/12.5）
//! - 输出该源连接器页要点（`skills/connectors/<source>.md`——内嵌或仓库工作副本任一可得）
//!   与 `[harvest].budget` 三维预算（拉取次数/token/时长——技能侧自限，命令只承载配置，FR-12.3）
//! - staging 管理：`inbox/staging/<source>/`（收割会话先写暂存；本命令列出/转正）
//! - 转正：staging 候选按 confidence 降序取前 N（N = --limit 或 `[harvest.sources].first_batch`
//!   缺省 30，上限 30——bootstrap 首批，FR-4.3）→ **全量走守卫 0–8**（bootstrap 限流放宽）
//!   → 移入 inbox 顶层（标准命名式）→ git commit `harvest: stage <source> ×<n>`
//! - `--dry-run`：仅列出将转正的候选，不落盘
//! - 退出码 0 / 2 参数错（未注册源、坏候选文件）/ 5 超出首批上限 / 9 密钥
//!
//! **D 组任务**：实现 run + 集成测试（首批 30 截断、staging 豁免 lint 联动、dry-run）。

use crate::cmd::propose::{auto_commit, submit_proposal, SubmitOutcome};
use crate::cmd::skills::EMBEDDED_SKILLS;
use crate::cmd::CommonOpts;
use crate::guard_runtime::{today_local, Ctx};
use clap::Args;
use dex_core::guard::ProposalInput;
use dex_core::proposal::parse_proposal_file;
use dex_core::{DexError, Warning};
use dex_store::inbox::Inbox;
use serde_json::{json, Value};

#[derive(Args, Debug)]
pub struct HarvestArgs {
    #[command(flatten)]
    pub common: CommonOpts,
    /// 连接器 source id（须 ∈ [harvest.sources]）
    #[arg(long)]
    pub from: String,
    /// bootstrap 首批上限（封顶 30）
    #[arg(long)]
    pub limit: Option<usize>,
    /// 仅列出将转正的候选
    #[arg(long)]
    pub dry_run: bool,
}

/// 连接器页摘要行数上限（载荷面防失控；六要素页通常 ≤40 行）
const CONNECTOR_SUMMARY_LINES: usize = 12;

/// 连接器页只读引用（发行物内嵌单一源，FR-11.6/12.2）：未内嵌不报错——
/// 连接器页属行为知识、现场发现后编写（§5.7）。
fn connector_payload(from: &str) -> Value {
    let rel = format!("connectors/{from}.md");
    match EMBEDDED_SKILLS
        .get_file(&rel)
        .and_then(|f| f.contents_utf8())
    {
        Some(text) => {
            let summary: Vec<&str> = text
                .lines()
                .filter(|l| !l.trim().is_empty())
                .take(CONNECTOR_SUMMARY_LINES)
                .collect();
            json!({ "page": rel, "found": true, "summary": summary.join("\n") })
        }
        None => json!({
            "page": rel,
            "found": false,
            "summary": format!("未找到内嵌连接器页 {rel}——连接器页现场发现后编写（FR-12.2/§5.7）"),
        }),
    }
}

/// staging 候选（frontmatter 解析产物；坏文件跳过并计入 invalid）
struct Candidate {
    /// staging 内实际文件名（promote_move 用）
    staged_name: String,
    meta: dex_core::proposal::ProposalMeta,
    content: String,
}

/// 转正成功条目 → data.promoted 元素（file = inbox 顶层相对路径）
fn promoted_ok(outcome: &SubmitOutcome) -> Value {
    json!({ "file": outcome.file })
}

/// 转正失败条目 → data.promoted 元素（该条留 staging，记录 error；单条拒绝不阻断批次）
fn promoted_failed(file: &str, error: &DexError) -> Value {
    json!({ "file": file, "error": error.to_string() })
}

pub fn run(args: &HarvestArgs, ctx: &Ctx) -> Result<i32, DexError> {
    // --from 须已注册（FR-12.5）
    let Some(source_conf) = ctx.config.harvest.sources.get(&args.from) else {
        return Err(DexError::BadArgs {
            message: format!(
                "--from {:?} 未注册于 [harvest.sources]（收割源接入五步清单第 4 步，§5.7）",
                args.from
            ),
        });
    };
    let connector = connector_payload(&args.from);
    let budget = json!({
        "pulls": ctx.config.harvest.budget_pulls,
        "tokens": ctx.config.harvest.budget_tokens,
        "minutes": ctx.config.harvest.budget_minutes,
    });

    // staging 候选：坏文件跳过并计入 invalid（由 lint 报告，store 冻结契约）
    let inbox = Inbox::new(&ctx.root);
    let staging_paths = inbox.list_staging(&args.from);
    let mut invalid = 0usize;
    let mut candidates: Vec<Candidate> = Vec::new();
    for path in &staging_paths {
        let staged_name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let raw = std::fs::read_to_string(path).unwrap_or_default();
        match parse_proposal_file(&raw) {
            Ok((meta, content)) => candidates.push(Candidate {
                staged_name,
                meta,
                content,
            }),
            Err(_) => invalid += 1,
        }
    }
    // confidence 降序取前 N（N = --limit 或 first_batch，封顶 30——bootstrap 首批，FR-4.3）
    candidates.sort_by_key(|c| std::cmp::Reverse(c.meta.confidence));
    let n = args.limit.unwrap_or(source_conf.first_batch).min(30);
    let selected: Vec<Candidate> = candidates.into_iter().take(n).collect();

    // --dry-run：仅列出将转正候选，不落盘不提交
    if args.dry_run {
        let promoted: Vec<Value> = selected
            .iter()
            .map(|c| json!({ "file": c.staged_name }))
            .collect();
        let remaining = staging_paths.len().saturating_sub(promoted.len());
        return Ok(ctx.emit(
            0,
            Vec::new(),
            harvest_data(
                &args.from, &connector, &budget, promoted, invalid, remaining, true,
            ),
            harvest_text(
                &args.from,
                &connector,
                &budget,
                "dry-run：将转正",
                selected.len(),
            ),
        ));
    }

    // 逐条全量走守卫 0–8（submit_proposal 内 bootstrap 限流放宽）→ promote_move 移出 staging；
    // 单条拒绝不阻断批次（该条留 staging、记录 error）；全部失败 → 返回首条错误
    let date = today_local();
    let mut promoted: Vec<Value> = Vec::new();
    let mut promoted_ok_count = 0usize;
    let mut first_err: Option<DexError> = None;
    let mut warnings: Vec<Warning> = Vec::new();
    for c in selected {
        let Candidate {
            staged_name,
            meta,
            content,
        } = c;
        let input = ProposalInput {
            source: meta.source.clone(),
            kind: meta.kind.clone(),
            confidence: meta.confidence,
            evidence: meta.evidence.clone(),
            content,
        };
        match submit_proposal(ctx, &input, &date) {
            Ok(outcome) => {
                warnings.extend(outcome.warnings.iter().cloned());
                // 守卫侧已写 inbox/<标准名>；promote 把 staging 原件移至同名目标
                // （解析等价：幂等域 source+kind+content 一致，frontmatter 取 staging 原文）
                let new_name = std::path::Path::new(&outcome.file)
                    .file_name()
                    .map(|f| f.to_string_lossy().into_owned())
                    .unwrap_or_default();
                match inbox.promote_move(&args.from, &staged_name, &new_name) {
                    Ok(_) => {
                        promoted.push(promoted_ok(&outcome));
                        promoted_ok_count += 1;
                    }
                    Err(e) => {
                        if first_err.is_none() {
                            first_err = Some(e.clone());
                        }
                        promoted.push(promoted_failed(&staged_name, &e));
                    }
                }
            }
            Err(e) => {
                if first_err.is_none() {
                    first_err = Some(e.clone());
                }
                promoted.push(promoted_failed(&staged_name, &e));
            }
        }
    }
    if !promoted.is_empty() && promoted_ok_count == 0 {
        // 全部失败：透传首条错误（5 限流 / 9 密钥等按 propose 错误码）
        return Err(first_err.unwrap_or_else(|| DexError::RepoState {
            message: format!("harvest 转正全部失败（source {:?}）", args.from),
        }));
    }
    // 成功批次一次提交（§2.3 模板；n = 成功数）
    let (_, commit_warnings) = if promoted_ok_count > 0 {
        auto_commit(
            ctx,
            &format!("harvest: stage {} ×{}", args.from, promoted_ok_count),
        )
    } else {
        (false, Vec::new())
    };
    warnings.extend(commit_warnings);
    let remaining = inbox.list_staging(&args.from).len();
    Ok(ctx.emit(
        0,
        warnings,
        harvest_data(
            &args.from, &connector, &budget, promoted, invalid, remaining, false,
        ),
        harvest_text(&args.from, &connector, &budget, "已转正", promoted_ok_count),
    ))
}

fn harvest_data(
    source: &str,
    connector: &Value,
    budget: &Value,
    promoted: Vec<Value>,
    invalid: usize,
    remaining: usize,
    dry_run: bool,
) -> Value {
    json!({
        "source": source,
        "connector": connector,
        "budget": budget,
        "promoted": promoted,
        "invalid": invalid,
        "remaining": remaining,
        "dry_run": dry_run,
    })
}

fn harvest_text(
    source: &str,
    connector: &Value,
    budget: &Value,
    action: &str,
    count: usize,
) -> String {
    let found = connector["found"].as_bool().unwrap_or(false);
    let page = connector["page"].as_str().unwrap_or_default();
    let pulls = budget["pulls"].as_i64().unwrap_or_default();
    let tokens = budget["tokens"].as_i64().unwrap_or_default();
    let minutes = budget["minutes"].as_i64().unwrap_or_default();
    format!(
        "连接器页 {page}：{}\n预算：拉取 {pulls} 次 / {tokens} tokens / {minutes} 分钟（技能侧自限，FR-12.3）\n{action} {count} 条（source {source}）\n",
        if found { "已内嵌" } else { "未内嵌（现场编写，FR-12.2）" },
    )
}
