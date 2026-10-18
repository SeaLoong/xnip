//! Precise byte-preserving text editing for agents.
//!
//! CLI and MCP adapters share read queries, edit plans and persistence contracts.
//! See `docs/architecture.md` for the module map and operational limits.

pub mod apply;
pub mod cli;
pub mod core;
pub mod doctor;
pub mod mcp;
pub mod output;
pub mod update;

/// 当前 crate 版本（编译期注入）。
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// 编译期 git commit（CI release 时通过 `XNIP_BUILD_COMMIT` 环境变量注入；本地构建为 `unknown`）。
pub const BUILD_COMMIT: &str = match option_env!("XNIP_BUILD_COMMIT") {
    Some(c) => c,
    None => "unknown",
};
