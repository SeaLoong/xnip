use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;

fn apply(root: &std::path::Path, format: &str, source: &str) -> Command {
    let mut command = Command::cargo_bin("xnip").unwrap();
    command
        .current_dir(root)
        .args(["apply", "--from-stdin", "--format", format])
        .write_stdin(source);
    command
}
#[test]
fn json_batch_create_edit_rename_delete_uses_input_order() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("old"), b"original").unwrap();
    let manifest = r#"[
      {"op":"create","file":"nested/a","text":"alpha\nbeta"},
      {"op":"edit","file":"nested/a","old":"beta","text":"gamma"},
      {"op":"rename","file":"nested/a","to-file":"result"},
      {"op":"delete","file":"old","was":"original"}
    ]"#;
    apply(dir.path(), "json", manifest)
        .arg("--dry-run")
        .assert()
        .success();
    assert!(!dir.path().join("nested").exists());
    assert!(dir.path().join("old").exists());
    apply(dir.path(), "json", manifest).assert().success();
    assert_eq!(
        fs::read(dir.path().join("result")).unwrap(),
        b"alpha\ngamma"
    );
    assert!(!dir.path().join("old").exists());
    assert!(!dir.path().join("nested/a").exists());
}
#[test]
fn invalid_late_operation_prevents_all_writes_and_directories() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("original"), b"keep").unwrap();
    let source = r#"[{"op":"create","file":"nested/new","text":"new"},{"op":"delete","file":"original","was":"wrong"}]"#;
    apply(dir.path(), "json", source).assert().code(3);
    assert!(!dir.path().join("nested").exists());
    assert_eq!(fs::read(dir.path().join("original")).unwrap(), b"keep");
}
#[test]
fn native_and_yaml_file_operations_work() {
    let dir = tempfile::tempdir().unwrap();
    apply(
        dir.path(),
        "native",
        "create a \"old\"\nedit a \"old\" \"new\"\nrename a b\n",
    )
    .assert()
    .success();
    assert_eq!(fs::read(dir.path().join("b")).unwrap(), b"new");
    apply(
        dir.path(),
        "yaml",
        "- op: delete\n  file: b\n  was: new\n- op: create\n  file: c\n  text: hello\n",
    )
    .assert()
    .success();
    assert!(!dir.path().join("b").exists());
    assert_eq!(fs::read(dir.path().join("c")).unwrap(), b"hello");
}
#[test]
fn explicit_sequential_allows_dependent_line_edits() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("a"), b"one\ntwo\n").unwrap();
    let source = r#"[{"op":"replace","file":"a","pattern":"one","repl":"first"},{"op":"replace","file":"a","match-line":"^first$","text":"done"}]"#;
    apply(dir.path(), "json", source).assert().code(3);
    apply(dir.path(), "json", source)
        .arg("--sequential")
        .assert()
        .success();
    assert_eq!(fs::read(dir.path().join("a")).unwrap(), b"done\ntwo\n");
}
#[test]
fn rename_preview_backup_and_no_clobber() {
    let dir = tempfile::tempdir().unwrap();
    let from = dir.path().join("a");
    let to = dir.path().join("nested/b");
    fs::write(&from, b"keep\r\nEOF").unwrap();
    Command::cargo_bin("xnip")
        .unwrap()
        .arg("edit")
        .arg(&from)
        .arg("--rename-to")
        .arg(&to)
        .arg("--dry-run")
        .assert()
        .success();
    assert!(!to.parent().unwrap().exists());
    Command::cargo_bin("xnip")
        .unwrap()
        .arg("edit")
        .arg(&from)
        .arg("--rename-to")
        .arg(&to)
        .arg("--backup")
        .assert()
        .success();
    assert!(!from.exists());
    assert_eq!(fs::read(&to).unwrap(), b"keep\r\nEOF");
    assert_eq!(
        fs::read(xnip::core::atomic::bak_path(&from)).unwrap(),
        b"keep\r\nEOF"
    );
    fs::write(&from, b"new").unwrap();
    Command::cargo_bin("xnip")
        .unwrap()
        .arg("edit")
        .arg(&from)
        .arg("--rename-to")
        .arg(&to)
        .assert()
        .code(3);
    assert_eq!(fs::read(&from).unwrap(), b"new");
}
#[test]
fn apply_patch_format_updates_moves_adds_and_deletes() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("a"), b"head\r\nold\r\ntail\r\n").unwrap();
    fs::write(dir.path().join("obsolete"), b"delete").unwrap();
    let patch = "*** Begin Patch\n*** Update File: a\n*** Move to: moved\n@@\n head\n-old\n+new\n tail\n*** Add File: nested/new\n+hello\n*** Delete File: obsolete\n*** End Patch\n";
    apply(dir.path(), "patch", patch)
        .arg("--check")
        .assert()
        .success();
    assert!(!dir.path().join("nested").exists());
    apply(dir.path(), "patch", patch).assert().success();
    assert_eq!(
        fs::read(dir.path().join("moved")).unwrap(),
        b"head\r\nnew\r\ntail\r\n"
    );
    assert_eq!(fs::read(dir.path().join("nested/new")).unwrap(), b"hello\n");
    assert!(!dir.path().join("a").exists());
    assert!(!dir.path().join("obsolete").exists());
}
#[test]
fn unified_diff_handles_offsets_and_no_newline_markers() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("a"), b"prefix\nold").unwrap();
    let patch = "--- a/a\n+++ b/a\n@@ -1 +1,2 @@\n-old\n\\ No newline at end of file\n+new\n+last\n\\ No newline at end of file\n";
    apply(dir.path(), "patch", patch).assert().success();
    assert_eq!(
        fs::read(dir.path().join("a")).unwrap(),
        b"prefix\nnew\nlast"
    );
}
#[test]
fn patch_rejects_traversal_ambiguous_context_and_bad_counts() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("a"), b"old\nold\n").unwrap();
    apply(
        dir.path(),
        "patch",
        "*** Begin Patch\n*** Add File: ../outside\n+bad\n*** End Patch\n",
    )
    .assert()
    .failure();
    apply(
        dir.path(),
        "patch",
        "*** Begin Patch\n*** Update File: a\n@@\n-old\n+new\n*** End Patch\n",
    )
    .assert()
    .code(3)
    .stderr(predicate::str::contains("ambiguous"));
    apply(
        dir.path(),
        "patch",
        "--- a/a\n+++ b/a\n@@ -1,3 +1 @@\n-old\n+new\n",
    )
    .assert()
    .code(1);
    assert_eq!(fs::read(dir.path().join("a")).unwrap(), b"old\nold\n");
}
#[test]
fn unified_create_delete_and_pure_git_rename() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("old"), b"old\n").unwrap();
    let patch = "--- /dev/null\n+++ b/new\n@@ -0,0 +1 @@\n+created\n--- a/old\n+++ /dev/null\n@@ -1 +0,0 @@\n-old\n";
    apply(dir.path(), "patch", patch).assert().success();
    apply(
        dir.path(),
        "patch",
        "diff --git a/new b/moved\nsimilarity index 100%\nrename from new\nrename to moved\n",
    )
    .assert()
    .success();
    assert_eq!(fs::read(dir.path().join("moved")).unwrap(), b"created\n");
    assert!(!dir.path().join("old").exists());
}
#[cfg(unix)]
#[test]
fn rename_preserves_inode_and_patch_rejects_symlink_escape() {
    use std::os::unix::fs::{MetadataExt, symlink};
    let dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let from = dir.path().join("a");
    fs::write(&from, b"source").unwrap();
    let inode = fs::metadata(&from).unwrap().ino();
    Command::cargo_bin("xnip")
        .unwrap()
        .arg("edit")
        .arg(&from)
        .arg("--rename-to")
        .arg(dir.path().join("b"))
        .assert()
        .success();
    assert_eq!(fs::metadata(dir.path().join("b")).unwrap().ino(), inode);
    symlink(outside.path(), dir.path().join("escape")).unwrap();
    apply(
        dir.path(),
        "patch",
        "*** Begin Patch\n*** Add File: escape/secret\n+bad\n*** End Patch\n",
    )
    .assert()
    .failure();
    assert!(!outside.path().join("secret").exists());
}

#[test]
fn io_failure_rolls_back_and_cli_can_recover_from_retained_snapshots() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a");
    let b = dir.path().join("b");
    fs::write(&a, b"old-a").unwrap();
    fs::write(&b, b"old-b").unwrap();
    let original_permissions = fs::metadata(&b).unwrap().permissions();
    let mut readonly = original_permissions.clone();
    readonly.set_readonly(true);
    fs::set_permissions(&b, readonly).unwrap();
    let source = r#"[{"op":"edit","file":"a","old":"old-a","text":"new-a"},{"op":"edit","file":"b","old":"old-b","text":"new-b"}]"#;
    let output = apply(dir.path(), "json", source)
        .assert()
        .code(4)
        .get_output()
        .clone();
    fs::set_permissions(&b, original_permissions).unwrap();
    assert_eq!(fs::read(&a).unwrap(), b"old-a");
    assert_eq!(fs::read(&b).unwrap(), b"old-b");
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("rollback: completed"), "{stderr}");
    let journal = std::path::PathBuf::from(
        stderr
            .lines()
            .find_map(|line| line.strip_prefix("xnip transaction: recovery journal "))
            .unwrap(),
    );
    assert!(
        journal
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("xnip-transaction-")
    );
    assert_eq!(
        fs::canonicalize(journal.parent().unwrap()).unwrap(),
        fs::canonicalize(std::env::temp_dir()).unwrap()
    );
    // Model the on-disk state left by interruption after one committed write.
    fs::write(&a, b"new-a").unwrap();
    Command::cargo_bin("xnip")
        .unwrap()
        .arg("recover")
        .arg(&journal)
        .arg("--dry-run")
        .assert()
        .success()
        .stdout(predicate::str::contains("+old-a"));
    assert_eq!(fs::read(&a).unwrap(), b"new-a");
    Command::cargo_bin("xnip")
        .unwrap()
        .arg("recover")
        .arg(&journal)
        .assert()
        .success();
    assert_eq!(fs::read(&a).unwrap(), b"old-a");
    Command::cargo_bin("xnip")
        .unwrap()
        .arg("recover")
        .arg(&journal)
        .arg("--check")
        .assert()
        .success();
    fs::remove_dir_all(journal).unwrap();
}

#[test]
fn patch_rejects_invalid_eof_marker_and_zero_context_offset() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("empty"), b"").unwrap();
    apply(
        dir.path(),
        "patch",
        "--- a/empty\n+++ b/empty\n@@ -99,0 +1 @@\n+wrong\n",
    )
    .assert()
    .code(3);
    apply(dir.path(),"patch","--- a/empty\n+++ b/empty\n@@ -0,0 +1,2 @@\n+first\n\\ No newline at end of file\n+second\n").assert().code(3);
    assert_eq!(fs::read(dir.path().join("empty")).unwrap(), b"");
}

#[cfg(windows)]
#[test]
fn case_aliases_in_new_file_plan_fail_before_writes() {
    let dir = tempfile::tempdir().unwrap();
    apply(dir.path(),"json",r#"[{"op":"create","file":"name","text":"one"},{"op":"create","file":"NAME","text":"two"}]"#).assert().code(3);
    assert!(!dir.path().join("name").exists());
}

#[cfg(windows)]
#[test]
fn readonly_rename_is_rejected_before_creating_a_second_link() {
    let dir = tempfile::tempdir().unwrap();
    let from = dir.path().join("a");
    let to = dir.path().join("b");
    fs::write(&from, b"original").unwrap();
    let permissions = fs::metadata(&from).unwrap().permissions();
    let mut readonly = permissions.clone();
    readonly.set_readonly(true);
    fs::set_permissions(&from, readonly).unwrap();
    let output = Command::cargo_bin("xnip")
        .unwrap()
        .arg("edit")
        .arg(&from)
        .arg("--rename-to")
        .arg(&to)
        .output()
        .unwrap();
    fs::set_permissions(&from, permissions).unwrap();
    assert_eq!(output.status.code(), Some(3));
    assert!(!to.exists());
    assert_eq!(fs::read(from).unwrap(), b"original");
}

#[test]
fn git_quoted_unicode_paths_are_decoded() {
    let dir = tempfile::tempdir().unwrap();
    let patch =
        "--- /dev/null\n+++ \"b/\\344\\270\\255\\346\\226\\207.txt\"\n@@ -0,0 +1 @@\n+hello\n";
    apply(dir.path(), "patch", patch).assert().success();
    assert_eq!(fs::read(dir.path().join("中文.txt")).unwrap(), b"hello\n");
}
#[test]
fn lifecycle_rejects_trailing_directory_separator() {
    let dir = tempfile::tempdir().unwrap();
    apply(
        dir.path(),
        "json",
        r#"[{"op":"create","file":"directory/","text":"wrong"}]"#,
    )
    .assert()
    .code(3);
    assert!(!dir.path().join("directory").exists());
}

#[test]
fn patch_root_is_independent_of_process_working_directory() {
    let project = tempfile::tempdir().unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    let patch = "*** Begin Patch\n*** Add File: hello.txt\n+hello\n*** End Patch\n";
    Command::cargo_bin("xnip")
        .unwrap()
        .current_dir(elsewhere.path())
        .args(["apply", "--from-stdin", "--patch-root"])
        .arg(project.path())
        .write_stdin(patch)
        .assert()
        .success();
    assert_eq!(
        fs::read(project.path().join("hello.txt")).unwrap(),
        b"hello\n"
    );
    assert!(!elsewhere.path().join("hello.txt").exists());
}

#[cfg(windows)]
#[test]
fn new_parent_case_aliases_do_not_cause_false_conflicts() {
    let dir = tempfile::tempdir().unwrap();
    apply(dir.path(),"json",r#"[{"op":"create","file":"Directory/a","text":"one"},{"op":"create","file":"directory/b","text":"two"}]"#).assert().success();
    assert_eq!(fs::read(dir.path().join("Directory/a")).unwrap(), b"one");
    assert_eq!(fs::read(dir.path().join("Directory/b")).unwrap(), b"two");
}

#[test]
fn unified_same_paths_in_a_directory_are_not_mistaken_for_git_prefixes() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("a")).unwrap();
    fs::write(dir.path().join("a/file"), b"old\n").unwrap();
    fs::write(dir.path().join("file"), b"unrelated\n").unwrap();
    apply(
        dir.path(),
        "patch",
        "--- a/file\n+++ a/file\n@@ -1 +1 @@\n-old\n+new\n",
    )
    .assert()
    .success();
    assert_eq!(fs::read(dir.path().join("a/file")).unwrap(), b"new\n");
    assert_eq!(fs::read(dir.path().join("file")).unwrap(), b"unrelated\n");
}
