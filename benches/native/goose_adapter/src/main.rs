//! Pinned Goose edit executor and Goose MCP client; no model/provider runtime.
#![allow(dead_code)]
use rmcp::model::{CallToolResult, ContentBlock};
use serde_json::{json, Value};
use std::{
    io::{self, BufRead, Write},
    path::PathBuf,
    sync::{atomic::AtomicU64, Arc},
    time::{Duration, Instant},
};

mod official_edit {
    include!(env!("GOOSE_NATIVE_EDIT_SOURCE"));
}
mod official_catalog {
    include!(concat!(env!("OUT_DIR"), "/goose_catalog.rs"));
}

// Only non-editing agent/session surroundings are adapted. The complete official
// editor and MCP client below are compiled unchanged, including filesystem IO.
mod action_required_manager {
    use serde_json::Value;
    pub enum ElicitationOutcome {
        Accept(Value),
        Decline,
        Cancel,
    }
    pub struct ActionRequiredManager;
    impl ActionRequiredManager {
        pub fn new() -> Self {
            Self
        }
        pub async fn request_and_wait(
            &self,
            _: String,
            _: String,
            _: String,
            _: Value,
            _: std::time::Duration,
        ) -> Result<ElicitationOutcome, String> {
            Err("Human elicitation is outside this non-model fixture adapter".into())
        }
    }
}
mod session_context {
    pub const SESSION_ID_HEADER: &str = "agent-session-id";
    pub const TOOL_CALL_REQUEST_ID_HEADER: &str = "agent-tool-call-request-id";
    pub const WORKING_DIR_HEADER: &str = "agent-working-dir";
}
mod agents {
    pub mod tool_execution {
        pub struct ToolCallContext {
            pub session_id: String,
            pub working_dir: Option<std::path::PathBuf>,
            pub tool_call_request_id: Option<String>,
        }
        impl ToolCallContext {
            pub fn working_dir_str(&self) -> Option<&str> {
                self.working_dir.as_ref().and_then(|p| p.to_str())
            }
        }
    }
    pub mod mcp_client {
        include!(env!("GOOSE_NATIVE_MCP_SOURCE"));
        impl McpClient {
            pub async fn fixture_close(self) -> Result<(), String> {
                let running = Arc::try_unwrap(self.client.into_inner())
                    .map_err(|_| "native MCP client still shared")?;
                running
                    .cancel()
                    .await
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            }
        }
    }
}

fn tools() -> Value {
    serde_json::to_value(official_catalog::OfficialCatalog::edit_tools()).unwrap()
}

fn text_content(result: &CallToolResult) -> Vec<String> {
    result
        .content
        .iter()
        .filter_map(|part| match part {
            ContentBlock::Text(text) => Some(text.text.clone()),
            _ => None,
        })
        .collect()
}

fn root(request: &Value) -> Result<PathBuf, String> {
    let root = request["root"].as_str().ok_or("root is required")?;
    let path = std::fs::canonicalize(root).map_err(|e| format!("invalid fixture root: {e}"))?;
    if !path.is_dir() {
        return Err("root must be an existing fixture directory".into());
    }
    Ok(path)
}

fn describe() -> Value {
    let pins: Value = serde_json::from_str(include_str!("../../goose_sources.json")).unwrap();
    json!({"ok":true,"status":"available","backend":"goose-edit","version":pins["version"],
        "commit":pins["commit"],"source_revisions":pins["files"],"repository":pins["repository"],
        "execution_level":"official Goose EditTools and GooseClient/McpClient, without model/provider orchestration",
        "tools":tools(),"model_calls":0,"global_configuration_changes":false,
        "native_tool":"EditTools.file_edit_with_cwd/file_write_with_cwd",
        "dependencies":{"rmcp":"3.4.1","schemars":"1.2.2","serde":"1.0.229","serde_json":"1.0.151","tokio":"1.53.1"},
        "context_adapters":["Direct editor dispatch around unchanged EditTools; edit/write Tool declarations and schema helper are compiled verbatim from the verified official registration",
            "Goose ToolCallContext retains only session_id/working_dir/tool_call_request_id; containers and extension leases are excluded",
            "Human elicitation manager rejects requests; no approval UI, model provider, telemetry or credential store is constructed",
            "Session header constants preserve Goose names; otel feature is disabled",
            "GooseClient/McpClient are included in full without rewriting their handshake, requests, timeouts or filesystem operations"],
        "dependency_lock_sha256":std::env::var("GOOSE_NATIVE_LOCK_SHA256").ok(),
        "runtime":{"binary_sha256":std::env::var("GOOSE_NATIVE_BINARY_SHA256").ok(),"compiler":std::env::var("GOOSE_NATIVE_COMPILER").ok()},
        "model_visible_result":null})
}

fn execute(request: &Value) -> Result<Value, String> {
    let root = root(request)?;
    let args = request
        .get("arguments")
        .or_else(|| request.get("args"))
        .ok_or("arguments is required")?
        .clone();
    let name = request["tool"].as_str().ok_or("tool is required")?;
    let tools = official_edit::EditTools::new();
    let started = Instant::now();
    let result = match name {
        "edit" => tools.file_edit_with_cwd(
            serde_json::from_value(args.clone()).map_err(|e| e.to_string())?,
            Some(&root),
        ),
        "write" => tools.file_write_with_cwd(
            serde_json::from_value(args.clone()).map_err(|e| e.to_string())?,
            Some(&root),
        ),
        "read" => tools.file_read_with_cwd(
            serde_json::from_value(args.clone()).map_err(|e| e.to_string())?,
            Some(&root),
        ),
        _ => return Err(format!("unknown native tool: {name}")),
    };
    let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
    let output = text_content(&result).join("\n");
    Ok(
        json!({"ok":!result.is_error.unwrap_or(false),"native_input":{"name":name,"arguments":args},
        "result":result,"native_result":result,"content":result.content,"output":output,"elapsed_ms":elapsed_ms,
        "model_visible_result":null,"model_visible_scope":"raw native MCP content is retained; no provider message projection is claimed"}),
    )
}

async fn bridge(request: &Value) -> Result<Value, String> {
    use agents::mcp_client::{
        ConnectContext, GooseMcpClientCapabilities, McpClient, McpClientTrait,
    };
    use agents::tool_execution::ToolCallContext;
    use rmcp::transport::TokioChildProcess;
    use tokio_util::sync::CancellationToken;
    let root = root(request)?;
    let executable = request["command"]
        .as_str()
        .ok_or("command must name the xnip executable")?;
    let args: Vec<String> = request
        .get("arguments")
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .map_err(|e| e.to_string())?
        .unwrap_or_else(|| vec!["mcp".into(), "--receipts".into(), "text".into()]);
    let mut command = tokio::process::Command::new(executable);
    command
        .args(&args)
        .current_dir(&root)
        .env("XNIP_AUTO_UPDATE", "0");
    let (transport, stderr) = TokioChildProcess::builder(command)
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    let stderr_reader = tokio::spawn(async move {
        use tokio::io::AsyncReadExt;
        let mut output = String::new();
        if let Some(mut stderr) = stderr {
            let _ = stderr.read_to_string(&mut output).await;
        }
        output
    });
    let mut capabilities = GooseMcpClientCapabilities::default();
    if let Some(version) = request.get("protocol_version") {
        capabilities.protocol_version =
            Some(serde_json::from_value(version.clone()).map_err(|e| e.to_string())?);
    }
    let context = ConnectContext {
        timeout: Duration::from_secs(15),
        client_name: "xnip-goose-fixture".into(),
        capabilities,
        working_dir: root.clone(),
        docker_container: None,
        action_required: Arc::new(action_required_manager::ActionRequiredManager::new()),
        tools_version: Arc::new(AtomicU64::new(0)),
    };
    let client = McpClient::connect(transport, context)
        .await
        .map_err(|e| e.to_string())?;
    let info = client.get_info().cloned();
    let listed = client
        .list_tools("xnip-goose-fixture", None, CancellationToken::new())
        .await
        .map_err(|e| e.to_string())?;
    let mut results = Vec::new();
    for (index, call) in request["calls"]
        .as_array()
        .ok_or("calls must be an array")?
        .iter()
        .enumerate()
    {
        let name = call["name"].as_str().ok_or("call name is required")?;
        let arguments = call["arguments"]
            .as_object()
            .ok_or("call arguments must be an object")?
            .clone();
        let ctx = ToolCallContext {
            session_id: "xnip-goose-fixture".into(),
            working_dir: Some(root.clone()),
            tool_call_request_id: Some(format!("fixture-{index}")),
        };
        let started = Instant::now();
        let result = client
            .call_tool(&ctx, name, Some(arguments), CancellationToken::new())
            .await
            .map_err(|e| e.to_string())?;
        results.push(json!({"native_input":call,"ok":!result.is_error.unwrap_or(false),"result":result,"elapsed_ms":started.elapsed().as_secs_f64()*1000.0}));
    }
    client.fixture_close().await?;
    let stderr = tokio::time::timeout(Duration::from_secs(5), stderr_reader)
        .await
        .map_err(|_| "child stderr did not close")?
        .map_err(|e| e.to_string())?;
    Ok(
        json!({"ok":true,"execution_level":"unchanged official GooseClient/McpClient over stdio to xnip; no model", "server_info":info,
        "tools":listed,"calls":results,"stderr":stderr,"spawn":{"command":executable,"arguments":args,"cwd":root},
        "protocol_profile":request.get("protocol_version").cloned().unwrap_or(json!("official-default-auto")),"model_calls":0}),
    )
}

async fn handle(request: &Value) -> Result<Value, String> {
    match request["action"].as_str().unwrap_or("execute") {
        "describe" => Ok(describe()),
        "execute" => execute(request),
        "bridge" => bridge(request).await,
        name => Err(format!("unknown action: {name}")),
    }
}

fn main() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let stdin = io::stdin();
    let mut stdout = io::BufWriter::new(io::stdout().lock());
    for line in stdin.lock().lines() {
        let response = match line {
            Ok(line) if line.trim().is_empty() => continue,
            Ok(line) => match serde_json::from_str::<Value>(&line) {
                Ok(request) => runtime
                    .block_on(handle(&request))
                    .unwrap_or_else(|error| json!({"ok":false,"adapter_error":error})),
                Err(error) => json!({"ok":false,"adapter_error":error.to_string()}),
            },
            Err(_) => break,
        };
        if writeln!(stdout, "{}", response)
            .and_then(|_| stdout.flush())
            .is_err()
        {
            break;
        }
    }
}
