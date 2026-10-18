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
        let mut command = std::process::Command::cargo_bin("xnip").unwrap();
        command.env("XNIP_AUTO_UPDATE", "0").arg("mcp");
        if full {
            command.args(["--toolset", "full"]);
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
        assert_ne!(response["result"]["isError"], true, "{response}");
        response["result"].clone()
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
        if matches!(name, "xnip_edit" | "xnip_apply" | "xnip_recover") {
            assert_eq!(
                tool["outputSchema"]["properties"]["written"]["type"],
                "boolean"
            );
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
    let mut rpc = Rpc::start(false);
    let preview = rpc.ok(
        "xnip_recover",
        json!({"journal":journal,"dry_run":true,"max_bytes":32}),
    );
    assert_eq!(preview["structuredContent"]["mode"], "preview");
    assert_eq!(preview["structuredContent"]["written"], false);
    assert!(text(&preview).contains("truncated"));
    w.assert_bytes("a", b"new-a");
    let result = rpc.ok("xnip_recover", json!({"journal":journal}));
    assert_eq!(result["structuredContent"]["written"], true);
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
}
