//! Reader contracts are bounded by default, explicit about truncation and read-only.
mod common;
use common::{Workspace, utf8};

#[test]
fn peek_numbered_ranges_and_merged_context_preserve_unicode_and_eof_content() {
    let w = Workspace::new(&[("a", "head\n你好🌟\nmid\n你好🌟\nEOF".as_bytes())]);
    w.cli(&["peek", "a", "--lines", "2"])
        .assert()
        .success()
        .stdout("     2: 你好🌟\n");
    w.cli(&["peek", "a", "--lines", "4-5"])
        .assert()
        .success()
        .stdout("     4: 你好🌟\n     5: EOF\n");
    w.cli(&["peek", "a", "--match-line", "你好", "--context", "1"])
        .assert()
        .success()
        .stdout("     1: head\n     2: 你好🌟\n     3: mid\n     4: 你好🌟\n     5: EOF\n");
    w.assert_bytes("a", "head\n你好🌟\nmid\n你好🌟\nEOF".as_bytes());
}

#[test]
fn peek_line_limits_apply_to_every_selection_mode_and_zero_is_unlimited() {
    let source = "x\n".repeat(205);
    let w = Workspace::new(&[("a", source.as_bytes())]);
    for locator in [
        vec!["--all"],
        vec!["--lines", "1-205"],
        vec!["--match-line", "x"],
    ] {
        let output = w
            .cli(&["peek", "a"])
            .args(&locator)
            .assert()
            .success()
            .get_output()
            .clone();
        assert_eq!(utf8(&output.stdout).lines().count(), 200);
        assert!(utf8(&output.stderr).contains("truncated"));
        let output = w
            .cli(&["peek", "a"])
            .args(&locator)
            .args(["--max-lines", "2"])
            .assert()
            .success()
            .get_output()
            .clone();
        assert_eq!(utf8(&output.stdout).lines().count(), 2);
        assert!(utf8(&output.stderr).contains("truncated"));
        let output = w
            .cli(&["peek", "a"])
            .args(locator)
            .args(["--max-lines", "0", "--max-bytes", "0"])
            .assert()
            .success()
            .get_output()
            .clone();
        assert_eq!(utf8(&output.stdout).lines().count(), 205);
        assert!(!utf8(&output.stderr).contains("truncated"));
    }
    w.assert_bytes("a", source.as_bytes());
}

#[test]
fn peek_byte_budget_handles_a_single_giant_line_without_splitting_utf8() {
    let source = "好".repeat(10_000);
    let w = Workspace::new(&[("a", source.as_bytes())]);
    for budget in [None, Some("32")] {
        let mut command = w.cli(&["peek", "a", "--all"]);
        if let Some(limit) = budget {
            command.args(["--max-bytes", limit]);
        }
        let output = command.assert().success().get_output().clone();
        assert!(output.stdout.len() <= if budget.is_some() { 32 } else { 12_000 });
        assert!(!utf8(&output.stdout).contains('�'));
        assert!(utf8(&output.stderr).contains("truncated"));
    }
    let output = w
        .cli(&["peek", "a", "--all", "--max-bytes", "0"])
        .assert()
        .success()
        .get_output()
        .clone();
    assert!(utf8(&output.stdout).contains(&source));
    assert!(!utf8(&output.stderr).contains("truncated"));
}

#[test]
fn find_preserves_file_order_and_byte_columns_for_cross_line_patterns() {
    let w = Workspace::new(&[("a", "你好x\nnext x\n".as_bytes()), ("b", b"x\nx\n")]);
    w.cli(&["find", "b", "a", "--match-line", "x"])
        .assert()
        .success()
        .stdout("b:1\nb:2\na:1\na:2\n");
    w.cli(&["find", "a", "--pattern", "x"])
        .assert()
        .success()
        .stdout("a:1:7\na:2:3\na:2:6\n");
    w.cli(&["find", "a", "--pattern", "x\nnext"])
        .assert()
        .success()
        .stdout("a:1:7\n");
    w.cli(&["find", "b", "a", "--pattern", "x", "--first-only"])
        .assert()
        .success()
        .stdout("b:1:1\na:1:7\n");
}

#[test]
fn find_global_match_limits_have_the_same_semantics_in_both_modes() {
    let source = "x\n".repeat(70);
    let w = Workspace::new(&[("a", source.as_bytes()), ("b", source.as_bytes())]);
    for locator in ["--match-line", "--pattern"] {
        for (limit, expected, truncated) in [
            (None, 100, true),
            (Some("2"), 2, true),
            (Some("0"), 140, false),
        ] {
            let mut command = w.cli(&["find", "a", "b", locator, "x"]);
            if let Some(value) = limit {
                command.args(["--max-matches", value]);
            }
            let output = command.assert().success().get_output().clone();
            assert_eq!(utf8(&output.stdout).lines().count(), expected);
            assert_eq!(utf8(&output.stderr).contains("truncated"), truncated);
        }
    }
}

#[test]
fn find_byte_budget_is_independent_from_match_budget() {
    let w = Workspace::new(&[("long-file-name", b"x\nx\nx\n")]);
    let bounded = w
        .cli(&[
            "find",
            "long-file-name",
            "--pattern",
            "x",
            "--max-bytes",
            "18",
        ])
        .assert()
        .success()
        .get_output()
        .clone();
    assert!(bounded.stdout.len() <= 18);
    assert!(utf8(&bounded.stderr).contains("truncated"));
    let full = w
        .cli(&[
            "find",
            "long-file-name",
            "--pattern",
            "x",
            "--max-bytes",
            "0",
        ])
        .assert()
        .success()
        .get_output()
        .clone();
    assert_eq!(utf8(&full.stdout).lines().count(), 3);
    assert!(!utf8(&full.stderr).contains("truncated"));
}

#[test]
fn find_checks_unreadable_inputs_even_after_the_match_limit_was_reached() {
    let w = Workspace::new(&[("a", b"x\nx\n")]);
    for limit in ["1", "100"] {
        w.cli(&[
            "find",
            "a",
            "missing",
            "--pattern",
            "x",
            "--max-matches",
            limit,
        ])
        .assert()
        .code(2);
    }
    w.cli(&["find", "a", "missing", "--pattern", "x"])
        .assert()
        .code(2)
        .stdout("a:1:1\na:2:1\n");
}

#[test]
fn empty_files_and_no_matches_are_distinct_from_io_errors() {
    let w = Workspace::new(&[("empty", b""), ("a", b"content\n")]);
    w.cli(&["peek", "empty", "--all"])
        .assert()
        .success()
        .stdout("");
    w.cli(&["find", "empty", "a", "--pattern", "missing"])
        .assert()
        .code(1)
        .stdout("");
    w.cli(&["peek", "missing", "--all"]).assert().code(2);
    w.cli(&["find", "missing", "--pattern", "x"])
        .assert()
        .code(2);
}

#[test]
fn invalid_reader_locators_are_usage_errors() {
    let w = Workspace::new(&[("a", b"one\ntwo\n")]);
    for args in [
        vec!["peek", "a"],
        vec!["peek", "a", "--all", "--lines", "1"],
        vec!["peek", "a", "--lines", "99"],
        vec!["peek", "a", "--lines", "0"],
        vec!["peek", "a", "--match-line", "["],
        vec!["find", "a"],
        vec!["find", "a", "--pattern", "x", "--match-line", "x"],
        vec!["find", "a", "--pattern", "["],
    ] {
        w.cli(&args).assert().code(1);
    }
    w.assert_bytes("a", b"one\ntwo\n");
}
