use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use xnip::core::edit::replace_exact;

#[test]
fn literal_blocks_preserve_crlf_and_unterminated_eof() {
    let (result, count) = replace_exact(
        b"first\r\nold\r\nlast",
        b"old\r\n",
        b"new\r\nextra\r\n",
        false,
    )
    .unwrap();
    assert_eq!(result, b"first\r\nnew\r\nextra\r\nlast");
    assert_eq!(count, 1);
    assert_eq!(
        replace_exact(b"a\0\xffb", b"\0\xff", b"$1", false)
            .unwrap()
            .0,
        b"a$1b"
    );
}

#[test]
fn ambiguous_including_overlapping_matches_require_explicit_all() {
    assert!(replace_exact(b"aaa", b"aa", b"X", false).is_err());
    assert_eq!(
        replace_exact(b"aaa", b"aa", b"X", true).unwrap(),
        (b"Xa".to_vec(), 1)
    );
    assert_eq!(
        replace_exact(b"foo foo", b"foo", b"$1", true).unwrap(),
        (b"$1 $1".to_vec(), 2)
    );
    assert!(replace_exact(b"foo", b"", b"X", true).is_err());
    assert!(replace_exact(b"foo", b"bar", b"X", false).is_err());
}

#[test]
fn create_preview_and_check_leave_no_directories_then_create_exact_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nested/new.txt");
    for flag in ["--dry-run", "--check"] {
        Command::cargo_bin("xnip")
            .unwrap()
            .arg("edit")
            .arg(&path)
            .args(["--create", "--text", "hello", flag])
            .assert()
            .success();
        assert!(!dir.path().join("nested").exists());
    }
    Command::cargo_bin("xnip")
        .unwrap()
        .arg("edit")
        .arg(&path)
        .args(["--create", "--text", "hello"])
        .assert()
        .success();
    assert_eq!(fs::read(&path).unwrap(), b"hello");
    Command::cargo_bin("xnip")
        .unwrap()
        .arg("edit")
        .arg(&path)
        .args(["--create", "--text", "overwrite"])
        .assert()
        .code(3);
    assert_eq!(fs::read(&path).unwrap(), b"hello");
}

#[test]
fn replace_uses_old_and_payload_files_with_backup() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("a.txt");
    let old = dir.path().join("old");
    let new = dir.path().join("new");
    fs::write(&target, b"start\r\nold\r\nend").unwrap();
    fs::write(&old, b"old\r\n").unwrap();
    fs::write(&new, b"new\r\nextra\r\n").unwrap();
    Command::cargo_bin("xnip")
        .unwrap()
        .arg("edit")
        .arg(&target)
        .arg("--old-file")
        .arg(&old)
        .arg("--text-file")
        .arg(&new)
        .arg("--backup")
        .assert()
        .success();
    assert_eq!(fs::read(&target).unwrap(), b"start\r\nnew\r\nextra\r\nend");
    assert_eq!(
        fs::read(xnip::core::atomic::bak_path(&target)).unwrap(),
        b"start\r\nold\r\nend"
    );
}

#[test]
fn replace_rejects_missing_or_ambiguous_context_without_modifying_target() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a");
    fs::write(&path, b"foo foo").unwrap();
    for old in ["missing", "foo"] {
        Command::cargo_bin("xnip")
            .unwrap()
            .arg("edit")
            .arg(&path)
            .args(["--old", old, "--text", "new", "--backup"])
            .assert()
            .code(3);
        assert_eq!(fs::read(&path).unwrap(), b"foo foo");
        assert!(!xnip::core::atomic::bak_path(&path).exists());
    }
    Command::cargo_bin("xnip")
        .unwrap()
        .arg("edit")
        .arg(&path)
        .args(["--old", "foo", "--text", "new", "--all"])
        .assert()
        .success();
    assert_eq!(fs::read(path).unwrap(), b"new new");
}

#[test]
fn noop_does_not_write_backup_and_delete_requires_exact_optional_precondition() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a");
    fs::write(&path, b"old").unwrap();
    Command::cargo_bin("xnip")
        .unwrap()
        .arg("edit")
        .arg(&path)
        .args(["--old", "old", "--text", "old", "--backup"])
        .assert()
        .success();
    assert!(!xnip::core::atomic::bak_path(&path).exists());
    Command::cargo_bin("xnip")
        .unwrap()
        .arg("edit")
        .arg(&path)
        .args(["--delete", "--old", "wrong"])
        .assert()
        .code(3);
    Command::cargo_bin("xnip")
        .unwrap()
        .arg("edit")
        .arg(&path)
        .args(["--delete", "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains("-old"));
    assert!(path.exists());
    Command::cargo_bin("xnip")
        .unwrap()
        .arg("edit")
        .arg(&path)
        .args(["--delete", "--old", "old", "--backup"])
        .assert()
        .success();
    assert!(!path.exists());
    assert_eq!(
        fs::read(xnip::core::atomic::bak_path(&path)).unwrap(),
        b"old"
    );
}

#[test]
fn empty_files_and_stdin_payloads_are_supported() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a");
    Command::cargo_bin("xnip")
        .unwrap()
        .arg("edit")
        .arg(&path)
        .args(["--create", "--text-stdin"])
        .write_stdin("")
        .assert()
        .success();
    assert_eq!(fs::read(&path).unwrap(), b"");
    Command::cargo_bin("xnip")
        .unwrap()
        .arg("edit")
        .arg(&path)
        .arg("--delete")
        .assert()
        .success();
    assert!(!path.exists());
}

#[test]
fn invalid_modes_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a");
    for args in [
        vec!["--create"],
        vec!["--create", "--old", "x", "--text", "y"],
        vec!["--create", "--text", "x", "--all"],
        vec!["--create", "--delete", "--text", "x"],
        vec!["--create", "--text", "x", "--check", "--dry-run"],
    ] {
        Command::cargo_bin("xnip")
            .unwrap()
            .arg("edit")
            .arg(&path)
            .args(args)
            .assert()
            .code(1);
        assert!(!path.exists());
    }
}

#[cfg(unix)]
#[test]
fn edit_preserves_symlink_on_replace_and_rejects_symlink_delete_or_create() {
    use std::os::unix::fs::symlink;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a");
    let link = dir.path().join("link");
    fs::write(&path, b"old").unwrap();
    symlink(&path, &link).unwrap();
    Command::cargo_bin("xnip")
        .unwrap()
        .arg("edit")
        .arg(&link)
        .args(["--old", "old", "--text", "new"])
        .assert()
        .success();
    assert_eq!(fs::read(&path).unwrap(), b"new");
    Command::cargo_bin("xnip")
        .unwrap()
        .arg("edit")
        .arg(&link)
        .arg("--delete")
        .assert()
        .code(1);
    assert!(
        fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    fs::remove_file(&path).unwrap();
    Command::cargo_bin("xnip")
        .unwrap()
        .arg("edit")
        .arg(&link)
        .args(["--create", "--text", "x"])
        .assert()
        .code(3);
    assert!(!path.exists());
}
