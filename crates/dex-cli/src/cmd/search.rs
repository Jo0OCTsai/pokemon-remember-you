//! `dex search <query>`（FR-6.1 / §8.1）：v1 内嵌 ripgrep 同源引擎全文检索。
//!
//! 冻结语义：
//! - 缺省检索集 = 该客户端白名单 ∩ 四层 scope 目录（CLI/MCP 同口径，§5.3）；
//!   显式 `--scope`：整单 fail-closed 白名单判定（§5.1 放行规则：journal/archive 显式可检索）
//! - 输出行 `path:line:scope:content`；`--limit` 封顶 20（clamp，负数/0 → E_BAD_ARGS）
//! - 退出码 0 命中 / 1 无结果（E_NOT_FOUND）/ 2 参数错（未知 scope、坏正则）/ 3 scope 拒绝
//! - 越权/路径拒绝写 audit.log

use crate::cmd::CommonOpts;
use crate::guard_runtime::Ctx;
use clap::Args;
use dex_core::scope::{default_search_dirs, expand, ScopeId};
use dex_core::DexError;
use serde_json::json;

/// `--limit` 缺省值（与 MCP §8.2 一致）
const DEFAULT_LIMIT: usize = 10;
/// `--limit` 上限（§8.1）
pub const MAX_LIMIT: usize = 20;

#[derive(Args, Debug)]
pub struct SearchArgs {
    #[command(flatten)]
    pub common: CommonOpts,
    /// 检索词（正则语法；编译失败 → E_BAD_ARGS）
    pub query: String,
    /// scope 列表（逗号分隔；缺省 = 白名单 ∩ 四层）
    #[arg(long, value_delimiter = ',')]
    pub scope: Option<Vec<String>>,
    /// 结果条数上限（封顶 20，§8.1）
    #[arg(long)]
    pub limit: Option<usize>,
    /// 忽略派生索引（v1 恒无索引，等价 no-op）
    #[arg(long)]
    pub no_index: bool,
}

pub fn run(args: &SearchArgs, ctx: &Ctx) -> Result<i32, DexError> {
    // 1. scope 解析：逗号分隔字符串 → ScopeId（未知/非法 → E_BAD_ARGS · 2）
    let declared: Vec<ScopeId> = args
        .scope
        .as_deref()
        .unwrap_or(&[])
        .iter()
        .map(|s| ScopeId::new(s.as_str()))
        .collect::<Result<Vec<_>, _>>()?;

    // 2. --limit：None → 10；0 → E_BAD_ARGS；>20 → clamp 20（§8.1）
    let limit = match args.limit {
        None => DEFAULT_LIMIT,
        Some(0) => {
            return Err(DexError::BadArgs {
                message: "--limit 须为 1–20（0 无意义）".into(),
            })
        }
        Some(n) => n.min(MAX_LIMIT),
    };

    // 3. 白名单 fail-closed 展开；拒绝事件由命令侧先 audit 再返回 Err（main 只打信封，FR-10.6）
    let grant = ctx.grant();
    // WORKAROUND（框架缺口，dex-core scope.rs expand）：空声明 + ReadGrant::Full 分支对
    // FOUR_LAYERS 裸名（domains/apps/projects 非合法 ScopeId）调 ScopeId::new().unwrap() 会 panic；
    // default_search_dirs（§5.3 缺省检索集）与该分支语义等价（Full → 四层、Scopes → 白名单 ∩ 四层），
    // 此处空声明改用之——core 修复后可回归 expand。
    let dirs = if declared.is_empty() {
        default_search_dirs(&grant)
    } else {
        match expand(&declared, &grant) {
            Ok(v) => v.dirs,
            Err(e) => {
                let requested = declared
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(",");
                ctx.audit(&requested, e.code());
                return Err(e);
            }
        }
    };

    // 4. ripgrep 同源直扫（正则编译失败已在 store 层映射 E_BAD_ARGS；v1 无索引，--no-index no-op）
    let hits = dex_store::search::search(&ctx.root, &dirs, &args.query, limit)?;
    if hits.is_empty() {
        return Err(DexError::NotFound {
            what: "search 无命中".into(),
        });
    }

    // 5. 输出：text 每行 `path:line:scope:content`；json data = {query, count, results[]}
    let text = hits
        .iter()
        .map(|h| format!("{}:{}:{}:{}\n", h.path, h.line, h.scope, h.content))
        .collect();
    let data = json!({
        "query": args.query,
        "count": hits.len(),
        "results": hits
            .iter()
            .map(|h| json!({
                "path": h.path,
                "line": h.line,
                "scope": h.scope,
                "content": h.content,
            }))
            .collect::<Vec<_>>(),
    });
    Ok(ctx.emit(0, Vec::new(), data, text))
}
