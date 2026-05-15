//! MCP (Model Context Protocol) server 模式。
//!
//! 通过 `xnip mcp` 子命令启动 stdio MCP server，向 LLM agent 暴露 8 个工具：
//! - `xnip_peek` / `xnip_find`：只读查询
//! - `xnip_replace` / `xnip_insert` / `xnip_move` / `xnip_indent`：单文件写
//! - `xnip_apply`：批量原子写
//! - `xnip_doctor`：环境自检
//!
//! 设计原则（PLAN.md §7.1 的延伸）：
//! 1. **复用 `core::ops::*` 与 `apply::commit::*` 而非 cli 层**——cli 是另一种“前端”，
//!    MCP 是平行的前端，二者都直接调 core；不互相依赖。
//! 2. **不暴露 `--dry-run` / `--check` / `--revert`**：
//!    - dry-run 的语义对 LLM 不友好（拿到 diff 不如让 LLM 自己看新文件）
//!    - check 用 `Err(McpError)` 表达更直接
//!    - revert 是 cli 的便利特性，对 LLM 直接构造反向编辑成本很低
//! 3. **保留 `was` 与 `backup`**：
//!    - was 是关键并发保护——MCP 长会话中文件可能被外部改动
//!    - backup 是用户安全旁路，按需开启
//! 4. **彩色输出禁用**：MCP 输出到 LLM，ANSI 转义只会污染。
//! 5. **错误用 `McpError`**：tool 函数返回 `Result<CallToolResult, McpError>`；
//!    用户错误 `invalid_params`，定位失败 `invalid_request`，IO 错误 `internal_error`。

// MCP tool handler 函数习惯上 `args` 按值传入（rmcp 宏生成的 wrapper 是这样），
// 这里统一抑制 pedantic 警告。
#![allow(clippy::needless_pass_by_value)]

mod server;
mod tools;

pub use server::serve_stdio;
