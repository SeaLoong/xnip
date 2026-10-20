//! Public stdio compatibility with modern discovery and legacy initialization.
//! Requests and metadata are serialized through the official SDK model types.
mod common;

use assert_cmd::cargo::CommandCargoExt;
use common::Workspace;
use rmcp::model::{
    CallToolRequest, CallToolRequestParams, CallToolResult, ClientCapabilities,
    ClientJsonRpcMessage, ClientNotification, ClientRequest, CustomRequest, DiscoverRequest,
    DiscoverRequestParams, DiscoverResult, ErrorCode, ErrorData, Implementation, InitializeRequest,
    InitializeRequestParams, InitializeResult, InitializedNotification, ListToolsRequest,
    ListToolsResult, PaginatedRequestParams, ProtocolVersion, RequestId, RequestMetaObject,
};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

struct Wire {
    child: Child,
    stdin: Option<ChildStdin>,
    replies: Receiver<Result<Value, String>>,
    stdout_reader: Option<std::thread::JoinHandle<()>>,
    stderr_reader: Option<std::thread::JoinHandle<Vec<u8>>>,
    next_id: i64,
    meta: Option<RequestMetaObject>,
}

impl Wire {
    fn start(workspace: &Workspace) -> Self {
        let mut command = std::process::Command::cargo_bin("xnip").unwrap();
        let mut child = command
            .env("XNIP_AUTO_UPDATE", "0")
            .current_dir(workspace.root())
            .args(["mcp", "--receipts", "text"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let mut stderr = child.stderr.take().unwrap();
        let (sender, replies) = channel();
        let stdout_reader = std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let decoded = line.map_err(|error| error.to_string()).and_then(|line| {
                    serde_json::from_str(&line).map_err(|error| error.to_string())
                });
                if sender.send(decoded).is_err() {
                    break;
                }
            }
        });
        let stderr_reader = std::thread::spawn(move || {
            let mut bytes = Vec::new();
            stderr.read_to_end(&mut bytes).unwrap();
            bytes
        });
        Self {
            child,
            stdin: Some(stdin),
            replies,
            stdout_reader: Some(stdout_reader),
            stderr_reader: Some(stderr_reader),
            next_id: 1,
            meta: None,
        }
    }

    fn send(&mut self, message: &ClientJsonRpcMessage) {
        let stdin = self.stdin.as_mut().unwrap();
        serde_json::to_writer(&mut *stdin, message).unwrap();
        writeln!(stdin).unwrap();
        stdin.flush().unwrap();
    }

    fn request(&mut self, request: ClientRequest) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        self.send(&ClientJsonRpcMessage::request(
            request,
            RequestId::Number(id),
        ));
        loop {
            let reply = self
                .replies
                .recv_timeout(Duration::from_secs(10))
                .expect("live stdio server must reply before stdin closes")
                .expect("MCP stdout must contain only JSON-RPC");
            if reply["id"] == id {
                return reply;
            }
        }
    }

    fn discover(&mut self) {
        // This is the same per-request metadata shape used by Goose's real
        // rmcp client during discovery, without a preceding initialize call.
        let meta = RequestMetaObject::with_client_context(
            ProtocolVersion::V_2026_07_28,
            Implementation::new("discovery-contract", "1"),
            ClientCapabilities::default(),
        );
        let mut request = DiscoverRequest::new(DiscoverRequestParams {});
        request.extensions.insert(meta.clone());
        let reply = self.request(ClientRequest::DiscoverRequest(request));
        assert!(reply.get("error").is_none(), "{reply}");
        let result: DiscoverResult = serde_json::from_value(reply["result"].clone()).unwrap();
        assert!(
            result
                .supported_versions
                .contains(&ProtocolVersion::V_2026_07_28)
        );
        assert!(result.capabilities.tools.is_some());
        assert_eq!(result.server_info().unwrap().name, "xnip");
        self.meta = Some(meta);
    }

    fn initialize_legacy(&mut self) {
        let params = InitializeRequestParams::new(
            ClientCapabilities::default(),
            Implementation::new("legacy-contract", "1"),
        )
        .with_protocol_version(ProtocolVersion::V_2025_11_25);
        let reply = self.request(ClientRequest::InitializeRequest(InitializeRequest::new(
            params,
        )));
        assert!(reply.get("error").is_none(), "{reply}");
        let result: InitializeResult = serde_json::from_value(reply["result"].clone()).unwrap();
        assert_eq!(result.protocol_version, ProtocolVersion::V_2025_11_25);
        self.send(&ClientJsonRpcMessage::notification(
            ClientNotification::InitializedNotification(InitializedNotification::default()),
        ));
    }

    fn list(&mut self) -> ListToolsResult {
        let mut request = ListToolsRequest::with_param(PaginatedRequestParams::default());
        if let Some(meta) = &self.meta {
            request.extensions.insert(meta.clone());
        }
        let reply = self.request(ClientRequest::ListToolsRequest(request));
        assert!(reply.get("error").is_none(), "{reply}");
        serde_json::from_value(reply["result"].clone()).unwrap()
    }

    fn edit(&mut self, old: &str, text: &str) -> Result<CallToolResult, ErrorData> {
        let arguments = json!({"file":"目录 with spaces/a.txt","old":old,"text":text})
            .as_object()
            .unwrap()
            .clone();
        let mut request =
            CallToolRequest::new(CallToolRequestParams::new("xnip_edit").with_arguments(arguments));
        if let Some(meta) = &self.meta {
            request.extensions.insert(meta.clone());
        }
        let reply = self.request(ClientRequest::CallToolRequest(request));
        if let Some(error) = reply.get("error") {
            Err(serde_json::from_value(error.clone()).unwrap())
        } else {
            Ok(serde_json::from_value(reply["result"].clone()).unwrap())
        }
    }

    fn unknown_request(&mut self) -> ErrorData {
        let mut request = CustomRequest::new("xnip/unknown-future-method", Some(json!({})));
        if let Some(meta) = &self.meta {
            request.extensions.insert(meta.clone());
        }
        let reply = self.request(ClientRequest::CustomRequest(request));
        serde_json::from_value(reply["error"].clone()).unwrap()
    }

    fn close(mut self) {
        drop(self.stdin.take());
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                assert!(
                    status.success(),
                    "stdio EOF must shut down cleanly: {status}"
                );
                break;
            }
            assert!(
                Instant::now() < deadline,
                "stdio server did not stop after EOF"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        self.stdout_reader.take().unwrap().join().unwrap();
        let stderr = self.stderr_reader.take().unwrap().join().unwrap();
        assert!(stderr.is_empty(), "{}", String::from_utf8_lossy(&stderr));
    }
}

impl Drop for Wire {
    fn drop(&mut self) {
        drop(self.stdin.take());
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.stdout_reader.take() {
            let _ = reader.join();
        }
        if let Some(reader) = self.stderr_reader.take() {
            let _ = reader.join();
        }
    }
}

fn exercise_edit(wire: &mut Wire, workspace: &Workspace) {
    let listed = wire.list();
    assert_eq!(listed.tools.len(), 5);
    assert!(listed.tools.iter().any(|tool| tool.name == "xnip_edit"));
    let rejected = wire.edit("stale text", "new").unwrap_err();
    assert_eq!(rejected.code, ErrorCode::INVALID_REQUEST);
    assert!(rejected.message.contains("not found"), "{rejected}");
    workspace.assert_bytes("目录 with spaces/a.txt", b"head\r\nold");
    let written = wire.edit("old", "new").unwrap();
    assert!(!written.is_error.unwrap_or(false));
    assert!(
        written.structured_content.is_none(),
        "text receipt mode stays minimal"
    );
    workspace.assert_bytes("目录 with spaces/a.txt", b"head\r\nnew");
}

#[test]
fn modern_discovery_lists_edits_and_shuts_down_without_initialize() {
    let workspace = Workspace::new(&[("目录 with spaces/a.txt", b"head\r\nold")]);
    let mut wire = Wire::start(&workspace);
    wire.discover();
    exercise_edit(&mut wire, &workspace);
    wire.close();
}

#[test]
fn legacy_2025_initialize_lists_edits_and_shuts_down() {
    let workspace = Workspace::new(&[("目录 with spaces/a.txt", b"head\r\nold")]);
    let mut wire = Wire::start(&workspace);
    wire.initialize_legacy();
    exercise_edit(&mut wire, &workspace);
    wire.close();
}

#[test]
fn unknown_requests_return_correlated_errors_and_leave_both_lifecycles_usable() {
    for modern in [true, false] {
        let workspace = Workspace::new(&[("目录 with spaces/a.txt", b"head\r\nold")]);
        let mut wire = Wire::start(&workspace);
        if modern {
            wire.discover();
        } else {
            wire.initialize_legacy();
        }
        let error = wire.unknown_request();
        assert_eq!(error.code, ErrorCode::METHOD_NOT_FOUND);
        exercise_edit(&mut wire, &workspace);
        wire.close();
    }
}
