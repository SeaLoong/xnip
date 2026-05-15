//! `xnip peek` 集成测试。

mod common;

use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn peek_lines_basic() {
    let (_dir, path) = common::tempfile_with(b"a\nb\nc\nd\n");
    Command::cargo_bin("xnip")
        .unwrap()
        .args(["peek", path.to_str().unwrap(), "--lines", "2-3"])
        .assert()
        .success()
        .stdout("     2: b\n     3: c\n");
}

#[test]
fn peek_lines_single() {
    let (_dir, path) = common::tempfile_with(b"a\nb\nc\n");
    Command::cargo_bin("xnip")
        .unwrap()
        .args(["peek", path.to_str().unwrap(), "--lines", "1"])
        .assert()
        .success()
        .stdout("     1: a\n");
}

#[test]
fn peek_all_outputs_full_file() {
    let (_dir, path) = common::tempfile_with(b"x\ny\nz\n");
    Command::cargo_bin("xnip")
        .unwrap()
        .args(["peek", path.to_str().unwrap(), "--all"])
        .assert()
        .success()
        .stdout("     1: x\n     2: y\n     3: z\n");
}

#[test]
fn peek_match_line_with_context() {
    let (_dir, path) = common::tempfile_with(b"a\nfoo\nb\nc\n");
    Command::cargo_bin("xnip")
        .unwrap()
        .args([
            "peek",
            path.to_str().unwrap(),
            "--match-line",
            "^foo",
            "--context",
            "1",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("     1: a"))
        .stdout(predicate::str::contains("     2: foo"))
        .stdout(predicate::str::contains("     3: b"));
}

#[test]
fn peek_lines_out_of_bounds_returns_usage() {
    let (_dir, path) = common::tempfile_with(b"a\n");
    Command::cargo_bin("xnip")
        .unwrap()
        .args(["peek", path.to_str().unwrap(), "--lines", "100"])
        .assert()
        .failure()
        .code(1);
}

#[test]
fn peek_max_lines_truncates_with_stderr_notice() {
    let (_dir, path) = common::tempfile_with(b"1\n2\n3\n4\n5\n");
    Command::cargo_bin("xnip")
        .unwrap()
        .args(["peek", path.to_str().unwrap(), "--all", "--max-lines", "2"])
        .assert()
        .success()
        .stdout("     1: 1\n     2: 2\n")
        .stderr(predicate::str::contains("truncated"));
}

#[test]
fn peek_no_range_errors() {
    let (_dir, path) = common::tempfile_with(b"a\n");
    Command::cargo_bin("xnip")
        .unwrap()
        .args(["peek", path.to_str().unwrap()])
        .assert()
        .failure()
        .code(1);
}

#[test]
fn peek_conflicting_ranges_errors() {
    let (_dir, path) = common::tempfile_with(b"a\nb\n");
    Command::cargo_bin("xnip")
        .unwrap()
        .args(["peek", path.to_str().unwrap(), "--lines", "1", "--all"])
        .assert()
        .failure()
        .code(1);
}
