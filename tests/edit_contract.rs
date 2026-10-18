//! Editing contracts through CLI; bytes and failure atomicity are the assertions.
mod common;
use common::Workspace;
use serde_json::json;

#[test]
fn exact_edit_preserves_bytes_and_treats_replacement_literally() {
    for (original, old, new, expected) in [
        (
            b"head\r\nold\r\nlast".as_slice(),
            b"old\r\n".as_slice(),
            b"new\r\nmore\r\n".as_slice(),
            b"head\r\nnew\r\nmore\r\nlast".as_slice(),
        ),
        (b"a\0\xffb", b"\0\xff", b"$1", b"a$1b"),
        (b"old", b"old", b"", b""),
    ] {
        let w = Workspace::new(&[("target", original), ("old", old), ("new", new)]);
        w.cli(&[
            "edit",
            "target",
            "--old-file",
            "old",
            "--text-file",
            "new",
            "--backup",
        ])
        .assert()
        .success();
        w.assert_bytes("target", expected);
        w.assert_bytes("target.bak", original);
    }
}

#[test]
fn ambiguous_missing_empty_and_overlapping_context_fail_without_side_effects() {
    for (original, old) in [
        ("foo foo", "foo"),
        ("foo", "missing"),
        ("aaa", "aa"),
        ("foo", ""),
    ] {
        let w = Workspace::new(&[("target", original.as_bytes())]);
        w.cli(&["edit", "target", "--old", old, "--text", "new", "--backup"])
            .assert()
            .code(if old.is_empty() { 1 } else { 3 });
        w.assert_bytes("target", original.as_bytes());
        w.absent("target.bak");
    }
    for (original, old, expected) in [("foo foo", "foo", "$1 $1"), ("aaa", "aa", "$1a")] {
        let w = Workspace::new(&[("target", original.as_bytes())]);
        w.cli(&["edit", "target", "--old", old, "--text", "$1", "--all"])
            .assert()
            .success();
        w.assert_bytes("target", expected.as_bytes());
    }
}

#[test]
fn create_preview_check_commit_and_no_clobber_form_one_lifecycle() {
    for payload in ["hello\r\nEOF", ""] {
        let w = Workspace::new(&[]);
        for mode in ["--dry-run", "--check"] {
            w.cli(&["edit", "nested/new", "--create", "--text", payload, mode])
                .assert()
                .success();
            w.absent("nested");
        }
        w.cli(&["edit", "nested/new", "--create", "--text-stdin"])
            .write_stdin(payload)
            .assert()
            .success();
        w.assert_bytes("nested/new", payload.as_bytes());
        w.cli(&["edit", "nested/new", "--create", "--text", "overwrite"])
            .assert()
            .code(3);
        w.assert_bytes("nested/new", payload.as_bytes());
    }
}

#[test]
fn unchanged_edit_does_not_replace_an_existing_backup() {
    let w = Workspace::new(&[("a.txt", b"old"), ("a.txt.bak", b"previous backup")]);
    w.cli(&["edit", "a.txt", "--old", "old", "--text", "old", "--backup"])
        .assert()
        .success();
    w.assert_bytes("a.txt", b"old");
    w.assert_bytes("a.txt.bak", b"previous backup");
    w.cli(&["edit", "a.txt.bak", "--old", "previous", "--text", "edited"])
        .assert()
        .success();
    w.assert_bytes("a.txt.bak", b"edited backup");
}

#[test]
fn delete_checks_whole_content_and_preview_never_deletes() {
    let w = Workspace::new(&[("a", b"old\r\nEOF")]);
    w.cli(&["edit", "a", "--delete", "--old", "old"])
        .assert()
        .code(3);
    w.assert_bytes("a", b"old\r\nEOF");
    for mode in ["--check", "--dry-run"] {
        w.cli(&["edit", "a", "--delete", mode]).assert().success();
        w.assert_bytes("a", b"old\r\nEOF");
    }
    w.cli(&["edit", "a", "--delete", "--old", "old\r\nEOF", "--backup"])
        .assert()
        .success();
    w.absent("a");
    w.assert_bytes("a.bak", b"old\r\nEOF");
}

#[test]
fn rename_is_no_clobber_and_preview_does_not_create_parents() {
    let w = Workspace::new(&[("source", b"old\r\nEOF")]);
    for mode in ["--check", "--dry-run"] {
        w.cli(&["edit", "source", "--rename-to", "nested/dest", mode])
            .assert()
            .success();
        w.assert_bytes("source", b"old\r\nEOF");
        w.absent("nested");
    }
    w.cli(&["edit", "source", "--rename-to", "nested/dest", "--backup"])
        .assert()
        .success();
    w.absent("source");
    w.assert_bytes("nested/dest", b"old\r\nEOF");
    w.assert_bytes("source.bak", b"old\r\nEOF");
    w.write("source", b"second");
    w.cli(&["edit", "source", "--rename-to", "nested/dest"])
        .assert()
        .code(3);
    w.assert_bytes("source", b"second");
    w.assert_bytes("nested/dest", b"old\r\nEOF");
}

#[test]
fn incompatible_edit_arguments_are_usage_errors_without_writes() {
    for args in [
        vec!["--create"],
        vec!["--create", "--old", "x", "--text", "y"],
        vec!["--create", "--text", "x", "--all"],
        vec!["--create", "--delete", "--text", "x"],
        vec!["--create", "--text", "x", "--check", "--dry-run"],
        vec!["--old", "x", "--old-file", "old", "--text", "y"],
        vec!["--old", "x", "--text", "y", "--text-file", "new"],
    ] {
        let w = Workspace::new(&[]);
        w.cli(&["edit", "target"]).args(args).assert().code(1);
        w.absent("target");
    }
}

#[test]
fn all_range_locators_select_the_expected_region() {
    let original = b"head\nBEGIN\nold\nEND\nold\ntail\n";
    for (locator, expected) in [
        (
            vec!["--lines", "3"],
            b"head\nBEGIN\nnew\nEND\nold\ntail\n".as_slice(),
        ),
        (
            vec!["--match-line", "^old$", "--occurrence", "2"],
            b"head\nBEGIN\nold\nEND\nnew\ntail\n",
        ),
        (
            vec!["--between", "BEGIN..END"],
            b"head\nBEGIN\nnew\nEND\nold\ntail\n",
        ),
        (
            vec!["--between-re", "^BEGIN$..^END$", "--inclusive"],
            b"head\nnew\nold\ntail\n",
        ),
        (vec!["--lines", "2-4"], b"head\nnew\nold\ntail\n"),
    ] {
        let w = Workspace::new(&[("a", original)]);
        w.cli(&["replace", "a"])
            .args(locator)
            .args(["--text", "new"])
            .assert()
            .success();
        w.assert_bytes("a", expected);
    }
    let w = Workspace::new(&[("a", original)]);
    w.cli(&["replace", "a", "--lines", "3", "--text", ""])
        .assert()
        .success();
    w.assert_bytes("a", b"head\nBEGIN\nEND\nold\ntail\n");
}

#[test]
fn pattern_edits_support_capture_counts_and_reject_unchecked_preconditions() {
    let w = Workspace::new(&[("a", b"a1 a2 a3\n")]);
    w.cli(&[
        "replace",
        "a",
        "--pattern",
        "a([0-9])",
        "--repl",
        "b$1",
        "--count",
        "2",
    ])
    .assert()
    .success();
    w.assert_bytes("a", b"b1 b2 a3\n");
    for flags in [vec!["--was", "b1 b2 a3\n"], vec!["--revert"]] {
        w.cli(&["replace", "a", "--pattern", "[0-9]+", "--repl", "x"])
            .args(flags)
            .assert()
            .failure();
        w.assert_bytes("a", b"b1 b2 a3\n");
    }
}

#[test]
fn range_preconditions_payload_sources_and_preview_are_byte_exact() {
    let w = Workspace::new(&[
        ("a", b"one\ntwo\n"),
        ("old", b"one\n"),
        ("payload", b"first"),
    ]);
    w.cli(&[
        "replace", "a", "--lines", "1", "--was", "wrong", "--text", "bad",
    ])
    .assert()
    .code(3);
    w.assert_bytes("a", b"one\ntwo\n");
    w.cli(&[
        "replace",
        "a",
        "--lines",
        "1",
        "--was-file",
        "old",
        "--text-file",
        "payload",
        "--dry-run",
    ])
    .assert()
    .success();
    w.assert_bytes("a", b"one\ntwo\n");
    w.cli(&[
        "replace",
        "a",
        "--lines",
        "1",
        "--was-file",
        "old",
        "--text-file",
        "payload",
        "--backup",
    ])
    .assert()
    .success();
    w.assert_bytes("a", b"first\ntwo\n");
    w.assert_bytes("a.bak", b"one\ntwo\n");
}

#[test]
fn insert_defines_empty_eof_and_anchor_behavior() {
    for (original, args, expected) in [
        (b"".as_slice(), vec!["--lines", "1"], b"new\n".as_slice()),
        (b"one", vec!["--lines", "1"], b"one\nnew\n"),
        (
            b"one\ntwo\n",
            vec!["--lines", "1", "--position", "before"],
            b"new\none\ntwo\n",
        ),
        (
            b"one\ntwo\n",
            vec!["--match-line", "^two$"],
            b"one\ntwo\nnew\n",
        ),
    ] {
        let w = Workspace::new(&[("a", original)]);
        w.cli(&["insert", "a"])
            .args(&args)
            .args(["--text", "new", "--dry-run"])
            .assert()
            .success();
        w.assert_bytes("a", original);
        w.cli(&["insert", "a"])
            .args(args)
            .args(["--text", "new"])
            .assert()
            .success();
        w.assert_bytes("a", expected);
    }
}

#[test]
fn move_and_indent_work_for_blocks_anchors_and_all_conversions() {
    for (args, expected) in [
        (
            vec!["--from-lines", "1-2", "--to", "4"],
            b"c\nd\na\nb\n".as_slice(),
        ),
        (
            vec!["--from-lines", "3-4", "--to", "1", "--position", "before"],
            b"c\nd\na\nb\n",
        ),
        (
            vec!["--from-match-line", "^b$", "--to", "4"],
            b"a\nc\nd\nb\n",
        ),
    ] {
        let w = Workspace::new(&[("a", b"a\nb\nc\nd\n")]);
        w.cli(&["move", "a"])
            .args(&args)
            .arg("--dry-run")
            .assert()
            .success();
        w.assert_bytes("a", b"a\nb\nc\nd\n");
        w.cli(&["move", "a"]).args(args).assert().success();
        w.assert_bytes("a", expected);
    }
    for (original, args, expected) in [
        (
            b"a\nb\n".as_slice(),
            vec!["--lines", "2", "--add", "2"],
            b"a\n  b\n".as_slice(),
        ),
        (b" a\n   b\n", vec!["--all", "--remove", "2"], b"a\n b\n"),
        (
            b"\ta\n\t\tb\n",
            vec!["--all", "--tabs-to-spaces", "2"],
            b"  a\n    b\n",
        ),
        (
            b"  a\n    b\n",
            vec!["--all", "--spaces-to-tabs", "2"],
            b"\ta\n\t\tb\n",
        ),
    ] {
        let w = Workspace::new(&[("a", original)]);
        w.cli(&["indent", "a"])
            .args(&args)
            .arg("--dry-run")
            .assert()
            .success();
        w.assert_bytes("a", original);
        w.cli(&["indent", "a"]).args(args).assert().success();
        w.assert_bytes("a", expected);
    }
}

#[test]
fn supported_reverts_restore_original_bytes_through_the_cli() {
    for (forward, reverse) in [
        (
            vec!["replace", "a", "--lines", "1", "--text", "new"],
            vec![
                "replace", "a", "--lines", "1", "--text", "new", "--was", "a\n", "--revert",
            ],
        ),
        (
            vec!["insert", "a", "--lines", "1", "--text", "new\nmore"],
            vec![
                "insert",
                "a",
                "--lines",
                "1",
                "--text",
                "new\nmore",
                "--revert",
            ],
        ),
        (
            vec!["move", "a", "--from-lines", "1-2", "--to", "4"],
            vec!["move", "a", "--from-lines", "1-2", "--to", "4", "--revert"],
        ),
        (
            vec!["indent", "a", "--all", "--add", "2"],
            vec!["indent", "a", "--all", "--add", "2", "--revert"],
        ),
        (
            vec!["replace", "a", "--pattern", "a", "--repl", "A"],
            vec!["replace", "a", "--pattern", "a", "--repl", "A", "--revert"],
        ),
    ] {
        let w = Workspace::new(&[("a", b"a\nb\nc\nd\n")]);
        w.cli(&forward).assert().success();
        w.cli(&reverse).assert().success();
        w.assert_bytes("a", b"a\nb\nc\nd\n");
    }
}

#[test]
fn invalid_legacy_operations_fail_without_modifying_the_file() {
    for args in [
        vec!["replace", "a", "--lines", "99", "--text", "x"],
        vec![
            "replace",
            "a",
            "--lines",
            "1",
            "--pattern",
            "a",
            "--text",
            "x",
        ],
        vec!["insert", "a", "--pattern", "a", "--text", "x"],
        vec!["insert", "a", "--lines", "1-2", "--text", "x"],
        vec!["move", "a", "--from-lines", "1-2", "--to", "2"],
        vec!["move", "a", "--to", "3"],
        vec![
            "move",
            "a",
            "--from-match-line",
            "a",
            "--to",
            "3",
            "--revert",
        ],
        vec!["indent", "a", "--all"],
        vec!["indent", "a", "--add", "2"],
    ] {
        let w = Workspace::new(&[("a", b"a\nb\nc\n")]);
        w.cli(&args).assert().failure();
        w.assert_bytes("a", b"a\nb\nc\n");
    }
}

#[test]
fn library_move_inverse_covers_all_small_valid_positions_and_eof_styles() {
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
                    for position in [Position::Before, Position::After] {
                        let moved = move_lines(original, start, end, target, position).unwrap();
                        let (s, e, t, p) = reverse_params(start, end, target, position).unwrap();
                        assert_eq!(move_lines(&moved, s, e, t, p).unwrap(), original);
                    }
                }
            }
        }
    }
}

#[test]
fn missing_files_and_directories_have_stable_failure_categories() {
    let w = Workspace::new(&[]);
    w.cli(&["edit", "missing", "--old", "a", "--text", "b"])
        .assert()
        .code(2);
    w.cli(&["replace", "missing", "--lines", "1", "--text", "b"])
        .assert()
        .code(2);
    w.cli(&["edit", ".", "--delete"]).assert().failure();
    w.batch(&json!([{"op":"create","file":"directory/","text":"bad"}]))
        .assert()
        .code(3);
    w.absent("directory");
}

#[cfg(unix)]
#[test]
fn replacement_preserves_executable_mode_and_symlinks_but_delete_refuses_links() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
    let w = Workspace::new(&[("a", b"old")]);
    std::fs::set_permissions(w.path("a"), std::fs::Permissions::from_mode(0o751)).unwrap();
    symlink(w.path("a"), w.path("link")).unwrap();
    w.cli(&["edit", "link", "--old", "old", "--text", "new"])
        .assert()
        .success();
    w.assert_bytes("a", b"new");
    assert!(
        std::fs::symlink_metadata(w.path("link"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        std::fs::metadata(w.path("a")).unwrap().mode() & 0o777,
        0o751
    );
    w.cli(&["edit", "link", "--delete"]).assert().code(1);
    let inode = std::fs::metadata(w.path("a")).unwrap().ino();
    w.cli(&["edit", "a", "--rename-to", "moved"])
        .assert()
        .success();
    assert_eq!(std::fs::metadata(w.path("moved")).unwrap().ino(), inode);
    w.cli(&["edit", "link", "--create", "--text", "bad"])
        .assert()
        .code(3);
    w.absent("a");
}

#[cfg(windows)]
#[test]
fn readonly_rename_fails_before_creating_a_destination() {
    let w = Workspace::new(&[("a", b"original")]);
    let permissions = std::fs::metadata(w.path("a")).unwrap().permissions();
    let mut readonly = permissions.clone();
    readonly.set_readonly(true);
    std::fs::set_permissions(w.path("a"), readonly).unwrap();
    let result = w.cli(&["edit", "a", "--rename-to", "b"]).output().unwrap();
    std::fs::set_permissions(w.path("a"), permissions).unwrap();
    assert_eq!(result.status.code(), Some(3));
    w.assert_bytes("a", b"original");
    w.absent("b");
}
