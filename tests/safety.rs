//! Regression tests for persistence, validation and machine-readable output.
use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::json;
use xnip::core::atomic::{atomic_write, bak_path};
use xnip::core::ops::find::{FindMode, FindOpts, scan};

fn apply(dir: &std::path::Path, manifest: &serde_json::Value) -> Command {
    let mut cmd = Command::cargo_bin("xnip").unwrap();
    cmd.current_dir(dir)
        .args(["apply", "--from-stdin", "--format", "json"])
        .write_stdin(manifest.to_string());
    cmd
}

#[test]
fn aliases_of_one_file_preserve_both_edits() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.txt");
    std::fs::write(&path, b"a\nb\n").unwrap();
    apply(
        dir.path(),
        &json!([
            {"op":"replace","file":"a.txt","lines":1,"text":"A"},
            {"op":"replace","file":path,"lines":2,"text":"B"}
        ]),
    )
    .args(["--parallel", "8"])
    .assert()
    .success();
    assert_eq!(std::fs::read(path).unwrap(), b"A\nB\n");
}

#[test]
fn anchors_are_resolved_before_edits_introduce_new_matches() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a");
    std::fs::write(&path, b"a\nx\nb\nx\n").unwrap();
    apply(
        dir.path(),
        &json!([
            {"op":"replace","file":"a","match-line":"x","occurrence":2,"text":"X"},
            {"op":"replace","file":"a","lines":3,"text":"x"}
        ]),
    )
    .assert()
    .success();
    assert_eq!(std::fs::read(path).unwrap(), b"a\nx\nx\nX\n");
}

#[test]
fn ambiguous_mixed_batch_operations_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a"), b"a\nb\n").unwrap();
    for op in [
        json!({"op":"replace","file":"a","pattern":"a","repl":"A"}),
        json!({"op":"move","file":"a","lines":1,"to":2}),
    ] {
        apply(
            dir.path(),
            &json!([op, {"op":"replace","file":"a","lines":2,"text":"B"}]),
        )
        .assert()
        .code(3);
    }
    assert_eq!(std::fs::read(dir.path().join("a")).unwrap(), b"a\nb\n");
}

#[test]
fn insert_handles_empty_files_and_unterminated_last_lines() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a");
    std::fs::write(&path, b"").unwrap();
    Command::cargo_bin("xnip")
        .unwrap()
        .arg("insert")
        .arg(&path)
        .args(["--lines", "1", "--text", "first"])
        .assert()
        .success();
    assert_eq!(std::fs::read(&path).unwrap(), b"first\n");
    std::fs::write(&path, b"").unwrap();
    apply(
        dir.path(),
        &json!([{"op":"insert","file":"a","lines":1,"text":"first"}]),
    )
    .assert()
    .success();
    assert_eq!(std::fs::read(&path).unwrap(), b"first\n");
    std::fs::write(&path, b"first").unwrap();
    Command::cargo_bin("xnip")
        .unwrap()
        .arg("insert")
        .arg(&path)
        .args(["--lines", "1", "--text", "second"])
        .assert()
        .success();
    assert_eq!(std::fs::read(&path).unwrap(), b"first\nsecond\n");
}

#[test]
fn moves_round_trip_for_all_small_ranges_including_unterminated_eof() {
    use xnip::core::ops::{
        insert::Position,
        move_op::{move_lines, reverse_params},
    };
    for original in [b"1\n2\n3\n4\n".as_slice(), b"1\n2\n3\n4".as_slice()] {
        for start in 1..=4 {
            for end in start..=4 {
                for target in 1..=4 {
                    if (start..=end).contains(&target) {
                        continue;
                    }
                    for pos in [Position::Before, Position::After] {
                        let moved = move_lines(original, start, end, target, pos).unwrap();
                        let (s, e, t, p) = reverse_params(start, end, target, pos).unwrap();
                        assert_eq!(move_lines(&moved, s, e, t, p).unwrap(), original);
                    }
                }
            }
        }
    }
}

#[test]
fn missing_files_return_io_exit_code() {
    let dir = tempfile::tempdir().unwrap();
    Command::cargo_bin("xnip")
        .unwrap()
        .current_dir(dir.path())
        .args(["peek", "missing", "--all"])
        .assert()
        .code(2);
    Command::cargo_bin("xnip")
        .unwrap()
        .current_dir(dir.path())
        .args(["apply", "missing.json"])
        .assert()
        .code(2);
}

#[test]
fn pattern_replacement_does_not_silently_ignore_preconditions() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a");
    std::fs::write(&path, b"old\n").unwrap();
    Command::cargo_bin("xnip")
        .unwrap()
        .arg("replace")
        .arg(&path)
        .args(["--pattern", "old", "--repl", "new", "--was", "wrong"])
        .assert()
        .code(1);
    apply(
        dir.path(),
        &json!([{"op":"replace","file":"a","pattern":"old","repl":"new","was":"wrong"}]),
    )
    .assert()
    .code(3);
    assert_eq!(std::fs::read(path).unwrap(), b"old\n");
}

#[test]
fn backup_names_preserve_extensions_and_backups_can_be_edited() {
    let dir = tempfile::tempdir().unwrap();
    for name in ["a.rs", "a.txt", "a.bak"] {
        let path = dir.path().join(name);
        std::fs::write(&path, name).unwrap();
        atomic_write(&path, b"new", true).unwrap();
        assert_eq!(std::fs::read(bak_path(&path)).unwrap(), name.as_bytes());
    }
}

#[test]
fn backup_target_collision_is_rejected_before_any_write() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a"), b"a\n").unwrap();
    std::fs::write(dir.path().join("a.bak"), b"b\n").unwrap();
    apply(
        dir.path(),
        &json!([
            {"op":"replace","file":"a","lines":1,"text":"A"},
            {"op":"replace","file":"a.bak","lines":1,"text":"B"}
        ]),
    )
    .arg("--backup")
    .assert()
    .code(3);
    assert_eq!(std::fs::read(dir.path().join("a")).unwrap(), b"a\n");
    assert_eq!(std::fs::read(dir.path().join("a.bak")).unwrap(), b"b\n");
}

#[test]
fn apply_no_match_does_not_commit_other_files() {
    let dir = tempfile::tempdir().unwrap();
    for name in ["a", "b"] {
        std::fs::write(dir.path().join(name), b"old\n").unwrap();
    }
    apply(
        dir.path(),
        &json!([
            {"op":"replace","file":"a","lines":1,"text":"new"},
            {"op":"replace","file":"b","pattern":"missing","repl":"new"}
        ]),
    )
    .assert()
    .code(3);
    assert_eq!(std::fs::read(dir.path().join("a")).unwrap(), b"old\n");
}

#[test]
fn json_check_contains_only_json_events() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a"), b"old\n").unwrap();
    let output = apply(
        dir.path(),
        &json!([
            {"op":"replace","file":"a","lines":1,"text":"new"}
        ]),
    )
    .args(["--json", "--check"])
    .assert()
    .success()
    .get_output()
    .stdout
    .clone();
    let events: Vec<serde_json::Value> = String::from_utf8(output)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(events.len(), 2);
    assert_eq!(std::fs::read(dir.path().join("a")).unwrap(), b"old\n");
    apply(dir.path(), &json!([]))
        .args(["--json", "--dry-run"])
        .assert()
        .code(1);
}

#[test]
fn invalid_manifest_fields_and_positions_are_rejected() {
    for manifest in [
        json!([{"op":"insert","file":"a","lines":1,"text":"x","where":"befroe"}]),
        json!([{"op":"replace","file":"a","lines":1,"text":"x","wsa":"old"}]),
    ] {
        assert!(xnip::apply::parse_json::parse(&manifest.to_string()).is_err());
        assert!(xnip::apply::parse_yaml::parse(&manifest.to_string()).is_err());
    }
}

#[test]
fn scan_limits_hits_in_both_modes_and_handles_zero() {
    let line = regex::Regex::new("x").unwrap();
    let pattern = regex::bytes::Regex::new("x").unwrap();
    for mode in [FindMode::MatchLine(&line), FindMode::Pattern(&pattern)] {
        let mut opts = FindOpts {
            mode,
            max_matches: Some(2),
            first_only: false,
        };
        let hits = scan(b"x\nx\nx\nx\n", &opts);
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[1].line, 2);
        opts.max_matches = Some(0);
        assert_eq!(
            scan(b"x\n", &opts),
            Vec::<xnip::core::ops::find::Hit>::new()
        );
    }
}

#[test]
fn find_reports_io_errors_even_when_another_file_matches() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a"), b"x\n").unwrap();
    Command::cargo_bin("xnip")
        .unwrap()
        .current_dir(dir.path())
        .args(["find", "a", "missing", "--pattern", "x"])
        .assert()
        .code(2)
        .stdout(predicate::str::contains("a:1:1"));
}

#[test]
fn regex_revert_is_rejected_without_modifying_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a");
    std::fs::write(&path, b"123\n").unwrap();
    Command::cargo_bin("xnip")
        .unwrap()
        .arg("replace")
        .arg(&path)
        .args(["--pattern", "[0-9]+", "--repl", "123", "--revert"])
        .assert()
        .failure();
    assert_eq!(std::fs::read(&path).unwrap(), b"123\n");
}

#[cfg(unix)]
#[test]
fn atomic_and_batch_writes_preserve_executable_permissions_and_symlinks() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a");
    let link = dir.path().join("link");
    std::fs::write(&path, b"old\n").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o751)).unwrap();
    symlink(&path, &link).unwrap();
    atomic_write(&link, b"new\n", false).unwrap();
    assert!(
        std::fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(std::fs::read(&path).unwrap(), b"new\n");
    apply(
        dir.path(),
        &json!([{"op":"replace","file":"link","lines":1,"text":"batch"}]),
    )
    .assert()
    .success();
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o751
    );
    assert!(
        std::fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(std::fs::read(&path).unwrap(), b"batch\n");
}
