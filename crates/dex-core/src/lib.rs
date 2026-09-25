//! dex 领域层：纯逻辑、无 I/O（DESIGN §9）。
//!
//! 依赖方向严格单向：`cli → core ← store`；本 crate 不做任何文件/网络/进程 I/O。
//! 守卫校验序 6/7（inbox 计数与幂等查重）经 [`guard::InboxView`] 端口声明、由 store 实现。

pub mod decay;
pub mod entry;
pub mod errors;
pub mod guard;
pub mod inject;
pub mod proposal;
pub mod scope;

pub use errors::{DexError, Warning};
pub use scope::{ReadGrant, ScopeId, VisibleSet};
