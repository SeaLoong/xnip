//! `xnip move` 集成测试。

mod common;

use assert_cmd::Command;

#[test]
fn move_block_forward() {
    let (_dir, path) = common::tempfile_with(b"1\n2\n3\n4\n");
    Command::cargo_bin("xnip")
        .unwrap()
        .args([
            "move",
            path.to_str().unwrap(),
            "--from-lines",
            "1",
            "--to",
            "3",
            "--position",
            "after",
        ])
        .assert()
        .success();
    assert_eq!(common::read(&path), b"2\n3\n1\n4\n");
}

#[test]
fn move_block_backward() {
    let (_dir, path) = common::tempfile_with(b"1\n2\n3\n4\n");
    Command::cargo_bin("xnip")
        .unwrap()
        .args([
            "move",
            path.to_str().unwrap(),
            "--from-lines",
            "3",
            "--to",
            "1",
            "--position",
            "before",
        ])
        .assert()
        .success();
    assert_eq!(common::read(&path), b"3\n1\n2\n4\n");
}

#[test]
fn move_multiline_block() {
    let (_dir, path) = common::tempfile_with(b"1\n2\n3\n4\n5\n");
    Command::cargo_bin("xnip")
        .unwrap()
        .args([
            "move",
            path.to_str().unwrap(),
            "--from-lines",
            "2-3",
            "--to",
            "5",
            "--position",
            "after",
        ])
        .assert()
        .success();
    assert_eq!(common::read(&path), b"1\n4\n5\n2\n3\n");
}

#[test]
fn move_target_inside_source_errors() {
    let (_dir, path) = common::tempfile_with(b"1\n2\n3\n");
    Command::cargo_bin("xnip")
        .unwrap()
        .args([
            "move",
            path.to_str().unwrap(),
            "--from-lines",
            "1-2",
            "--to",
            "2",
        ])
        .assert()
        .failure()
        .code(1);
}

#[test]
fn move_dry_run_outputs_to_stdout() {
    let (_dir, path) = common::tempfile_with(b"1\n2\n3\n");
    Command::cargo_bin("xnip")
        .unwrap()
        .args([
            "move",
            path.to_str().unwrap(),
            "--from-lines",
            "1",
            "--to",
            "3",
            "--position",
            "after",
            "--dry-run",
        ])
        .assert()
        .success()
        .stdout("2\n3\n1\n");
    assert_eq!(common::read(&path), b"1\n2\n3\n");
}

#[test]
fn move_match_line_anchor() {
    let (_dir, path) = common::tempfile_with(b"a\nFOO\nb\nc\n");
    Command::cargo_bin("xnip")
        .unwrap()
        .args([
            "move",
            path.to_str().unwrap(),
            "--from-match-line",
            "^FOO$",
            "--to",
            "4",
            "--position",
            "after",
        ])
        .assert()
        .success();
    assert_eq!(common::read(&path), b"a\nb\nc\nFOO\n");
}

#[test]
fn move_requires_source_arg() {
    let (_dir, path) = common::tempfile_with(b"a\nb\n");
    Command::cargo_bin("xnip")
        .unwrap()
        .args(["move", path.to_str().unwrap(), "--to", "2"])
        .assert()
        .failure()
        .code(1);
}
