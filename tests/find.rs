//! `xnip find` 集成测试。

mod common;

use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn find_match_line_basic() {
    let (_dir, path) = common::tempfile_with(b"a\nfoo\nb\nfoo\n");
    let p = path.to_str().unwrap();
    Command::cargo_bin("xnip")
        .unwrap()
        .args(["find", p, "--match-line", "^foo"])
        .assert()
        .success()
        .stdout(format!("{p}:2\n{p}:4\n"));
}

#[test]
fn find_pattern_emits_col() {
    let (_dir, path) = common::tempfile_with(b"abc foo def\n");
    let p = path.to_str().unwrap();
    Command::cargo_bin("xnip")
        .unwrap()
        .args(["find", p, "--pattern", "foo"])
        .assert()
        .success()
        .stdout(format!("{p}:1:5\n"));
}

#[test]
fn find_no_match_returns_usage() {
    let (_dir, path) = common::tempfile_with(b"a\nb\n");
    let p = path.to_str().unwrap();
    Command::cargo_bin("xnip")
        .unwrap()
        .args(["find", p, "--match-line", "nope"])
        .assert()
        .failure()
        .code(1);
}

#[test]
fn find_max_matches_caps_output() {
    let (_dir, path) = common::tempfile_with(b"foo\nfoo\nfoo\nfoo\n");
    let p = path.to_str().unwrap();
    Command::cargo_bin("xnip")
        .unwrap()
        .args(["find", p, "--match-line", "^foo", "--max-matches", "2"])
        .assert()
        .success()
        .stdout(format!("{p}:1\n{p}:2\n"));
}

#[test]
fn find_first_only_stops_at_one_per_file() {
    let (_dir, path) = common::tempfile_with(b"foo\nfoo\nfoo\n");
    let p = path.to_str().unwrap();
    Command::cargo_bin("xnip")
        .unwrap()
        .args(["find", p, "--match-line", "^foo", "--first-only"])
        .assert()
        .success()
        .stdout(format!("{p}:1\n"));
}

#[test]
fn find_requires_locator() {
    let (_dir, path) = common::tempfile_with(b"a\n");
    Command::cargo_bin("xnip")
        .unwrap()
        .args(["find", path.to_str().unwrap()])
        .assert()
        .failure()
        .code(1);
}

#[test]
fn find_pattern_and_match_line_conflict() {
    let (_dir, path) = common::tempfile_with(b"a\n");
    Command::cargo_bin("xnip")
        .unwrap()
        .args([
            "find",
            path.to_str().unwrap(),
            "--pattern",
            "a",
            "--match-line",
            "a",
        ])
        .assert()
        .failure()
        .code(1);
}

#[test]
fn find_across_multiple_files_preserves_order() {
    let dir = tempfile::tempdir().unwrap();
    let p1 = dir.path().join("a.txt");
    let p2 = dir.path().join("b.txt");
    std::fs::write(&p1, b"foo\n").unwrap();
    std::fs::write(&p2, b"foo\n").unwrap();

    Command::cargo_bin("xnip")
        .unwrap()
        .args([
            "find",
            p1.to_str().unwrap(),
            p2.to_str().unwrap(),
            "--match-line",
            "^foo",
        ])
        .assert()
        .success()
        .stdout(predicate::str::ends_with(format!(
            "{}:1\n{}:1\n",
            p1.display(),
            p2.display()
        )));
}
