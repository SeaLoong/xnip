//! 8 个 MCP 工具的实现。
//!
//! 每个工具一个子模块，包含：
//! - [`Args`]：JsonSchema 输入结构（与 cli args 字段同名同义，便于复用）
//! - `run(args) -> Result<CallToolResult, McpError>`：同步入口
//! - tool description 直接写在 [`super::server::XnipServer`] 的 `#[tool]` 属性里。
//!
//! 所有工具共享 [`common`] 模块的辅助函数（错误转换、Locator 构造等）。
#![allow(clippy::doc_markdown)] // [`Args`] 是 intra-doc link，clippy 误报

pub mod common;

pub mod apply;
pub mod doctor;
pub mod find;
pub mod indent;
pub mod insert;
pub mod move_op;
pub mod peek;
pub mod replace;
