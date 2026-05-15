//! `xnip insert` 集成测试。

mod common;

use assert_cmd::Command;

#[test]
fn insert_after_specific_line() {
    let (_dir, path) = common::tempfile_with(b"a\nb\nc\n");
    Command::cargo_bin("xnip")
        .unwrap()
        .args([
            "insert",
            path.to_str().unwrap(),
            "--lines",
            "2",
            "--position",
            "after",
            "--text",
            "X",
        ])
        .assert()
        .success();
    assert_eq!(common::read(&path), b"a\nb\nX\nc\n");
}

#[test]
fn insert_before_first_line() {
    let (_dir, path) = common::tempfile_with(b"a\nb\n");
    Command::cargo_bin("xnip")
        .unwrap()
        .args([
            "insert",
            path.to_str().unwrap(),
            "--lines",
            "1",
            "--position",
            "before",
            "--text",
            "X",
        ])
        .assert()
        .success();
    assert_eq!(common::read(&path), b"X\na\nb\n");
}

#[test]
fn insert_match_line_anchor() {
    let (_dir, path) = common::tempfile_with(b"a\nMARK\nb\n");
    Command::cargo_bin("xnip")
        .unwrap()
        .args([
            "insert",
            path.to_str().unwrap(),
            "--match-line",
            "^MARK$",
            "--position",
            "after",
            "--text",
            "X",
        ])
        .assert()
        .success();
    assert_eq!(common::read(&path), b"a\nMARK\nX\nb\n");
}

#[test]
fn insert_multiline_payload() {
    let (_dir, path) = common::tempfile_with(b"a\nb\n");
    Command::cargo_bin("xnip")
        .unwrap()
        .args([
            "insert",
            path.to_str().unwrap(),
            "--lines",
            "1",
            "--position",
            "after",
            "--text",
            "X\nY",
        ])
        .assert()
        .success();
    assert_eq!(common::read(&path), b"a\nX\nY\nb\n");
}

#[test]
fn insert_dry_run_does_not_write() {
    let (_dir, path) = common::tempfile_with(b"a\n");
    Command::cargo_bin("xnip")
        .unwrap()
        .args([
            "insert",
            path.to_str().unwrap(),
            "--lines",
            "1",
            "--position",
            "after",
            "--text",
            "X",
            "--dry-run",
        ])
        .assert()
        .success()
        .stdout("a\nX\n");
    assert_eq!(common::read(&path), b"a\n");
}

#[test]
fn insert_rejects_pattern_locator() {
    let (_dir, path) = common::tempfile_with(b"foo\n");
    Command::cargo_bin("xnip")
        .unwrap()
        .args([
            "insert",
            path.to_str().unwrap(),
            "--pattern",
            "foo",
            "--text",
            "X",
        ])
        .assert()
        .failure()
        .code(1);
}

#[test]
fn insert_rejects_range_locator() {
    let (_dir, path) = common::tempfile_with(b"a\nb\nc\n");
    Command::cargo_bin("xnip")
        .unwrap()
        .args([
            "insert",
            path.to_str().unwrap(),
            "--lines",
            "1-2",
            "--text",
            "X",
        ])
        .assert()
        .failure()
        .code(1);
}
