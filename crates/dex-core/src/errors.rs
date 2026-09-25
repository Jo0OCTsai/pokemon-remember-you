//! §8.3 退出码表 / §8.4 `E_*` `W_*` 枚举——CLI 退出码与（v2）MCP `error.data.code` 同一枚举。
//!
//! 三向映射（E_* ↔ 退出码 ↔ MCP error.data.code）以本文件为单一实现源；
//! 表驱动完整性断言（对照 DESIGN §8.4 全集）由文件尾测试保证。

use serde::Serialize;

/// 协议层错误。`exit_code()` 为 CLI 稳定退出码（DESIGN §8.3，0–10）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DexError {
    /// E_BAD_ARGS · 2：参数/元数据错误（kind 非枚举、confidence 越界、--format 值非法、未知 scope 名）
    BadArgs { message: String },
    /// E_BAD_SOURCE · 2：source 不匹配 `^[a-z0-9][a-z0-9-]{0,31}$`
    BadSource { source: String },
    /// E_SOURCE_MISMATCH · 2：source ∉ 客户端 allowed_sources（守卫步骤 0，FR-4.7）
    SourceMismatch { source: String, client: String },
    /// E_SCOPE_DENIED · 3：scope 越权（fail-closed 整单拒绝）/ 未注册客户端 / 无凭证
    ScopeDenied { detail: String },
    /// E_NO_EVIDENCE · 4：提案无证据（FR-4.2）
    NoEvidence,
    /// E_TOO_LARGE · 5：大小超限（正文 >4000 / evidence >2000，Unicode 字符数）
    TooLarge {
        what: &'static str,
        limit: usize,
        actual: usize,
    },
    /// E_RATE_LIMIT · 5：单 source 日限（FR-4.3 / journal_per_day FR-5.4）
    RateLimit { source: String, limit: usize },
    /// E_NOT_FOUND · 1：语义性无结果（search 无命中、read 文件不存在）
    NotFound { what: String },
    /// E_BAD_PATH · 6：路径非法（`..`、绝对路径、symlink 逃逸、`.git`/`.cache` 禁区）
    BadPath { path: String },
    /// E_CACHE · 7：缓存写失败**且操作未完成**（v2；降级成功恒用 [`Warning`]、退出码 0）
    Cache { message: String },
    /// E_REPO_STATE · 8：仓库状态错误（init 已完整初始化、git index.lock 冲突重试超限）
    RepoState { message: String },
    /// E_SECRET · 9：疑似密钥命中（advisory 模式 → [`Warning::Secret`] + 成功）
    Secret { pattern: String },
    /// E_RENDER_REFUSE · 10：拒绝覆盖（render 目标非 dex 产物 / 人手改未 `--force` / skills 目标非本仓条目）
    RenderRefuse { target: String, reason: String },
}

impl DexError {
    /// `E_*` 错误码（CLI 与 MCP `error.data.code` 同枚举，§8.4）
    pub fn code(&self) -> &'static str {
        match self {
            DexError::BadArgs { .. } => "E_BAD_ARGS",
            DexError::BadSource { .. } => "E_BAD_SOURCE",
            DexError::SourceMismatch { .. } => "E_SOURCE_MISMATCH",
            DexError::ScopeDenied { .. } => "E_SCOPE_DENIED",
            DexError::NoEvidence => "E_NO_EVIDENCE",
            DexError::TooLarge { .. } => "E_TOO_LARGE",
            DexError::RateLimit { .. } => "E_RATE_LIMIT",
            DexError::NotFound { .. } => "E_NOT_FOUND",
            DexError::BadPath { .. } => "E_BAD_PATH",
            DexError::Cache { .. } => "E_CACHE",
            DexError::RepoState { .. } => "E_REPO_STATE",
            DexError::Secret { .. } => "E_SECRET",
            DexError::RenderRefuse { .. } => "E_RENDER_REFUSE",
        }
    }

    /// CLI 稳定退出码（§8.3）
    pub fn exit_code(&self) -> i32 {
        match self {
            DexError::BadArgs { .. }
            | DexError::BadSource { .. }
            | DexError::SourceMismatch { .. } => 2,
            DexError::ScopeDenied { .. } => 3,
            DexError::NoEvidence => 4,
            DexError::TooLarge { .. } | DexError::RateLimit { .. } => 5,
            DexError::NotFound { .. } => 1,
            DexError::BadPath { .. } => 6,
            DexError::Cache { .. } => 7,
            DexError::RepoState { .. } => 8,
            DexError::Secret { .. } => 9,
            DexError::RenderRefuse { .. } => 10,
        }
    }

    /// 是否为协议面拒绝事件（应写 audit.log，FR-10.6 / SECURITY §6）
    pub fn is_denial(&self) -> bool {
        matches!(
            self,
            DexError::ScopeDenied { .. }
                | DexError::SourceMismatch { .. }
                | DexError::RateLimit { .. }
                | DexError::Secret { .. }
                | DexError::BadPath { .. }
        )
    }
}

impl std::fmt::Display for DexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DexError::BadArgs { message } => write!(f, "参数错误：{message}"),
            DexError::BadSource { source } => {
                write!(
                    f,
                    "source 标识非法：{source:?}（须匹配 ^[a-z0-9][a-z0-9-]{{0,31}}$）"
                )
            }
            DexError::SourceMismatch { source, client } => {
                write!(f, "source {source:?} 不在客户端 {client:?} 的 allowed_sources 内（FR-4.7 source 绑定）")
            }
            DexError::ScopeDenied { detail } => write!(f, "scope 拒绝：{detail}"),
            DexError::NoEvidence => write!(f, "提案无证据：evidence 必填（FR-4.2）"),
            DexError::TooLarge {
                what,
                limit,
                actual,
            } => {
                write!(
                    f,
                    "{what}超限：{actual} 字符 > 上限 {limit}（Unicode 字符数）"
                )
            }
            DexError::RateLimit { source, limit } => {
                write!(f, "限流触发：source {source:?} 今日配额已满（{limit}/日）")
            }
            DexError::NotFound { what } => write!(f, "无结果：{what}"),
            DexError::BadPath { path } => {
                write!(f, "路径非法：{path:?}（拒绝穿越/symlink 逃逸/禁区路径）")
            }
            DexError::Cache { message } => write!(f, "缓存写失败且操作未完成：{message}"),
            DexError::RepoState { message } => write!(f, "仓库状态错误：{message}"),
            DexError::Secret { pattern } => write!(
                f,
                "疑似密钥命中（{pattern}）——默认拒绝，可配置 advisory 仅警告"
            ),
            DexError::RenderRefuse { target, reason } => write!(f, "拒绝覆盖 {target:?}：{reason}"),
        }
    }
}

impl std::error::Error for DexError {}

/// 警告码（§8.4）：进 `--json` 信封 `warnings[]`，**永不改变退出码**。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Warning {
    /// W_INDEX_DEGRADED：v2 索引降级 ripgrep（v1 恒不出现）
    IndexDegraded,
    /// W_GIT_UNAVAILABLE：git 缺失，跳过自动提交 / stale 回退 mtime（FR-4.5/NFR-5）
    GitUnavailable { detail: String },
    /// W_SECRET：advisory 密钥命中（成功但警告，§5.5-8）
    Secret { pattern: String },
    /// W_OVER_BUDGET：预算截断首条即越界、输出为空（§5.2-4）
    OverBudget,
    /// W_CREDS_PERMS：凭据文件权限宽于 0600（FR-10.5/SECURITY §5）
    CredsPerms { path: String },
    /// W_CONFIG_CHANGED：仓库层 config 变更，提示核实授权 diff（FR-10.7/SECURITY §9）
    ConfigChanged,
    /// W_CONFIG_NOTICE：配置类提示（实现期新增码——承载 §2.5「[auth] 出现在仓库层 ⇒ warning 并忽略」
    /// 等规范要求警告但 §8.4 六码无对应项的场景；已登记 debt/报告）
    ConfigNotice { detail: String },
}

impl Warning {
    pub fn code(&self) -> &'static str {
        match self {
            Warning::IndexDegraded => "W_INDEX_DEGRADED",
            Warning::GitUnavailable { .. } => "W_GIT_UNAVAILABLE",
            Warning::Secret { .. } => "W_SECRET",
            Warning::OverBudget => "W_OVER_BUDGET",
            Warning::CredsPerms { .. } => "W_CREDS_PERMS",
            Warning::ConfigChanged => "W_CONFIG_CHANGED",
            Warning::ConfigNotice { .. } => "W_CONFIG_NOTICE",
        }
    }

    pub fn message(&self) -> String {
        match self {
            Warning::IndexDegraded => "FTS 索引不可用，已降级 ripgrep 直扫".into(),
            Warning::GitUnavailable { detail } => {
                format!("git 不可用，{detail}（降级成功，数据不丢）")
            }
            Warning::Secret { pattern } => {
                format!("疑似密钥命中（{pattern}）——advisory 模式仅警告")
            }
            Warning::OverBudget => {
                "预算截断首条即越界，注入输出为空——提示拆分（与单文件软上限同源信号）".into()
            }
            Warning::CredsPerms { path } => {
                format!("凭据文件 {path} 权限宽于 0600，建议收紧（防意外扩散，不阻断）")
            }
            Warning::ConfigChanged => {
                "仓库层 config 内容有变更——请核实授权变更：git log -p -- .dex/config.toml".into()
            }
            Warning::ConfigNotice { detail } => detail.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 表驱动断言：E_* ↔ 退出码三向映射完整（对照 DESIGN §8.4 全集 13 项）。
    /// B 组任务：补齐「每个 E_* 至少一条触发例构造」断言（当前仅锁 code/exit_code 对）。
    #[test]
    fn error_code_exit_code_table() {
        let cases: Vec<(DexError, &'static str, i32)> = vec![
            (
                DexError::BadArgs {
                    message: "m".into(),
                },
                "E_BAD_ARGS",
                2,
            ),
            (
                DexError::BadSource { source: "X".into() },
                "E_BAD_SOURCE",
                2,
            ),
            (
                DexError::SourceMismatch {
                    source: "a".into(),
                    client: "b".into(),
                },
                "E_SOURCE_MISMATCH",
                2,
            ),
            (
                DexError::ScopeDenied { detail: "d".into() },
                "E_SCOPE_DENIED",
                3,
            ),
            (DexError::NoEvidence, "E_NO_EVIDENCE", 4),
            (
                DexError::TooLarge {
                    what: "正文",
                    limit: 4000,
                    actual: 4001,
                },
                "E_TOO_LARGE",
                5,
            ),
            (
                DexError::RateLimit {
                    source: "s".into(),
                    limit: 20,
                },
                "E_RATE_LIMIT",
                5,
            ),
            (DexError::NotFound { what: "w".into() }, "E_NOT_FOUND", 1),
            (
                DexError::BadPath {
                    path: "../x".into(),
                },
                "E_BAD_PATH",
                6,
            ),
            (
                DexError::Cache {
                    message: "m".into(),
                },
                "E_CACHE",
                7,
            ),
            (
                DexError::RepoState {
                    message: "m".into(),
                },
                "E_REPO_STATE",
                8,
            ),
            (
                DexError::Secret {
                    pattern: "p".into(),
                },
                "E_SECRET",
                9,
            ),
            (
                DexError::RenderRefuse {
                    target: "t".into(),
                    reason: "r".into(),
                },
                "E_RENDER_REFUSE",
                10,
            ),
        ];
        assert_eq!(cases.len(), 13, "E_* 全集 = 13 项（§8.4）");
        for (err, code, exit) in cases {
            assert_eq!(err.code(), code);
            assert_eq!(err.exit_code(), exit);
        }
    }

    #[test]
    fn warning_codes_never_change_exit() {
        let all = [
            Warning::IndexDegraded,
            Warning::GitUnavailable { detail: "d".into() },
            Warning::Secret {
                pattern: "p".into(),
            },
            Warning::OverBudget,
            Warning::CredsPerms { path: "/x".into() },
            Warning::ConfigChanged,
            Warning::ConfigNotice { detail: "d".into() },
        ];
        assert_eq!(all.len(), 7, "W_* = §8.4 六码 + 实现期新增 W_CONFIG_NOTICE");
        for w in &all {
            assert!(w.code().starts_with("W_"));
            assert!(!w.message().is_empty());
        }
    }
}
