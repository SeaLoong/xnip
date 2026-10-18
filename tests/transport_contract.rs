//! MCP is exercised as an agent sees it: a live stdio JSON-RPC connection.
// Request values are consumed at call sites to keep the JSON-RPC fixture ergonomic.
#![allow(clippy::needless_pass_by_value)]
mod common;
use assert_cmd::cargo::CommandCargoExt;
use common::Workspace;
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

struct Rpc {
    child: Child,
    input: Option<ChildStdin>,
    output: Receiver<Value>,
    reader: Option<std::thread::JoinHandle<()>>,
    next_id: u64,
    initialization: Value,
}

impl Rpc {
    fn start(full: bool) -> Self {
        Self::with_receipts(full, None)
    }

    fn start_text(full: bool) -> Self {
        Self::with_receipts(full, Some("text"))
    }

    fn with_receipts(full: bool, receipts: Option<&str>) -> Self {
        let mut command = std::process::Command::cargo_bin("xnip").unwrap();
        command.env("XNIP_AUTO_UPDATE", "0").arg("mcp");
        if full {
            command.args(["--toolset", "full"]);
        }
        if let Some(receipts) = receipts {
            command.args(["--receipts", receipts]);
        }
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let input = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (sender, output) = channel();
        let reader = std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let line = line.unwrap();
                let frame = serde_json::from_str(&line).expect("MCP stdout must be JSON-RPC only");
                if sender.send(frame).is_err() {
                    break;
                }
            }
        });
        let mut rpc = Self {
            child,
            input: Some(input),
            output,
            reader: Some(reader),
            next_id: 1,
            initialization: Value::Null,
        };
        rpc.initialization = rpc.request(
            "initialize",
            json!({
                "protocolVersion":"2025-06-18","capabilities":{},
                "clientInfo":{"name":"contract-tests","version":"1"}
            }),
        )["result"]
            .clone();
        rpc.send(json!({"jsonrpc":"2.0","method":"notifications/initialized"}));
        rpc
    }

    fn send(&mut self, frame: Value) {
        let input = self.input.as_mut().unwrap();
        writeln!(input, "{frame}").unwrap();
        input.flush().unwrap();
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        self.send(json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}));
        loop {
            let frame = self
                .output
                .recv_timeout(Duration::from_secs(15))
                .expect("server must respond before stdin EOF");
            if frame["id"] == id {
                return frame;
            }
        }
    }

    fn list(&mut self) -> Vec<Value> {
        self.request("tools/list", json!({}))["result"]["tools"]
            .as_array()
            .unwrap()
            .clone()
    }

    fn call(&mut self, name: &str, arguments: Value) -> Value {
        self.request("tools/call", json!({"name":name,"arguments":arguments}))
    }

    fn ok(&mut self, name: &str, arguments: Value) -> Value {
        let response = self.call(name, arguments);
        assert!(response.get("error").is_none(), "{response}");
        let result = &response["result"];
        assert!(
            result.get("isError").is_none(),
            "successful tools omit the optional false flag: {response}"
        );
        let decoded: rmcp::model::CallToolResult = serde_json::from_value(result.clone()).unwrap();
        assert_eq!(decoded.is_error, None);
        result.clone()
    }
}

impl Drop for Rpc {
    fn drop(&mut self) {
        drop(self.input.take());
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            reader.join().unwrap();
        }
    }
}

fn text(result: &Value) -> &str {
    result["content"][0]["text"].as_str().unwrap()
}

fn assert_no_false_defaults(schema: &Value) {
    match schema {
        Value::Object(fields) => {
            assert_ne!(fields.get("default"), Some(&Value::Bool(false)), "{schema}");
            for value in fields.values() {
                assert_no_false_defaults(value);
            }
        }
        Value::Array(values) => values.iter().for_each(assert_no_false_defaults),
        _ => {}
    }
}

fn schema_node<'a>(root: &'a Value, node: &'a Value) -> &'a Value {
    if let Some(reference) = node.get("$ref").and_then(Value::as_str) {
        return root.pointer(reference.strip_prefix('#').unwrap()).unwrap();
    }
    node
}

#[test]
fn text_receipts_remove_only_change_output_schemas_and_default_stays_structured() {
    for full in [false, true] {
        let mut default = Rpc::start(full);
        let mut structured = Rpc::with_receipts(full, Some("structured"));
        let mut minimal = Rpc::start_text(full);
        let index = |tools: Vec<Value>| {
            tools
                .into_iter()
                .map(|tool| (tool["name"].as_str().unwrap().to_owned(), tool))
                .collect::<std::collections::BTreeMap<_, _>>()
        };
        let defaults = index(default.list());
        for server in [&default, &structured, &minimal] {
            assert!(
                server.initialization["instructions"]
                    .as_str()
                    .unwrap()
                    .contains("Omitted input flags are false.")
            );
        }
        assert_eq!(defaults, index(structured.list()));
        assert_eq!(
            default.initialization["instructions"],
            structured.initialization["instructions"]
        );
        assert!(
            minimal.initialization["instructions"]
                .as_str()
                .unwrap()
                .contains("return text receipts")
        );
        let tools = index(minimal.list());
        assert_eq!(tools.len(), if full { 10 } else { 5 });
        assert_eq!(
            tools.keys().collect::<Vec<_>>(),
            defaults.keys().collect::<Vec<_>>()
        );
        for (name, tool) in tools {
            assert_no_false_defaults(&tool["inputSchema"]);
            assert_no_false_defaults(&tool["outputSchema"]);
            assert_eq!(tool["inputSchema"], defaults[&name]["inputSchema"]);
            assert_eq!(tool["annotations"], defaults[&name]["annotations"]);
            if matches!(name.as_str(), "xnip_edit" | "xnip_apply" | "xnip_recover") {
                assert!(tool.get("outputSchema").is_none());
                assert!(defaults[&name].get("outputSchema").is_some());
            } else {
                assert_eq!(
                    tool.get("outputSchema"),
                    defaults[&name].get("outputSchema")
                );
            }
        }
    }
}

#[test]
fn receipt_presentations_preserve_edit_preview_check_commit_and_noop() {
    for text_mode in [false, true] {
        edit_receipts(text_mode);
    }
}

fn edit_receipts(text_mode: bool) {
    let w = Workspace::new(&[("a", b"old\r\nEOF")]);
    let mut rpc = if text_mode {
        Rpc::start_text(false)
    } else {
        Rpc::start(false)
    };
    for (mode, flags) in [
        ("preview", json!({"dry_run":true})),
        ("check", json!({"check":true})),
        ("write", json!({})),
    ] {
        let mut args = json!({"file":w.path("a"),"old":"old","text":"new"});
        args.as_object_mut()
            .unwrap()
            .extend(flags.as_object().unwrap().clone());
        let result = rpc.ok("xnip_edit", args);
        assert_eq!(
            result.get("structuredContent").is_none(),
            text_mode,
            "{result}"
        );
        if !text_mode {
            assert_eq!(result["structuredContent"]["operation"], "replace");
            assert_eq!(result["structuredContent"]["mode"], mode);
            assert_eq!(result["structuredContent"]["written"], mode == "write");
        }
        let receipt = text(&result);
        if mode == "write" {
            assert_eq!(receipt, "written");
        } else {
            assert!(receipt.starts_with(&format!("{mode}; written=false")));
            assert_eq!(receipt.contains("-old"), mode == "preview");
        }
        w.assert_bytes(
            "a",
            if mode == "write" {
                b"new\r\nEOF"
            } else {
                b"old\r\nEOF"
            },
        );
    }
    let result = rpc.ok(
        "xnip_edit",
        json!({"file":w.path("a"),"old":"new","text":"new","backup":true}),
    );
    assert_eq!(text(&result), "unchanged");
    assert_eq!(result.get("structuredContent").is_none(), text_mode);
    w.absent("a.bak");
    for mode in ["dry_run", "check"] {
        let mut args = json!({"file":w.path("nested/new"),"operation":"write","text":""});
        args[mode] = json!(true);
        let result = rpc.ok("xnip_edit", args);
        assert_eq!(result.get("structuredContent").is_none(), text_mode);
        if !text_mode {
            assert_eq!(result["structuredContent"]["written"], false);
        }
        assert!(text(&result).contains("written=false"));
        w.absent("nested");
    }
}

#[test]
fn receipt_presentations_keep_apply_success_small_and_nonwriting_modes_explicit() {
    for text_mode in [false, true] {
        apply_receipts(text_mode);
    }
}

fn apply_receipts(text_mode: bool) {
    let w = Workspace::new(&[("a", b"old-a"), ("b", b"old-b")]);
    let mut rpc = if text_mode {
        Rpc::start_text(false)
    } else {
        Rpc::start(false)
    };
    let files = json!({
        w.path("a").to_string_lossy().as_ref(): [["old-a", "new-a"]],
        w.path("b").to_string_lossy().as_ref(): {"text":"new-b","old":"old-b"},
        w.path("nested/empty").to_string_lossy().as_ref(): ""
    });
    for mode in ["dry_run", "check"] {
        let mut args = json!({"files":files});
        args[mode] = json!(true);
        let result = rpc.ok("xnip_apply", args);
        assert_eq!(result.get("structuredContent").is_none(), text_mode);
        if !text_mode {
            assert_eq!(result["structuredContent"]["written"], false);
            assert_eq!(
                result["structuredContent"]["files"]
                    .as_array()
                    .unwrap()
                    .len(),
                3
            );
        }
        assert!(text(&result).starts_with(if mode == "dry_run" {
            "preview; written=false"
        } else {
            "check; written=false"
        }));
        w.assert_bytes("a", b"old-a");
        w.assert_bytes("b", b"old-b");
        w.absent("nested");
    }
    let result = rpc.ok("xnip_apply", json!({"files":files}));
    assert_eq!(text(&result), "written");
    assert_eq!(result["content"].as_array().unwrap().len(), 1);
    assert_eq!(result.get("structuredContent").is_none(), text_mode);
    if !text_mode {
        assert_eq!(result["structuredContent"]["written"], true);
    }
    w.absent("a.bak");
    w.absent("b.bak");
    w.assert_bytes("a", b"new-a");
    w.assert_bytes("b", b"new-b");
    w.assert_bytes("nested/empty", b"");
    let unchanged = rpc.ok(
        "xnip_apply",
        json!({"files":{
            w.path("a").to_string_lossy().as_ref(): [["new-a","new-a"]]
        }}),
    );
    assert_eq!(text(&unchanged), "unchanged");
    assert_eq!(unchanged.get("structuredContent").is_none(), text_mode);
}

#[test]
fn text_receipts_preserve_read_results_and_tool_errors() {
    let w = Workspace::new(&[("a", b"x\nx\nold\n")]);
    let mut structured = Rpc::start(false);
    let mut minimal = Rpc::start_text(false);
    for (tool, args) in [
        (
            "xnip_peek",
            json!({"file":w.path("a"),"all":true,"max_bytes":20}),
        ),
        (
            "xnip_find",
            json!({"files":[w.path("a")],"pattern":"x","max_matches":1}),
        ),
    ] {
        assert_eq!(structured.ok(tool, args.clone()), minimal.ok(tool, args));
    }
    for (tool, args) in [
        (
            "xnip_edit",
            json!({"file":w.path("a"),"old":"absent","text":"new"}),
        ),
        (
            "xnip_apply",
            json!({"files":{w.path("a").to_string_lossy().as_ref(): [["absent","new"]]}}),
        ),
        ("xnip_recover", json!({"journal":w.path("absent-journal")})),
    ] {
        let expected = structured.call(tool, args.clone());
        let actual = minimal.call(tool, args);
        assert!(expected.get("error").is_some() || expected["result"]["isError"] == true);
        assert_eq!(expected.get("error"), actual.get("error"));
        assert_eq!(expected.get("result"), actual.get("result"));
        w.assert_bytes("a", b"x\nx\nold\n");
    }
}

#[test]
fn read_and_manifest_errors_keep_actionable_causes_in_both_receipt_modes() {
    let w = Workspace::new(&[
        ("a", b"unchanged\n"),
        ("broken.native", b"replace a\n"),
        ("broken.input", b"\n  {\"a\": [[\"old\"]]}\n"),
    ]);
    for text_mode in [false, true] {
        let mut rpc = if text_mode {
            Rpc::start_text(false)
        } else {
            Rpc::start(false)
        };
        for (tool, args, details) in [
            (
                "xnip_peek",
                json!({"file":w.path("a"),"match_line":"["}),
                ["invalid match_line regex", "unclosed character class"],
            ),
            (
                "xnip_find",
                json!({"files":[w.path("a")],"pattern":"("}),
                ["invalid pattern regex", "unclosed group"],
            ),
            (
                "xnip_apply",
                json!({"path":w.path("broken.native")}),
                ["line 1", "missing locator"],
            ),
            (
                "xnip_apply",
                json!({"manifest_text":"replace a\n"}),
                ["line 1", "missing locator"],
            ),
            (
                "xnip_apply",
                json!({"path":w.path("broken.input")}),
                ["attempt with Json", "invalid compact apply manifest"],
            ),
            (
                "xnip_apply",
                json!({"manifest_text":"\n  {\"a\": [[\"old\"]]}\n"}),
                ["attempt with Json", "invalid compact apply manifest"],
            ),
        ] {
            let response = rpc.call(tool, args);
            assert_eq!(response["error"]["code"], -32602, "{response}");
            let message = response["error"]["message"].as_str().unwrap();
            for detail in details {
                assert!(message.contains(detail), "{tool}: {message}");
            }
            w.assert_bytes("a", b"unchanged\n");
        }
    }
}

#[test]
fn omitted_apply_flags_preserve_independent_line_validation_and_unbacked_commit() {
    for text_mode in [false, true] {
        let w = Workspace::new(&[("a", b"old\n")]);
        let mut rpc = if text_mode {
            Rpc::start_text(false)
        } else {
            Rpc::start(false)
        };
        let manifest = json!([
            {"op":"replace","file":w.path("a"),"lines":1,"text":"middle"},
            {"op":"replace","file":w.path("a"),"lines":1,"text":"final"}
        ])
        .to_string();
        let response = rpc.call("xnip_apply", json!({"manifest_text":manifest}));
        assert!(response.get("error").is_some() || response["result"]["isError"] == true);
        w.assert_bytes("a", b"old\n");
        let result = rpc.ok(
            "xnip_apply",
            json!({"manifest_text":manifest,"sequential":true}),
        );
        assert_eq!(text(&result), "written");
        assert_eq!(result.get("structuredContent").is_none(), text_mode);
        w.assert_bytes("a", b"final\n");
        w.absent("a.bak");
    }
}

#[test]
fn handshake_and_compact_toolset_respond_on_a_live_connection() {
    let mut rpc = Rpc::start(false);
    assert_eq!(rpc.initialization["serverInfo"]["name"], "xnip");
    assert_eq!(rpc.initialization["serverInfo"]["version"], xnip::VERSION);
    assert_eq!(rpc.initialization["protocolVersion"], "2025-06-18");
    let tools = rpc.list();
    let mut names: Vec<_> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    names.sort_unstable();
    assert_eq!(
        names,
        [
            "xnip_apply",
            "xnip_edit",
            "xnip_find",
            "xnip_peek",
            "xnip_recover"
        ]
    );
}

#[test]
fn full_toolset_metadata_distinguishes_reads_writes_and_result_shapes() {
    let mut rpc = Rpc::start(true);
    let tools = rpc.list();
    assert_eq!(tools.len(), 10);
    for tool in tools {
        let name = tool["name"].as_str().unwrap();
        let read_only = matches!(name, "xnip_peek" | "xnip_find" | "xnip_doctor");
        assert_eq!(tool["annotations"]["readOnlyHint"], read_only, "{name}");
        assert_eq!(tool["annotations"]["destructiveHint"], !read_only, "{name}");
        assert_eq!(tool["annotations"]["openWorldHint"], false, "{name}");
        let schema = &tool["inputSchema"];
        assert_eq!(schema["type"], "object", "{name}");
        if name == "xnip_edit" {
            assert_eq!(schema["required"], json!(["file"]));
            assert_eq!(schema["additionalProperties"], false);
            let operation = schema_node(schema, &schema["properties"]["operation"]);
            assert_eq!(
                operation["enum"],
                json!(["replace", "write", "create", "delete", "rename"])
            );
            for flag in ["all", "backup", "check", "dry_run"] {
                assert_eq!(schema["properties"][flag]["type"], "boolean");
            }
        }
        if name == "xnip_apply" {
            assert_eq!(schema["additionalProperties"], false);
            let files = schema_node(schema, &schema["properties"]["files"]);
            assert_eq!(files["type"], "object");
            let group = schema_node(schema, &files["additionalProperties"]);
            let choices = group["anyOf"].as_array().unwrap();
            let pairs = choices
                .iter()
                .map(|node| schema_node(schema, node))
                .find(|node| node["type"] == "array")
                .unwrap();
            assert_eq!(pairs["items"]["minItems"], 2);
            assert_eq!(pairs["items"]["maxItems"], 2);
            let write = choices
                .iter()
                .map(|node| schema_node(schema, node))
                .find(|node| node["type"] == "object")
                .unwrap();
            assert_eq!(write["required"], json!(["text"]));
            assert_eq!(write["additionalProperties"], false);
        }
        if matches!(name, "xnip_edit" | "xnip_apply" | "xnip_recover") {
            assert_eq!(
                tool["outputSchema"]["properties"]["written"]["type"],
                "boolean"
            );
            let required = tool["outputSchema"]["required"].as_array().unwrap();
            for field in ["operation", "mode", "has_changes", "written", "files"] {
                assert!(required.contains(&json!(field)), "{name}: {required:?}");
            }
            assert_eq!(tool["outputSchema"]["properties"]["files"]["type"], "array");
        }
    }
    assert!(text(&rpc.ok("xnip_doctor", json!({}))).contains("stdin-tty:  false"));
    let rejected = rpc.call("xnip_doctor", json!({"dry_runn": true}));
    assert!(rejected.get("error").is_some() || rejected["result"]["isError"] == true);
}

#[test]
fn compact_toolset_cannot_call_unadvertised_legacy_tools() {
    let mut rpc = Rpc::start(false);
    let response = rpc.call("xnip_doctor", json!({}));
    assert!(
        response.get("error").is_some() || response["result"]["isError"] == true,
        "{response}"
    );
}

#[test]
fn edit_result_modes_are_truthful_for_preview_check_commit_and_noop() {
    let w = Workspace::new(&[("a", b"old")]);
    let mut rpc = Rpc::start(false);
    for (mode, flags) in [
        ("preview", json!({"dry_run":true})),
        ("check", json!({"check":true})),
        ("write", json!({})),
    ] {
        let mut args = json!({"file":w.path("a"),"old":"old","text":"new"});
        args.as_object_mut()
            .unwrap()
            .extend(flags.as_object().unwrap().clone());
        let result = rpc.ok("xnip_edit", args);
        let data = &result["structuredContent"];
        assert_eq!(data["mode"], mode);
        assert_eq!(data["written"], mode == "write");
        assert_eq!(data["has_changes"], true);
        assert_eq!(data["replacements"], 1);
        assert_eq!(data["files"], json!([w.path("a")]));
        if mode == "write" {
            assert_eq!(text(&result), "written");
        }
        assert_eq!(text(&result).contains("-old"), mode == "preview");
        assert!(!text(&result).contains('\x1b'));
        w.assert_bytes("a", if mode == "write" { b"new" } else { b"old" });
    }
    let unchanged = rpc.ok(
        "xnip_edit",
        json!({"file":w.path("a"),"old":"new","text":"new","backup":true}),
    );
    assert_eq!(unchanged["structuredContent"]["has_changes"], false);
    assert_eq!(unchanged["structuredContent"]["written"], false);
    w.absent("a.bak");
}

#[test]
fn edit_lifecycle_preview_and_rename_report_complete_paths() {
    let w = Workspace::new(&[]);
    let mut rpc = Rpc::start(false);
    for dry_run in [true, false] {
        rpc.ok("xnip_edit", json!({"file":w.path("nested/a"),"operation":"create","text":"hello","dry_run":dry_run}));
        if dry_run {
            w.absent("nested");
        }
    }
    let result = rpc.ok(
        "xnip_edit",
        json!({"file":w.path("nested/a"),"operation":"rename","to":w.path("moved")}),
    );
    assert_eq!(
        result["structuredContent"]["files"],
        json!([w.path("nested/a"), w.path("moved")])
    );
    assert_eq!(
        result["structuredContent"]["destination"],
        json!(w.path("moved"))
    );
    w.absent("nested/a");
    w.assert_bytes("moved", b"hello");
    let rejected = rpc.call(
        "xnip_edit",
        json!({"file":w.path("moved"),"operation":"delete","old":"wrong"}),
    );
    assert!(rejected.get("error").is_some() || rejected["result"]["isError"] == true);
    w.assert_bytes("moved", b"hello");
    rpc.ok(
        "xnip_edit",
        json!({"file":w.path("moved"),"operation":"delete","old":"hello"}),
    );
    w.absent("moved");
}

#[test]
fn compact_apply_edits_are_sequential_and_validate_every_edit_before_writing() {
    let w = Workspace::new(&[("a", b"old"), ("b", b"keep")]);
    let mut rpc = Rpc::start(false);
    let edits = json!([
        {"file":w.path("a"),"old":"old","text":"middle"},
        {"file":w.path("a"),"old":"middle","text":"final"},
        {"file":w.path("b"),"old":"keep","text":"changed"}
    ]);
    for flag in ["dry_run", "check"] {
        let mut args = json!({"edits":edits});
        args[flag] = json!(true);
        let result = rpc.ok("xnip_apply", args);
        assert_eq!(result["structuredContent"]["written"], false);
        w.assert_bytes("a", b"old");
        w.assert_bytes("b", b"keep");
    }
    let mut invalid = edits.clone();
    invalid
        .as_array_mut()
        .unwrap()
        .push(json!({"file":w.path("b"),"old":"missing","text":"bad"}));
    let response = rpc.call("xnip_apply", json!({"edits":invalid}));
    assert!(response.get("error").is_some() || response["result"]["isError"] == true);
    w.assert_bytes("a", b"old");
    w.assert_bytes("b", b"keep");
    let result = rpc.ok("xnip_apply", json!({"edits":edits}));
    assert_eq!(result["structuredContent"]["written"], true);
    w.assert_bytes("a", b"final");
    w.assert_bytes("b", b"changed");
}

#[test]
fn compact_apply_supports_explicit_all_but_rejects_empty_or_mixed_inputs() {
    let w = Workspace::new(&[("a", b"x x")]);
    let mut rpc = Rpc::start(false);
    for extra in [
        json!({"manifest_text":"[]"}),
        json!({"path":"manifest"}),
        json!({"format":"json"}),
        json!({"patch_root":w.root()}),
    ] {
        let mut args = json!({"edits":[{"file":w.path("a"),"old":"x","text":"y","all":true}]});
        args.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        let response = rpc.call("xnip_apply", args);
        assert!(
            response.get("error").is_some() || response["result"]["isError"] == true,
            "{response}"
        );
        w.assert_bytes("a", b"x x");
    }
    let response = rpc.call(
        "xnip_apply",
        json!({"edits":[{"file":w.path("a"),"old":"x","text":"y","al":true}]}),
    );
    assert!(response.get("error").is_some() || response["result"]["isError"] == true);
    w.assert_bytes("a", b"x x");
    let response = rpc.call("xnip_apply", json!({"edits":[]}));
    assert!(response.get("error").is_some() || response["result"]["isError"] == true);
    for (tool, args) in [
        (
            "xnip_edit",
            json!({"file":w.path("a"),"old":"x","text":"y"}),
        ),
        (
            "xnip_apply",
            json!({"edits":[{"file":w.path("a"),"old":"x","text":"y"}]}),
        ),
    ] {
        let response = rpc.call(tool, args);
        assert!(response.get("error").is_some() || response["result"]["isError"] == true);
        w.assert_bytes("a", b"x x");
    }
    rpc.ok(
        "xnip_apply",
        json!({"edits":[{"file":w.path("a"),"old":"x","text":"y","all":true}]}),
    );
    w.assert_bytes("a", b"y y");
}

#[test]
fn mcp_preview_budget_is_explicit_and_zero_preserves_the_complete_diff() {
    let content = "large diff content\n".repeat(1000);
    let w = Workspace::new(&[("a", b"old")]);
    let mut rpc = Rpc::start(false);
    for max_bytes in [None, Some(64), Some(0)] {
        let mut args = json!({"file":w.path("a"),"old":"old","text":content,"dry_run":true});
        if let Some(limit) = max_bytes {
            args["max_bytes"] = json!(limit);
        }
        let result = rpc.ok("xnip_edit", args);
        if max_bytes == Some(0) {
            assert_eq!(text(&result).matches("+large diff content").count(), 1000);
            assert!(!text(&result).contains("truncated"));
        } else {
            assert!(text(&result).contains("truncated"));
            assert!(text(&result).len() < if max_bytes == Some(64) { 1000 } else { 14_000 });
        }
        assert_eq!(result["structuredContent"]["written"], false);
        w.assert_bytes("a", b"old");
    }
}

#[test]
fn apply_patch_preview_uses_explicit_root_and_complete_structured_file_list() {
    let w = Workspace::new(&[]);
    let mut rpc = Rpc::start(false);
    let patch = "*** Begin Patch\n*** Add File: nested/new\n+hello\n*** End Patch\n";
    rpc.ok(
        "xnip_apply",
        json!({"manifest_text":patch,"patch_root":w.root(),"dry_run":true}),
    );
    w.absent("nested");
    rpc.ok(
        "xnip_apply",
        json!({"manifest_text":patch,"patch_root":w.root()}),
    );
    w.assert_bytes("nested/new", b"hello\n");
    let ops: Vec<_> = (0..23).map(|i| json!({"op":"create","file":w.path(&format!("new-{i}")),"text":format!("unique {i}")})).collect();
    let result = rpc.ok("xnip_apply", json!({"manifest_text":json!(ops).to_string(),"format":"json","dry_run":true,"max_bytes":0}));
    assert_eq!(
        result["structuredContent"]["files"]
            .as_array()
            .unwrap()
            .len(),
        23
    );
    assert_eq!(result["structuredContent"]["written"], false);
    for i in 0..23 {
        assert!(text(&result).contains(&format!("+unique {i}")));
        w.absent(&format!("new-{i}"));
    }
}

#[test]
fn mcp_readers_apply_defaults_and_explicit_unlimited_output() {
    let source = "x\n".repeat(205);
    let w = Workspace::new(&[("a", source.as_bytes())]);
    let mut rpc = Rpc::start(false);
    for (tool, mut args, marker, default_count) in [
        (
            "xnip_peek",
            json!({"file":w.path("a"),"all":true}),
            ": x",
            200,
        ),
        (
            "xnip_find",
            json!({"files":[w.path("a")],"pattern":"x"}),
            ":1\n",
            100,
        ),
    ] {
        let bounded = rpc.ok(tool, args.clone());
        assert!(text(&bounded).contains("truncated"));
        assert_eq!(text(&bounded).matches(marker).count(), default_count);
        args[if tool == "xnip_peek" {
            "max_lines"
        } else {
            "max_matches"
        }] = json!(0);
        args["max_bytes"] = json!(0);
        let full = rpc.ok(tool, args);
        assert_eq!(text(&full).matches(marker).count(), 205);
        assert!(!text(&full).contains("truncated"));
    }
}

#[test]
fn mcp_recovery_previews_then_restores_a_known_interrupted_transaction() {
    recovery_receipts(false);
}

#[test]
fn text_recovery_receipts_preserve_preview_and_retained_journal() {
    recovery_receipts(true);
}

fn recovery_receipts(text_mode: bool) {
    let w = Workspace::new(&[("a", b"old-a"), ("b", b"old-b")]);
    let permissions = std::fs::metadata(w.path("b")).unwrap().permissions();
    let mut readonly = permissions.clone();
    readonly.set_readonly(true);
    std::fs::set_permissions(w.path("b"), readonly).unwrap();
    let failure = w
        .batch(&json!([
            {"op":"edit","file":"a","old":"old-a","text":"new-a"},
            {"op":"edit","file":"b","old":"old-b","text":"new-b"}
        ]))
        .output()
        .unwrap();
    std::fs::set_permissions(w.path("b"), permissions).unwrap();
    assert_eq!(failure.status.code(), Some(4));
    let stderr = common::utf8(&failure.stderr);
    let journal = std::path::PathBuf::from(
        stderr
            .rsplit_once("recovery journal: ")
            .unwrap()
            .1
            .split("; planned paths ")
            .next()
            .unwrap()
            .trim(),
    );
    w.write("a", b"new-a");
    let mut rpc = if text_mode {
        Rpc::start_text(false)
    } else {
        Rpc::start(false)
    };
    let preview = rpc.ok(
        "xnip_recover",
        json!({"journal":journal,"dry_run":true,"max_bytes":32}),
    );
    if text_mode {
        assert!(preview.get("structuredContent").is_none());
    } else {
        assert_eq!(preview["structuredContent"]["mode"], "preview");
        assert_eq!(preview["structuredContent"]["written"], false);
    }
    assert!(text(&preview).starts_with("preview; written=false"));
    assert!(text(&preview).contains("journal="));
    assert!(text(&preview).contains("truncated"));
    w.assert_bytes("a", b"new-a");
    let result = rpc.ok("xnip_recover", json!({"journal":journal}));
    if text_mode {
        assert!(result.get("structuredContent").is_none());
    } else {
        assert_eq!(result["structuredContent"]["written"], true);
    }
    assert!(text(&result).starts_with("written; journal="));
    w.assert_bytes("a", b"old-a");
    std::fs::remove_dir_all(journal).unwrap();
}

#[test]
fn every_write_tool_rejects_unknown_fields_instead_of_silently_ignoring_them() {
    let w = Workspace::new(&[("a", b"old\n")]);
    let mut rpc = Rpc::start(true);
    for (tool, mut args) in [
        (
            "xnip_edit",
            json!({"file":w.path("a"),"old":"old","text":"new"}),
        ),
        (
            "xnip_apply",
            json!({"edits":[{"file":w.path("a"),"old":"old","text":"new"}]}),
        ),
        ("xnip_recover", json!({"journal":w.path("missing")})),
        (
            "xnip_replace",
            json!({"file":w.path("a"),"lines":"1","text":"new"}),
        ),
        (
            "xnip_insert",
            json!({"file":w.path("a"),"lines":"1","text":"new"}),
        ),
        (
            "xnip_move",
            json!({"file":w.path("a"),"from_lines":"1","to":2}),
        ),
        (
            "xnip_indent",
            json!({"file":w.path("a"),"all":true,"add":2}),
        ),
    ] {
        args["bakcup"] = json!(true);
        let response = rpc.call(tool, args);
        assert!(
            response.get("error").is_some() || response["result"]["isError"] == true,
            "{tool}: {response}"
        );
        w.assert_bytes("a", b"old\n");
    }
}

#[test]
fn legacy_write_arguments_still_work_with_strict_schema_validation() {
    let w = Workspace::new(&[("a", b"a\nb\nc\n")]);
    let mut rpc = Rpc::start(true);
    let rejected = rpc.call(
        "xnip_replace",
        json!({"file":w.path("a"),"lines":"1","text":"bad","was":"wrong"}),
    );
    assert!(rejected.get("error").is_some() || rejected["result"]["isError"] == true);
    w.assert_bytes("a", b"a\nb\nc\n");
    rpc.ok(
        "xnip_replace",
        json!({"file":w.path("a"),"lines":"1","text":"A","was":"a\n"}),
    );
    rpc.ok(
        "xnip_insert",
        json!({"file":w.path("a"),"match_line":"^b$","text":"extra","position":"before"}),
    );
    rpc.ok(
        "xnip_move",
        json!({"file":w.path("a"),"from_lines":"1","to":4,"position":"after"}),
    );
    rpc.ok(
        "xnip_indent",
        json!({"file":w.path("a"),"lines":"1-2","add":2}),
    );
    w.assert_bytes("a", b"  extra\n  b\nc\nA\n");
    w.absent("a.bak");
}

#[test]
fn file_groups_support_guarded_and_unguarded_whole_writes_with_small_receipts() {
    let w = Workspace::new(&[("a", b"old\r\n"), ("b", b"was")]);
    let mut rpc = Rpc::start(false);
    let files = json!({
        w.path("a").to_string_lossy().as_ref(): [["old","middle"],["middle","final"]],
        w.path("b").to_string_lossy().as_ref(): {"old":"was","text":"now"},
        w.path("nested/empty").to_string_lossy().as_ref(): ""
    });
    for flag in ["dry_run", "check"] {
        let mut args = json!({"files":files});
        args[flag] = json!(true);
        let result = rpc.ok("xnip_apply", args);
        assert_eq!(result["structuredContent"]["written"], false);
        w.assert_bytes("a", b"old\r\n");
        w.absent("nested");
    }
    let result = rpc.ok("xnip_apply", json!({"files":files}));
    assert_eq!(text(&result), "written");
    assert_eq!(
        result["structuredContent"]["files"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    w.assert_bytes("a", b"final\r\n");
    w.assert_bytes("b", b"now");
    w.assert_bytes("nested/empty", b"");
}

#[test]
fn file_groups_reject_mixed_inputs_aliases_unknown_modifiers_and_late_conflicts() {
    let w = Workspace::new(&[("a", b"old")]);
    let mut rpc = Rpc::start(false);
    let files = json!({w.path("a").to_string_lossy().as_ref(): [["old","new"]]});
    for extra in [
        json!({"edits":[]}),
        json!({"path":"manifest"}),
        json!({"manifest_text":"[]"}),
        json!({"format":"compact"}),
        json!({"patch_root":w.root()}),
    ] {
        let mut args = json!({"files":files});
        args.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        let response = rpc.call("xnip_apply", args);
        assert!(
            response.get("error").is_some() || response["result"]["isError"] == true,
            "{response}"
        );
        w.assert_bytes("a", b"old");
    }
    for bad in [
        json!({}),
        json!({w.path("a").to_string_lossy().as_ref(): {"text":"new","all":true}}),
        json!({w.path("a").to_string_lossy().as_ref(): "new", w.path("./a").to_string_lossy().as_ref(): "other"}),
        json!({w.path("a").to_string_lossy().as_ref(): "new", w.path("z/new").to_string_lossy().as_ref(): {"old":"","text":"bad"}}),
    ] {
        let response = rpc.call("xnip_apply", json!({"files":bad}));
        assert!(
            response.get("error").is_some() || response["result"]["isError"] == true,
            "{response}"
        );
        w.assert_bytes("a", b"old");
        w.absent("z");
    }
}

#[test]
fn raw_compact_manifest_rejects_duplicate_keys_before_any_guard_can_be_lost() {
    let w = Workspace::new(&[("a", b"old")]);
    let mut rpc = Rpc::start(false);
    let key = serde_json::to_string(&w.path("a")).unwrap();
    let source = format!("{{{key}:[[\"old\",\"new\"]],{key}:\"bad\"}}");
    let response = rpc.call(
        "xnip_apply",
        json!({"manifest_text":source,"format":"compact"}),
    );
    assert!(response.get("error").is_some() || response["result"]["isError"] == true);
    w.assert_bytes("a", b"old");
}

#[test]
fn explicit_write_and_literal_anchor_insert_are_available_over_mcp() {
    let w = Workspace::new(&[]);
    let mut rpc = Rpc::start(false);
    rpc.ok(
        "xnip_edit",
        json!({"file":w.path("a"),"operation":"write","text":"anchor\r\nend"}),
    );
    rpc.ok(
        "xnip_edit",
        json!({"file":w.path("a"),"old":"anchor","text":"BEFORE","position":"before"}),
    );
    rpc.ok(
        "xnip_edit",
        json!({"file":w.path("a"),"old":"end","text":"AFTER","position":"after"}),
    );
    w.assert_bytes("a", b"BEFOREanchor\r\nendAFTER");
    let response = rpc.call(
        "xnip_edit",
        json!({"file":w.path("a"),"operation":"write","old":"wrong","text":"bad"}),
    );
    assert!(response.get("error").is_some() || response["result"]["isError"] == true);
    w.assert_bytes("a", b"BEFOREanchor\r\nendAFTER");
}
