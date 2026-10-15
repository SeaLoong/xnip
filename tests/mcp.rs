//! `xnip mcp` stdio MCP server 端到端冒烟测试。
//!
//! 这里**不依赖 rmcp client**——直接对子进程喂裸 JSON-RPC 帧，验证：
//! 1. `initialize` 握手返回正确 `serverInfo`（name/version 来自 xnip 自身而非 rmcp）
//! 2. `tools/list` 返回 10 个我们注册的工具
//! 3. `tools/call xnip_peek` 实际工作：行号格式 + 字节透传
//! 4. `tools/call xnip_replace` + `was` 校验失败时返回 JSON-RPC error 且文件未改动

use std::io::Write;
use std::process::{Command, Stdio};

use assert_cmd::cargo::CommandCargoExt;

#[test]
fn doctor_responds_while_stdin_remains_open() {
    use std::io::{BufRead, BufReader};
    use std::sync::mpsc;
    use std::time::Duration;

    struct Server(std::process::Child);
    impl Drop for Server {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let mut server = Server(
        Command::cargo_bin("xnip")
            .unwrap()
            .arg("mcp")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let mut input = server.0.stdin.take().unwrap();
    let output = server.0.stdout.take().unwrap();
    let (tx, rx) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(output).lines() {
            if tx.send(line.unwrap()).is_err() {
                break;
            }
        }
    });
    let response = |id: u64| -> serde_json::Value {
        loop {
            let line = rx
                .recv_timeout(Duration::from_secs(10))
                .expect("MCP response timed out with stdin still open");
            let value: serde_json::Value = serde_json::from_str(&line).unwrap();
            if value["id"].as_u64() == Some(id) {
                return value;
            }
        }
    };
    input.write_all(frames(&[]).as_bytes()).unwrap();
    input.flush().unwrap();
    assert!(response(1).get("result").is_some());
    writeln!(
        input,
        "{}",
        serde_json::json!({
            "jsonrpc":"2.0","id":2,"method":"tools/call",
            "params":{"name":"xnip_doctor","arguments":{}}
        })
    )
    .unwrap();
    input.flush().unwrap();
    let doctor = response(2);
    assert!(
        doctor["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("stdin-tty:  false")
    );
    writeln!(
        input,
        "{}",
        serde_json::json!({
            "jsonrpc":"2.0","id":3,"method":"tools/list","params":{}
        })
    )
    .unwrap();
    input.flush().unwrap();
    assert_eq!(response(3)["result"]["tools"].as_array().unwrap().len(), 10);
    drop(input);
    drop(server);
    reader.join().unwrap();
}

/// 构造一组 JSON-RPC 帧（initialize → notifications/initialized → 用户给的额外帧）。
fn frames(extras: &[&str]) -> String {
    let mut s = String::new();
    s.push_str(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"xnip-test","version":"0"}}}"#);
    s.push('\n');
    s.push_str(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#);
    s.push('\n');
    for e in extras {
        s.push_str(e);
        s.push('\n');
    }
    s
}

/// 运行 `xnip mcp`，喂入 frames，回收 stdout 字符串。
fn run_mcp(input: &str) -> String {
    let mut child = Command::cargo_bin("xnip")
        .expect("xnip bin")
        .arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn xnip mcp");

    {
        let stdin = child.stdin.as_mut().expect("stdin");
        stdin.write_all(input.as_bytes()).expect("write stdin");
    }

    let output = child.wait_with_output().expect("wait");
    assert!(
        output.status.success(),
        "xnip mcp exited non-zero: status={:?}, stderr={}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("utf-8 stdout")
}

#[test]
fn initialize_returns_xnip_server_info() {
    let out = run_mcp(&frames(&[]));
    // 验证 initialize 响应包含 xnip 名字与版本（而非 rmcp 默认）
    assert!(
        out.contains(r#""serverInfo":{"name":"xnip","version""#),
        "expected xnip serverInfo in: {out}"
    );
    assert!(out.contains(r#""protocolVersion":"2025-06-18""#));
}

#[test]
fn tools_list_returns_ten_tools() {
    let out = run_mcp(&frames(&[
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#,
    ]));
    for name in [
        "xnip_edit",
        "xnip_recover",
        "xnip_peek",
        "xnip_find",
        "xnip_replace",
        "xnip_insert",
        "xnip_move",
        "xnip_indent",
        "xnip_apply",
        "xnip_doctor",
    ] {
        let needle = format!(r#""name":"{name}""#);
        assert!(out.contains(&needle), "missing tool {name} in: {out}");
    }
}

#[test]
fn tool_call_xnip_peek_works() {
    let dir = tempfile::tempdir().expect("tmp");
    let file = dir.path().join("sample.txt");
    std::fs::write(&file, b"line one\nline two\nline three\nline four\n").unwrap();

    let call = format!(
        r#"{{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{{"name":"xnip_peek","arguments":{{"file":{:?},"lines":"2-3"}}}}}}"#,
        file.display().to_string()
    );
    let out = run_mcp(&frames(&[&call]));
    // 期待返回 result.content[0].text 含两行带行号的内容
    assert!(
        out.contains(r"     2: line two"),
        "missing line 2 in: {out}"
    );
    assert!(
        out.contains(r"     3: line three"),
        "missing line 3 in: {out}"
    );
    assert!(out.contains(r#""isError":false"#));
}

#[test]
fn tool_call_xnip_replace_was_mismatch_keeps_file_intact() {
    let dir = tempfile::tempdir().expect("tmp");
    let file = dir.path().join("sample.txt");
    let original = b"alpha\nbeta\ngamma\n";
    std::fs::write(&file, original).unwrap();

    let call = format!(
        r#"{{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{{"name":"xnip_replace","arguments":{{"file":{:?},"lines":"2","text":"X","was":"WRONG\n"}}}}}}"#,
        file.display().to_string()
    );
    let out = run_mcp(&frames(&[&call]));
    // 应该返回 JSON-RPC error 而非 result
    assert!(
        out.contains(r#""error":{"code":-32600"#),
        "expected error in: {out}"
    );
    assert!(out.contains("`was` check failed"));
    // 文件未被修改
    let after = std::fs::read(&file).unwrap();
    assert_eq!(after, original);
}

#[test]
fn tool_call_xnip_replace_writes_atomically() {
    let dir = tempfile::tempdir().expect("tmp");
    let file = dir.path().join("sample.txt");
    std::fs::write(&file, b"alpha\nbeta\ngamma\n").unwrap();

    let call = format!(
        r#"{{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{{"name":"xnip_replace","arguments":{{"file":{:?},"lines":"2","text":"BETA","was":"beta\n"}}}}}}"#,
        file.display().to_string()
    );
    let out = run_mcp(&frames(&[&call]));
    assert!(out.contains(r#""isError":false"#), "expected ok in: {out}");
    assert_eq!(std::fs::read(&file).unwrap(), b"alpha\nBETA\ngamma\n");
}

#[test]
fn edit_tool_can_preview_create_and_exact_replace() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("nested/new.txt");
    for dry_run in [true, false] {
        let call = serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{
            "name":"xnip_edit", "arguments":{"file":file,"operation":"create","text":"old\nline","dry_run":dry_run}
        }}).to_string();
        let output = run_mcp(&frames(&[&call]));
        assert!(output.contains(r#""isError":false"#), "{output}");
        assert_eq!(file.exists(), !dry_run);
    }
    for dry_run in [true, false] {
        let call = serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{
            "name":"xnip_edit", "arguments":{"file":file,"old":"old\nline","text":"new\nblock\nend","dry_run":dry_run}
        }}).to_string();
        let output = run_mcp(&frames(&[&call]));
        assert!(output.contains(r#""isError":false"#), "{output}");
        assert_eq!(
            std::fs::read(&file).unwrap(),
            if dry_run {
                b"old\nline".as_slice()
            } else {
                b"new\nblock\nend".as_slice()
            }
        );
    }
    let call = serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{
        "name":"xnip_edit", "arguments":{"file":file,"operation":"delete","old":"wrong"}
    }})
    .to_string();
    assert!(run_mcp(&frames(&[&call])).contains(r#""error":{"code":-32600"#));
    assert!(file.exists());
}

#[test]
fn apply_tool_preview_supports_lifecycle_without_stdout_pollution() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("new/file");
    let manifest = serde_json::json!([{"op":"create","file":file,"text":"hello"}]).to_string();
    let call=serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"xnip_apply","arguments":{"manifest_text":manifest,"format":"json","dry_run":true}}}).to_string();
    let output = run_mcp(&frames(&[&call]));
    for line in output.lines() {
        serde_json::from_str::<serde_json::Value>(line).unwrap();
    }
    assert!(output.contains("validated"));
    assert!(!file.parent().unwrap().exists());
}

#[test]
fn apply_tool_patch_root_and_preview_do_not_depend_on_server_cwd() {
    let project = tempfile::tempdir().unwrap();
    let patch = "*** Begin Patch\n*** Add File: hello.txt\n+hello\n*** End Patch\n";
    for dry_run in [true, false] {
        let call=serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"xnip_apply","arguments":{"manifest_text":patch,"patch_root":project.path(),"dry_run":dry_run}}}).to_string();
        let output = run_mcp(&frames(&[&call]));
        assert!(output.contains(r#""isError":false"#), "{output}");
        assert_eq!(project.path().join("hello.txt").exists(), !dry_run);
    }
}
