//! M6 增量：`@-` per-op stdin / `--parallel` / 全 op `--revert` 端到端测试。

mod common;

use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn apply_at_dash_consumes_process_stdin_for_op() {
    // 清单在文件里；op 用 `@-` 指代从本进程 stdin 读入 payload
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("a.txt");
    std::fs::write(&target, b"keep-this-line\n").unwrap();

    let m = dir.path().join("edits.txt");
    std::fs::write(&m, format!("replace {} 1 @-\n", target.display())).unwrap();

    Command::cargo_bin("xnip")
        .unwrap()
        .args(["apply", m.to_str().unwrap()])
        .write_stdin("INJECTED-FROM-STDIN")
        .assert()
        .success();
    assert_eq!(common::read(&target), b"INJECTED-FROM-STDIN\n");
}

#[test]
fn apply_at_dash_multiple_in_manifest_errors() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("a.txt");
    std::fs::write(&target, b"a\nb\n").unwrap();

    let m = dir.path().join("edits.txt");
    std::fs::write(
        &m,
        format!(
            "replace {} 1 @-\nreplace {} 2 @-\n",
            target.display(),
            target.display()
        ),
    )
    .unwrap();

    Command::cargo_bin("xnip")
        .unwrap()
        .args(["apply", m.to_str().unwrap()])
        .write_stdin("X")
        .assert()
        .failure()
        .code(3)
        .stderr(predicate::str::contains("at most once"));
    // 文件保持原样
    assert_eq!(common::read(&target), b"a\nb\n");
}

#[test]
fn apply_stdin_file_option_provides_payload() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("a.txt");
    std::fs::write(&target, b"before\n").unwrap();
    let payload = dir.path().join("payload.txt");
    std::fs::write(&payload, b"AFTER").unwrap();

    let m = dir.path().join("edits.txt");
    std::fs::write(&m, format!("replace {} 1 @-\n", target.display())).unwrap();

    Command::cargo_bin("xnip")
        .unwrap()
        .args([
            "apply",
            m.to_str().unwrap(),
            "--stdin-file",
            payload.to_str().unwrap(),
        ])
        .assert()
        .success();
    assert_eq!(common::read(&target), b"AFTER\n");
}

#[test]
fn apply_parallel_two_files() {
    let dir = tempfile::tempdir().unwrap();
    let p1 = dir.path().join("a.txt");
    let p2 = dir.path().join("b.txt");
    std::fs::write(&p1, b"1\n").unwrap();
    std::fs::write(&p2, b"2\n").unwrap();

    let m = dir.path().join("edits.txt");
    std::fs::write(
        &m,
        format!(
            "replace {} 1 \"A\"\nreplace {} 1 \"B\"\n",
            p1.display(),
            p2.display()
        ),
    )
    .unwrap();

    Command::cargo_bin("xnip")
        .unwrap()
        .args(["apply", m.to_str().unwrap(), "--parallel", "4"])
        .assert()
        .success();
    assert_eq!(common::read(&p1), b"A\n");
    assert_eq!(common::read(&p2), b"B\n");
}

#[test]
fn apply_parallel_phase1_failure_leaves_nothing_modified() {
    // 两个 op：一个正常，一个越界；即使并行，第一个也不应提交
    let dir = tempfile::tempdir().unwrap();
    let p1 = dir.path().join("a.txt");
    let p2 = dir.path().join("b.txt");
    std::fs::write(&p1, b"1\n").unwrap();
    std::fs::write(&p2, b"2\n").unwrap();

    let m = dir.path().join("edits.txt");
    std::fs::write(
        &m,
        format!(
            "replace {} 1 \"A\"\nreplace {} 100-200 \"B\"\n",
            p1.display(),
            p2.display()
        ),
    )
    .unwrap();

    Command::cargo_bin("xnip")
        .unwrap()
        .args(["apply", m.to_str().unwrap(), "--parallel", "4"])
        .assert()
        .failure()
        .code(3);
    // 两文件都应保持原样（两阶段提交保证）
    assert_eq!(common::read(&p1), b"1\n");
    assert_eq!(common::read(&p2), b"2\n");
}

#[test]
fn apply_parallel_consistency_with_sequential() {
    // 并行与串行结果一致性
    let dir = tempfile::tempdir().unwrap();
    let mut files = Vec::new();
    for i in 0..5 {
        let p = dir.path().join(format!("f{i}.txt"));
        std::fs::write(&p, b"orig\n").unwrap();
        files.push(p);
    }

    let body: String = files
        .iter()
        .map(|p| format!(r#"replace {} 1 "NEW""#, p.display()))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";

    // 并行
    let dir_par = tempfile::tempdir().unwrap();
    let mut files_par = Vec::new();
    for i in 0..5 {
        let p = dir_par.path().join(format!("f{i}.txt"));
        std::fs::write(&p, b"orig\n").unwrap();
        files_par.push(p);
    }
    let body_par: String = files_par
        .iter()
        .map(|p| format!(r#"replace {} 1 "NEW""#, p.display()))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    let m_par = dir_par.path().join("edits.txt");
    std::fs::write(&m_par, &body_par).unwrap();
    Command::cargo_bin("xnip")
        .unwrap()
        .args(["apply", m_par.to_str().unwrap(), "--parallel", "4"])
        .assert()
        .success();

    // 串行
    let m_seq = dir.path().join("edits.txt");
    std::fs::write(&m_seq, &body).unwrap();
    Command::cargo_bin("xnip")
        .unwrap()
        .args(["apply", m_seq.to_str().unwrap()])
        .assert()
        .success();

    for (a, b) in files.iter().zip(files_par.iter()) {
        assert_eq!(common::read(a), common::read(b));
        assert_eq!(common::read(a), b"NEW\n");
    }
}

// ----------------- 全 op --revert 端到端 -----------------

#[test]
fn cli_replace_range_revert_with_was_restores_original() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("a.txt");
    std::fs::write(&p, b"orig\nkeep\n").unwrap();

    // forward
    Command::cargo_bin("xnip")
        .unwrap()
        .args([
            "replace",
            p.to_str().unwrap(),
            "--lines",
            "1",
            "--text",
            "new",
        ])
        .assert()
        .success();
    assert_eq!(common::read(&p), b"new\nkeep\n");

    // revert (要求 --was 提供原内容)
    Command::cargo_bin("xnip")
        .unwrap()
        .args([
            "replace",
            p.to_str().unwrap(),
            "--lines",
            "1",
            "--text",
            "new",
            "--was",
            "orig\n",
            "--revert",
        ])
        .assert()
        .success();
    assert_eq!(common::read(&p), b"orig\nkeep\n");
}

#[test]
fn cli_insert_revert_deletes_inserted_line() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("a.txt");
    std::fs::write(&p, b"a\nc\n").unwrap();

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
        .success();
    assert_eq!(common::read(&p), b"a\nb\nc\n");

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
            "--revert",
        ])
        .assert()
        .success();
    assert_eq!(common::read(&p), b"a\nc\n");
}

#[test]
fn cli_move_revert_restores_original_order() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("a.txt");
    std::fs::write(&p, b"a\nb\nc\nd\ne\n").unwrap();

    // forward: 1-2 → after 5
    Command::cargo_bin("xnip")
        .unwrap()
        .args([
            "move",
            p.to_str().unwrap(),
            "--from-lines",
            "1-2",
            "--to",
            "5",
            "--position",
            "after",
        ])
        .assert()
        .success();
    assert_eq!(common::read(&p), b"c\nd\ne\na\nb\n");

    // revert：用相同参数 + --revert
    Command::cargo_bin("xnip")
        .unwrap()
        .args([
            "move",
            p.to_str().unwrap(),
            "--from-lines",
            "1-2",
            "--to",
            "5",
            "--position",
            "after",
            "--revert",
        ])
        .assert()
        .success();
    assert_eq!(common::read(&p), b"a\nb\nc\nd\ne\n");
}

#[test]
fn cli_indent_revert_add_remove_pair() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("a.txt");
    std::fs::write(&p, b"a\nb\n").unwrap();

    Command::cargo_bin("xnip")
        .unwrap()
        .args(["indent", p.to_str().unwrap(), "--all", "--add", "2"])
        .assert()
        .success();
    assert_eq!(common::read(&p), b"  a\n  b\n");

    Command::cargo_bin("xnip")
        .unwrap()
        .args([
            "indent",
            p.to_str().unwrap(),
            "--all",
            "--add",
            "2",
            "--revert",
        ])
        .assert()
        .success();
    assert_eq!(common::read(&p), b"a\nb\n");
}

#[test]
fn cli_move_revert_with_match_line_is_rejected() {
    // --from-match-line --revert 在 forward 后文件里 resolve 出的不是原源块，
    // 这种组合无法正确反向；应被显式拒绝。
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("a.txt");
    std::fs::write(&p, b"a\nb\nc\n").unwrap();

    Command::cargo_bin("xnip")
        .unwrap()
        .args([
            "move",
            p.to_str().unwrap(),
            "--from-match-line",
            "^a$",
            "--to",
            "3",
            "--position",
            "after",
            "--revert",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--from-lines"));
    // 文件保持原样
    assert_eq!(common::read(&p), b"a\nb\nc\n");
}

#[test]
fn apply_without_at_dash_does_not_consume_unrelated_stdin() {
    // Lazy stdin：manifest 没有 @-，进程 stdin 不应被读取。
    // 喂大量字节也不会卡住或报错；命令应像没读 stdin 一样按时返回。
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("a.txt");
    std::fs::write(&target, b"orig\n").unwrap();

    let m = dir.path().join("edits.txt");
    std::fs::write(&m, format!(r#"replace {} 1 "NEW""#, target.display())).unwrap();

    // 喂一个非空 stdin（无 @- 的 manifest 不应消费它）
    Command::cargo_bin("xnip")
        .unwrap()
        .args(["apply", m.to_str().unwrap()])
        .write_stdin("THIS-SHOULD-BE-IGNORED")
        .assert()
        .success();
    assert_eq!(common::read(&target), b"NEW\n");
}

// ----------------- 原生清单 @-/between literal/revert -----------------

#[test]
fn apply_native_between_literal_full_path() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("a.txt");
    std::fs::write(&p, b"head\n// BEGIN\nold1\nold2\n// END\ntail\n").unwrap();

    let m = dir.path().join("edits.txt");
    std::fs::write(
        &m,
        format!(r#"replace {} "// BEGIN".."// END" "NEW""#, p.display()) + "\n",
    )
    .unwrap();

    Command::cargo_bin("xnip")
        .unwrap()
        .args(["apply", m.to_str().unwrap()])
        .assert()
        .success();
    // between literal 不 inclusive → 只替换两锚点之间（old1/old2 两行），锚点保留
    assert_eq!(common::read(&p), b"head\n// BEGIN\nNEW\n// END\ntail\n");
}

#[test]
fn apply_native_between_literal_inclusive_full_path() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("a.txt");
    std::fs::write(&p, b"head\n// BEGIN\nold\n// END\ntail\n").unwrap();

    let m = dir.path().join("edits.txt");
    std::fs::write(
        &m,
        format!(r#"replace {} "// BEGIN".."// END"i """#, p.display()) + "\n",
    )
    .unwrap();

    Command::cargo_bin("xnip")
        .unwrap()
        .args(["apply", m.to_str().unwrap()])
        .assert()
        .success();
    // inclusive → 把两锚点和中间都删掉
    assert_eq!(common::read(&p), b"head\ntail\n");
}
