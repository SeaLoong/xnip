//! xnip — Precise text editing CLI for LLM agents.
//!
//! 库形态导出，便于集成测试与未来嵌入；二进制入口见 `src/main.rs`。
//!
//! 顶层模块组织遵循 `PLAN.md` 7.1：
//! - [`cli`]：clap derive 结构与子命令分发
//! - [`core`]：定位 / 内容 / 原子写入 / diff / revert / ops 七命令纯函数实现
//! - [`apply`]：批量编辑清单解析（native / json / yaml）+ 两阶段提交
//! - [`output`]：人类向输出 / NDJSON / 退出码常量
//! - [`doctor`]：环境自检
//!
//! M0 阶段各模块均为占位骨架。

#![doc(html_root_url = "https://docs.rs/xnip/0.1.0")]

pub mod apply;
pub mod cli;
pub mod core;
pub mod doctor;
pub mod mcp;
pub mod output;

/// 当前 crate 版本（编译期注入）。
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// 编译期 git commit（CI release 时通过 `XNIP_BUILD_COMMIT` 环境变量注入；本地构建为 `unknown`）。
pub const BUILD_COMMIT: &str = match option_env!("XNIP_BUILD_COMMIT") {
    Some(c) => c,
    None => "unknown",
};
