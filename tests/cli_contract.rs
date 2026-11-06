//! Command discovery, output formats and executable documentation are public contracts.
mod common;
use common::{Workspace, json_lines, utf8};
use serde_json::{Value, json};

#[test]
fn native_diagnostics_and_clean_cover_the_empty_installation_journey() {
    let w = Workspace::new(&[]);
    for (directory, code) in [("missing", "state_missing"), ("empty", "state_empty")] {
        if directory == "empty" {
            w.write("empty/operation.lock", b"");
        }
        let output = w
            .cli(&["native", "doctor", "--state-dir", directory])
            .assert()
            .success()
            .get_output()
            .clone();
        let report: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report["code"], code);
        assert_eq!(report["installed"], false);
        assert_eq!(
            report["state_dir"],
            json!(xnip::core::transaction::normalize(&w.path(directory)).unwrap())
        );
        assert!(report["message"].as_str().unwrap().contains("--state-dir"));
        let output = w
            .cli(&["doctor", "--native", "--state-dir", directory])
            .assert()
            .success()
            .get_output()
            .clone();
        assert!(utf8(&output.stdout).contains(code));
        assert!(utf8(&output.stdout).contains("dsh:"));
        for _ in 0..2 {
            let output = w
                .cli(&[
                    "native",
                    "clean",
                    "--state-dir",
                    directory,
                    "--purge-user-data",
                ])
                .assert()
                .success()
                .get_output()
                .clone();
            let report: Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(report["installation_data"], "cleared");
            assert_eq!(report["removed"], json!([]));
            assert_eq!(
                report["retained_coordination_paths"],
                if directory == "empty" {
                    json!([
                        xnip::core::transaction::normalize(&w.path("empty/operation.lock"))
                            .unwrap()
                    ])
                } else {
                    json!([])
                }
            );
        }
    }
    w.absent("missing");
    w.assert_bytes("empty/operation.lock", b"");
}

#[cfg(unix)]
#[test]
fn native_state_cli_preserves_link_targets_when_resolving_state_paths() {
    use std::os::unix::fs::symlink;

    let w = Workspace::new(&[
        ("target/retained.txt", b"state target bytes"),
        ("parent/child/retained.txt", b"ancestor target bytes"),
    ]);
    symlink("target", w.path("linked-state")).unwrap();
    symlink("parent", w.path("linked-parent")).unwrap();
    for state in ["linked-state", "linked-parent/child"] {
        for args in [
            vec!["native", "doctor", "--state-dir", state],
            vec!["doctor", "--native", "--state-dir", state],
            vec!["native", "clean", "--state-dir", state, "--purge-user-data"],
        ] {
            let output = w.cli(&args).assert().failure().get_output().clone();
            assert!(utf8(&output.stderr).contains("symlink"));
            w.assert_bytes("target/retained.txt", b"state target bytes");
            w.assert_bytes("parent/child/retained.txt", b"ancestor target bytes");
            w.absent("target/operation.lock");
            w.absent("parent/child/operation.lock");
            assert_eq!(
                std::fs::read_link(w.path("linked-state")).unwrap(),
                std::path::Path::new("target")
            );
            assert_eq!(
                std::fs::read_link(w.path("linked-parent")).unwrap(),
                std::path::Path::new("parent")
            );
        }
    }
}

#[test]
fn native_diagnostics_preserve_retained_evidence_when_the_manifest_is_missing() {
    let w = Workspace::new(&[("state/blobs/original", b"original bytes")]);
    for args in [
        vec!["native", "doctor", "--state-dir", "state"],
        vec!["doctor", "--native", "--state-dir", "state"],
        vec![
            "native",
            "clean",
            "--state-dir",
            "state",
            "--purge-user-data",
        ],
    ] {
        let output = w.cli(&args).assert().failure().get_output().clone();
        let message = utf8(&output.stderr);
        assert!(message.contains("manifest_missing"));
        assert!(message.contains("install-manifest.json"));
        assert!(message.contains("state"));
        w.assert_bytes("state/blobs/original", b"original bytes");
    }
}

#[test]
fn mcp_reports_an_unavailable_working_directory_before_protocol_startup() {
    let w = Workspace::new(&[]);
    let output = w
        .cli(&["mcp", "--cwd", "missing-project"])
        .write_stdin("")
        .assert()
        .failure()
        .get_output()
        .clone();
    assert_eq!(output.stdout, b"");
    assert!(utf8(&output.stderr).contains("working directory missing-project"));
}

#[test]
fn command_discovery_version_and_doctor_work_without_a_project() {
    let w = Workspace::new(&[]);
    for args in [vec![], vec!["--help"]] {
        let output = w.cli(&args).assert().success().get_output().clone();
        let help = utf8(&output.stdout);
        for command in ["edit", "apply", "peek", "find", "recover", "doctor", "mcp"] {
            assert!(help.contains(command), "{command} missing from help");
        }
    }
    let version = w
        .cli(&["--version"])
        .assert()
        .success()
        .get_output()
        .clone();
    assert!(utf8(&version.stdout).contains(xnip::VERSION));
    w.cli(&["unknown-command"]).assert().code(1);
    let output = w.cli(&["doctor"]).assert().success().get_output().clone();
    let report = utf8(&output.stdout);
    for field in ["xnip", "os:", "arch:", "stdin-tty:", "stdout-tty:"] {
        assert!(report.contains(field), "missing {field}: {report}");
    }
}

#[test]
fn compact_write_is_a_single_line_receipt_and_quiet_suppresses_only_the_receipt() {
    for quiet in [false, true] {
        let w = Workspace::new(&[("a", b"old")]);
        let mut command = w.cli(&["edit", "a", "--old", "old", "--text", "new"]);
        if quiet {
            command.arg("--quiet");
        }
        let output = command.assert().success().get_output().clone();
        assert_eq!(output.stdout, b"");
        assert_eq!(utf8(&output.stderr).lines().count(), usize::from(!quiet));
        if !quiet {
            assert!(utf8(&output.stderr).contains("written"));
        }
        w.assert_bytes("a", b"new");
    }
}

#[test]
fn preview_keeps_diff_in_stdout_and_quiet_does_not_hide_data() {
    for output_mode in ["compact", "human"] {
        for quiet in [false, true] {
            let w = Workspace::new(&[("a", b"old\n")]);
            let mut command = w.cli(&[
                "edit",
                "a",
                "--old",
                "old",
                "--text",
                "new",
                "--dry-run",
                "--no-color",
                "--output",
                output_mode,
            ]);
            if quiet {
                command.arg("--quiet");
            }
            let output = command.assert().success().get_output().clone();
            assert!(utf8(&output.stdout).contains("-old\n+new\n"));
            assert!(!utf8(&output.stdout).contains('\x1b'));
            assert_eq!(output.stderr.is_empty(), quiet);
            w.assert_bytes("a", b"old\n");
        }
    }
}

#[test]
fn json_change_results_are_one_complete_record_for_each_execution_mode() {
    for (flag, mode) in [
        (Some("--dry-run"), "preview"),
        (Some("--check"), "check"),
        (None, "write"),
    ] {
        for batch in [false, true] {
            let w = Workspace::new(&[("a", b"old")]);
            let mut command = if batch {
                w.batch(&json!([{"op":"edit","file":"a","old":"old","text":"new"}]))
            } else {
                w.cli(&["edit", "a", "--old", "old", "--text", "new"])
            };
            command.args(["--output", "json"]);
            if let Some(flag) = flag {
                command.arg(flag);
            }
            let output = command.assert().success().get_output().clone();
            assert_eq!(output.stderr, b"", "{}", utf8(&output.stderr));
            let record: Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(record["mode"], mode);
            assert_eq!(record["written"], mode == "write");
            assert_eq!(record["has_changes"], true);
            assert_eq!(record["files"].as_array().unwrap().len(), 1);
            if mode == "preview" {
                assert!(record["diff"].as_str().unwrap().contains("+new"));
            }
            w.assert_bytes("a", if mode == "write" { b"new" } else { b"old" });
        }
    }
}

#[test]
fn json_record_keeps_complete_preview_while_compact_has_an_explicit_budget() {
    let content = "content\n".repeat(3000);
    let w = Workspace::new(&[("a", b"old")]);
    for mode in ["compact", "human", "json"] {
        let output = w
            .cli(&[
                "edit",
                "a",
                "--old",
                "old",
                "--text",
                &content,
                "--dry-run",
                "--output",
                mode,
            ])
            .assert()
            .success()
            .get_output()
            .clone();
        let stdout = utf8(&output.stdout);
        match mode {
            "compact" => {
                assert!(stdout.contains("truncated"));
                assert!(stdout.len() < 14_000);
            }
            "human" => assert_eq!(stdout.matches("+content").count(), 3000),
            "json" => {
                let value: Value = serde_json::from_str(stdout).unwrap();
                assert_eq!(
                    value["diff"].as_str().unwrap().matches("+content").count(),
                    3000
                );
            }
            _ => unreachable!(),
        }
        w.assert_bytes("a", b"old");
    }
}

#[test]
fn json_result_and_legacy_event_stream_are_separate_and_cannot_be_mixed() {
    let w = Workspace::new(&[("a", b"old\n")]);
    let manifest = json!([{"op":"replace","file":"a","lines":1,"text":"new"}]);
    for check in [false, true] {
        let mut command = w.batch(&manifest);
        command.arg("--json");
        if check {
            command.arg("--check");
        }
        let output = command.assert().success().get_output().clone();
        let events = json_lines(&output.stdout);
        assert_eq!(events.first().unwrap()["event"], "start");
        assert_eq!(events.last().unwrap()["event"], "done");
        assert_eq!(output.stderr, b"");
    }
    w.batch(&manifest)
        .args(["--json", "--output", "json"])
        .assert()
        .failure();
    w.batch(&manifest)
        .args(["--json", "--dry-run"])
        .assert()
        .code(1);
    let output = w
        .batch(&json!([{"op":"replace","file":"a","lines":99,"text":"bad"}]))
        .arg("--json")
        .assert()
        .code(3)
        .get_output()
        .clone();
    let events = json_lines(&output.stdout);
    assert_eq!(events.first().unwrap()["event"], "start");
    assert_eq!(events.last().unwrap()["event"], "error");
}

#[test]
fn trace_quiet_and_no_color_flags_are_global_and_errors_remain_visible() {
    let w = Workspace::new(&[("a", b"old\n")]);
    let default = w.cli(&["doctor"]).assert().success().get_output().clone();
    assert!(!utf8(&default.stderr).contains("[xnip trace]"));
    for args in [vec!["--trace", "doctor"], vec!["doctor", "--trace"]] {
        let output = w.cli(&args).assert().success().get_output().clone();
        assert!(utf8(&output.stderr).contains("[xnip trace]"));
    }
    w.cli(&["--quiet", "edit", "a", "--old", "missing", "--text", "bad"])
        .assert()
        .code(3)
        .stderr(predicates::str::is_empty().not());
    for no_color in [true, false] {
        let mut command = w.cli(&["edit", "a", "--old", "old", "--text", "new", "--dry-run"]);
        if no_color {
            command.arg("--no-color");
        } else {
            command.env("NO_COLOR", "1");
        }
        let output = command.assert().success().get_output().clone();
        assert!(!utf8(&output.stdout).contains('\x1b'));
    }
}

#[test]
fn legacy_write_commands_keep_quiet_and_check_receipts_consistent() {
    for args in [
        vec!["replace", "a", "--lines", "1", "--text", "A"],
        vec!["replace", "a", "--pattern", "a", "--repl", "A", "--check"],
        vec!["insert", "a", "--lines", "1", "--text", "new"],
        vec!["move", "a", "--from-lines", "1", "--to", "3"],
        vec!["indent", "a", "--all", "--add", "2"],
    ] {
        for quiet in [false, true] {
            let w = Workspace::new(&[("a", b"a\nb\nc\n")]);
            let mut command = w.cli(&args);
            if quiet {
                command.arg("--quiet");
            }
            let output = command.assert().success().get_output().clone();
            assert_eq!(output.stderr.is_empty(), quiet, "{args:?}");
        }
    }
    for quiet in [false, true] {
        let w = Workspace::new(&[("a", b"old")]);
        let mut command = w.batch(&json!([{"op":"edit","file":"a","old":"old","text":"new"}]));
        if quiet {
            command.arg("--quiet");
        }
        let output = command.assert().success().get_output().clone();
        assert_eq!(utf8(&output.stderr).lines().count(), usize::from(!quiet));
        w.assert_bytes("a", b"new");
    }
}

#[test]
fn legacy_previews_preserve_raw_bytes_and_bound_every_command_even_when_quiet() {
    let unicode = "好".repeat(5000);
    for args in [
        vec!["replace", "a", "--lines", "1", "--text", "new"],
        vec!["insert", "a", "--lines", "1", "--text", "new"],
        vec!["move", "a", "--from-lines", "1", "--to", "2"],
        vec!["indent", "a", "--all", "--add", "2"],
    ] {
        let original = format!("old\n{unicode}\n");
        let w = Workspace::new(&[("a", original.as_bytes())]);
        let full = w
            .cli(&args)
            .args(["--dry-run", "--output", "human"])
            .assert()
            .success()
            .get_output()
            .clone();
        assert!(full.stdout.len() > 12_000);
        for quiet in [false, true] {
            let mut command = w.cli(&args);
            command.arg("--dry-run");
            if quiet {
                command.arg("--quiet");
            }
            let bounded = command.assert().success().get_output().clone();
            assert!(bounded.stdout.len() <= 12_000);
            assert!(full.stdout.starts_with(&bounded.stdout));
            assert!(!utf8(&bounded.stdout).contains('�'));
            assert!(utf8(&bounded.stderr).contains("truncated"));
            w.assert_bytes("a", original.as_bytes());
        }
    }
    let mut original = b"old\n".to_vec();
    original.extend(vec![0xff; 15_000]);
    original.push(b'\n');
    let w = Workspace::new(&[("a", &original)]);
    let full = w
        .cli(&[
            "replace",
            "a",
            "--lines",
            "1",
            "--text",
            "new",
            "--dry-run",
            "--output",
            "human",
        ])
        .assert()
        .success()
        .get_output()
        .clone();
    let bounded = w
        .cli(&[
            "replace",
            "a",
            "--lines",
            "1",
            "--text",
            "new",
            "--dry-run",
            "--quiet",
        ])
        .assert()
        .success()
        .get_output()
        .clone();
    assert_eq!(bounded.stdout.len(), 12_000);
    assert!(full.stdout.starts_with(&bounded.stdout));
    assert!(bounded.stdout.contains(&0xff));
    assert!(utf8(&bounded.stderr).contains("truncated"));
    w.assert_bytes("a", &original);
}

#[test]
fn quiet_readers_still_report_truncation_and_plain_previews_keep_exact_bytes() {
    let source = "x\n".repeat(205);
    let w = Workspace::new(&[("a", source.as_bytes())]);
    for args in [
        vec!["peek", "a", "--all", "--quiet"],
        vec!["find", "a", "--pattern", "x", "--quiet"],
    ] {
        let output = w.cli(&args).assert().success().get_output().clone();
        assert!(utf8(&output.stderr).contains("truncated"));
    }
    w.write("a", b"old\r\n\xffEOF");
    let output = w
        .cli(&["replace", "a", "--lines", "1", "--text", "new", "--dry-run"])
        .assert()
        .success()
        .get_output()
        .clone();
    assert_eq!(output.stdout, b"new\n\xffEOF");
    w.assert_bytes("a", b"old\r\n\xffEOF");
}

fn json_blocks(source: &str) -> impl Iterator<Item = &str> {
    source
        .split("```json")
        .skip(1)
        .map(|block| block.split("```").next().unwrap().trim())
}

#[test]
fn public_documentation_json_examples_are_valid_and_manifest_examples_parse() {
    use xnip::apply::detect::{Format, parse_with};
    let mut manifests = 0;
    for source in [
        include_str!("../docs/apply-format.md"),
        include_str!("../docs/examples.md"),
        include_str!("../docs/cli.md"),
        include_str!("../docs/mcp.md"),
    ] {
        for block in json_blocks(source) {
            let value: Value = serde_json::from_str(block).expect("documented JSON must parse");
            if value.is_array() {
                parse_with(block, Format::Json).expect("documented manifest must parse");
                manifests += 1;
            }
            if let Some(manifest) = value["arguments"]["manifest_text"].as_str() {
                let format = match value["arguments"]["format"].as_str() {
                    Some("json") => Format::Json,
                    Some("yaml") => Format::Yaml,
                    _ => Format::Native,
                };
                parse_with(manifest, format).expect("documented MCP manifest must parse");
            }
        }
    }
    assert!(
        manifests >= 3,
        "fixture stopped checking executable manifests"
    );
}

use predicates::prelude::*;
