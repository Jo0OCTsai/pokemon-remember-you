//! dex CLI 命令面（§8.1）。每个子目录一个命令；Args 结构冻结 CLI 表面，
//! `run` 由 D 组实现。公共参数经 [`CommonOpts`] 展平进每个命令。

use clap::Args;

/// 全命令公共参数（FR-6.10：`--json` ≡ `--format json`，同给冲突时 `--format` 优先；
/// FR-10.3：非交互调用须 `--client`）。
#[derive(Args, Debug)]
pub struct CommonOpts {
    /// JSON 机器可读输出（≡ --format json；同给冲突时 --format 优先）
    #[arg(long)]
    pub json: bool,
    /// 输出格式：text | json
    #[arg(long, value_name = "text|json")]
    pub format: Option<String>,
    /// 非交互调用的客户端 id（FR-10.3）
    #[arg(long, value_name = "ID")]
    pub client: Option<String>,
}

/// render 专用公共参数：其 `--format` 语义为**产物格式**（merged|import，§8.1），
/// 机器可读输出走 `--json`（特例冻结：render 的 --format 域不同）。
#[derive(Args, Debug)]
pub struct JsonClientOpts {
    /// JSON 机器可读输出
    #[arg(long)]
    pub json: bool,
    /// 非交互调用的客户端 id（FR-10.3）
    #[arg(long, value_name = "ID")]
    pub client: Option<String>,
}

pub mod harvest;
pub mod init;
pub mod interview;
pub mod journal;
pub mod lint;
pub mod propose;
pub mod read;
pub mod reindex_stub;
pub mod render;
pub mod review;
pub mod search;
pub mod skills;
pub mod stale;
