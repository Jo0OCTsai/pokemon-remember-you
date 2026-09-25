//! `dex interview`（FR-6.14 / §8.1）：渐进式面试草稿提案（便利封装，蒸馏在技能侧）。
//!
//! 冻结语义：
//! - `--round core`（缺省）：输出首轮 5 核心问（角色与主业／主力栈与工具／语言与沟通偏好／
//!   硬性禁区／常用输出格式）+ 收割会话操作指引（供 agent 会话执行）
//! - `--round follow-up`：输出渐进补全引导（后续会话顺手补问、增量 propose）
//! - `--stdin`：从 stdin 逐行读答案（`问题序号: 答案`），逐条走 propose 守卫落
//!   `inbox/`（source = 客户端 allowed_sources 首个；evidence = `interview <round> <date>`，
//!   人审对照面试原文）；人显式自提案经 human 客户端
//! - 退出码 0（守卫拒绝按 propose 错误码透传）

use crate::cmd::CommonOpts;
use crate::guard_runtime::{today_local, Ctx};
use clap::Args;
use dex_core::guard::ProposalInput;
use dex_core::DexError;
use serde_json::{json, Value};

#[derive(Args, Debug)]
pub struct InterviewArgs {
    #[command(flatten)]
    pub common: CommonOpts,
    /// core（首轮 5 问）| follow-up（渐进补全）
    #[arg(long, default_value = "core")]
    pub round: String,
    /// 从 stdin 读「问题序号: 答案」批量提案
    #[arg(long)]
    pub stdin: bool,
}

/// 首轮 5 核心问（US-01 步骤 2；技能 dex-bootstrap 同源）
pub const CORE_QUESTIONS: [&str; 5] = [
    "1. 你的角色与主业是什么？（日常在做什么、对谁负责）",
    "2. 你的主力技术栈与工具链是什么？（语言/框架/编辑器/终端工具）",
    "3. 你的语言与沟通偏好？（中文/英文、直接还是委婉、详略程度）",
    "4. 你的硬性禁区有哪些？（绝不做的事、敏感边界）",
    "5. 你常用的输出格式偏好？（代码注释风格、文档结构、报告格式）",
];

/// core 轮操作指引（收割会话在 agent 会话执行；蒸馏在技能侧，FR-6.14）
const CORE_GUIDANCE: &str = "在 agent 会话（dex-bootstrap 技能）中逐题向训练家提问，答案须指回原话；\
逐条落盘走 `dex propose --source <客户端 allowed_sources 首个> --kind fact \
--evidence \"interview core <date> Q<n>\"`，或用 `dex interview --stdin` 批量提交（格式：`问题序号: 答案`）。\
人审时对照面试原文回放。";

/// follow-up 轮渐进补全引导（后续会话顺手补问、增量 propose）
const FOLLOWUP_GUIDANCE: &str =
    "后续收割/工作会话中顺手补问缺口（决策/人物/偏好/模式四象限未覆盖处），\
不重发已覆盖问题；补问答案增量经 `dex propose` 落 inbox，evidence 指回当次会话原文。";

/// 解析一行 `问题序号: 答案`（1–5；容忍 `Q1:`/`q1:` 前缀与全角冒号）。
fn parse_qa(line: &str) -> Result<(usize, &str), DexError> {
    let bad = || DexError::BadArgs {
        message: format!("stdin 答案行格式非法：{line:?}（须为「问题序号: 答案」，序号 1–5）"),
    };
    let rest = line.strip_prefix(['Q', 'q']).unwrap_or(line);
    let (num, ans) = rest
        .split_once(':')
        .or_else(|| rest.split_once('：'))
        .ok_or_else(bad)?;
    let n: usize = num.trim().parse().map_err(|_| bad())?;
    if !(1..=CORE_QUESTIONS.len()).contains(&n) {
        return Err(bad());
    }
    Ok((n, ans.trim()))
}

pub fn run(args: &InterviewArgs, ctx: &Ctx) -> Result<i32, DexError> {
    if !matches!(args.round.as_str(), "core" | "follow-up") {
        return Err(DexError::BadArgs {
            message: format!("--round 非法：{:?}（core|follow-up）", args.round),
        });
    }
    let date = today_local();

    // 问题集输出（无 --stdin）：core = 5 核心问 + 收割会话指引；follow-up = 渐进补全引导
    if !args.stdin {
        let (questions, guidance): (Vec<&str>, &str) = if args.round == "core" {
            (CORE_QUESTIONS.to_vec(), CORE_GUIDANCE)
        } else {
            (Vec::new(), FOLLOWUP_GUIDANCE)
        };
        let data = json!({
            "round": args.round,
            "questions": questions,
            "guidance": guidance,
        });
        let mut text = String::new();
        if args.round == "core" {
            for q in &CORE_QUESTIONS {
                text.push_str(q);
                text.push('\n');
            }
        }
        text.push_str(guidance);
        text.push('\n');
        return Ok(ctx.emit(0, Vec::new(), data, text));
    }

    // --stdin：逐行读答案，逐条走 propose 守卫（submit_proposal 含步骤 0 绑定 + audit）
    let source = ctx
        .client
        .conf
        .allowed_sources
        .first()
        .cloned()
        .ok_or_else(|| DexError::BadArgs {
            message: format!(
                "客户端 {:?} 无 allowed_sources——面试批量提案须有可用 source（FR-4.7）",
                ctx.client.id
            ),
        })?;
    let mut raw = String::new();
    std::io::Read::read_to_string(&mut std::io::stdin(), &mut raw).map_err(|e| {
        DexError::BadArgs {
            message: format!("stdin 读取失败：{e}"),
        }
    })?;
    let mut items: Vec<Value> = Vec::new();
    let mut files: Vec<String> = Vec::new();
    let mut warnings = Vec::new();
    let mut first_err: Option<DexError> = None;
    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let (n, answer) = parse_qa(line)?;
        let input = ProposalInput {
            source: source.clone(),
            kind: "fact".to_string(),
            confidence: None,
            evidence: format!("interview {} {} Q{}", args.round, date, n),
            content: answer.to_string(),
        };
        match crate::cmd::propose::submit_proposal(ctx, &input, &date) {
            Ok(outcome) => {
                warnings.extend(outcome.warnings);
                items.push(json!({
                    "q": n,
                    "file": outcome.file,
                    "idempotent": outcome.idempotent,
                }));
                files.push(outcome.file);
            }
            Err(e) => {
                items.push(json!({ "q": n, "error": e.to_string() }));
                if first_err.is_none() {
                    first_err = Some(e);
                }
            }
        }
    }
    // 守卫拒绝按 propose 错误码透传（已落盘条目保留，逐条成败在重放时经幂等域可恢复）
    if let Some(e) = first_err {
        return Err(e);
    }
    let data = json!({
        "round": args.round,
        "date": date,
        "items": items,
    });
    let text = files.iter().map(|f| format!("{f}\n")).collect::<String>();
    Ok(ctx.emit(0, warnings, data, text))
}
