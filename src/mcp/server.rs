//! MCP server bootstrap：把 `XnipServer` 与 stdio 传输 wire 起来。
//!
//! rmcp 1.7 API 形态（参考官方 `tests/test_tool_macros.rs::MinimalServer`）：
//! ```ignore
//! XnipServer::new()
//!     .serve(rmcp::transport::stdio())
//!     .await?
//!     .waiting()
//!     .await?;
//! ```
//!
//! 这里把它包装成同步入口 `serve_stdio()`，由 `cli/mcp.rs` 调用，
//! 内部启动 tokio 单线程 runtime（避免 panic=abort 与多 worker 冲突）。

use anyhow::{Context, Result};
use rmcp::{
    ErrorData as McpError, ServerHandler, ServiceExt,
    handler::server::router::tool::ToolRouter,
    model::{ServerCapabilities, ServerInfo},
    tool_handler, tool_router,
};

use super::tools;

/// MCP server 状态。
///
/// 当前所有 tool 都是无状态纯计算(输入文件路径 + 选项 → 结果)，
/// 因此 `XnipServer` 自身只持有 `ToolRouter`(rmcp 宏要求字段名为 `tool_router`)。
#[derive(Debug, Clone)]
pub struct XnipServer {
    // rmcp 的 #[tool_router] / #[tool_handler] 宏在实现块里使用此字段，
    // 但文本分析器看不到跨宏读路径，会报 dead_code。
    #[allow(dead_code)]
    tool_router: ToolRouter<Self>,
}

impl XnipServer {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }
}

impl Default for XnipServer {
    fn default() -> Self {
        Self::new()
    }
}

#[tool_router]
impl XnipServer {
    // === 8 个 tool 的实际实现委托给 super::tools::* ===
    // 为什么 description 直写而不用模块常量:
    // rmcp 的 #[tool] 属性宏要求 description = 字面字符串，
    // 不接受路径表达式(如 tools::peek::DESCRIPTION)。

    #[rmcp::tool(
        description = "Print numbered lines from a file. Provide exactly one of `lines` (range like '30' or '30-45'), `match_line` (regex on whole line), or `all=true`. Use `context` (only with `match_line`) for surrounding lines, and `max_lines` to cap output. Read-only."
    )]
    async fn xnip_peek(
        &self,
        params: rmcp::handler::server::wrapper::Parameters<tools::peek::Args>,
    ) -> Result<rmcp::model::CallToolResult, McpError> {
        tools::peek::run(params.0)
    }

    #[rmcp::tool(
        description = "Search and locate matches across one or more files. Provide one of `pattern` (cross-line byte regex, output `path:line:col`) or `match_line` (whole-line regex, output `path:line`). Read-only."
    )]
    async fn xnip_find(
        &self,
        params: rmcp::handler::server::wrapper::Parameters<tools::find::Args>,
    ) -> Result<rmcp::model::CallToolResult, McpError> {
        tools::find::run(params.0)
    }

    #[rmcp::tool(
        description = "Replace or delete a region in a file. Pick exactly one locator: `lines`/`match_line`/`between`/`between_re`/`pattern`. Provide `text` or `text_file` (or `repl` with `pattern`). Pass `was`/`was_file` for concurrency safety; `backup=true` keeps a `.bak` sidecar. Atomic write."
    )]
    async fn xnip_replace(
        &self,
        params: rmcp::handler::server::wrapper::Parameters<tools::replace::Args>,
    ) -> Result<rmcp::model::CallToolResult, McpError> {
        tools::replace::run(params.0)
    }

    #[rmcp::tool(
        description = "Insert text before/after a single-line anchor. Locator must resolve to one line; use `position`='before'|'after' (default 'after'). Provide `text` or `text_file`. Atomic write."
    )]
    async fn xnip_insert(
        &self,
        params: rmcp::handler::server::wrapper::Parameters<tools::insert::Args>,
    ) -> Result<rmcp::model::CallToolResult, McpError> {
        tools::insert::run(params.0)
    }

    #[rmcp::tool(
        name = "xnip_move",
        description = "Move a contiguous block of lines to another location. Source via `from_lines` (e.g. '30-45') or `from_match_line`; target via `to` (line number) and `position`. Atomic write."
    )]
    async fn xnip_move_op(
        &self,
        params: rmcp::handler::server::wrapper::Parameters<tools::move_op::Args>,
    ) -> Result<rmcp::model::CallToolResult, McpError> {
        tools::move_op::run(params.0)
    }

    #[rmcp::tool(
        description = "Adjust indentation or convert tabs/spaces over a line range. Pick exactly one op: `add`/`remove` N spaces, or `tabs_to_spaces`/`spaces_to_tabs` N. Range via `lines` or `all=true`. Atomic write."
    )]
    async fn xnip_indent(
        &self,
        params: rmcp::handler::server::wrapper::Parameters<tools::indent::Args>,
    ) -> Result<rmcp::model::CallToolResult, McpError> {
        tools::indent::run(params.0)
    }

    #[rmcp::tool(
        description = "Apply a batch of edits atomically. Provide `path` (manifest file) or `manifest_text` (inline native/json/yaml content). Two-phase commit: stage 1 plans all writes, stage 2 commits or rolls back on any failure."
    )]
    async fn xnip_apply(
        &self,
        params: rmcp::handler::server::wrapper::Parameters<tools::apply::Args>,
    ) -> Result<rmcp::model::CallToolResult, McpError> {
        tools::apply::run(params.0)
    }

    #[rmcp::tool(
        description = "Self-diagnose xnip environment: version, build commit, runtime info. No arguments."
    )]
    async fn xnip_doctor(&self) -> Result<rmcp::model::CallToolResult, McpError> {
        tools::doctor::run()
    }
}
#[tool_handler]
impl ServerHandler for XnipServer {
    fn get_info(&self) -> ServerInfo {
        let mut info = ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions(INSTRUCTIONS);
        // 覆盖默认的 rmcp/0 为本 crate 的 name/version
        info.server_info.name = env!("CARGO_PKG_NAME").to_string();
        info.server_info.version = env!("CARGO_PKG_VERSION").to_string();
        info
    }
}

/// 顶层指令文本：告诉 LLM 这个 server 是什么、有哪些约定。
/// 越精炼越好——LLM 上下文宝贵，不写废话。
const INSTRUCTIONS: &str = r"xnip — precise text editing for LLM agents.

All tools operate on local file paths (relative to current working directory) \
and use 1-based closed-interval line numbers. Use `xnip_peek` first to inspect a \
file before editing; use `xnip_find` to locate edit anchors. Write tools (replace/ \
insert/move/indent/apply) commit atomically; pass `was`/`was_file` for concurrency \
safety, or `backup=true` to keep a `.bak` sidecar.";

/// 启动 stdio MCP server 并阻塞至连接关闭。
///
/// 在内部建立单线程 tokio runtime；rmcp 的 stdio 传输基于 `tokio::io::stdin/stdout`。
/// 单线程是有意为之：
/// - MCP stdio 单连接串行处理，多 worker 收益为零
/// - 与 `panic = "abort"` 兼容（多 worker panic 会让进程死）
///
/// # Errors
/// runtime 构造失败、序列化失败、传输错误等。
pub fn serve_stdio() -> Result<()> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("failed to build tokio runtime for MCP server")?;

    rt.block_on(async {
        let transport = (tokio::io::stdin(), tokio::io::stdout());
        let service = XnipServer::new()
            .serve(transport)
            .await
            .context("MCP handshake failed")?;
        // waiting() 直到 client 关闭连接（stdin EOF）。
        service
            .waiting()
            .await
            .context("MCP service ended with error")?;
        Ok::<_, anyhow::Error>(())
    })
}
