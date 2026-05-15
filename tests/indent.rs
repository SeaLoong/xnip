//! `xnip indent` 集成测试。

mod common;

use assert_cmd::Command;

#[test]
fn indent_add_to_subrange() {
    let (_dir, path) = common::tempfile_with(b"a\nb\nc\n");
    Command::cargo_bin("xnip")
        .unwrap()
        .args([
            "indent",
            path.to_str().unwrap(),
            "--lines",
            "2",
            "--add",
            "4",
        ])
        .assert()
        .success();
    assert_eq!(common::read(&path), b"a\n    b\nc\n");
}

#[test]
fn indent_remove_to_all() {
    let (_dir, path) = common::tempfile_with(b"  a\n  b\n  c\n");
    Command::cargo_bin("xnip")
        .unwrap()
        .args(["indent", path.to_str().unwrap(), "--all", "--remove", "2"])
        .assert()
        .success();
    assert_eq!(common::read(&path), b"a\nb\nc\n");
}

#[test]
fn indent_tabs_to_spaces() {
    let (_dir, path) = common::tempfile_with(b"\ta\n\t\tb\n");
    Command::cargo_bin("xnip")
        .unwrap()
        .args([
            "indent",
            path.to_str().unwrap(),
            "--all",
            "--tabs-to-spaces",
            "4",
        ])
        .assert()
        .success();
    assert_eq!(common::read(&path), b"    a\n        b\n");
}

#[test]
fn indent_spaces_to_tabs() {
    let (_dir, path) = common::tempfile_with(b"        a\n    b\n");
    Command::cargo_bin("xnip")
        .unwrap()
        .args([
            "indent",
            path.to_str().unwrap(),
            "--all",
            "--spaces-to-tabs",
            "4",
        ])
        .assert()
        .success();
    assert_eq!(common::read(&path), b"\t\ta\n\tb\n");
}

#[test]
fn indent_round_trip_add_remove() {
    let (_dir, path) = common::tempfile_with(b"a\nb\n");
    Command::cargo_bin("xnip")
        .unwrap()
        .args(["indent", path.to_str().unwrap(), "--all", "--add", "4"])
        .assert()
        .success();
    Command::cargo_bin("xnip")
        .unwrap()
        .args(["indent", path.to_str().unwrap(), "--all", "--remove", "4"])
        .assert()
        .success();
    assert_eq!(common::read(&path), b"a\nb\n");
}

#[test]
fn indent_requires_op() {
    let (_dir, path) = common::tempfile_with(b"a\n");
    Command::cargo_bin("xnip")
        .unwrap()
        .args(["indent", path.to_str().unwrap(), "--all"])
        .assert()
        .failure()
        .code(1);
}

#[test]
fn indent_requires_range() {
    let (_dir, path) = common::tempfile_with(b"a\n");
    Command::cargo_bin("xnip")
        .unwrap()
        .args(["indent", path.to_str().unwrap(), "--add", "2"])
        .assert()
        .failure()
        .code(1);
}

#[test]
fn indent_dry_run_does_not_write() {
    let (_dir, path) = common::tempfile_with(b"a\n");
    Command::cargo_bin("xnip")
        .unwrap()
        .args([
            "indent",
            path.to_str().unwrap(),
            "--all",
            "--add",
            "2",
            "--dry-run",
        ])
        .assert()
        .success()
        .stdout("  a\n");
    assert_eq!(common::read(&path), b"a\n");
}
