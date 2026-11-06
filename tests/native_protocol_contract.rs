//! Public native-host protocol: authority, staged edits and byte receipts.
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{Value, json};

struct Session {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    next_id: u64,
}

impl Session {
    fn new() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_xnip"))
            .args(["native", "serve"])
            .env("XNIP_AUTO_UPDATE", "0")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        Self {
            input: child.stdin.take().unwrap(),
            output: BufReader::new(child.stdout.take().unwrap()),
            child,
            next_id: 0,
        }
    }

    fn call(&mut self, mut request: Value) -> Value {
        self.next_id += 1;
        request["id"] = json!(self.next_id);
        if request.get("protocol").is_none() {
            request["protocol"] = json!(1);
        }
        serde_json::to_writer(&mut self.input, &request).unwrap();
        self.input.write_all(b"\n").unwrap();
        self.input.flush().unwrap();
        let mut response = String::new();
        assert!(
            self.output.read_line(&mut response).unwrap() > 0,
            "native session exited"
        );
        let response: Value = serde_json::from_str(&response).unwrap();
        assert_eq!(response["id"], self.next_id);
        response
    }

    fn commit(&mut self, plan: &Value, root: &std::path::Path) -> Value {
        assert!(plan["error"].is_null(), "{plan}");
        let reply = self.call(
            json!({"action":"commit","plan_id":plan["result"]["plan_id"],
            "policy":{"read_only":false,"writable_roots":[root]}}),
        );
        assert!(reply["error"].is_null(), "{reply}");
        reply
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn unsupported_protocol_and_unknown_plan_do_not_poison_session() {
    let mut editor = Session::new();
    assert!(editor.call(json!({"protocol":99,"action":"capabilities"}))["error"].is_object());
    assert!(editor.call(json!({"action":"abort","plan_id":999}))["error"].is_object());
    let capabilities = editor.call(json!({"action":"capabilities"}));
    assert!(capabilities["result"].is_object(), "{capabilities}");
    let missing_authority =
        editor.call(json!({"action":"commit","plan_id":1,"policy":{"read_only":false}}));
    assert!(
        missing_authority["error"].is_object(),
        "{missing_authority}"
    );
    assert!(editor.call(json!({"action":"capabilities","unexpected":true}))["error"].is_object());
}

#[test]
fn preview_then_guarded_commit_preserves_crlf_and_returns_exact_bytes() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("file.txt");
    let before = b"name=old\r\nkeep=1";
    let after = b"name=new\r\nkeep=1";
    std::fs::write(&path, before).unwrap();
    let mut editor = Session::new();
    let plan = editor.call(json!({"action":"plan","operations":[{
        "op":"exact_replace","path":path,"old":b"old".to_vec(),"text":b"new".to_vec(),"all":false
    }]}));
    assert!(plan["error"].is_null(), "{plan}");
    assert_eq!(std::fs::read(&path).unwrap(), before);
    let plan_id = &plan["result"]["plan_id"];
    assert_eq!(
        plan["result"]["changes"][0]["before"],
        json!(before.to_vec())
    );
    assert_eq!(plan["result"]["changes"][0]["after"], json!(after.to_vec()));
    let commit = editor.call(json!({"action":"commit","plan_id":plan_id,"policy":{
        "read_only":false,"writable_roots":[temp.path()]
    }}));
    assert!(commit["error"].is_null(), "{commit}");
    assert_eq!(std::fs::read(&path).unwrap(), after);
    let duplicate = editor.call(json!({"action":"commit","plan_id":plan_id,"policy":{
        "read_only":false,"writable_roots":[temp.path()]
    }}));
    assert!(duplicate["error"].is_object());
}

#[test]
fn host_policy_denial_and_stale_input_leave_files_untouched() {
    let temp = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let path = temp.path().join("file.txt");
    std::fs::write(&path, b"old").unwrap();
    let mut editor = Session::new();
    for policy in [
        json!({"read_only":true,"writable_roots":[temp.path()]}),
        json!({"read_only":false,"writable_roots":[outside.path()]}),
    ] {
        let plan = editor.call(json!({"action":"plan","operations":[{
            "op":"exact_replace","path":path,"old":b"old".to_vec(),"text":b"new".to_vec(),"all":false
        }]}));
        assert!(plan["error"].is_null(), "{plan}");
        let response = editor
            .call(json!({"action":"commit","plan_id":plan["result"]["plan_id"],"policy":policy}));
        assert!(response["error"].is_object(), "{response}");
        assert_eq!(std::fs::read(&path).unwrap(), b"old");
    }
    let plan = editor.call(json!({"action":"plan","operations":[{
        "op":"exact_replace","path":path,"old":b"old".to_vec(),"text":b"new".to_vec(),"all":false
    }]}));
    std::fs::write(&path, b"external").unwrap();
    let response = editor.call(
        json!({"action":"commit","plan_id":plan["result"]["plan_id"],"policy":{
            "read_only":false,"writable_roots":[temp.path()]
        }}),
    );
    assert!(response["error"].is_object(), "{response}");
    assert_eq!(std::fs::read(&path).unwrap(), b"external");
}

#[test]
fn eof_after_commit_request_waits_for_the_transaction_result() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("file");
    std::fs::write(&path, b"before").unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_xnip"))
        .args(["native", "serve"])
        .env("XNIP_AUTO_UPDATE", "0")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    serde_json::to_writer(
        &mut input,
        &json!({"id":1,"protocol":1,"action":"plan","operations":[{
            "op":"write","path":path,"text":b"after".to_vec()
        }]}),
    )
    .unwrap();
    input.write_all(b"\n").unwrap();
    input.flush().unwrap();
    let mut line = String::new();
    output.read_line(&mut line).unwrap();
    let plan: Value = serde_json::from_str(&line).unwrap();
    assert!(plan["error"].is_null(), "{plan}");
    serde_json::to_writer(
        &mut input,
        &json!({"id":2,"protocol":1,"action":"commit","plan_id":plan["result"]["plan_id"],"policy":{
            "read_only":false,"writable_roots":[temporary.path()]
        }}),
    )
    .unwrap();
    input.write_all(b"\n").unwrap();
    input.flush().unwrap();
    drop(input);
    line.clear();
    assert!(output.read_line(&mut line).unwrap() > 0);
    let result: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(result["id"], 2);
    assert_eq!(result["result"]["written"], true, "{result}");
    assert!(child.wait().unwrap().success());
    assert_eq!(std::fs::read(path).unwrap(), b"after");
}

#[test]
fn ordered_lifecycle_operations_share_virtual_state_and_binary_receipts() {
    let directory = tempfile::tempdir().unwrap();
    let created = directory.path().join("created");
    let renamed = directory.path().join("nested/renamed");
    let deleted = directory.path().join("deleted");
    std::fs::write(&deleted, b"remove me").unwrap();
    let mut session = Session::new();
    let plan = session.call(json!({"action":"plan","operations":[
        {"op":"create","path":created,"text":[0,255,13,10]},
        {"op":"rename","path":created,"to":renamed,"old":[0,255,13,10]},
        {"op":"write","path":renamed,"old":[0,255,13,10],"text":[239,187,191,65,13,10]},
        {"op":"delete","path":deleted,"old":b"remove me".to_vec()}
    ]}));
    assert!(plan["error"].is_null(), "{plan}");
    assert!(!created.exists());
    assert!(!renamed.parent().unwrap().exists());
    assert_eq!(std::fs::read(&deleted).unwrap(), b"remove me");
    let committed = session.commit(&plan, directory.path());
    assert_eq!(committed["result"]["written"], true);
    assert!(!created.exists());
    assert!(!deleted.exists());
    assert_eq!(
        std::fs::read(&renamed).unwrap(),
        [239, 187, 191, 65, 13, 10]
    );
    let changes = committed["result"]["changes"].as_array().unwrap();
    let receipt = changes
        .iter()
        .find(|change| change["path"] == json!(renamed))
        .unwrap();
    assert!(receipt["before"].is_null());
    assert_eq!(receipt["after"], json!([239, 187, 191, 65, 13, 10]));
}

#[test]
fn line_move_indent_and_literal_insert_use_the_same_ordered_plan() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("source.txt");
    let before = b"one\ntwo\nthree\n";
    std::fs::write(&path, before).unwrap();
    let mut session = Session::new();
    let plan = session.call(json!({"action":"plan","operations":[
        {"op":"line_replace","path":path,"start":2,"end":2,"was":b"two\n".to_vec(),"text":b"TWO".to_vec()},
        {"op":"line_insert","path":path,"line":1,"position":"after","text":b"inserted".to_vec()},
        {"op":"move","path":path,"start":4,"end":4,"to":1,"position":"before"},
        {"op":"indent","path":path,"start":2,"end":3,"indent":{"kind":"add","unit":2}},
        {"op":"insert_exact","path":path,"anchor":b"TWO\n".to_vec(),"position":"after","text":b"tail".to_vec()}
    ]}));
    assert_eq!(std::fs::read(&path).unwrap(), before);
    session.commit(&plan, directory.path());
    assert_eq!(
        std::fs::read(path).unwrap(),
        b"three\n  one\n  inserted\nTWO\ntail"
    );
}

#[test]
fn patch_and_bounded_read_execute_through_the_public_native_process() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("source.txt");
    std::fs::write(&path, b"old\n").unwrap();
    let mut session = Session::new();
    let patch = "*** Begin Patch\n*** Update File: source.txt\n@@\n-old\n+new\n*** Add File: added.txt\n+created\n*** End Patch\n";
    let plan = session.call(
        json!({"action":"plan","operations":[{"op":"patch","root":directory.path(),"text":patch}]}),
    );
    session.commit(&plan, directory.path());
    assert_eq!(std::fs::read(&path).unwrap(), b"new\n");
    assert_eq!(
        std::fs::read(directory.path().join("added.txt")).unwrap(),
        b"created\n"
    );
    let read = session.call(json!({"action":"read","path":path,"offset":1,"max_bytes":2}));
    assert_eq!(read["result"]["bytes"], json!([101, 119]));
    assert_eq!(read["result"]["total_bytes"], 4);
    assert_eq!(read["result"]["truncated"], true);
}

#[test]
fn late_batch_error_discards_virtual_create_without_creating_directories() {
    let directory = tempfile::tempdir().unwrap();
    let existing = directory.path().join("existing");
    let created = directory.path().join("nested/new");
    std::fs::write(&existing, b"old").unwrap();
    let mut session = Session::new();
    let response = session.call(json!({"action":"plan","operations":[
        {"op":"create","path":created,"text":b"virtual".to_vec()},
        {"op":"exact_replace","path":existing,"old":b"missing".to_vec(),"text":b"new".to_vec()}
    ]}));
    assert!(response["error"].is_object());
    assert!(!created.parent().unwrap().exists());
    assert_eq!(std::fs::read(existing).unwrap(), b"old");
}

#[test]
fn utf8_transport_preserves_bom_crlf_eof_and_keeps_byte_receipts_available() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("utf8.txt");
    let before = "\u{feff}标题=旧值\r\nkeep=1";
    let after = "\u{feff}标题=新值 🌏\r\nkeep=1";
    std::fs::write(&path, before).unwrap();
    let mut session = Session::new();
    let capabilities = session.call(json!({"action":"capabilities"}));
    assert_eq!(
        capabilities["result"]["content_encodings"],
        json!(["bytes", "utf8"])
    );
    let read = session.call(json!({"action":"read","path":path,"content_encoding":"utf8"}));
    assert_eq!(read["result"]["text"], before);
    let plan = session.call(
        json!({"action":"plan","content_encoding":"utf8","operations":[
            {"op":"exact_replace","path":path,"old":"旧值","text":"新值 🌏"}
        ]}),
    );
    assert_eq!(plan["result"]["changes"][0]["before"], before);
    assert_eq!(plan["result"]["changes"][0]["after"], after);
    assert_eq!(std::fs::read(&path).unwrap(), before.as_bytes());
    let committed = session.commit(&plan, directory.path());
    assert_eq!(committed["result"]["changes"][0]["after"], after);
    assert_eq!(std::fs::read(&path).unwrap(), after.as_bytes());
    let read = session.call(json!({"action":"read","path":path}));
    assert_eq!(read["result"]["bytes"], json!(after.as_bytes()));
}

#[test]
fn utf8_transport_keeps_binary_before_images_as_bytes() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("binary");
    let before = [255, 0, 13, 10];
    std::fs::write(&path, before).unwrap();
    let mut session = Session::new();
    let read = session.call(json!({"action":"read","path":path,"content_encoding":"utf8"}));
    assert_eq!(read["result"]["bytes"], json!(before));
    assert!(read["result"].get("text").is_none());
    let plan = session.call(
        json!({"action":"plan","content_encoding":"utf8","operations":[
            {"op":"write","path":path,"old":before,"text":"text\r\n"}
        ]}),
    );
    assert_eq!(plan["result"]["changes"][0]["before"], json!(before));
    assert_eq!(plan["result"]["changes"][0]["after"], "text\r\n");
    let committed = session.commit(&plan, directory.path());
    assert_eq!(committed["result"]["changes"][0]["before"], json!(before));
    assert_eq!(std::fs::read(path).unwrap(), b"text\r\n");
}

#[test]
fn string_payloads_cover_ordered_lifecycle_and_line_guards() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source");
    let destination = directory.path().join("destination");
    let mut session = Session::new();
    let plan = session.call(json!({"action":"plan","content_encoding":"utf8","operations":[
        {"op":"create","path":source,"text":"one\ntwo\n"},
        {"op":"rename","path":source,"to":destination,"old":"one\ntwo\n"},
        {"op":"line_replace","path":destination,"start":2,"end":2,"was":"two\n","text":"TWO"},
        {"op":"line_insert","path":destination,"line":1,"position":"after","text":"between"},
        {"op":"insert_exact","path":destination,"anchor":"TWO","position":"before","text":"last="}
    ]}));
    session.commit(&plan, directory.path());
    assert_eq!(
        std::fs::read(&destination).unwrap(),
        b"one\nbetween\nlast=TWO\n"
    );
    let plan = session.call(json!({"action":"plan","operations":[
        {"op":"delete","path":destination,"old":"one\nbetween\nlast=TWO\n"}
    ]}));
    session.commit(&plan, directory.path());
    assert!(!destination.exists());
}
