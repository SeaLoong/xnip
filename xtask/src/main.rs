//! `cargo xtask` — 仓库级辅助任务。
//!
//! 子命令：
//! - `sync-integrations` — 同步 Codex 编辑指南及 generic、GitHub、Claude skills。
//!   简短 integration（cursor/aider/copilot/agents-md）保留各自风格的人工维护版本。
//!
//! 用法：`cargo run -p xtask -- <subcommand>`

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("sync-integrations") => sync_integrations(),
        Some(cmd) => {
            eprintln!("xtask: unknown subcommand '{cmd}'");
            eprintln!("  available: sync-integrations");
            ExitCode::from(1)
        }
        None => {
            eprintln!("xtask: missing subcommand");
            eprintln!("  available: sync-integrations");
            ExitCode::from(1)
        }
    }
}

fn sync_integrations() -> ExitCode {
    match sync_skill_files() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("xtask: skill sync failed: {error}");
            ExitCode::from(1)
        }
    }
}

fn sync_skill_files() -> std::io::Result<()> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();
    let references = [
        "apply-format.md",
        "error-codes.md",
        "examples.md",
        "patch.md",
        "recovery.md",
        "edit.md",
        "architecture.md",
        "releasing.md",
    ];
    let mut body = std::fs::read_to_string(root.join("docs/SKILL.md"))?.replace("\r\n", "\n");
    for name in references {
        body = body.replace(&format!("({name})"), &format!("(references/{name})"));
    }
    for (destination, name) in [
        ("integrations/codex", None),
        ("integrations/generic", Some("xnip-editing")),
        (".github/skills/xnip-editing", Some("xnip-editing")),
        ("integrations/claude-code", Some("xnip")),
    ] {
        let directory = root.join(destination);
        std::fs::create_dir_all(directory.join("references"))?;
        let (entrypoint, content) = if let Some(name) = name {
            (
                "SKILL.md",
                body.replacen("name: xnip-editing", &format!("name: {name}"), 1),
            )
        } else {
            let guide = body
                .strip_prefix("---\n")
                .and_then(|text| text.split_once("\n---\n"))
                .map(|(_, text)| text.trim_start())
                .ok_or_else(|| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "missing skill frontmatter",
                    )
                })?;
            (
                "guide.md",
                guide.replace("Loading this skill", "Loading these instructions"),
            )
        };
        std::fs::write(directory.join(entrypoint), content)?;
        for reference in references {
            let reference_text = std::fs::read_to_string(root.join("docs").join(reference))?
                .replace(
                    "(../CONTRIBUTING.md)",
                    "(https://github.com/SeaLoong/xnip/blob/master/CONTRIBUTING.md)",
                );
            std::fs::write(directory.join("references").join(reference), reference_text)?;
        }
        eprintln!("xtask: synced {}", directory.display());
    }
    Ok(())
}
