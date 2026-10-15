//! `cargo xtask` — 仓库级辅助任务。
//!
//! 子命令：
//! - `sync-integrations` — 同步 generic、GitHub、Claude skills 及其按需参考文档。
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
    let references = ["apply-format.md", "error-codes.md", "examples.md"];
    let mut body = std::fs::read_to_string(root.join("docs/SKILL.md"))?;
    for name in references {
        body = body.replace(&format!("({name})"), &format!("(references/{name})"));
    }
    for (destination, name) in [
        ("integrations/generic", "xnip-editing"),
        (".github/skills/xnip-editing", "xnip-editing"),
        ("integrations/claude-code", "xnip"),
    ] {
        let directory = root.join(destination);
        std::fs::create_dir_all(directory.join("references"))?;
        std::fs::write(
            directory.join("SKILL.md"),
            body.replacen("name: xnip-editing", &format!("name: {name}"), 1),
        )?;
        for reference in references {
            std::fs::copy(
                root.join("docs").join(reference),
                directory.join("references").join(reference),
            )?;
        }
        eprintln!("xtask: synced {}", directory.display());
    }
    Ok(())
}
