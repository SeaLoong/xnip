//! `xnip mcp` 子命令：启动 stdio MCP server。
//!
//! PLAN.md §6.10（M9 Agent 集成与 MCP）。
//!
//! 设计要点：
//! - **不接受额外参数**：MCP server 由 stdio 协议驱动，所有请求来自 client 连接，
//!   xnip 自身不需要 cli flag。未来若新增 `--http <addr>` 等传输方式，再扩展。
//! - **强制禁用全局彩色与 quiet**：MCP 输出经 stdout 走协议，stderr 上的提示对
//!   client 透明。这里不强制 quiet——MCP server 可以在 stderr 写日志辅助调试。
//! - **退出码**：成功握手且对端关闭 → 0；任何错误 → 1。

use clap::Args as ClapArgs;

use crate::output::exit;

#[derive(Debug, ClapArgs)]
pub struct Args {
    // 占位：未来扩展 transport 选项时填这里。
    // 目前 stdio 是唯一支持的 transport。
}

// 保持与其它 cli 命令 (`peek::run` / `find::run` 等) 同样的 `Result<u8>` 签名，
// 便于 cli/mod.rs 里统一 dispatch。Clippy 会警告“return value unnecessarily wrapped”，抑制。
#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
pub fn run(_a: Args) -> anyhow::Result<u8> {
    // ANSI 颜色对 MCP 协议 stdout 是污染——强制关闭彩色诊断输出。
    // (NO_COLOR 已在 cli::run 提前判定；这里再次确保。)
    crate::trace!("xnip mcp: starting stdio server");

    match crate::mcp::serve_stdio() {
        Ok(()) => Ok(exit::SUCCESS),
        Err(e) => {
            // MCP server 错误走 stderr，不污染 stdout 协议流。
            eprintln!("xnip mcp: {e:#}");
            Ok(1)
        }
    }
}
