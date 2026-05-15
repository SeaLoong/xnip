//! M4 集成测试：doctor + apply --json + 全局参数。

mod common;

use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn doctor_prints_version_and_platform() {
    Command::cargo_bin("xnip")
        .unwrap()
        .arg("doctor")
        .assert()
        .success()
        .stdout(predicate::str::starts_with("xnip "))
        .stdout(predicate::str::contains("os:"))
        .stdout(predicate::str::contains("arch:"));
}

#[test]
fn doctor_includes_tty_flags() {
    Command::cargo_bin("xnip")
        .unwrap()
        .arg("doctor")
        .assert()
        .success()
        .stdout(predicate::str::contains("stdin-tty:"))
        .stdout(predicate::str::contains("stdout-tty:"));
}

#[test]
fn apply_json_emits_start_and_done_events() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("a.txt");
    std::fs::write(&target, b"a\n").unwrap();
    let m = dir.path().join("edits.txt");
    std::fs::write(&m, format!(r#"replace {} 1 "X""#, target.display())).unwrap();

    Command::cargo_bin("xnip")
        .unwrap()
        .args(["apply", m.to_str().unwrap(), "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"event\":\"start\""))
        .stdout(predicate::str::contains("\"event\":\"done\""));
    assert_eq!(common::read(&target), b"X\n");
}

#[test]
fn apply_json_emits_error_event_on_phase1_failure() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("a.txt");
    std::fs::write(&target, b"a\n").unwrap();
    let m = dir.path().join("edits.txt");
    std::fs::write(&m, format!(r#"replace {} 100-200 "X""#, target.display())).unwrap();

    Command::cargo_bin("xnip")
        .unwrap()
        .args(["apply", m.to_str().unwrap(), "--json"])
        .assert()
        .failure()
        .code(3)
        .stdout(predicate::str::contains("\"event\":\"start\""))
        .stdout(predicate::str::contains("\"event\":\"error\""))
        .stdout(predicate::str::contains("\"kind\":\"phase1\""));
}

#[test]
fn doctor_via_command_help_shows_in_global_help() {
    Command::cargo_bin("xnip")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("doctor"));
}
