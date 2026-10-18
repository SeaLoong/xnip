//! Patch compatibility includes exact content, path confinement and malformed input.
mod common;
use common::Workspace;

#[test]
fn apply_patch_lifecycle_supports_crlf_move_add_delete_and_nonwriting_modes() {
    let w = Workspace::new(&[("a", b"head\r\nold\r\ntail\r\n"), ("obsolete", b"delete")]);
    let patch = "*** Begin Patch\n*** Update File: a\n*** Move to: moved\n@@\n head\n-old\n+new\n tail\n*** Add File: nested/new\n+hello\n*** Delete File: obsolete\n*** End Patch\n";
    for mode in ["--check", "--dry-run"] {
        w.apply("patch", patch).arg(mode).assert().success();
        w.assert_bytes("a", b"head\r\nold\r\ntail\r\n");
        w.assert_bytes("obsolete", b"delete");
        w.absent("nested");
        w.absent("moved");
    }
    w.apply("patch", patch).assert().success();
    w.assert_bytes("moved", b"head\r\nnew\r\ntail\r\n");
    w.assert_bytes("nested/new", b"hello\n");
    w.absent("a");
    w.absent("obsolete");
}

#[test]
fn unified_diff_finds_offset_context_and_preserves_unterminated_eof() {
    let w = Workspace::new(&[("a", b"prefix\nold")]);
    w.apply("patch", "--- a/a\n+++ b/a\n@@ -1 +1,2 @@\n-old\n\\ No newline at end of file\n+new\n+last\n\\ No newline at end of file\n")
        .assert().success();
    w.assert_bytes("a", b"prefix\nnew\nlast");
}

#[test]
fn unified_create_delete_and_git_rename_are_interoperable() {
    let w = Workspace::new(&[("old", b"old\n")]);
    w.apply("patch", "--- /dev/null\n+++ b/new\n@@ -0,0 +1 @@\n+created\n--- a/old\n+++ /dev/null\n@@ -1 +0,0 @@\n-old\n")
        .assert().success();
    w.absent("old");
    w.assert_bytes("new", b"created\n");
    w.apply(
        "patch",
        "diff --git a/new b/moved\nsimilarity index 100%\nrename from new\nrename to moved\n",
    )
    .assert()
    .success();
    w.absent("new");
    w.assert_bytes("moved", b"created\n");
}

#[test]
fn git_quoted_utf8_paths_are_decoded_without_confusing_real_prefix_directories() {
    let w = Workspace::new(&[("a/file", b"old\n"), ("file", b"unrelated\n")]);
    w.apply(
        "patch",
        "--- /dev/null\n+++ \"b/\\344\\270\\255\\346\\226\\207.txt\"\n@@ -0,0 +1 @@\n+hello\n",
    )
    .assert()
    .success();
    w.assert_bytes("中文.txt", b"hello\n");
    w.apply("patch", "--- a/file\n+++ a/file\n@@ -1 +1 @@\n-old\n+new\n")
        .assert()
        .success();
    w.assert_bytes("a/file", b"new\n");
    w.assert_bytes("file", b"unrelated\n");
}

#[test]
fn patch_root_is_explicit_and_independent_of_process_working_directory() {
    let project = Workspace::new(&[]);
    let elsewhere = Workspace::new(&[]);
    let patch = "*** Begin Patch\n*** Add File: nested/hello\n+hello\n*** End Patch\n";
    for mode in [Some("--dry-run"), None] {
        let mut command = elsewhere.cli(&["apply", "--from-stdin", "--patch-root"]);
        command.arg(project.root()).write_stdin(patch);
        if let Some(flag) = mode {
            command.arg(flag);
        }
        command.assert().success();
        if mode.is_some() {
            project.absent("nested");
        }
        elsewhere.absent("nested");
    }
    project.assert_bytes("nested/hello", b"hello\n");
}

#[test]
fn malformed_patch_counts_eof_and_ambiguous_context_never_write() {
    for (original, patch, code) in [
        (
            b"old\nold\n".as_slice(),
            "*** Begin Patch\n*** Update File: a\n@@\n-old\n+new\n*** End Patch\n",
            3,
        ),
        (
            b"old\nold\n",
            "--- a/a\n+++ b/a\n@@ -1,3 +1 @@\n-old\n+new\n",
            1,
        ),
        (b"", "--- a/a\n+++ b/a\n@@ -99,0 +1 @@\n+wrong\n", 3),
        (
            b"",
            "--- a/a\n+++ b/a\n@@ -0,0 +1,2 @@\n+first\n\\ No newline at end of file\n+second\n",
            3,
        ),
        (
            b"old\n",
            "*** Begin Patch\n*** Update File: a\n@@\n-missing\n+new\n*** End Patch\n",
            3,
        ),
    ] {
        let w = Workspace::new(&[("a", original)]);
        w.apply("patch", patch).assert().code(code);
        w.assert_bytes("a", original);
    }
}

#[test]
fn patch_paths_cannot_escape_the_root_or_overwrite_existing_destinations() {
    for relative in ["../outside", "nested/../../outside"] {
        let w = Workspace::new(&[("a", b"old\n")]);
        let patch = format!(
            "*** Begin Patch\n*** Update File: a\n@@\n-old\n+new\n*** Add File: {relative}\n+bad\n*** End Patch\n"
        );
        w.apply("patch", &patch).assert().failure();
        w.assert_bytes("a", b"old\n");
        w.absent("nested");
    }
    let w = Workspace::new(&[("a", b"old\n"), ("occupied", b"keep")]);
    w.apply("patch", "*** Begin Patch\n*** Update File: a\n*** Move to: occupied\n@@\n-old\n+new\n*** End Patch\n")
        .assert().failure();
    w.assert_bytes("a", b"old\n");
    w.assert_bytes("occupied", b"keep");
}

#[cfg(unix)]
#[test]
fn patch_confinement_resolves_symlink_parents_before_creating_files() {
    use std::os::unix::fs::symlink;
    let w = Workspace::new(&[("a", b"old\n")]);
    let outside = Workspace::new(&[]);
    symlink(outside.root(), w.path("escape")).unwrap();
    w.apply("patch", "*** Begin Patch\n*** Update File: a\n@@\n-old\n+new\n*** Add File: escape/secret\n+bad\n*** End Patch\n")
        .assert().failure();
    w.assert_bytes("a", b"old\n");
    outside.absent("secret");
}
