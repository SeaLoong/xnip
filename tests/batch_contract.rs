//! Batch planning, lifecycle, persistence and recovery use public CLI contracts.
mod common;
use common::{Workspace, utf8};
use serde_json::json;

#[test]
fn all_manifest_formats_have_equivalent_edit_and_lifecycle_semantics() {
    for (format, source) in [
        (
            "native",
            "replace old 1 \"NEW\"\ncreate nested/a \"first\"\nedit nested/a \"first\" \"last\"\nrename nested/a result\ndelete obsolete\n",
        ),
        (
            "json",
            r#"[{"op":"replace","file":"old","lines":1,"text":"NEW"},{"op":"create","file":"nested/a","text":"first"},{"op":"edit","file":"nested/a","old":"first","text":"last"},{"op":"rename","file":"nested/a","to-file":"result"},{"op":"delete","file":"obsolete"}]"#,
        ),
        (
            "yaml",
            "- op: replace\n  file: old\n  lines: 1\n  text: NEW\n- op: create\n  file: nested/a\n  text: first\n- op: edit\n  file: nested/a\n  old: first\n  text: last\n- op: rename\n  file: nested/a\n  to-file: result\n- op: delete\n  file: obsolete\n",
        ),
    ] {
        let w = Workspace::new(&[("old", b"old\nkeep\n"), ("obsolete", b"delete")]);
        for mode in ["--dry-run", "--check"] {
            w.apply(format, source).arg(mode).assert().success();
            w.assert_bytes("old", b"old\nkeep\n");
            w.assert_bytes("obsolete", b"delete");
            w.absent("nested");
            w.absent("result");
        }
        w.apply(format, source).assert().success();
        w.assert_bytes("old", b"NEW\nkeep\n");
        w.assert_bytes("result", b"last");
        w.absent("obsolete");
        w.absent("nested/a");
    }
}

#[test]
fn late_invalid_operations_prevent_every_write_and_parent_creation() {
    for parallel in ["1", "8"] {
        for bad in [
            json!({"op":"delete","file":"a","was":"wrong"}),
            json!({"op":"replace","file":"a","lines":99,"text":"bad"}),
            json!({"op":"replace","file":"a","pattern":"missing","repl":"bad"}),
            json!({"op":"rename","file":"a","to-file":"occupied"}),
        ] {
            let w = Workspace::new(&[("a", b"old\n"), ("occupied", b"keep")]);
            w.batch(&json!([
                {"op":"create","file":"nested/new","text":"new"},
                {"op":"edit","file":"occupied","old":"keep","text":"changed"},
                bad
            ]))
            .args(["--parallel", parallel])
            .assert()
            .code(3);
            w.assert_bytes("a", b"old\n");
            w.assert_bytes("occupied", b"keep");
            w.absent("nested");
        }
    }
}

#[test]
fn parallel_and_serial_planning_produce_the_same_bytes_for_all_operations() {
    for parallel in ["0", "1", "8"] {
        let w = Workspace::new(&[
            ("a", b"one\ntwo\nthree\n"),
            ("b", b"one\ntwo\nthree\n"),
            ("c", b"one\ntwo\nthree\n"),
            ("d", b"one\ntwo\nthree\n"),
        ]);
        w.batch(&json!([
            {"op":"replace","file":"a","lines":2,"text":"TWO"},
            {"op":"insert","file":"b","lines":1,"text":"extra","where":"before"},
            {"op":"move","file":"c","lines":1,"to":3,"where":"after"},
            {"op":"indent","file":"d","lines":"1-2","by":2}
        ]))
        .args(["--parallel", parallel, "--backup"])
        .assert()
        .success();
        w.assert_bytes("a", b"one\nTWO\nthree\n");
        w.assert_bytes("b", b"extra\none\ntwo\nthree\n");
        w.assert_bytes("c", b"two\nthree\none\n");
        w.assert_bytes("d", b"  one\n  two\nthree\n");
        for file in ["a.bak", "b.bak", "c.bak", "d.bak"] {
            w.assert_bytes(file, b"one\ntwo\nthree\n");
        }
    }
}

#[test]
fn ordinary_batches_resolve_anchors_and_line_offsets_against_original_bytes() {
    let w = Workspace::new(&[("a", b"a\nx\nb\nx\n")]);
    w.batch(&json!([
        {"op":"replace","file":"a","match-line":"^x$","occurrence":2,"text":"X"},
        {"op":"replace","file":"a","lines":3,"text":"x"},
        {"op":"insert","file":"a","lines":1,"text":"inserted"}
    ]))
    .assert()
    .success();
    w.assert_bytes("a", b"a\ninserted\nx\nx\nX\n");
}

#[test]
fn canonical_file_aliases_share_one_plan_in_parallel() {
    let w = Workspace::new(&[("a", b"a\nb\n")]);
    w.batch(&json!([
        {"op":"replace","file":"./a","lines":1,"text":"A"},
        {"op":"replace","file":w.path("a"),"lines":2,"text":"B"}
    ]))
    .args(["--parallel", "8"])
    .assert()
    .success();
    w.assert_bytes("a", b"A\nB\n");
}

#[test]
fn dependent_legacy_edits_require_explicit_sequential_planning() {
    let w = Workspace::new(&[("a", b"one\ntwo\n")]);
    let manifest = json!([
        {"op":"replace","file":"a","pattern":"one","repl":"first"},
        {"op":"replace","file":"a","match-line":"^first$","text":"done"}
    ]);
    w.batch(&manifest).assert().code(3);
    w.assert_bytes("a", b"one\ntwo\n");
    w.batch(&manifest).arg("--sequential").assert().success();
    w.assert_bytes("a", b"done\ntwo\n");
}

#[test]
fn overlapping_and_mixed_plans_are_rejected_without_partial_edits() {
    for first in [
        json!({"op":"replace","file":"a","pattern":"a","repl":"A"}),
        json!({"op":"move","file":"a","lines":1,"to":2}),
        json!({"op":"replace","file":"a","lines":"1-2","text":"x"}),
    ] {
        let w = Workspace::new(&[("a", b"a\nb\n")]);
        w.batch(&json!([first, {"op":"replace","file":"a","lines":2,"text":"B"}]))
            .assert()
            .code(3);
        w.assert_bytes("a", b"a\nb\n");
    }
}

#[test]
fn explicit_format_native_substitution_and_pattern_preconditions_reach_execution() {
    let w = Workspace::new(&[("a", b"foo bar foo\n")]);
    w.apply("native", "replace a s/foo/BAZ/g\n")
        .assert()
        .success();
    w.assert_bytes("a", b"BAZ bar BAZ\n");
    w.write(
        "misleading.unknown",
        br#"[{"op":"replace","file":"a","pattern":"BAZ","repl":"new"}]"#,
    );
    w.cli(&["apply", "misleading.unknown", "--format", "json"])
        .assert()
        .success();
    w.assert_bytes("a", b"new bar new\n");
    w.batch(&json!([{"op":"replace","file":"a","pattern":"new","repl":"bad","was":"wrong"}]))
        .assert()
        .code(3);
    w.assert_bytes("a", b"new bar new\n");
    w.batch(&json!([])).assert().success();
    w.assert_bytes("a", b"new bar new\n");
}

#[test]
fn backup_collision_validation_happens_before_any_input_is_overwritten() {
    let w = Workspace::new(&[("a", b"old"), ("a.bak", b"input")]);
    w.batch(&json!([
        {"op":"edit","file":"a","old":"old","text":"new"},
        {"op":"edit","file":"a.bak","old":"input","text":"changed"}
    ]))
    .arg("--backup")
    .assert()
    .code(3);
    w.assert_bytes("a", b"old");
    w.assert_bytes("a.bak", b"input");
}

#[test]
fn manifest_path_stdin_and_payload_sources_remain_independent() {
    for payload_file in [false, true] {
        let w = Workspace::new(&[
            ("a", b"old\n"),
            ("manifest", b"replace a 1 @-\n"),
            ("payload", b"NEW"),
        ]);
        let mut command = w.cli(&["apply", "manifest"]);
        if payload_file {
            command.args(["--stdin-file", "payload"]);
        } else {
            command.write_stdin("NEW");
        }
        command.assert().success();
        w.assert_bytes("a", b"NEW\n");
    }
    let w = Workspace::new(&[
        ("a", b"a\nb\n"),
        ("manifest", b"replace a 1 @-\nreplace a 2 @-\n"),
    ]);
    w.cli(&["apply", "manifest"])
        .write_stdin("NEW")
        .assert()
        .code(3);
    w.assert_bytes("a", b"a\nb\n");
    w.write("manifest", b"replace a 1 \"NEW\"\n");
    w.cli(&["apply", "manifest"])
        .write_stdin("unrelated input")
        .assert()
        .success();
    w.assert_bytes("a", b"NEW\nb\n");
}

#[test]
fn format_inference_and_native_marker_ranges_are_executable() {
    let w = Workspace::new(&[("a", b"head\nBEGIN\nold\nEND\ntail\n")]);
    w.write(
        "edits.json",
        br#"[{"op":"replace","file":"a","between":["BEGIN","END"],"text":"NEW"}]"#,
    );
    w.cli(&["apply", "edits.json"]).assert().success();
    w.assert_bytes("a", b"head\nBEGIN\nNEW\nEND\ntail\n");
    w.apply("native", "replace a \"BEGIN\"..\"END\"i \"\"\n")
        .assert()
        .success();
    w.assert_bytes("a", b"head\ntail\n");
}

#[test]
fn invalid_fields_positions_and_shared_stdin_fail_early() {
    for source in [
        r#"[{"op":"insert","file":"a","lines":1,"text":"x","where":"befroe"}]"#,
        r#"[{"op":"replace","file":"a","lines":1,"text":"x","wsa":"old"}]"#,
        r#"[{"op":"unknown","file":"a"}]"#,
    ] {
        let w = Workspace::new(&[("a", b"old\n")]);
        for format in ["json", "yaml"] {
            w.apply(format, source).assert().failure();
            w.assert_bytes("a", b"old\n");
        }
    }
    let w = Workspace::new(&[("a", b"old\n")]);
    w.apply("native", "replace a 1 @-\n").assert().failure();
    w.assert_bytes("a", b"old\n");
}

#[test]
fn commit_failure_rolls_back_and_retains_a_conflict_checked_recovery_journal() {
    let w = Workspace::new(&[("a", b"old-a"), ("b", b"old-b")]);
    let permissions = std::fs::metadata(w.path("b")).unwrap().permissions();
    let mut readonly = permissions.clone();
    readonly.set_readonly(true);
    std::fs::set_permissions(w.path("b"), readonly).unwrap();
    let output = w
        .batch(&json!([
            {"op":"edit","file":"a","old":"old-a","text":"new-a"},
            {"op":"edit","file":"b","old":"old-b","text":"new-b"}
        ]))
        .output()
        .unwrap();
    std::fs::set_permissions(w.path("b"), permissions).unwrap();
    assert_eq!(output.status.code(), Some(4), "{}", utf8(&output.stderr));
    w.assert_bytes("a", b"old-a");
    w.assert_bytes("b", b"old-b");
    let stderr = utf8(&output.stderr);
    assert!(stderr.contains("rollback: completed"), "{stderr}");
    let journal = std::path::PathBuf::from(
        stderr
            .rsplit_once("recovery journal: ")
            .expect("commit errors must expose their recovery journal")
            .1
            .split("; planned paths ")
            .next()
            .unwrap()
            .trim(),
    );
    assert_eq!(
        std::fs::canonicalize(journal.parent().unwrap()).unwrap(),
        std::fs::canonicalize(std::env::temp_dir()).unwrap()
    );
    // Reproduce interruption after one successful commit using only public files.
    w.write("a", b"new-a");
    for flag in ["--dry-run", "--check"] {
        w.cli(&["recover"])
            .arg(&journal)
            .arg(flag)
            .assert()
            .success();
        w.assert_bytes("a", b"new-a");
    }
    // External writers must never be overwritten by recovery.
    w.write("a", b"external content");
    w.cli(&["recover"]).arg(&journal).assert().failure();
    w.assert_bytes("a", b"external content");
    w.write("a", b"new-a");
    let record = w
        .cli(&["recover"])
        .arg(&journal)
        .args(["--check", "--output", "json"])
        .assert()
        .success()
        .get_output()
        .clone();
    let value: serde_json::Value = serde_json::from_slice(&record.stdout).unwrap();
    assert_eq!(value["mode"], "check");
    assert_eq!(value["written"], false);
    w.assert_bytes("a", b"new-a");
    w.cli(&["recover"]).arg(&journal).assert().success();
    w.assert_bytes("a", b"old-a");
    w.cli(&["recover"])
        .arg(&journal)
        .arg("--check")
        .assert()
        .success();
    std::fs::remove_dir_all(journal).unwrap();
}

#[cfg(windows)]
#[test]
fn case_aliases_conflict_for_files_and_backups_but_not_shared_parent_directories() {
    let w = Workspace::new(&[]);
    w.batch(&json!([
        {"op":"create","file":"name","text":"one"},
        {"op":"create","file":"NAME","text":"two"}
    ]))
    .assert()
    .code(3);
    w.absent("name");
    w.batch(&json!([
        {"op":"create","file":"Directory/a","text":"one"},
        {"op":"create","file":"directory/b","text":"two"}
    ]))
    .assert()
    .success();
    w.assert_bytes("Directory/a", b"one");
    w.assert_bytes("Directory/b", b"two");
    w.write("a", b"one");
    w.write("A.BAK", b"two");
    w.batch(&json!([
        {"op":"edit","file":"a","old":"one","text":"ONE"},
        {"op":"edit","file":"A.BAK","old":"two","text":"TWO"}
    ]))
    .arg("--backup")
    .assert()
    .code(3);
    w.assert_bytes("a", b"one");
    w.assert_bytes("A.BAK", b"two");
}

#[cfg(unix)]
#[test]
fn batch_replacement_preserves_symlinks_and_executable_permissions() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let w = Workspace::new(&[("a", b"old\n")]);
    std::fs::set_permissions(w.path("a"), std::fs::Permissions::from_mode(0o751)).unwrap();
    symlink(w.path("a"), w.path("link")).unwrap();
    w.batch(&json!([{"op":"replace","file":"link","lines":1,"text":"new"}]))
        .assert()
        .success();
    w.assert_bytes("a", b"new\n");
    assert!(
        std::fs::symlink_metadata(w.path("link"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        std::fs::metadata(w.path("a")).unwrap().permissions().mode() & 0o777,
        0o751
    );
}
