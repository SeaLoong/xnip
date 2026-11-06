//! Legacy specialized frontends keep their backup and read-only contracts when
//! persistence is routed through the shared transactions.
mod common;

use assert_cmd::cargo::CommandCargoExt;
use common::Workspace;
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

const ORIGINAL: &[u8] = b"one\ntwo\n";

fn cases() -> Vec<(Vec<&'static str>, &'static str, Value)> {
    vec![
        (
            vec![
                "replace",
                "target",
                "--pattern",
                "one",
                "--repl",
                "one",
                "--backup",
            ],
            "xnip_replace",
            json!({"file":"target","pattern":"one","repl":"one","backup":true}),
        ),
        (
            vec!["insert", "target", "--lines", "1", "--text", "", "--backup"],
            "xnip_insert",
            json!({"file":"target","lines":"1","text":"","backup":true}),
        ),
        (
            vec![
                "move",
                "target",
                "--from-lines",
                "1",
                "--to",
                "2",
                "--position",
                "before",
                "--backup",
            ],
            "xnip_move",
            json!({"file":"target","from_lines":"1","to":2,"position":"before","backup":true}),
        ),
        (
            vec!["indent", "target", "--all", "--add", "0", "--backup"],
            "xnip_indent",
            json!({"file":"target","all":true,"add":0,"backup":true}),
        ),
    ]
}

struct Rpc {
    child: Child,
    input: ChildStdin,
    output: Receiver<Value>,
    reader: Option<std::thread::JoinHandle<()>>,
    next_id: u64,
}

impl Rpc {
    fn start(workspace: &Workspace) -> Self {
        let mut child = std::process::Command::cargo_bin("xnip")
            .unwrap()
            .env("XNIP_AUTO_UPDATE", "0")
            .current_dir(workspace.root())
            .args(["mcp", "--toolset", "full"])
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
                let frame = serde_json::from_str(&line.unwrap()).unwrap();
                if sender.send(frame).is_err() {
                    break;
                }
            }
        });
        let mut rpc = Self {
            child,
            input,
            output,
            reader: Some(reader),
            next_id: 1,
        };
        let response = rpc.request(
            "initialize",
            &json!({
                "protocolVersion":"2025-06-18", "capabilities":{},
                "clientInfo":{"name":"frontend-transactions-contract","version":"1"}
            }),
        );
        assert!(response.get("result").is_some(), "{response}");
        writeln!(
            rpc.input,
            "{}",
            json!({"jsonrpc":"2.0","method":"notifications/initialized"})
        )
        .unwrap();
        rpc.input.flush().unwrap();
        rpc
    }

    fn request(&mut self, method: &str, params: &Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        writeln!(
            self.input,
            "{}",
            json!({"jsonrpc":"2.0","id":id,"method":method,"params":params})
        )
        .unwrap();
        self.input.flush().unwrap();
        loop {
            let frame = self.output.recv_timeout(Duration::from_secs(15)).unwrap();
            if frame["id"] == id {
                return frame;
            }
        }
    }
}

impl Drop for Rpc {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            reader.join().unwrap();
        }
    }
}

#[test]
fn specialized_noop_backups_still_capture_original_bytes_in_cli_and_mcp() {
    for (args, tool, arguments) in cases() {
        let workspace = Workspace::new(&[("target", ORIGINAL), ("target.bak", b"previous backup")]);
        workspace.cli(&args).assert().success();
        workspace.assert_bytes("target", ORIGINAL);
        workspace.assert_bytes("target.bak", ORIGINAL);

        workspace.write("target.bak", b"previous backup");
        let mut rpc = Rpc::start(&workspace);
        let response = rpc.request("tools/call", &json!({"name":tool,"arguments":arguments}));
        assert!(response.get("error").is_none(), "{response}");
        assert_ne!(response["result"]["isError"], true, "{response}");
        workspace.assert_bytes("target", ORIGINAL);
        workspace.assert_bytes("target.bak", ORIGINAL);
    }
}

#[cfg(unix)]
#[test]
fn specialized_readonly_failures_leave_targets_and_backups_unchanged() {
    use std::os::unix::fs::PermissionsExt;

    for mutate in [false, true] {
        for (mut args, tool, mut arguments) in cases() {
            if mutate {
                let (index, key, value) = match tool {
                    "xnip_replace" => (5, "repl", json!("changed")),
                    "xnip_insert" => (5, "text", json!("changed")),
                    "xnip_move" => (7, "position", json!("after")),
                    "xnip_indent" => (4, "add", json!(1)),
                    _ => unreachable!(),
                };
                args[index] = match tool {
                    "xnip_move" => "after",
                    "xnip_indent" => "1",
                    _ => "changed",
                };
                arguments[key] = value;
            }
            let workspace =
                Workspace::new(&[("target", ORIGINAL), ("target.bak", b"previous backup")]);
            std::fs::set_permissions(
                workspace.path("target"),
                std::fs::Permissions::from_mode(0o444),
            )
            .unwrap();
            workspace.cli(&args).assert().failure();
            workspace.assert_bytes("target", ORIGINAL);
            workspace.assert_bytes("target.bak", b"previous backup");

            let mut rpc = Rpc::start(&workspace);
            let response = rpc.request("tools/call", &json!({"name":tool,"arguments":arguments}));
            assert!(
                response.get("error").is_some() || response["result"]["isError"] == true,
                "{response}"
            );
            workspace.assert_bytes("target", ORIGINAL);
            workspace.assert_bytes("target.bak", b"previous backup");
        }
    }
}

#[cfg(unix)]
#[test]
fn exact_edit_readonly_failure_preserves_existing_backup_and_io_error_category() {
    use std::os::unix::fs::PermissionsExt;

    let workspace = Workspace::new(&[("target", ORIGINAL), ("target.bak", b"previous backup")]);
    std::fs::set_permissions(
        workspace.path("target"),
        std::fs::Permissions::from_mode(0o444),
    )
    .unwrap();
    let output = workspace
        .cli(&[
            "edit", "target", "--old", "one", "--text", "changed", "--backup",
        ])
        .assert()
        .code(2)
        .get_output()
        .clone();
    assert!(String::from_utf8_lossy(&output.stderr).contains("read-only"));
    assert!(!String::from_utf8_lossy(&output.stderr).contains("recovery journal"));
    workspace.assert_bytes("target", ORIGINAL);
    workspace.assert_bytes("target.bak", b"previous backup");

    let mut rpc = Rpc::start(&workspace);
    let response = rpc.request(
        "tools/call",
        &json!({"name":"xnip_edit","arguments":{
            "file":"target","old":"one","text":"changed","backup":true
        }}),
    );
    assert_eq!(response["error"]["code"], -32603, "{response}");
    assert!(
        response["error"]["message"]
            .as_str()
            .unwrap()
            .contains("read-only")
    );
    workspace.assert_bytes("target", ORIGINAL);
    workspace.assert_bytes("target.bak", b"previous backup");
}
