//! `xnip apply` 集成测试。

mod common;

use assert_cmd::Command;
use predicates::prelude::*;

fn write_manifest(dir: &std::path::Path, name: &str, body: &str) -> std::path::PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, body).unwrap();
    p
}

#[test]
fn apply_native_single_replace() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("a.txt");
    std::fs::write(&target, b"a\nb\nc\n").unwrap();
    let m = write_manifest(
        dir.path(),
        "edits.txt",
        &format!(r#"replace {} 2 "B""#, target.display()),
    );
    Command::cargo_bin("xnip")
        .unwrap()
        .args(["apply", m.to_str().unwrap()])
        .assert()
        .success();
    assert_eq!(common::read(&target), b"a\nB\nc\n");
}

#[test]
fn apply_json_single_replace() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("a.txt");
    std::fs::write(&target, b"a\nb\nc\n").unwrap();
    let body = serde_json::json!([
        {"op":"replace","file":target.to_str().unwrap(),"lines":"2","text":"B"}
    ])
    .to_string();
    let m = write_manifest(dir.path(), "edits.json", &body);
    Command::cargo_bin("xnip")
        .unwrap()
        .args(["apply", m.to_str().unwrap()])
        .assert()
        .success();
    assert_eq!(common::read(&target), b"a\nB\nc\n");
}

#[test]
fn apply_yaml_single_replace() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("a.txt");
    std::fs::write(&target, b"a\nb\nc\n").unwrap();
    let body = format!(
        "- op: replace\n  file: {}\n  lines: \"2\"\n  text: B\n",
        target.display()
    );
    let m = write_manifest(dir.path(), "edits.yaml", &body);
    Command::cargo_bin("xnip")
        .unwrap()
        .args(["apply", m.to_str().unwrap()])
        .assert()
        .success();
    assert_eq!(common::read(&target), b"a\nB\nc\n");
}

#[test]
fn apply_three_formats_produce_equivalent_results() {
    // PLAN G6：三格式等价性验证（独立运行三次，比较最终文件内容）
    fn run_one(fmt: &str, body: &str) -> Vec<u8> {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("x.txt");
        std::fs::write(&target, b"a\nb\nc\nd\n").unwrap();
        let manifest_name = match fmt {
            "native" => "edits.txt",
            "json" => "edits.json",
            "yaml" => "edits.yaml",
            _ => unreachable!(),
        };
        let m = write_manifest(
            dir.path(),
            manifest_name,
            &body.replace("__TARGET__", target.to_str().unwrap()),
        );
        Command::cargo_bin("xnip")
            .unwrap()
            .args(["apply", m.to_str().unwrap()])
            .assert()
            .success();
        std::fs::read(&target).unwrap()
    }

    let native_body = r#"replace __TARGET__ 2 "B""#.to_string();
    let json_body = serde_json::json!([
        {"op":"replace","file":"__TARGET__","lines":"2","text":"B"}
    ])
    .to_string();
    let yaml_body = "- op: replace\n  file: __TARGET__\n  lines: \"2\"\n  text: B\n".to_string();

    let n = run_one("native", &native_body);
    let j = run_one("json", &json_body);
    let y = run_one("yaml", &yaml_body);
    assert_eq!(n, j, "native vs json must produce same result");
    assert_eq!(j, y, "json vs yaml must produce same result");
    assert_eq!(n, b"a\nB\nc\nd\n");
}

#[test]
fn apply_check_does_not_modify_files() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("a.txt");
    std::fs::write(&target, b"orig\n").unwrap();
    let m = write_manifest(
        dir.path(),
        "edits.txt",
        &format!(r#"replace {} 1 "new""#, target.display()),
    );
    Command::cargo_bin("xnip")
        .unwrap()
        .args(["apply", m.to_str().unwrap(), "--check"])
        .assert()
        .success()
        .stdout(predicate::str::contains("OK"));
    assert_eq!(common::read(&target), b"orig\n");
}

#[test]
fn apply_dry_run_emits_diff_no_write() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("a.txt");
    std::fs::write(&target, b"a\nb\n").unwrap();
    let m = write_manifest(
        dir.path(),
        "edits.txt",
        &format!(r#"replace {} 1 "X""#, target.display()),
    );
    Command::cargo_bin("xnip")
        .unwrap()
        .args(["apply", m.to_str().unwrap(), "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains("---"))
        .stdout(predicate::str::contains("+++"))
        .stdout(predicate::str::contains("-a"))
        .stdout(predicate::str::contains("+X"));
    assert_eq!(common::read(&target), b"a\nb\n");
}

#[test]
fn apply_phase1_failure_returns_exit_3() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("a.txt");
    std::fs::write(&target, b"a\n").unwrap();
    let m = write_manifest(
        dir.path(),
        "edits.txt",
        &format!(r#"replace {} 100-200 "X""#, target.display()),
    );
    Command::cargo_bin("xnip")
        .unwrap()
        .args(["apply", m.to_str().unwrap()])
        .assert()
        .failure()
        .code(3);
    assert_eq!(common::read(&target), b"a\n");
}

#[test]
fn apply_multiple_ops_same_file() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("a.txt");
    std::fs::write(&target, b"1\n2\n3\n4\n5\n").unwrap();
    let body = format!(
        "replace {} 2 \"X\"\nreplace {} 4 \"Y\"\n",
        target.display(),
        target.display()
    );
    let m = write_manifest(dir.path(), "edits.txt", &body);
    Command::cargo_bin("xnip")
        .unwrap()
        .args(["apply", m.to_str().unwrap()])
        .assert()
        .success();
    assert_eq!(common::read(&target), b"1\nX\n3\nY\n5\n");
}

#[test]
fn apply_backup_writes_bak() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("a.txt");
    std::fs::write(&target, b"v1\n").unwrap();
    let m = write_manifest(
        dir.path(),
        "edits.txt",
        &format!(r#"replace {} 1 "v2""#, target.display()),
    );
    Command::cargo_bin("xnip")
        .unwrap()
        .args(["apply", m.to_str().unwrap(), "--backup"])
        .assert()
        .success();
    assert_eq!(common::read(&target), b"v2\n");
    let bak = target.with_extension("bak");
    assert!(bak.exists());
    assert_eq!(common::read(&bak), b"v1\n");
}

#[test]
fn apply_subst_pattern() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("a.txt");
    std::fs::write(&target, b"foo bar foo\n").unwrap();
    let m = write_manifest(
        dir.path(),
        "edits.txt",
        &format!("replace {} s/foo/BAZ/g\n", target.display()),
    );
    Command::cargo_bin("xnip")
        .unwrap()
        .args(["apply", m.to_str().unwrap()])
        .assert()
        .success();
    assert_eq!(common::read(&target), b"BAZ bar BAZ\n");
}

#[test]
fn apply_via_stdin() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("a.txt");
    std::fs::write(&target, b"a\nb\n").unwrap();
    let body = format!(r#"replace {} 1 "X""#, target.display());
    Command::cargo_bin("xnip")
        .unwrap()
        .args(["apply", "--from-stdin"])
        .write_stdin(body)
        .assert()
        .success();
    assert_eq!(common::read(&target), b"X\nb\n");
}

#[test]
fn apply_format_explicit_json() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("a.txt");
    std::fs::write(&target, b"a\n").unwrap();
    let body = serde_json::json!([
        {"op":"replace","file":target.to_str().unwrap(),"lines":"1","text":"X"}
    ])
    .to_string();
    // 后缀骗 detector 是 native；显式指定 json
    let m = write_manifest(dir.path(), "edits.unknown", &body);
    Command::cargo_bin("xnip")
        .unwrap()
        .args(["apply", m.to_str().unwrap(), "--format", "json"])
        .assert()
        .success();
    assert_eq!(common::read(&target), b"X\n");
}
