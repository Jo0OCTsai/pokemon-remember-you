//! dex 基础设施层：文件树遍历（fs）、git 适配（git）、ripgrep 检索后端（search）、
//! inbox 视图与收割暂存（inbox）、守卫审计日志（audit）。
//!
//! 依赖方向：`store → core`（实现 core 端口，如 [`inbox::Inbox`] 实现
//! [`dex_core::guard::InboxView`]）。git 一律子进程调用 ≥2.20（不用 libgit2，DESIGN §9）。

pub mod audit;
pub mod fs;
pub mod git;
pub mod inbox;
pub mod search;
