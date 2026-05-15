//! M0 烟囱测试：保证二进制可启动、`--version` 与 `--help` 工作。

use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn version_flag_prints_version() {
    Command::cargo_bin("xnip")
        .unwrap()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::starts_with("xnip "));
}

#[test]
fn help_flag_lists_commands() {
    Command::cargo_bin("xnip")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("peek"))
        .stdout(predicate::str::contains("apply").or(predicate::str::contains("replace")));
}

#[test]
fn no_args_prints_help_successfully() {
    Command::cargo_bin("xnip").unwrap().assert().success();
}

#[test]
fn unknown_subcommand_returns_usage_exit_code() {
    Command::cargo_bin("xnip")
        .unwrap()
        .arg("nonexistent")
        .assert()
        .failure()
        .code(1) // EXIT_USAGE
        .stderr(
            predicate::str::contains("unrecognized").or(predicate::str::contains("unexpected")),
        );
}
