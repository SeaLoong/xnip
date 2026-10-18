//! Compact file groups share public CLI transaction and byte-preservation contracts.
mod common;
use common::Workspace;
use serde_json::json;

#[test]
fn compact_pairs_execute_in_order_and_whole_writes_are_explicit() {
    for format in ["compact", "json"] {
        let w = Workspace::new(&[("a", b"old\r\nkeep"), ("b", b"was")]);
        let source = json!({
            "a": [["old", "middle"], ["middle", "final"]],
            "b": {"old":"was", "text":"now"},
            "nested/empty": ""
        })
        .to_string();
        for flag in ["--dry-run", "--check"] {
            w.apply(format, &source).arg(flag).assert().success();
            w.assert_bytes("a", b"old\r\nkeep");
            w.assert_bytes("b", b"was");
            w.absent("nested");
        }
        w.apply(format, &source).assert().success();
        w.assert_bytes("a", b"final\r\nkeep");
        w.assert_bytes("b", b"now");
        w.assert_bytes("nested/empty", b"");
    }
}

#[test]
fn late_guard_failure_cannot_write_earlier_file_groups() {
    for bad in [
        json!({"a":"changed", "zz/nested/new":{"old":"","text":"bad"}}),
        json!({"a":"changed", "z":[["absent","bad"]]}),
        json!({"a":"changed", "z":{"old":"wrong","text":"bad"}}),
    ] {
        let w = Workspace::new(&[("a", b"old"), ("z", b"keep")]);
        w.apply("compact", &bad.to_string()).assert().code(3);
        w.assert_bytes("a", b"old");
        w.assert_bytes("z", b"keep");
        w.absent("zz");
    }
}

#[test]
fn compact_missing_and_ambiguous_blocks_fail_without_fallback_to_writes() {
    for pair in [
        json!([["missing", "new"]]),
        json!([["x", "new"]]),
        json!([["", "new"]]),
    ] {
        let w = Workspace::new(&[("a", b"x x")]);
        w.apply("compact", &json!({"a":pair}).to_string())
            .assert()
            .failure();
        w.assert_bytes("a", b"x x");
    }
}

#[test]
fn malformed_or_aliased_groups_cannot_silently_choose_a_write() {
    for source in [
        r#"{"a":"x","a":"y"}"#,
        r#"{"a":"x","./a":"y"}"#,
        r#"{"a":[]}"#,
        r#"{"a":[["old","new","extra"]]}"#,
        r#"{"a":{"text":"new","all":true}}"#,
    ] {
        let w = Workspace::new(&[("a", b"old")]);
        w.apply("compact", source).assert().failure();
        w.assert_bytes("a", b"old");
    }
}

#[test]
fn native_unquoted_internal_carriage_returns_remain_literal_bytes() {
    let w = Workspace::new(&[]);
    w.apply(
        "native",
        "create a old\rmid\r\nedit a old\rmid new\rbody\r\nwrite b last\r\n",
    )
    .assert()
    .success();
    w.assert_bytes("a", b"new\rbody");
    w.assert_bytes("b", b"last");
    // A bare CR at EOF is content; only the CR in a CRLF record ending is removed.
    w.apply("native", "edit b last done\r").assert().success();
    w.assert_bytes("b", b"done\r");
}

#[test]
fn native_multiline_literals_preserve_line_endings_quotes_and_eof() {
    let w = Workspace::new(&[("a", b"old\r\nblock\r\nkeep")]);
    let program = "# comment\r\nedit a \"old\r\nblock\" \"new\r\nbody\"\r\nwrite nested/b \"raw\nline\\\"quoted\\\\end\"\nwrite empty \"\"\n";
    w.apply("native", program).assert().success();
    w.assert_bytes("a", b"new\r\nbody\r\nkeep");
    w.assert_bytes("nested/b", b"raw\nline\"quoted\\end");
    w.assert_bytes("empty", b"");
}

#[test]
fn native_multiline_comments_are_payload_and_missing_closing_quote_is_an_error() {
    let w = Workspace::new(&[("a", b"old")]);
    w.apply(
        "native",
        "write a \"first\n# content\nlast\"\nwrite b \"never closed\n",
    )
    .assert()
    .failure();
    w.assert_bytes("a", b"old");
    w.absent("b");
    w.apply("native", "write a \"first\n# content\nlast\" was=\"old\"\n")
        .assert()
        .success();
    w.assert_bytes("a", b"first\n# content\nlast");
    w.apply("native", "write a \"bad\" was=\"wrong\"\n")
        .assert()
        .code(3);
    w.assert_bytes("a", b"first\n# content\nlast");
}

#[test]
fn write_manifest_formats_share_whole_file_guards_and_virtual_content() {
    for (format, source) in [
        (
            "native",
            "write a \"mid\" was=\"old\"\nedit a mid final\nwrite nested/b \"\"\n",
        ),
        (
            "json",
            r#"[{"op":"write","file":"a","was":"old","text":"mid"},{"op":"edit","file":"a","old":"mid","text":"final"},{"op":"write","file":"nested/b","text":""}]"#,
        ),
        (
            "yaml",
            "- op: write\n  file: a\n  was: old\n  text: mid\n- op: edit\n  file: a\n  old: mid\n  text: final\n- op: write\n  file: nested/b\n  text: ''\n",
        ),
    ] {
        let w = Workspace::new(&[("a", b"old")]);
        w.apply(format, source).assert().success();
        w.assert_bytes("a", b"final");
        w.assert_bytes("nested/b", b"");
    }
}
