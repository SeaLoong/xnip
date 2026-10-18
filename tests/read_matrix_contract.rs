//! Cursor and visitor contracts use output oracles, not fragile memory thresholds.

use regex::{Regex, bytes::Regex as ByteRegex};
use std::fs;
use xnip::core::ops::find::{FindMode, FindOpts, Hit, scan, visit};
use xnip::core::ops::peek::{PeekError, PeekOpts, PeekRange, run};
use xnip::core::read::{FindQuery, PeekQuery};

#[test]
fn online_context_merging_matches_a_selection_oracle_for_every_small_hit_layout() {
    let regex = Regex::new("H").unwrap();
    for count in 0..=6 {
        for mask in 0..(1usize << count) {
            for final_newline in [false, true] {
                let mut content = Vec::new();
                let mut expected_lines = Vec::new();
                for index in 0..count {
                    let line = if mask & (1 << index) != 0 {
                        b"\xffH\r".as_slice()
                    } else {
                        b"miss\r".as_slice()
                    };
                    expected_lines.push(line);
                    content.extend_from_slice(line);
                    if index + 1 < count || final_newline {
                        content.push(b'\n');
                    }
                }
                for context in [0usize, 1, 2, 6, usize::MAX] {
                    let selected: Vec<_> = (0..count)
                        .map(|index: usize| {
                            (0..count).any(|hit: usize| {
                                mask & (1 << hit) != 0 && index.abs_diff(hit) <= context
                            })
                        })
                        .collect();
                    for max_lines in [Some(0), Some(1), Some(3), None] {
                        let options = PeekOpts {
                            range: PeekRange::MatchLine {
                                regex: regex.clone(),
                                context,
                            },
                            max_lines,
                        };
                        let mut actual = Vec::new();
                        let result = run(&content, &options, &mut actual);
                        if mask == 0 {
                            assert!(matches!(result, Err(PeekError::Locate(_))));
                            assert_eq!(actual, b"");
                            continue;
                        }
                        let result = result.unwrap();
                        let cap = max_lines.unwrap_or(usize::MAX);
                        let mut expected = Vec::new();
                        let mut selected_count = 0;
                        for (index, line) in expected_lines.iter().enumerate() {
                            if selected[index] {
                                if selected_count < cap {
                                    expected.extend_from_slice(
                                        format!("{:>6}: ", index + 1).as_bytes(),
                                    );
                                    expected.extend_from_slice(line);
                                    expected.push(b'\n');
                                }
                                selected_count += 1;
                            }
                        }
                        assert_eq!(
                            actual, expected,
                            "count={count}, mask={mask}, context={context}, cap={cap}"
                        );
                        assert_eq!(result.lines_written, selected_count.min(cap));
                        assert_eq!(result.truncated, selected_count > cap);
                    }
                }
            }
        }
    }
}

#[test]
fn numbered_range_validation_never_writes_an_invalid_prefix() {
    for max_lines in [Some(0), Some(1), None] {
        for (start, end) in [(0, 1), (1, 0), (2, 1), (1, 4), (8, 9)] {
            let mut actual = Vec::new();
            assert!(
                run(
                    b"a\r\nb\nc",
                    &PeekOpts {
                        range: PeekRange::Lines { start, end },
                        max_lines,
                    },
                    &mut actual
                )
                .is_err()
            );
            assert_eq!(actual, b"");
        }
    }
}

#[test]
fn visitors_stop_on_request_and_keep_scan_limits_positions_and_first_only_semantics() {
    let line = Regex::new("x").unwrap();
    let pattern = ByteRegex::new("x").unwrap();
    let content = "中x\n🙂x\nx\nx\n";
    for limit in [None, Some(0), Some(1), Some(2), Some(99)] {
        for first_only in [false, true] {
            for use_columns in [false, true] {
                let options = FindOpts {
                    mode: if use_columns {
                        FindMode::Pattern(&pattern)
                    } else {
                        FindMode::MatchLine(&line)
                    },
                    max_matches: limit,
                    first_only,
                };
                let cap = limit
                    .unwrap_or(usize::MAX)
                    .min(if first_only { 1 } else { usize::MAX });
                let expected: Vec<_> = [4, 5, 1, 1]
                    .into_iter()
                    .enumerate()
                    .map(|(index, col)| Hit {
                        line: index + 1,
                        col: if use_columns { col } else { 0 },
                    })
                    .take(cap)
                    .collect();
                assert_eq!(scan(content.as_bytes(), &options), expected);
                let mut actual = Vec::new();
                visit(content.as_bytes(), &options, |hit| {
                    actual.push(hit);
                    true
                });
                assert_eq!(actual, expected);
                let mut stopped = Vec::new();
                visit(content.as_bytes(), &options, |hit| {
                    stopped.push(hit);
                    stopped.len() < 2
                });
                assert_eq!(stopped, expected.into_iter().take(2).collect::<Vec<Hit>>());
            }
        }
    }
}

#[test]
fn bounded_reads_keep_large_dense_inputs_and_late_io_errors_observable() {
    const COUNT: usize = 250_000;
    let content = b"hit\r\n".repeat(COUNT);
    for (lines, pattern, all, context) in [
        (Some("1"), None, false, 0),
        (None, None, true, 0),
        (None, Some("hit"), false, 0),
        (None, Some("hit"), false, usize::MAX),
    ] {
        let result = PeekQuery::new(lines, pattern, all, context, Some(3), Some(17))
            .unwrap()
            .render(&content)
            .unwrap();
        assert_eq!(
            result.bytes,
            if lines.is_some() {
                b"     1: hit\r\n".as_slice()
            } else {
                b"     1: hit\r\n    "
            }
        );
        assert_eq!(result.items, if lines.is_some() { 1 } else { 3 });
        assert_eq!(result.truncated, lines.is_none());
    }
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("dense");
    fs::write(&path, content).unwrap();
    for pattern in [false, true] {
        let result = FindQuery::new(
            pattern.then_some("hit"),
            (!pattern).then_some("hit"),
            false,
            Some(0),
            Some(17),
        )
        .unwrap()
        .run(&[path.clone(), directory.path().join("missing")])
        .unwrap();
        assert_eq!(result.items, COUNT);
        assert!(result.bytes.len() <= 17);
        assert_eq!(result.load_error_count, 1);
        assert!(result.truncated);
        let receipt = result.into_text();
        assert!(receipt.contains("selected 250000 items"));
        assert!(receipt.contains("[1 load error(s);"));
    }
}
