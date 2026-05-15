//! `xnip replace` 集成测试。

mod common;

use assert_cmd::Command;

#[test]
fn replace_lines_with_text() {
    let (_dir, path) = common::tempfile_with(b"a\nb\nc\n");
    Command::cargo_bin("xnip")
        .unwrap()
        .args([
            "replace",
            path.to_str().unwrap(),
            "--lines",
            "2",
            "--text",
            "B",
        ])
        .assert()
        .success();
    assert_eq!(common::read(&path), b"a\nB\nc\n");
}

#[test]
fn replace_range_with_multiline_text() {
    let (_dir, path) = common::tempfile_with(b"a\nb\nc\nd\n");
    Command::cargo_bin("xnip")
        .unwrap()
        .args([
            "replace",
            path.to_str().unwrap(),
            "--lines",
            "2-3",
            "--text",
            "X\nY",
        ])
        .assert()
        .success();
    assert_eq!(common::read(&path), b"a\nX\nY\nd\n");
}

#[test]
fn replace_with_empty_deletes_range() {
    let (_dir, path) = common::tempfile_with(b"a\nb\nc\n");
    Command::cargo_bin("xnip")
        .unwrap()
        .args([
            "replace",
            path.to_str().unwrap(),
            "--lines",
            "2",
            "--text",
            "",
        ])
        .assert()
        .success();
    assert_eq!(common::read(&path), b"a\nc\n");
}

#[test]
fn replace_pattern_substitutes_globally() {
    let (_dir, path) = common::tempfile_with(b"foo bar foo\n");
    Command::cargo_bin("xnip")
        .unwrap()
        .args([
            "replace",
            path.to_str().unwrap(),
            "--pattern",
            "foo",
            "--repl",
            "BAZ",
        ])
        .assert()
        .success();
    assert_eq!(common::read(&path), b"BAZ bar BAZ\n");
}

#[test]
fn replace_pattern_with_count_limits_substitutions() {
    let (_dir, path) = common::tempfile_with(b"foo foo foo\n");
    Command::cargo_bin("xnip")
        .unwrap()
        .args([
            "replace",
            path.to_str().unwrap(),
            "--pattern",
            "foo",
            "--repl",
            "X",
            "--count",
            "1",
        ])
        .assert()
        .success();
    assert_eq!(common::read(&path), b"X foo foo\n");
}

#[test]
fn replace_was_check_passes() {
    let (_dir, path) = common::tempfile_with(b"a\nold\nc\n");
    Command::cargo_bin("xnip")
        .unwrap()
        .args([
            "replace",
            path.to_str().unwrap(),
            "--lines",
            "2",
            "--text",
            "new",
            "--was",
            "old\n",
        ])
        .assert()
        .success();
    assert_eq!(common::read(&path), b"a\nnew\nc\n");
}

#[test]
fn replace_was_check_fails_with_exit_3() {
    let (_dir, path) = common::tempfile_with(b"a\nactual\nc\n");
    Command::cargo_bin("xnip")
        .unwrap()
        .args([
            "replace",
            path.to_str().unwrap(),
            "--lines",
            "2",
            "--text",
            "new",
            "--was",
            "expected\n",
        ])
        .assert()
        .failure()
        .code(3); // EXIT_CHECK
    // 不写盘
    assert_eq!(common::read(&path), b"a\nactual\nc\n");
}

#[test]
fn replace_dry_run_prints_to_stdout_and_does_not_write() {
    let (_dir, path) = common::tempfile_with(b"a\nb\n");
    Command::cargo_bin("xnip")
        .unwrap()
        .args([
            "replace",
            path.to_str().unwrap(),
            "--lines",
            "1",
            "--text",
            "X",
            "--dry-run",
        ])
        .assert()
        .success()
        .stdout("X\nb\n");
    assert_eq!(common::read(&path), b"a\nb\n");
}

#[test]
fn replace_revert_swaps_pattern_and_repl() {
    // forward: foo→bar
    let (_dir, path) = common::tempfile_with(b"foo\n");
    Command::cargo_bin("xnip")
        .unwrap()
        .args([
            "replace",
            path.to_str().unwrap(),
            "--pattern",
            "foo",
            "--repl",
            "bar",
        ])
        .assert()
        .success();
    assert_eq!(common::read(&path), b"bar\n");

    // revert: 同样 pattern/repl + --revert，应得 foo
    Command::cargo_bin("xnip")
        .unwrap()
        .args([
            "replace",
            path.to_str().unwrap(),
            "--pattern",
            "foo",
            "--repl",
            "bar",
            "--revert",
        ])
        .assert()
        .success();
    assert_eq!(common::read(&path), b"foo\n");
}

#[test]
fn replace_backup_writes_bak_when_flag_set() {
    let (_dir, path) = common::tempfile_with(b"a\nb\n");
    Command::cargo_bin("xnip")
        .unwrap()
        .args([
            "replace",
            path.to_str().unwrap(),
            "--lines",
            "1",
            "--text",
            "X",
            "--backup",
        ])
        .assert()
        .success();
    assert_eq!(common::read(&path), b"X\nb\n");
    let bak = path.with_extension("bak");
    assert!(bak.exists(), "expected .bak to be created with --backup");
    assert_eq!(common::read(&bak), b"a\nb\n");
}
