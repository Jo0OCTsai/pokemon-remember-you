//! 检索后端（C3）：v1 ripgrep 直扫（内嵌同源引擎，FR-6.1——禁止 shell out 到 rg 二进制）。

pub mod rg;

pub use rg::{search, Hit};
