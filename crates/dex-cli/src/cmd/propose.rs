//! `dex propose`（FR-6.3 / §7.3）：守卫 0–8 编排 → 落盘 inbox → git 自动提交。
//!
//! 冻结语义：
//! - 步骤 0（guard_runtime：客户端/source 绑定）→ core::guard::check_proposal（1–8）
//! - 正文来源：位置参数 msg；`-` = stdin 全量读入
//! - 落盘 `inbox/{filename}`；幂等命中 → 返回既有文件路径（成功，不重复落盘）
//! - git 自动提交 `inbox: propose from <source>`（§2.3 模板；[git].auto_commit=false 跳过；
//!   git 缺失 → W_GIT_UNAVAILABLE + 退出码 0，数据不丢）
//! - 拒绝事件（SOURCE_MISMATCH/RATE_LIMIT/SECRET 等）写 audit.log
//! - 退出码 0 / 2 元数据或绑定错 / 4 无证据 / 5 超限或限流 / 9 密钥
//!
//! **D 组任务**：实现 run + 集成测试（守卫每条失败路径、幂等重放、git 缺失降级）。

use crate::cmd::CommonOpts;
use crate::guard_runtime::{check_source_binding, today_local, Ctx};
use clap::Args;
use dex_core::guard::{check_proposal, scan_secrets, ProposalInput};
use dex_core::proposal::{render_proposal_file, ProposalMeta};
use dex_core::{DexError, Warning};
use dex_store::git::{commit_all, CommitOutcome};
use dex_store::inbox::Inbox;
use serde_json::json;

#[derive(Args, Debug)]
pub struct ProposeArgs {
    #[command(flatten)]
    pub common: CommonOpts,
    #[arg(long)]
    pub source: String,
    #[arg(long)]
    pub kind: String,
    #[arg(long)]
    pub confidence: Option<i64>,
    /// 证据指针（必填；收割双件套 locator＋摘录合计 ≤2000 字符）
    #[arg(long)]
    pub evidence: String,
    /// 正文；`-` = 从 stdin 读
    pub msg: Option<String>,
}

/// 提交结果（propose/interview 单条与 harvest 转正共用）
#[derive(Debug, Clone)]
pub(crate) struct SubmitOutcome {
    /// inbox 顶层相对路径 `inbox/<filename>`（幂等命中 = 既有文件路径）
    pub file: String,
    /// 幂等命中（重提交返回既有文件，不重复落盘）
    pub idempotent: bool,
    /// git 是否实际产生提交（false = 未开自动提交 / 无变更 / harvest 延迟批量提交 / 降级）
    pub committed: bool,
    /// 随本条产生的警告（W_SECRET advisory / W_GIT_UNAVAILABLE）
    pub warnings: Vec<Warning>,
}

/// 共享提交助手（§7.3 全流程）：source 绑定（步骤 0）→ check_proposal（1–8）→
/// 落盘 inbox → 自动提交（harvest 场景除外——由调用方批量提交 `harvest: stage`，§2.3）。
///
/// 冻结签名：`submit_proposal(ctx, input, date)`；bootstrap 限流放宽经 `ctx.command`
/// 判定（仅 harvest 命令进入 bootstrap 模式，上限 = 该 source 注册的 first_batch 封顶 30，
/// FR-4.3 首批 ≤30 硬闸；`--limit` 只截断候选数、不放大上限）。
pub(crate) fn submit_proposal(
    ctx: &Ctx,
    input: &ProposalInput,
    date: &str,
) -> Result<SubmitOutcome, DexError> {
    // 步骤 0：source 与客户端身份绑定（FR-4.7）——失败写 audit
    if let Err(e) = check_source_binding(&ctx.client, &input.source) {
        ctx.audit(&input.source, e.code());
        return Err(e);
    }
    // 限流参数：harvest 转正走 bootstrap 放宽（first_batch 封顶 30），其余按客户端日限
    let bootstrap_first_batch = (ctx.command == "harvest").then(|| {
        ctx.config
            .harvest
            .sources
            .get(&input.source)
            .map(|s| s.first_batch.min(30))
            .unwrap_or(30)
    });
    let limits = ctx.limits(bootstrap_first_batch);

    // 步骤 1–8（core 纯函数）；协议面拒绝写 audit（FR-10.6）
    let outcome = match check_proposal(input, &limits, &Inbox::new(&ctx.root), date) {
        Ok(o) => o,
        Err(e) => {
            if e.is_denial() {
                ctx.audit(&input.source, e.code());
            }
            return Err(e);
        }
    };
    let mut warnings = Vec::new();
    // advisory 密钥命中 → W_SECRET 放行（check_proposal 静默通过，这里补警告面）
    if limits.secret_advisory {
        if let Some(pattern) = scan_secrets(&format!("{}{}", input.content, input.evidence)).first()
        {
            warnings.push(Warning::Secret {
                pattern: pattern.to_string(),
            });
        }
    }
    // 幂等命中：返回既有文件路径，不落盘不提交（§5.5-7）
    if let Some(existing) = &outcome.idempotent_existing {
        let filename = existing
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| outcome.filename.clone());
        return Ok(SubmitOutcome {
            file: format!("inbox/{filename}"),
            idempotent: true,
            committed: false,
            warnings,
        });
    }
    // 落盘 inbox/<filename>（frontmatter 由 render 统一生成；confidence 原样透传）
    let meta = ProposalMeta {
        source: input.source.clone(),
        kind: input.kind.clone(),
        confidence: input.confidence,
        evidence: input.evidence.clone(),
    };
    let inbox_dir = ctx.root.join("inbox");
    std::fs::create_dir_all(&inbox_dir).map_err(|e| DexError::RepoState {
        message: format!("inbox 目录创建失败：{e}"),
    })?;
    std::fs::write(
        inbox_dir.join(&outcome.filename),
        render_proposal_file(&meta, &input.content),
    )
    .map_err(|e| DexError::RepoState {
        message: format!("提案落盘失败（{}）：{e}", outcome.filename),
    })?;
    // git 自动提交（§2.3 模板）；harvest 由调用方批量提交，此处不重复
    let (committed, commit_warnings) = if ctx.command == "harvest" {
        (false, Vec::new())
    } else {
        auto_commit(ctx, &format!("inbox: propose from {}", input.source))
    };
    warnings.extend(commit_warnings);
    Ok(SubmitOutcome {
        file: format!("inbox/{}", outcome.filename),
        idempotent: false,
        committed,
        warnings,
    })
}

/// 自动提交 + 降级（FR-4.5/NFR-5）：git 缺失或任何提交失败 → W_GIT_UNAVAILABLE
/// （数据不丢、留痕缺失），**永不失败**；返回 (是否实际提交, 警告)。
pub(crate) fn auto_commit(ctx: &Ctx, message: &str) -> (bool, Vec<Warning>) {
    if !ctx.config.git_auto_commit {
        return (false, Vec::new());
    }
    match commit_all(&ctx.root, message) {
        Ok(CommitOutcome::Committed) => (true, Vec::new()),
        Ok(CommitOutcome::NothingToCommit) => (false, Vec::new()),
        // git_missing 与其他错误一律降级（git_missing(&e) 场景 detail 即「git 不可用…」）
        Err(e) => (
            false,
            vec![Warning::GitUnavailable {
                detail: e.to_string(),
            }],
        ),
    }
}

/// 正文读取：位置参数直取；`-` = stdin 全量读入（trim 尾换行——保留原文、仅去行尾换行）；
/// 两者皆缺 → E_BAD_ARGS。
pub(crate) fn read_body(msg: &Option<String>) -> Result<String, DexError> {
    match msg.as_deref() {
        Some("-") => {
            let mut buf = String::new();
            std::io::Read::read_to_string(&mut std::io::stdin(), &mut buf).map_err(|e| {
                DexError::BadArgs {
                    message: format!("stdin 读取失败：{e}"),
                }
            })?;
            Ok(buf.trim_end_matches(['\n', '\r']).to_string())
        }
        Some(text) => Ok(text.to_string()),
        None => Err(DexError::BadArgs {
            message: "正文必填：位置参数 msg 或 `-`（stdin）".to_string(),
        }),
    }
}

pub fn run(args: &ProposeArgs, ctx: &Ctx) -> Result<i32, DexError> {
    let content = read_body(&args.msg)?;
    let input = ProposalInput {
        source: args.source.clone(),
        kind: args.kind.clone(),
        confidence: args.confidence,
        evidence: args.evidence.clone(),
        content,
    };
    let outcome = submit_proposal(ctx, &input, &today_local())?;
    let data = json!({
        "file": outcome.file,
        "idempotent": outcome.idempotent,
        "committed": outcome.committed,
    });
    let text = format!("{}\n", outcome.file);
    Ok(ctx.emit(0, outcome.warnings, data, text))
}
