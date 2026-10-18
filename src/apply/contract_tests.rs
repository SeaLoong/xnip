//! Manifest and execution contracts share the public parser/plan seam.

#[test]
fn insert_same_anchor_is_ambiguous_in_original_line_mode_and_ordered_when_sequential() {
    for sequential in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("a");
        fs::write(&path, b"anchor\n").unwrap();
        let operation = |text: &[u8]| Op::Insert {
            target: Target::File(path.clone()),
            locator: crate::core::location::Locator::Lines { start: 1, end: 1 },
            content: OpContent::Text(text.to_vec()),
            position: MovePosition::Before,
            revert: false,
        };
        let result = execute_report(
            vec![operation(b"first"), operation(b"second")],
            &ExecOpts {
                sequential,
                ..ExecOpts::default()
            },
        );
        assert_eq!(result.is_ok(), sequential);
        assert_eq!(
            fs::read(path).unwrap(),
            if sequential {
                b"second\nfirst\nanchor\n".as_slice()
            } else {
                b"anchor\n"
            }
        );
    }
}

#[test]
fn every_reversible_operation_round_trips_through_batch_execution() {
    use crate::core::location::Locator;
    for kind in 0..4 {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("a");
        let original = b"one\ntwo\nthree\n";
        fs::write(&path, original).unwrap();
        let operation = |revert| match kind {
            0 => Op::Replace {
                target: Target::File(path.clone()),
                locator: Locator::Lines { start: 2, end: 2 },
                content: OpContent::Text(b"NEW".to_vec()),
                was: Some(b"two\n".to_vec()),
                revert,
            },
            1 => Op::Insert {
                target: Target::File(path.clone()),
                locator: Locator::Lines { start: 1, end: 1 },
                content: OpContent::Text(b"inserted".to_vec()),
                position: MovePosition::After,
                revert,
            },
            2 => Op::Move {
                target: Target::File(path.clone()),
                from: Locator::Lines { start: 1, end: 1 },
                to_line: 3,
                position: MovePosition::After,
                revert,
            },
            _ => Op::Indent {
                target: Target::File(path.clone()),
                locator: Locator::Lines { start: 1, end: 3 },
                kind: IndentKind::Adjust(2),
                revert,
            },
        };
        execute_report(vec![operation(false)], &ExecOpts::default()).unwrap();
        assert_ne!(fs::read(&path).unwrap(), original);
        execute_report(vec![operation(true)], &ExecOpts::default()).unwrap();
        assert_eq!(fs::read(&path).unwrap(), original);
    }
}

#[test]
fn mixed_modes_backup_inputs_and_failed_preconditions_never_partially_commit() {
    use crate::core::location::{Count, Locator};
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("a");
    let backup = crate::core::atomic::bak_path(&path);
    fs::write(&path, b"one\ntwo\n").unwrap();
    fs::write(&backup, b"backup input\n").unwrap();
    let line = || replace(&path, 1, 1, b"changed");
    let pattern = || Op::Replace {
        target: Target::File(path.clone()),
        locator: Locator::Pattern {
            regex: regex::bytes::Regex::new("two").unwrap(),
            count: Count::All,
        },
        content: OpContent::Repl("TWO".into()),
        was: None,
        revert: false,
    };
    assert!(execute_report(vec![line(), pattern()], &ExecOpts::default()).is_err());
    assert!(
        execute_report(
            vec![line(), replace(&backup, 1, 1, b"bad")],
            &ExecOpts {
                backup: true,
                ..ExecOpts::default()
            }
        )
        .is_err()
    );
    for was in [b"wrong".as_slice(), b"one".as_slice()] {
        let mut operation = line();
        if let Op::Replace { was: expected, .. } = &mut operation {
            *expected = Some(was.to_vec());
        }
        assert!(execute_report(vec![operation], &ExecOpts::default()).is_err());
    }
    assert_eq!(fs::read(&path).unwrap(), b"one\ntwo\n");
    assert_eq!(fs::read(&backup).unwrap(), b"backup input\n");
    execute_report(
        vec![line(), pattern()],
        &ExecOpts {
            sequential: true,
            ..ExecOpts::default()
        },
    )
    .unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"changed\nTWO\n");
}

use super::*;
use commit::{ExecOpts, execute_report};
use detect::Format;
use serde_json::json;
use std::{fs, path::Path};

#[test]
fn native_and_structured_syntax_produce_equivalent_operations() {
    let samples = [
        (
            r#"replace a 30 "X""#,
            json!({"op":"replace","file":"a","lines":30,"text":"X"}),
        ),
        (
            r#"replace a 1-9 """#,
            json!({"op":"replace","file":"a","lines":"1-9","text":""}),
        ),
        (
            r#"replace a =/^foo/2 "X""#,
            json!({"op":"replace","file":"a","match-line":"^foo","occurrence":2,"text":"X"}),
        ),
        (
            r#"replace a ~/^BEGIN/..~/^END/i """#,
            json!({"op":"replace","file":"a","between-re":["^BEGIN","^END"],"inclusive":true,"text":""}),
        ),
        (
            r#"replace a "BEGIN".."END" """#,
            json!({"op":"replace","file":"a","between":["BEGIN","END"],"text":""}),
        ),
        (
            r"replace a s/x/y/3",
            json!({"op":"replace","file":"a","pattern":"x","repl":"y","count":3}),
        ),
        (
            r"replace a revert s/x/y/g",
            json!({"op":"replace","file":"a","pattern":"x","repl":"y","count":"all","revert":true}),
        ),
        (
            r#"replace a 1 "new" was="old""#,
            json!({"op":"replace","file":"a","lines":1,"text":"new","was":"old"}),
        ),
        (
            r"replace a 1 @payload",
            json!({"op":"replace","file":"a","lines":1,"text-file":"payload"}),
        ),
        (
            r#"replace --files-from list 1 "X""#,
            json!({"op":"replace","files-from":"list","lines":1,"text":"X"}),
        ),
        (
            r#"insert a 5 before "X""#,
            json!({"op":"insert","file":"a","lines":5,"where":"before","text":"X"}),
        ),
        (
            r#"insert a 5 after "X""#,
            json!({"op":"insert","file":"a","lines":5,"where":"after","text":"X"}),
        ),
        (
            r"move a 10-20 100",
            json!({"op":"move","file":"a","lines":"10-20","to":100}),
        ),
        (
            r"indent a 1-9 +2",
            json!({"op":"indent","file":"a","lines":"1-9","by":2}),
        ),
        (
            r"indent a 1-9 -2",
            json!({"op":"indent","file":"a","lines":"1-9","by":-2}),
        ),
        (
            r"indent a 1-9 t2s:4",
            json!({"op":"indent","file":"a","lines":"1-9","tabs-to-spaces":4}),
        ),
        (
            r"indent a 1-9 s2t:4",
            json!({"op":"indent","file":"a","lines":"1-9","spaces-to-tabs":4}),
        ),
        (
            r#"create a "X""#,
            json!({"op":"create","file":"a","text":"X"}),
        ),
        (
            r#"edit a "X" "Y""#,
            json!({"op":"edit","file":"a","old":"X","text":"Y"}),
        ),
        (
            r"rename a b",
            json!({"op":"rename","file":"a","to-file":"b"}),
        ),
        (r"delete a", json!({"op":"delete","file":"a"})),
    ];
    for (native, structured) in samples {
        let expected = format!("{:?}", parse_native::parse(native).unwrap());
        let structured = json!([structured]).to_string();
        assert_eq!(
            format!("{:?}", parse_json::parse(&structured).unwrap()),
            expected,
            "{native}"
        );
        assert_eq!(
            format!("{:?}", parse_yaml::parse(&structured).unwrap()),
            expected,
            "{native}"
        );
    }
}

#[test]
fn yaml_block_scalars_and_native_escapes_keep_exact_text() {
    let yaml = "- op: replace\n  file: a\n  lines: 1\n  text: |\n    中文\n      next\n";
    let operations = parse_yaml::parse(yaml).unwrap();
    assert!(
        matches!(&operations[0],Op::Replace{content:OpContent::Text(text),..} if text=="中文\n  next\n".as_bytes())
    );
    let operations = parse_native::parse(r#"replace a 1 "a\nb\tc\\d\"e""#).unwrap();
    assert!(
        matches!(&operations[0],Op::Replace{content:OpContent::Text(text),..} if text==b"a\nb\tc\\d\"e")
    );
    let operations = parse_native::parse("# comment\n\nreplace a 1 \"X\"\n# end\n").unwrap();
    assert_eq!(operations.len(), 1);
}

#[test]
fn parsers_reject_unknown_fields_conflicting_selectors_and_incomplete_operations() {
    for value in [
        json!({"op":"unknown","file":"a"}),
        json!({"op":"replace","lines":1,"text":"X"}),
        json!({"op":"replace","file":"a","files-from":"list","lines":1,"text":"X"}),
        json!({"op":"replace","file":"a","lines":1,"match-line":"x","text":"X"}),
        json!({"op":"replace","file":"a","lines":1,"text":"X","wsa":"old"}),
        json!({"op":"replace","file":"a","between":["only"],"text":"X"}),
        json!({"op":"replace","file":"a","pattern":"[","repl":"X"}),
        json!({"op":"insert","file":"a","lines":1,"where":"befroe","text":"X"}),
        json!({"op":"create","file":"a","text":"X","lines":1}),
        json!({"op":"delete","file":"a","text":"X"}),
        json!({"op":"rename","file":"a"}),
    ] {
        let source = json!([value]).to_string();
        for parse in [parse_json::parse, parse_yaml::parse] {
            assert!(parse(&source).is_err(), "{source}");
        }
    }
    for source in [
        r"replace a",
        r"insert a 1",
        r"move a 1",
        r"indent a 1",
        r"frobnicate a 1",
        r#"replace a 1 "unterminated"#,
    ] {
        assert!(parse_native::parse(source).is_err(), "{source}");
    }
    assert!(parse_json::parse("not JSON").is_err());
    assert!(parse_yaml::parse("x: y: [").is_err());
    for parse in [parse_json::parse, parse_yaml::parse] {
        let operations = parse("[]").unwrap();
        assert_eq!(operations.len(), 0, "unexpected operations: {operations:?}");
    }
}

#[test]
fn format_detection_uses_hints_with_fallback_and_explicit_aliases() {
    for (source, format) in [
        (r#"replace a 1 "X""#, Format::Native),
        (
            r#"[{"op":"replace","file":"a","lines":1,"text":"X"}]"#,
            Format::Json,
        ),
        (
            "- op: replace\n  file: a\n  lines: 1\n  text: X\n",
            Format::Yaml,
        ),
    ] {
        let expected = format!("{:?}", detect::parse_with(source, format).unwrap());
        for path in [
            None,
            Some(Path::new("edits.json")),
            Some(Path::new("edits.yaml")),
            Some(Path::new("edits.unknown")),
        ] {
            assert_eq!(
                format!("{:?}", detect::parse_auto(source, path).unwrap()),
                expected
            );
        }
    }
    for (text, expected) in [
        ("NATIVE", Format::Native),
        ("json", Format::Json),
        ("yml", Format::Yaml),
        ("diff", Format::Patch),
    ] {
        assert_eq!(detect::parse_format_arg(text).unwrap(), expected);
    }
    assert!(detect::parse_format_arg("xml").is_err());
    assert!(detect::parse_auto("invalid manifest", None).is_err());
}

fn replace(path: &Path, start: usize, end: usize, text: &[u8]) -> Op {
    Op::Replace {
        target: Target::File(path.into()),
        locator: crate::core::location::Locator::Lines { start, end },
        content: OpContent::Text(text.to_vec()),
        was: None,
        revert: false,
    }
}

#[test]
fn original_line_plans_are_order_independent_and_parallel_plans_match_serial() {
    for parallel in [None, Some(0), Some(1), Some(4)] {
        for reverse in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let paths = [dir.path().join("a"), dir.path().join("b")];
            for path in &paths {
                fs::write(path, b"1\n2\n3\n4\n5\n").unwrap();
            }
            let mut operations = Vec::new();
            for path in &paths {
                operations.push(replace(path, 2, 2, b"two\nextra"));
                operations.push(replace(path, 4, 4, b"four"));
            }
            if reverse {
                operations.reverse();
            }
            let report = execute_report(
                operations,
                &ExecOpts {
                    parallel,
                    ..ExecOpts::default()
                },
            )
            .unwrap();
            assert_eq!(report.files.len(), 2);
            for path in &paths {
                assert_eq!(fs::read(path).unwrap(), b"1\ntwo\nextra\n3\nfour\n5\n");
            }
        }
    }
}

#[test]
fn overlapping_line_plans_require_sequential_order_and_fail_before_writes() {
    for sequential in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a");
        fs::write(&path, b"a\nb\n").unwrap();
        let result = execute_report(
            vec![replace(&path, 1, 2, b"x\ny"), replace(&path, 2, 2, b"last")],
            &ExecOpts {
                sequential,
                ..ExecOpts::default()
            },
        );
        assert_eq!(result.is_ok(), sequential);
        assert_eq!(
            fs::read(path).unwrap(),
            if sequential {
                b"x\nlast\n".as_slice()
            } else {
                b"a\nb\n"
            }
        );
    }
}

#[test]
fn preview_check_and_late_validation_failures_do_not_write_or_create_parents() {
    for (check, dry_run) in [(true, false), (false, true)] {
        let dir = tempfile::tempdir().unwrap();
        let existing = dir.path().join("a");
        let created = dir.path().join("new/deep/b");
        fs::write(&existing, b"old\n").unwrap();
        let operations = vec![
            replace(&existing, 1, 1, b"new"),
            Op::File {
                target: Target::File(created.clone()),
                operation: FileOp::Create(OpContent::Text(b"created".to_vec())),
            },
        ];
        let report = execute_report(
            operations,
            &ExecOpts {
                check,
                dry_run,
                ..ExecOpts::default()
            },
        )
        .unwrap();
        assert_eq!(report.files.len(), 2);
        assert_eq!(report.diff.is_empty(), check);
        assert_eq!(fs::read(&existing).unwrap(), b"old\n");
        assert!(!created.exists());
        assert!(!dir.path().join("new").exists());
    }
    for parallel in [None, Some(4)] {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a");
        let b = dir.path().join("b");
        fs::write(&a, b"old\n").unwrap();
        fs::write(&b, b"old\n").unwrap();
        assert!(
            execute_report(
                vec![replace(&a, 1, 1, b"new"), replace(&b, 9, 9, b"bad")],
                &ExecOpts {
                    parallel,
                    ..ExecOpts::default()
                }
            )
            .is_err()
        );
        for path in [a, b] {
            assert_eq!(fs::read(path).unwrap(), b"old\n");
        }
    }
}

#[test]
fn lifecycle_batches_coalesce_created_edited_renamed_and_deleted_states() {
    let dir = tempfile::tempdir().unwrap();
    let from = dir.path().join("new/from");
    let to = dir.path().join("to");
    let obsolete = dir.path().join("obsolete");
    fs::write(&obsolete, b"remove").unwrap();
    let operations = vec![
        Op::File {
            target: Target::File(from.clone()),
            operation: FileOp::Create(OpContent::Text(b"first".to_vec())),
        },
        Op::File {
            target: Target::File(from.clone()),
            operation: FileOp::Exact {
                old: OpContent::Text(b"first".to_vec()),
                text: OpContent::Text(b"last".to_vec()),
                all: false,
            },
        },
        Op::File {
            target: Target::File(from.clone()),
            operation: FileOp::Rename {
                to: to.clone(),
                was: Some(b"last".to_vec()),
            },
        },
        Op::File {
            target: Target::File(obsolete.clone()),
            operation: FileOp::Delete {
                was: Some(b"remove".to_vec()),
            },
        },
    ];
    execute_report(operations, &ExecOpts::default()).unwrap();
    assert!(!from.exists() && !obsolete.exists());
    assert_eq!(fs::read(to).unwrap(), b"last");
}

#[test]
fn file_lists_payload_files_stdin_and_backups_share_validation_contracts() {
    let dir = tempfile::tempdir().unwrap();
    let paths = [dir.path().join("a"), dir.path().join("b")];
    for path in &paths {
        fs::write(path, b"old\n").unwrap();
    }
    let list = dir.path().join("files");
    fs::write(
        &list,
        format!(
            "# comment\n{}\n\n{}\n",
            paths[0].display(),
            paths[1].display()
        ),
    )
    .unwrap();
    fs::write(dir.path().join("payload"), b"NEW").unwrap();
    let operation = Op::Replace {
        target: Target::FilesFrom(list),
        locator: crate::core::location::Locator::Lines { start: 1, end: 1 },
        content: OpContent::File("payload".into()),
        was: Some(b"old\n".to_vec()),
        revert: false,
    };
    execute_report(
        vec![operation],
        &ExecOpts {
            backup: true,
            manifest_dir: Some(dir.path().into()),
            ..ExecOpts::default()
        },
    )
    .unwrap();
    for path in &paths {
        assert_eq!(fs::read(path).unwrap(), b"NEW\n");
        assert_eq!(
            fs::read(crate::core::atomic::bak_path(path)).unwrap(),
            b"old\n"
        );
    }
    let operation = |path: &Path| Op::Replace {
        target: Target::File(path.into()),
        locator: crate::core::location::Locator::Lines { start: 1, end: 1 },
        content: OpContent::Stdin,
        was: None,
        revert: false,
    };
    assert!(execute_report(vec![operation(&paths[0])], &ExecOpts::default()).is_err());
    let opts = ExecOpts {
        stdin_bytes: Some(b"stdin".to_vec()),
        ..ExecOpts::default()
    };
    assert!(execute_report(vec![operation(&paths[0]), operation(&paths[1])], &opts).is_err());
    execute_report(vec![operation(&paths[0])], &opts).unwrap();
    assert_eq!(fs::read(&paths[0]).unwrap(), b"stdin\n");
}

#[test]
fn patch_import_executes_both_dialects_and_rejects_path_escape_and_wrong_context() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.txt");
    for source in [
        "--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-old\n+new\n",
        "*** Begin Patch\n*** Update File: a.txt\n@@\n-old\n+new\n*** End Patch\n",
    ] {
        fs::write(&path, b"old\n").unwrap();
        let operations = patch::parse_in(source, dir.path()).unwrap();
        execute_report(operations, &ExecOpts::default()).unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"new\n");
        let operations = patch::parse_in(source, dir.path()).unwrap();
        assert!(execute_report(operations, &ExecOpts::default()).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"new\n");
    }
    assert!(
        patch::parse_in(
            "*** Begin Patch\n*** Add File: ../escape\n+x\n*** End Patch\n",
            dir.path()
        )
        .is_err()
    );
}
