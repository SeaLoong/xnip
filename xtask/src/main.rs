//! `cargo xtask` — 仓库级辅助任务。
//!
//! 子命令：
//! - `sync-integrations` — 同步 generic 和 GitHub skill 及其参考文档。
//!   其它 integration（claude-code/cursor/aider/copilot/agents-md）保留各自风格的人工维护版本。
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
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // xtask manifest dir 是 xtask/，仓库根是上一级
    let repo_root = manifest_dir.parent().map_or_else(
        || std::path::PathBuf::from("."),
        std::path::Path::to_path_buf,
    );
    let src = repo_root.join("docs").join("SKILL.md");
    let out_dir = repo_root.join("integrations").join("generic");

    if !src.exists() {
        eprintln!("xtask: missing source: {}", src.display());
        return ExitCode::from(1);
    }
    if let Err(e) = std::fs::create_dir_all(&out_dir) {
        eprintln!("xtask: failed to create {}: {e}", out_dir.display());
        return ExitCode::from(1);
    }
    let dst = out_dir.join("SKILL.md");
    if let Err(e) = std::fs::copy(&src, &dst) {
        eprintln!("xtask: copy failed: {e}");
        return ExitCode::from(1);
    }
    eprintln!("xtask: synced {} → {}", src.display(), dst.display());
    let skill_dir = repo_root.join(".github/skills/xnip-editing");
    let result = (|| -> std::io::Result<()> {
        std::fs::create_dir_all(skill_dir.join("references"))?;
        let body = std::fs::read_to_string(&src)?
            .replace("(./apply-format.md)", "(./references/apply-format.md)");
        std::fs::write(
            skill_dir.join("SKILL.md"),
            format!(
                "---\nname: xnip-editing\ndescription: Precise text edits with xnip CLI, batch validation and per-file atomic writes.\n---\n\n{body}"
            ),
        )?;
        for name in ["apply-format.md", "error-codes.md", "examples.md"] {
            std::fs::copy(
                repo_root.join("docs").join(name),
                skill_dir.join("references").join(name),
            )?;
        }
        Ok(())
    })();
    if let Err(e) = result {
        eprintln!("xtask: GitHub skill sync failed: {e}");
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}
