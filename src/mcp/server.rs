//! Stdio MCP server, selected tool catalog and schema presentation.

use anyhow::{Context, Result};
use rmcp::{
    ErrorData as McpError, ServerHandler, ServiceExt,
    handler::server::router::tool::ToolRouter,
    model::{ServerCapabilities, ServerConfig},
    tool_handler, tool_router,
};

use super::tools;

/// MCP server 状态。
///
/// 编辑实现由共享工具处理；server 只保存工具路由与回执展示配置。
/// rmcp 宏要求路由字段名为 `tool_router`。
#[derive(Debug, Clone)]
pub struct XnipServer {
    tool_router: ToolRouter<Self>,
    receipts: Receipts,
}

/// Presentation of edit/apply/recover results; editing behavior is identical.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Receipts {
    /// Preserve machine-readable change metadata alongside the text receipt.
    #[default]
    Structured,
    /// Return only the existing text receipt and bounded preview.
    Text,
}

/// Which editing tools are advertised to the client.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Toolset {
    /// Five tools for everyday editing, reading and recovery.
    #[default]
    Compact,
    /// All tools, including specialized line operations and diagnostics.
    Full,
}

impl XnipServer {
    pub fn new() -> Self {
        Self::with_toolset(Toolset::Compact)
    }

    pub fn with_toolset(toolset: Toolset) -> Self {
        Self::with_options(toolset, Receipts::Structured)
    }

    pub fn with_options(toolset: Toolset, receipts: Receipts) -> Self {
        // Probe handles before Tokio starts its blocking stdin reader. Doing
        // this inside xnip_doctor can wait forever on a live Windows pipe.
        crate::doctor::capture_stdio_status();
        let mut tool_router = Self::tool_router();
        if toolset == Toolset::Compact {
            tool_router.map.retain(|name, _| {
                matches!(
                    name.as_ref(),
                    "xnip_edit" | "xnip_apply" | "xnip_peek" | "xnip_find" | "xnip_recover"
                )
            });
        }
        for (name, route) in &mut tool_router.map {
            // Schema titles describe Rust types, not facts a caller needs.
            let mut schema = serde_json::Value::Object((*route.attr.input_schema).clone());
            compact_schema(&mut schema);
            route.attr.input_schema =
                std::sync::Arc::new(schema.as_object().expect("object schema").clone());
            if receipts == Receipts::Text
                && matches!(name.as_ref(), "xnip_edit" | "xnip_apply" | "xnip_recover")
            {
                route.attr.output_schema = None;
            }
            if let Some(output) = &mut route.attr.output_schema {
                let mut schema = serde_json::Value::Object((**output).clone());
                compact_schema(&mut schema);
                *output = std::sync::Arc::new(schema.as_object().expect("object schema").clone());
            }
        }
        Self {
            tool_router,
            receipts,
        }
    }

    fn finish_receipt(
        &self,
        result: Result<rmcp::model::CallToolResult, McpError>,
    ) -> Result<rmcp::model::CallToolResult, McpError> {
        result.map(|mut receipt| {
            if self.receipts == Receipts::Text && receipt.is_error != Some(true) {
                receipt.structured_content = None;
            }
            receipt
        })
    }
}

impl Default for XnipServer {
    fn default() -> Self {
        Self::new()
    }
}

#[tool_router]
impl XnipServer {
    #[rmcp::tool(
        title = "Recover file changes",
        annotations(read_only_hint = false, destructive_hint = true, open_world_hint = false),
        output_schema = tools::common::change_schema(),
        description = "Restore a known failed transaction after its writer stops. Refuses external changes. dry_run previews."
    )]
    async fn xnip_recover(
        &self,
        params: rmcp::handler::server::wrapper::Parameters<tools::recover::Args>,
    ) -> Result<rmcp::model::CallToolResult, McpError> {
        self.finish_receipt(tools::recover::run(params.0, self.receipts))
    }
    // === 10 个 tool 的实际实现委托给 super::tools::* ===
    #[rmcp::tool(
        title = "Edit files",
        annotations(read_only_hint = false, destructive_hint = true, open_world_hint = false),
        output_schema = tools::common::change_schema(),
        description = "Replace unique old text, insert before/after it, create, write whole files, delete, or rename. write old guards the whole file. all replaces every match. dry_run/check never write."
    )]
    async fn xnip_edit(
        &self,
        params: rmcp::handler::server::wrapper::Parameters<tools::edit::Args>,
    ) -> Result<rmcp::model::CallToolResult, McpError> {
        self.finish_receipt(tools::edit::run(params.0, self.receipts))
    }
    // 为什么 description 直写而不用模块常量:
    // rmcp 的 #[tool] 属性宏要求 description = 字面字符串，
    // 不接受路径表达式(如 tools::peek::DESCRIPTION)。

    #[rmcp::tool(
        title = "Read file lines",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        ),
        description = "Read numbered lines. Choose lines, match_line, or all. Output is bounded; truncation is explicit."
    )]
    async fn xnip_peek(
        &self,
        params: rmcp::handler::server::wrapper::Parameters<tools::peek::Args>,
    ) -> Result<rmcp::model::CallToolResult, McpError> {
        tools::peek::run(params.0)
    }

    #[rmcp::tool(
        title = "Find text",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        ),
        description = "Find regex matches. Choose pattern (path:line:byte-column) or match_line (path:line)."
    )]
    async fn xnip_find(
        &self,
        params: rmcp::handler::server::wrapper::Parameters<tools::find::Args>,
    ) -> Result<rmcp::model::CallToolResult, McpError> {
        tools::find::run(params.0)
    }

    #[rmcp::tool(
        title = "Replace text ranges",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            open_world_hint = false
        ),
        description = "Replace or delete a region in a file. Pick exactly one locator: `lines`/`match_line`/`between`/`between_re`/`pattern`. Provide `text` or `text_file` (or `repl` with `pattern`). Pass `was`/`was_file` to check the expected original bytes; `backup=true` keeps a `.bak` sidecar. Atomic write."
    )]
    async fn xnip_replace(
        &self,
        params: rmcp::handler::server::wrapper::Parameters<tools::replace::Args>,
    ) -> Result<rmcp::model::CallToolResult, McpError> {
        tools::replace::run(params.0)
    }

    #[rmcp::tool(
        title = "Insert text",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            open_world_hint = false
        ),
        description = "Insert text before/after a single-line anchor. Locator must resolve to one line; use `position`='before'|'after' (default 'after'). Provide `text` or `text_file`. Atomic write."
    )]
    async fn xnip_insert(
        &self,
        params: rmcp::handler::server::wrapper::Parameters<tools::insert::Args>,
    ) -> Result<rmcp::model::CallToolResult, McpError> {
        tools::insert::run(params.0)
    }

    #[rmcp::tool(
        title = "Move text lines",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            open_world_hint = false
        ),
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
        title = "Adjust indentation",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            open_world_hint = false
        ),
        description = "Adjust indentation or convert tabs/spaces over a line range. Pick exactly one op: `add`/`remove` N spaces, or `tabs_to_spaces`/`spaces_to_tabs` N. Range via `lines` or `all=true`. Atomic write."
    )]
    async fn xnip_indent(
        &self,
        params: rmcp::handler::server::wrapper::Parameters<tools::indent::Args>,
    ) -> Result<rmcp::model::CallToolResult, McpError> {
        tools::indent::run(params.0)
    }

    #[rmcp::tool(
        title = "Apply file changes",
        annotations(read_only_hint = false, destructive_hint = true, open_world_hint = false),
        output_schema = tools::common::change_schema(),
        description = "Batch with files: path maps to [[old,text],...] unique pairs, full text, or {text,old?} guarded whole writes. Or use edits/path/manifest_text. Validates before writing; failed commits retain a recovery journal. dry_run/check never write."
    )]
    async fn xnip_apply(
        &self,
        params: rmcp::handler::server::wrapper::Parameters<tools::apply::Args>,
    ) -> Result<rmcp::model::CallToolResult, McpError> {
        self.finish_receipt(tools::apply::run(params.0, self.receipts))
    }

    #[rmcp::tool(
        title = "Check xnip environment",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        ),
        description = "Self-diagnose xnip environment: version, build commit, runtime info. No arguments."
    )]
    async fn xnip_doctor(
        &self,
        _params: rmcp::handler::server::wrapper::Parameters<tools::doctor::Args>,
    ) -> Result<rmcp::model::CallToolResult, McpError> {
        tools::doctor::run()
    }
}
// rmcp generates async trait methods; their signatures are owned by the SDK.
#[allow(unknown_lints, clippy::unused_async_trait_impl)]
#[tool_handler(router = self.tool_router)]
impl ServerHandler for XnipServer {
    fn get_info(&self) -> ServerConfig {
        let mut info = ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions(format!(
                "{INSTRUCTIONS_START}{}{INSTRUCTIONS_END}",
                match self.receipts {
                    Receipts::Structured => STRUCTURED_RECEIPTS,
                    Receipts::Text => TEXT_RECEIPTS,
                }
            ));
        // 覆盖默认的 rmcp/0 为本 crate 的 name/version
        info.server_info.name = env!("CARGO_PKG_NAME").to_string();
        info.server_info.version = env!("CARGO_PKG_VERSION").to_string();
        info
    }
}

/// 顶层指令文本：告诉 LLM 这个 server 是什么、有哪些约定。
/// 越精炼越好——LLM 上下文宝贵，不写废话。
const INSTRUCTIONS_START: &str = "Reuse known context; read only missing context. Prefer edit for exact blocks and file lifecycle; apply batches. Lines are 1-based inclusive. Omitted input flags are false. ";
const STRUCTURED_RECEIPTS: &str =
    "Results distinguish write/preview/check; written=false means no write. ";
const TEXT_RECEIPTS: &str = "Edit/apply/recover return text receipts: written confirms changes; unchanged means no change. preview/check include written=false; retained recovery journals stay in text. ";
const INSTRUCTIONS_END: &str = "Truncation is explicit; max_bytes=0 requests full output. Expected text is a precondition, not a writer lock. Specialized tools: restart with --toolset full.";

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
    serve_stdio_with_toolset(Toolset::Compact)
}

/// Serve the selected toolset over stdio.
pub fn serve_stdio_with_toolset(toolset: Toolset) -> Result<()> {
    serve_with_options(toolset, Receipts::Structured)
}

/// Serve the selected tool catalog and receipt presentation over stdio.
pub fn serve_with_options(toolset: Toolset, receipts: Receipts) -> Result<()> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("failed to build tokio runtime for MCP server")?;

    rt.block_on(async {
        let transport = (tokio::io::stdin(), tokio::io::stdout());
        let service = XnipServer::with_options(toolset, receipts)
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

/// Remove presentation-only schema annotations; retain validation and field guidance.
fn compact_schema(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            map.remove("title");
            map.remove("$schema");
            if matches!(
                map.get("default"),
                Some(serde_json::Value::Null | serde_json::Value::Bool(false))
            ) {
                map.remove("default");
            }
            for child in map.values_mut() {
                compact_schema(child);
            }
            // Optional fields are omitted, rather than advertised with null branches.
            if let Some(serde_json::Value::Array(types)) = map.get_mut("type") {
                types.retain(|kind| kind != "null");
                if types.len() == 1 {
                    let kind = types[0].clone();
                    map.insert("type".into(), kind);
                }
            }
            if let Some(serde_json::Value::Array(choices)) = map.get_mut("anyOf") {
                choices.retain(|choice| {
                    choice.get("type").and_then(serde_json::Value::as_str) != Some("null")
                });
                if choices.len() == 1 {
                    let only = choices[0].as_object().cloned();
                    if let Some(only) = only {
                        map.remove("anyOf");
                        for (key, value) in only {
                            map.entry(key).or_insert(value);
                        }
                    }
                }
            }
        }
        serde_json::Value::Array(values) => values.iter_mut().for_each(compact_schema),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::{Receipts, Toolset, XnipServer};
    use rmcp::{
        ErrorData,
        model::{CallToolResult, ContentBlock},
    };

    #[test]
    fn schema_compaction_retains_validation_and_nonfalse_defaults() {
        let mut schema = serde_json::json!({
            "type":"object", "required":["operation"], "additionalProperties":false,
            "properties": {
                "operation":{"type":"string","enum":["replace","write"],"default":"replace"},
                "flag":{"type":"boolean","default":false},
                "enabled":{"type":"boolean","default":true},
                "count":{"type":"integer","minimum":1,"default":7},
                "nullable":{"type":["string","null"],"default":null}
            }
        });
        let mut expected = schema.clone();
        for field in ["flag", "nullable"] {
            expected["properties"][field]
                .as_object_mut()
                .unwrap()
                .remove("default");
        }
        expected["properties"]["nullable"]["type"] = serde_json::json!("string");
        super::compact_schema(&mut schema);
        assert_eq!(schema, expected);
    }

    #[test]
    fn text_receipt_presentation_does_not_strip_error_details() {
        let server = XnipServer::with_options(Toolset::Compact, Receipts::Text);
        let mut failure = CallToolResult::success(vec![ContentBlock::text("failed")]);
        failure.is_error = Some(true);
        failure.structured_content = Some(serde_json::json!({"reason":"retained"}));
        let expected = serde_json::to_value(&failure).unwrap();
        let result = server.finish_receipt(Ok(failure)).unwrap();
        assert_eq!(serde_json::to_value(result).unwrap(), expected);
        let failure = ErrorData::invalid_request("unchanged error", None);
        let expected = serde_json::to_value(&failure).unwrap();
        let error = server.finish_receipt(Err(failure)).unwrap_err();
        assert_eq!(serde_json::to_value(error).unwrap(), expected);
    }
}
