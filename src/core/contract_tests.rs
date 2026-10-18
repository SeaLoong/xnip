//! Contract matrices exercise the same core interfaces as CLI and MCP adapters.

#[test]
fn byte_cuts_skip_invalid_prefixes_and_keep_partial_error_counts_visible() {
    let bytes = [0xff, b'a', 0xe4, 0xb8, 0xad, b'z'];
    for budget in [11, 12] {
        let output = read::PeekQuery::new(None, None, true, 0, None, Some(budget))
            .unwrap()
            .render(&bytes)
            .unwrap();
        assert_eq!(output.bytes, b"     1: \xffa");
        assert!(output.truncated);
    }
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("long-filename");
    fs::write(&path, b"x\nx\nx\n").unwrap();
    let output = read::FindQuery::new(None, Some("x"), false, None, Some(5))
        .unwrap()
        .run(&[path, directory.path().join("missing")])
        .unwrap();
    assert!(output.into_text().contains("[1 load error(s);"));
}

#[test]
fn anchor_end_occurrence_missing_and_empty_interval_failures_are_explicit() {
    let start_regex = regex::Regex::new("^BEGIN$").unwrap();
    let end_regex = regex::Regex::new("^END$").unwrap();
    for regex in [false, true] {
        let locator = |start_occ, end_occ, inclusive| {
            if regex {
                Locator::BetweenRe {
                    start: start_regex.clone(),
                    end: end_regex.clone(),
                    start_occ,
                    end_occ,
                    inclusive,
                }
            } else {
                Locator::Between {
                    start: b"BEGIN".to_vec(),
                    end: b"END".to_vec(),
                    start_occ,
                    end_occ,
                    inclusive,
                }
            }
        };
        let content = b"BEGIN\na\nEND\nb\nEND\n";
        assert_eq!(
            location::resolve(&locator(1, 2, false), content).unwrap(),
            Resolved {
                start_line: 2,
                end_line: 4
            }
        );
        assert!(matches!(
            location::resolve(&locator(2, 1, false), content),
            Err(LocateError::BetweenStartNotFound { .. })
        ));
        assert!(matches!(
            location::resolve(&locator(1, 3, false), content),
            Err(LocateError::BetweenEndNotFound { .. })
        ));
        assert!(matches!(
            location::resolve(&locator(1, 1, false), b"BEGIN\nEND\n"),
            Err(LocateError::InvalidRange { .. })
        ));
        assert!(matches!(
            location::resolve(&locator(1, 0, true), content),
            Err(LocateError::ZeroLine)
        ));
    }
}

#[test]
fn read_writers_propagate_io_failures_and_cli_payloads_preserve_invalid_utf8() {
    struct Fail;
    impl std::io::Write for Fail {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("injected"))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let options = ops::peek::PeekOpts {
        range: ops::peek::PeekRange::All,
        max_lines: None,
    };
    assert!(matches!(
        ops::peek::run(b"x\n", &options, Fail),
        Err(ops::peek::PeekError::Io(_))
    ));
    let hits = vec![(
        Path::new("f").into(),
        vec![ops::find::Hit { line: 1, col: 0 }],
    )];
    assert!(ops::find::write_hits(&hits, false, None, Fail).is_err());
    let result = read::PeekQuery::new(None, None, true, 0, Some(0), Some(0))
        .unwrap()
        .render(b"\xff\0\r\n")
        .unwrap();
    assert_eq!(result.bytes, b"     1: \xff\0\r\n");
    assert!(
        read::PeekQuery::new(None, None, true, 0, None, None)
            .unwrap()
            .render(b"")
            .unwrap()
            .bytes
            .is_empty()
    );
}

#[cfg(unix)]
#[test]
fn atomic_replacement_retains_unix_modes_and_rejects_symlink_backups() {
    use std::os::unix::{fs::PermissionsExt, fs::symlink};
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("file");
    let unrelated = directory.path().join("unrelated");
    fs::write(&path, b"old").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o751)).unwrap();
    atomic::atomic_write(&path, b"new", true).unwrap();
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o751
    );
    assert_eq!(
        fs::metadata(atomic::bak_path(&path))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o751
    );
    fs::remove_file(atomic::bak_path(&path)).unwrap();
    fs::write(&unrelated, b"untouched").unwrap();
    symlink(&unrelated, atomic::bak_path(&path)).unwrap();
    assert!(atomic::atomic_write(&path, b"blocked", true).is_err());
    assert_eq!(fs::read(path).unwrap(), b"new");
    assert_eq!(fs::read(unrelated).unwrap(), b"untouched");
}

use super::*;
use location::{Count, LocateError, Locator, Resolved};
use ops::insert::Position;
use std::fs;
use std::path::Path;

#[test]
fn line_locations_preserve_lf_crlf_and_unterminated_bytes() {
    for (bytes, expected) in [
        (
            b"a\nb\n".as_slice(),
            vec![b"a\n".as_slice(), b"b\n".as_slice()],
        ),
        (
            b"a\r\n\r\nb".as_slice(),
            vec![b"a\r\n".as_slice(), b"\r\n", b"b"],
        ),
        (
            "中\n🙂".as_bytes(),
            vec!["中\n".as_bytes(), "🙂".as_bytes()],
        ),
    ] {
        for (index, line) in expected.iter().enumerate() {
            let number = index + 1;
            assert_eq!(
                location::resolve(
                    &Locator::Lines {
                        start: number,
                        end: number
                    },
                    bytes
                )
                .unwrap(),
                Resolved {
                    start_line: number,
                    end_line: number
                }
            );
            assert_eq!(
                location::extract_line_range_with_newline(bytes, number, number),
                *line
            );
        }
        assert!(matches!(
            location::resolve(
                &Locator::Lines {
                    start: 1,
                    end: expected.len() + 1
                },
                bytes
            ),
            Err(LocateError::OutOfBounds { .. })
        ));
    }
    assert!(location::split_lines(b"").is_empty());
    for (start, end) in [(0, 1), (1, 0), (2, 1), (1, 9)] {
        assert!(location::resolve(&Locator::Lines { start, end }, b"a\n").is_err());
        assert!(location::extract_line_range_with_newline(b"a\n", start, end).is_empty());
    }
}

#[test]
fn anchor_locations_share_occurrence_and_inclusion_contracts() {
    let bytes = b"BEGIN\nx\nEND\nBEGIN\ny\nEND\n";
    let start_regex = regex::Regex::new("^BEGIN$").unwrap();
    let end_regex = regex::Regex::new("^END$").unwrap();
    for occurrence in 1..=2 {
        for inclusive in [false, true] {
            let literal = Locator::Between {
                start: b"BEGIN".to_vec(),
                end: b"END".to_vec(),
                start_occ: occurrence,
                end_occ: 1,
                inclusive,
            };
            let pattern = Locator::BetweenRe {
                start: start_regex.clone(),
                end: end_regex.clone(),
                start_occ: occurrence,
                end_occ: 1,
                inclusive,
            };
            let start = (occurrence - 1) * 3 + if inclusive { 1 } else { 2 };
            let end = (occurrence - 1) * 3 + if inclusive { 3 } else { 2 };
            for locator in [literal, pattern] {
                assert_eq!(
                    location::resolve(&locator, bytes).unwrap(),
                    Resolved {
                        start_line: start,
                        end_line: end
                    }
                );
            }
        }
        let locator = Locator::MatchLine {
            regex: start_regex.clone(),
            occurrence,
        };
        assert_eq!(
            location::resolve(&locator, bytes).unwrap().start_line,
            (occurrence - 1) * 3 + 1
        );
    }
    for (start, end) in [
        (b"".as_slice(), b"END".as_slice()),
        (b"BEGIN".as_slice(), b"".as_slice()),
    ] {
        let locator = Locator::Between {
            start: start.to_vec(),
            end: end.to_vec(),
            start_occ: 1,
            end_occ: 1,
            inclusive: true,
        };
        assert!(matches!(
            location::resolve(&locator, bytes),
            Err(LocateError::EmptyAnchor)
        ));
    }
    for occurrence in [0, 3] {
        assert!(
            location::resolve(
                &Locator::MatchLine {
                    regex: start_regex.clone(),
                    occurrence
                },
                bytes
            )
            .is_err()
        );
    }
    let pattern = Locator::Pattern {
        regex: regex::bytes::Regex::new("x").unwrap(),
        count: Count::All,
    };
    assert!(matches!(
        location::resolve(&pattern, bytes),
        Err(LocateError::PatternNotResolvable)
    ));
}

#[test]
fn replacement_ranges_match_an_independent_byte_slice_oracle() {
    for source in [
        b"a\r\n\nlast".as_slice(),
        b"a\nb\nc\n",
        b"\xff\n\0\n",
        "一\n二\n三".as_bytes(),
    ] {
        let lines: Vec<_> = source.split_inclusive(|byte| *byte == b'\n').collect();
        for start in 1..=lines.len() {
            for end in start..=lines.len() {
                for payload in [b"".as_slice(), b"X", b"X\nY\n", b"$1\r\n", b"\xfe"] {
                    let offset: usize = lines[..start - 1].iter().map(|line| line.len()).sum();
                    let finish: usize = lines[..end].iter().map(|line| line.len()).sum();
                    let mut expected = source[..offset].to_vec();
                    expected.extend_from_slice(payload);
                    if !payload.is_empty()
                        && !payload.ends_with(b"\n")
                        && lines[end - 1].ends_with(b"\n")
                    {
                        expected.push(b'\n');
                    }
                    expected.extend_from_slice(&source[finish..]);
                    assert_eq!(
                        ops::replace::replace_range(source, start, end, payload).unwrap(),
                        expected
                    );
                }
            }
        }
    }
    for (start, end) in [(0, 1), (1, 0), (2, 1), (1, 9)] {
        assert!(ops::replace::replace_range(b"a\n", start, end, b"x").is_err());
    }
}

#[test]
fn regex_replacement_handles_captures_limits_binary_and_empty_matches() {
    let regex = regex::bytes::Regex::new(r"(?P<key>[a-z])=(\d)").unwrap();
    for (count, expected, replacements) in [
        (Count::First(0), "a=1 b=2", 0),
        (Count::First(1), "1:a b=2", 1),
        (Count::All, "1:a 2:b", 2),
    ] {
        assert_eq!(
            ops::replace::replace_pattern(b"a=1 b=2", &regex, "${2}:${key}", count),
            (expected.as_bytes().to_vec(), replacements)
        );
    }
    let regex = regex::bytes::Regex::new("x").unwrap();
    assert_eq!(
        ops::replace::replace_pattern(b"\xffx\0x", &regex, "literal", Count::All),
        (b"\xffliteral\0literal".to_vec(), 2)
    );
    let regex = regex::bytes::Regex::new("").unwrap();
    assert_eq!(
        ops::replace::replace_pattern(b"ab", &regex, ".", Count::All),
        (b".a.b.".to_vec(), 3)
    );
}

#[test]
fn insertion_contract_covers_empty_file_eof_and_empty_payload() {
    for (source, anchor, position, payload, expected) in [
        (
            b"".as_slice(),
            1,
            Position::Before,
            b"x".as_slice(),
            b"x\n".as_slice(),
        ),
        (
            b"".as_slice(),
            1,
            Position::After,
            b"x".as_slice(),
            b"x\n".as_slice(),
        ),
        (b"a", 1, Position::After, b"x", b"a\nx\n"),
        (b"a\nb", 2, Position::Before, b"x", b"a\nx\nb"),
        (b"a\nb", 2, Position::After, b"", b"a\nb"),
        (b"a\r\nb", 1, Position::Before, b"x\r\n", b"x\r\na\r\nb"),
    ] {
        assert_eq!(
            ops::insert::insert_at(source, anchor, position, payload).unwrap(),
            expected
        );
    }
    for anchor in [0, 2] {
        assert!(ops::insert::insert_at(b"", anchor, Position::Before, b"x").is_err());
    }
    assert_eq!(
        ops::insert::resolve_anchor(&Locator::Lines { start: 1, end: 1 }, b"")
            .unwrap()
            .start_line,
        1
    );
}

#[test]
fn every_small_move_matches_a_sequence_oracle_and_its_inverse() {
    for count in 2..=6 {
        let original: Vec<_> = (1..=count).map(|index| format!("line{index}")).collect();
        for newline in ["", "\n"] {
            let source = original.join("\n") + newline;
            for start in 1..=count {
                for end in start..=count {
                    for target in 1..=count {
                        for position in [Position::Before, Position::After] {
                            if (start..=end).contains(&target) {
                                assert!(
                                    ops::move_op::move_lines(
                                        source.as_bytes(),
                                        start,
                                        end,
                                        target,
                                        position
                                    )
                                    .is_err()
                                );
                                continue;
                            }
                            let mut expected = original.clone();
                            let moved: Vec<_> = expected.drain(start - 1..end).collect();
                            let target_name = &original[target - 1];
                            let insertion = expected
                                .iter()
                                .position(|line| line == target_name)
                                .unwrap()
                                + usize::from(position == Position::After);
                            expected.splice(insertion..insertion, moved);
                            let moved = ops::move_op::move_lines(
                                source.as_bytes(),
                                start,
                                end,
                                target,
                                position,
                            )
                            .unwrap();
                            assert_eq!(moved, (expected.join("\n") + newline).as_bytes());
                            let (from, to, anchor, pos) =
                                ops::move_op::reverse_params(start, end, target, position).unwrap();
                            assert_eq!(
                                ops::move_op::move_lines(&moved, from, to, anchor, pos).unwrap(),
                                source.as_bytes()
                            );
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn indentation_changes_only_leading_bytes_inside_the_range() {
    use ops::indent::{IndentOp, apply_indent};
    for (op, expected) in [
        (
            IndentOp::Add(2),
            b"keep\n  \tvalue\tinside\n    two".as_slice(),
        ),
        (IndentOp::Remove(9), b"keep\n\tvalue\tinside\ntwo"),
        (IndentOp::TabsToSpaces(4), b"keep\n    value\tinside\n  two"),
        (IndentOp::SpacesToTabs(2), b"keep\n\tvalue\tinside\n\ttwo"),
        (IndentOp::Add(0), b"keep\n\tvalue\tinside\n  two"),
    ] {
        assert_eq!(
            apply_indent(b"keep\n\tvalue\tinside\n  two", 2, 3, op).unwrap(),
            expected
        );
    }
    for op in [IndentOp::TabsToSpaces(0), IndentOp::SpacesToTabs(0)] {
        assert!(apply_indent(b"x\n", 1, 1, op).is_err());
    }
    for (start, end) in [(0, 1), (1, 0), (2, 1), (1, 9)] {
        assert!(apply_indent(b"x\n", start, end, IndentOp::Add(1)).is_err());
    }
}

#[test]
fn diff_text_reports_changes_eof_and_unicode_without_altering_plain_content() {
    let path = Path::new("sample.txt");
    assert_eq!(diff::unified_diff(path, b"same", b"same"), "");
    let escapes = regex::Regex::new("\u{1b}\\[[0-9;]*m").unwrap();
    for (before, after, removed, added) in [
        (b"old\n".as_slice(), b"new\n".as_slice(), "-old", "+new"),
        (b"", "中文🙂".as_bytes(), "", "+中文🙂"),
        (b"gone", b"", "-gone", ""),
    ] {
        let plain = diff::unified_diff(path, before, after);
        assert!(plain.contains("--- sample.txt"));
        assert!(plain.contains("+++ sample.txt"));
        assert!(plain.contains(removed) && plain.contains(added));
        if (!before.is_empty() && !before.ends_with(b"\n"))
            || (!after.is_empty() && !after.ends_with(b"\n"))
        {
            assert!(plain.contains("\\ No newline at end of file"));
        }
        let colored = diff::colorize_unified_diff(&plain);
        assert_eq!(escapes.replace_all(&colored, ""), plain);
    }
}

#[test]
fn content_sources_and_literal_reverts_have_explicit_error_contracts() {
    use content::Content;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("payload");
    let bytes = b"\xff\0\r\n";
    fs::write(&path, bytes).unwrap();
    for source in [Content::Inline(bytes.to_vec()), Content::File(path.clone())] {
        assert_eq!(source.load().unwrap(), bytes);
        assert_eq!(content::load(&source).unwrap(), bytes);
        assert!(source.as_replacement().is_none());
    }
    assert_eq!(content::load_path(&path).unwrap(), bytes);
    let replacement = Content::Replacement("$1".into());
    assert!(replacement.load().is_err());
    assert_eq!(replacement.as_replacement(), Some("$1"));
    assert!(Content::File(dir.path().join("missing")).load().is_err());
    for flag in [false, true] {
        assert_eq!(revert::Direction::from_flag(flag).is_revert(), flag);
    }
    assert_eq!(
        revert::invert_pattern_replacement("old", "a.b").unwrap(),
        ("a\\.b".into(), "old".into())
    );
    for (pattern, replacement) in [("", "x"), ("a", ""), ("a.*", "x"), ("a", "$1")] {
        assert!(revert::invert_pattern_replacement(pattern, replacement).is_err());
    }
}

#[test]
fn staged_atomic_writes_keep_original_until_commit_and_backup_exactly() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("name.ext");
    fs::write(&path, b"old\r\n").unwrap();
    let staged = atomic::prepare_write(&path, b"new").unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"old\r\n");
    assert_eq!(fs::read(staged.path()).unwrap(), b"new");
    drop(staged);
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    atomic::atomic_write(&path, b"next", true).unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"next");
    assert_eq!(
        fs::read(dir.path().join("name.ext.bak")).unwrap(),
        b"old\r\n"
    );
    atomic::atomic_write(&path, b"final", true).unwrap();
    assert_eq!(fs::read(atomic::bak_path(&path)).unwrap(), b"next");
    assert!(atomic::atomic_write(&dir.path().join("missing/file"), b"x", false).is_err());
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&path, permissions).unwrap();
    assert!(atomic::atomic_write(&path, b"blocked", false).is_err());
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    permissions.set_readonly(false);
    fs::set_permissions(path, permissions).unwrap();
}

#[test]
fn literal_edits_validate_ambiguity_and_keep_preview_check_side_effect_free() {
    use edit::{Operation, Request};
    for (check, dry_run) in [(true, false), (false, true), (false, false)] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("new/deep/file");
        let request = Request {
            operation: Operation::Create,
            text: Some(b"a\r\nb".to_vec()),
            check,
            dry_run,
            ..Request::default()
        };
        let result = edit::execute(&path, &request).unwrap();
        assert!(result.changed);
        assert_eq!(path.exists(), !check && !dry_run);
        if check || dry_run {
            assert!(!dir.path().join("new").exists());
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("file");
    fs::write(&path, b"aaa").unwrap();
    let mut request = Request {
        old: Some(b"aa".to_vec()),
        text: Some(b"x".to_vec()),
        ..Request::default()
    };
    assert!(edit::execute(&path, &request).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"aaa");
    request.old = Some(b"aaa".to_vec());
    request.text = Some(b"aaa".to_vec());
    assert!(!edit::execute(&path, &request).unwrap().changed);
    request.text = Some(b"$1\r\n".to_vec());
    assert_eq!(edit::execute(&path, &request).unwrap().replacements, 1);
    assert_eq!(fs::read(&path).unwrap(), b"$1\r\n");
}

#[test]
fn bounded_peek_defaults_apply_to_every_selection_and_zero_means_unlimited() {
    let content = (1..=250).map(|_| "hit\n").collect::<String>();
    for (lines, pattern, all) in [
        (None, None, true),
        (Some("1-250"), None, false),
        (None, Some("hit"), false),
    ] {
        let query = read::PeekQuery::new(lines, pattern, all, 0, None, None).unwrap();
        let output = query.render(content.as_bytes()).unwrap();
        assert_eq!(output.items, 200);
        assert!(output.truncated);
        assert!(output.into_text().contains("[truncated:"));
        let query = read::PeekQuery::new(lines, pattern, all, 0, Some(0), Some(0)).unwrap();
        let output = query.render(content.as_bytes()).unwrap();
        assert_eq!(output.items, 250);
        assert!(!output.truncated);
    }
    for limit in 1..24 {
        let output = read::PeekQuery::new(None, None, true, 0, Some(0), Some(limit))
            .unwrap()
            .render("中文🙂abcdefghijklmnopqrstuvwxyz".as_bytes())
            .unwrap();
        assert!(output.truncated);
        assert!(output.bytes.len() <= limit);
        assert!(!output.text().contains('\u{fffd}'));
    }
    let content = "x".repeat(1_000_000);
    let output = read::PeekQuery::new(None, None, true, 0, None, None)
        .unwrap()
        .render(content.as_bytes())
        .unwrap();
    assert_eq!(output.bytes.len(), read::DEFAULT_MAX_BYTES);
    assert!(output.truncated);
}

#[test]
fn peek_context_merges_ranges_saturates_huge_context_and_validates_selection() {
    let query =
        read::PeekQuery::new(None, Some("hit"), false, usize::MAX, Some(0), Some(0)).unwrap();
    let result = query.render(b"a\nhit\nb\nhit\nc\n").unwrap();
    assert_eq!(result.items, 5);
    assert_eq!(result.text().lines().count(), 5);
    for (lines, pattern, all, context) in [
        (None, None, false, 0),
        (Some("1"), None, true, 0),
        (Some("1"), None, false, 1),
        (Some("0"), None, false, 0),
        (Some("2-1"), None, false, 0),
        (None, Some("["), false, 0),
    ] {
        assert!(read::PeekQuery::new(lines, pattern, all, context, None, None).is_err());
    }
    assert!(
        read::PeekQuery::new(None, Some("missing"), false, 0, None, None)
            .unwrap()
            .render(b"x\n")
            .is_err()
    );
}

#[test]
fn bounded_search_limits_total_hits_keeps_file_order_and_reports_late_io_errors() {
    let dir = tempfile::tempdir().unwrap();
    let files: Vec<_> = ["first", "second"].map(|name| dir.path().join(name)).into();
    for path in &files {
        fs::write(path, "x\n".repeat(125)).unwrap();
    }
    let query = read::FindQuery::new(None, Some("x"), false, None, None).unwrap();
    let result = query.run(&files).unwrap();
    assert_eq!(result.items, 100);
    assert!(result.truncated);
    assert!(
        result
            .text()
            .starts_with(&format!("{}:1\n", files[0].display()))
    );
    let query = read::FindQuery::new(Some("x"), None, false, Some(0), Some(0)).unwrap();
    let result = query.run(&files).unwrap();
    assert_eq!(result.items, 250);
    assert!(!result.truncated);
    let result = read::FindQuery::new(None, Some("x"), true, None, None)
        .unwrap()
        .run(&files)
        .unwrap();
    assert_eq!(result.items, 2);
    assert!(!result.truncated);
    let mut files = files;
    files.push(dir.path().join("missing"));
    let result = read::FindQuery::new(None, Some("x"), false, Some(1), None)
        .unwrap()
        .run(&files)
        .unwrap();
    assert_eq!(result.items, 1);
    assert_eq!(result.load_error_count, 1);
    assert!(result.into_text().contains("load error"));
    for (pattern, line) in [(None, None), (Some("x"), Some("x")), (Some("["), None)] {
        assert!(read::FindQuery::new(pattern, line, false, None, None).is_err());
    }
    assert!(query.run(&[]).is_err());
}

#[test]
fn low_level_find_caps_and_positions_match_both_scan_modes() {
    use ops::find::{FindMode, FindOpts, Hit, scan, write_hits};
    let line = regex::Regex::new("x").unwrap();
    let pattern = regex::bytes::Regex::new("x").unwrap();
    for cap in [0, 1, 2, 99] {
        for mode in [FindMode::MatchLine(&line), FindMode::Pattern(&pattern)] {
            let result = scan(
                b"x\na x\nx",
                &FindOpts {
                    mode,
                    max_matches: Some(cap),
                    first_only: false,
                },
            );
            assert_eq!(result.len(), cap.min(3));
        }
    }
    let hits = scan(
        "中x\n🙂x".as_bytes(),
        &FindOpts {
            mode: FindMode::Pattern(&pattern),
            max_matches: None,
            first_only: false,
        },
    );
    assert_eq!(hits, vec![Hit { line: 1, col: 4 }, Hit { line: 2, col: 5 }]);
    let mut bytes = Vec::new();
    assert_eq!(
        write_hits(
            &[(Path::new("f").to_path_buf(), hits)],
            true,
            Some(1),
            &mut bytes
        )
        .unwrap(),
        1
    );
    assert_eq!(bytes, b"f:1:4\n");
}

#[test]
fn transaction_plan_coalesces_lifecycle_and_refuses_external_changes() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a");
    let b = dir.path().join("b");
    fs::write(&a, b"old").unwrap();
    let mut plan = transaction::Plan::default();
    plan.create(&b, b"temporary".to_vec()).unwrap();
    plan.delete(&b, Some(b"temporary")).unwrap();
    assert!(plan.preview().files.is_empty());
    plan.rename(&a, &b, Some(b"old")).unwrap();
    plan.update(&b, b"new".to_vec()).unwrap();
    assert_eq!(plan.read(&b).unwrap(), b"new");
    assert!(plan.read(&a).is_err());
    assert_eq!(fs::read(&a).unwrap(), b"old");
    plan.commit(false).unwrap();
    assert!(!a.exists());
    assert_eq!(fs::read(&b).unwrap(), b"new");
    let mut plan = transaction::Plan::default();
    plan.update(&b, b"planned".to_vec()).unwrap();
    fs::write(&b, b"external").unwrap();
    assert!(plan.commit(false).is_err());
    assert_eq!(fs::read(&b).unwrap(), b"external");
}
