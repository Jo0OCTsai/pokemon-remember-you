//! `dex reindex`（FR-6.7，v2）：v1 存根——无索引层，检索恒为 ripgrep 直扫。

use crate::cmd::CommonOpts;
use crate::guard_runtime::Ctx;
use clap::Args;
use dex_core::DexError;

#[derive(Args, Debug)]
pub struct ReindexArgs {
    #[command(flatten)]
    pub common: CommonOpts,
    /// v2 语义：强制全量重建（v1 无效）
    #[arg(long)]
    pub force: bool,
}

pub fn run(args: &ReindexArgs, ctx: &Ctx) -> Result<i32, DexError> {
    let data = serde_json::json!({
        "note": "reindex 自 v2 起提供（FTS 派生索引）；当前 v1 无索引层，检索恒为 ripgrep 直扫",
        "force": args.force,
    });
    Ok(ctx.emit(
        0,
        vec![],
        data,
        "dex reindex 自 v2 起提供；当前版本无索引层（检索恒走内嵌 ripgrep 直扫）。\n".into(),
    ))
}
