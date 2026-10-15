//! 全局 flag 集成测试：`--quiet` / `--no-color` / `--trace`。

mod common;

use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn quiet_suppresses_status_eprintln_on_replace_check() {
    // replace --check 成功时会 `note!("N match(es) would be replaced")`；--quiet 时应无此 stderr
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("a.txt");
    std::fs::write(&p, b"foo foo\n").unwrap();

    Command::cargo_bin("xnip")
        .unwrap()
        .args([
            "replace",
            p.to_str().unwrap(),
            "--pattern",
            "foo",
            "--repl",
            "bar",
            "--check",
            "--quiet",
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains("would be replaced").not());
}

#[test]
fn without_quiet_replace_check_emits_status() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("a.txt");
    std::fs::write(&p, b"foo\n").unwrap();

    Command::cargo_bin("xnip")
        .unwrap()
        .args([
            "replace",
            p.to_str().unwrap(),
            "--pattern",
            "foo",
            "--repl",
            "bar",
            "--check",
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains("would be replaced"));
}

#[test]
fn trace_emits_trace_prefix_to_stderr() {
    // apply 走 execute，execute 开头有 trace! 点
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("a.txt");
    std::fs::write(&target, b"a\n").unwrap();
    let m = dir.path().join("edits.txt");
    std::fs::write(&m, format!(r#"replace {} 1 "X""#, target.display())).unwrap();

    Command::cargo_bin("xnip")
        .unwrap()
        .args(["--trace", "apply", m.to_str().unwrap()])
        .assert()
        .success()
        .stderr(predicate::str::contains("[xnip trace]"));
}

#[test]
fn no_trace_flag_no_trace_lines() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("a.txt");
    std::fs::write(&target, b"a\n").unwrap();
    let m = dir.path().join("edits.txt");
    std::fs::write(&m, format!(r#"replace {} 1 "X""#, target.display())).unwrap();

    Command::cargo_bin("xnip")
        .unwrap()
        .args(["apply", m.to_str().unwrap()])
        .assert()
        .success()
        .stderr(predicate::str::contains("[xnip trace]").not());
}

#[test]
fn no_color_flag_accepted_globally() {
    // doctor 不输出彩色（设计上只在 apply --dry-run 才有），加 --no-color 不应报错
    Command::cargo_bin("xnip")
        .unwrap()
        .args(["--no-color", "doctor"])
        .assert()
        .success();
}

#[test]
fn global_flags_work_after_subcommand_too() {
    // clap global = true 允许位置在子命令后
    Command::cargo_bin("xnip")
        .unwrap()
        .args(["doctor", "--trace"])
        .assert()
        .success()
        .stderr(predicate::str::contains("[xnip trace]"));
}

// ------- note! 在每个写命令成功后输出，--quiet 抑制 -------

#[test]
fn replace_range_emits_wrote_note_and_quiet_suppresses() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("a.txt");
    std::fs::write(&p, b"x\n").unwrap();

    // 默认：有 wrote note
    Command::cargo_bin("xnip")
        .unwrap()
        .args([
            "replace",
            p.to_str().unwrap(),
            "--lines",
            "1",
            "--text",
            "y",
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains("xnip replace: wrote"));

    // --quiet：无 wrote note
    std::fs::write(&p, b"x\n").unwrap();
    Command::cargo_bin("xnip")
        .unwrap()
        .args([
            "replace",
            p.to_str().unwrap(),
            "--lines",
            "1",
            "--text",
            "y",
            "--quiet",
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains("xnip replace: wrote").not());
}

#[test]
fn insert_emits_wrote_note() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("a.txt");
    std::fs::write(&p, b"a\n").unwrap();
    Command::cargo_bin("xnip")
        .unwrap()
        .args([
            "insert",
            p.to_str().unwrap(),
            "--lines",
            "1",
            "--position",
            "after",
            "--text",
            "b",
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains("xnip insert: wrote"));
}

#[test]
fn move_emits_wrote_note() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("a.txt");
    std::fs::write(&p, b"a\nb\nc\n").unwrap();
    Command::cargo_bin("xnip")
        .unwrap()
        .args([
            "move",
            p.to_str().unwrap(),
            "--from-lines",
            "1",
            "--to",
            "3",
            "--position",
            "after",
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains("xnip move: wrote"));
}

#[test]
fn indent_emits_wrote_note() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("a.txt");
    std::fs::write(&p, b"a\n").unwrap();
    Command::cargo_bin("xnip")
        .unwrap()
        .args(["indent", p.to_str().unwrap(), "--all", "--add", "2"])
        .assert()
        .success()
        .stderr(predicate::str::contains("xnip indent: wrote"));
}

#[test]
fn apply_emits_committed_note_and_quiet_suppresses() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("a.txt");
    std::fs::write(&target, b"x\n").unwrap();
    let m = dir.path().join("edits.txt");
    std::fs::write(&m, format!(r#"replace {} 1 "Y""#, target.display())).unwrap();

    Command::cargo_bin("xnip")
        .unwrap()
        .args(["apply", m.to_str().unwrap()])
        .assert()
        .success()
        .stderr(predicate::str::contains("Updated: 1 path(s)"));

    // 重置 + --quiet 抑制
    std::fs::write(&target, b"x\n").unwrap();
    Command::cargo_bin("xnip")
        .unwrap()
        .args(["--quiet", "apply", m.to_str().unwrap()])
        .assert()
        .success()
        .stderr(predicate::str::contains("Updated:").not());
}

// ------- 彩色 diff（apply --dry-run）-------

#[test]
fn apply_dry_run_no_color_when_stdout_is_pipe() {
    // assert_cmd 里 stdout 不是 TTY → 即使没有 --no-color，也应返回纯 diff
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("a.txt");
    std::fs::write(&target, b"orig\n").unwrap();
    let m = dir.path().join("edits.txt");
    std::fs::write(&m, format!(r#"replace {} 1 "NEW""#, target.display())).unwrap();

    let out = Command::cargo_bin("xnip")
        .unwrap()
        .args(["apply", m.to_str().unwrap(), "--dry-run"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8(out).unwrap();
    assert!(s.contains("--- "), "expect plain diff header, got: {s:?}");
    assert!(!s.contains("\x1b["), "must not emit ANSI on pipe: {s:?}");
}

#[test]
fn apply_dry_run_no_color_flag_disables_color() {
    // 显式 --no-color 也要走纯 diff
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("a.txt");
    std::fs::write(&target, b"orig\n").unwrap();
    let m = dir.path().join("edits.txt");
    std::fs::write(&m, format!(r#"replace {} 1 "NEW""#, target.display())).unwrap();

    let out = Command::cargo_bin("xnip")
        .unwrap()
        .args(["--no-color", "apply", m.to_str().unwrap(), "--dry-run"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8(out).unwrap();
    assert!(!s.contains("\x1b["), "must not emit ANSI: {s:?}");
}

#[test]
fn edit_preview_keeps_stdout_complete_and_quiet_only_suppresses_summary() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sample.txt");
    std::fs::write(&path, "old\n").unwrap();
    for quiet in [false, true] {
        let mut cmd = Command::cargo_bin("xnip").unwrap();
        cmd.arg("edit").arg(&path).args([
            "--old",
            "old",
            "--text",
            "new",
            "--dry-run",
            "--no-color",
        ]);
        if quiet {
            cmd.arg("--quiet");
        }
        let output = cmd.assert().success().get_output().clone();
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(stdout.contains("-old\n+new\n"));
        assert!(!stdout.contains('\x1b'));
        assert!(!stdout.contains("Preview"));
        if quiet {
            assert_eq!(output.stderr, [] as [u8; 0]);
        } else {
            assert!(
                String::from_utf8(output.stderr)
                    .unwrap()
                    .starts_with("Preview (no files written):")
            );
        }
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "old\n");
    }
}

#[test]
fn apply_check_and_json_keep_their_machine_readable_stdout() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sample.txt");
    let manifest = dir.path().join("batch.json");
    std::fs::write(
        &manifest,
        serde_json::to_vec(&serde_json::json!([
            {"op":"create","file":path,"text":"hello"}
        ]))
        .unwrap(),
    )
    .unwrap();
    Command::cargo_bin("xnip")
        .unwrap()
        .arg("apply")
        .arg(&manifest)
        .arg("--check")
        .assert()
        .success()
        .stdout("OK\n")
        .stderr(predicate::str::contains("Check passed"));
    let output = Command::cargo_bin("xnip")
        .unwrap()
        .arg("apply")
        .arg(&manifest)
        .args(["--json", "--check"])
        .assert()
        .success()
        .get_output()
        .clone();
    assert_eq!(output.stderr, [] as [u8; 0]);
    for line in String::from_utf8(output.stdout).unwrap().lines() {
        serde_json::from_str::<serde_json::Value>(line).unwrap();
    }
    assert!(!path.exists());
}
