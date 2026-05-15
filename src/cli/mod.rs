//! CLI 解析与分发，基于 clap derive。
//!
//! 顶层 [`Cli`] 持有 `--quiet/--json/--no-color/--trace` 这类与具体命令解耦的全局开关。
//! 写命令的 `--dry-run/--check/--backup/--was*/--revert` 在每个命令的 `Args` 上重复定义
//! （而非顶层），因为它们仅适用于写命令；这种就近声明对人类阅读 `--help` 更友好。

use std::ffi::OsString;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use crate::output::exit;

mod apply;
pub mod common;
mod find;
mod indent;
mod insert;
mod mcp;
mod move_op;
mod peek;
mod replace;

/// xnip — precise text editing CLI for LLM agents.
#[derive(Debug, Parser)]
#[command(
    name = "xnip",
    version,
    about = "Precise text editing CLI for LLM agents",
    long_about = None,
    propagate_version = true,
    arg_required_else_help = false,
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    /// 压制非错误性 stderr 提示（错误仍输出）。
    #[arg(long, global = true, default_value_t = false)]
    pub quiet: bool,

    /// 禁用 ANSI 颜色输出（`NO_COLOR` 环境变量同样生效）。
    #[arg(long, global = true, default_value_t = false)]
    pub no_color: bool,

    /// 启用详细 trace 日志到 stderr（前缀 `[xnip trace]`）。
    #[arg(long, global = true, default_value_t = false)]
    pub trace: bool,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Print numbered lines in a range (read-only).
    Peek(peek::Args),
    /// Search and locate matches (read-only).
    Find(find::Args),
    /// Replace or delete a region (write).
    Replace(replace::Args),
    /// Insert text before/after a position (write).
    Insert(insert::Args),
    /// Move a line block (write).
    #[command(name = "move")]
    Move(move_op::Args),
    /// Adjust indentation / tab-space conversion (write).
    Indent(indent::Args),
    /// Apply a batch of edits atomically (write).
    Apply(apply::Args),
    /// Run as a Model Context Protocol (MCP) server over stdio.
    Mcp(mcp::Args),
    /// Self-diagnose environment and version.
    Doctor,
}

/// 程序入口：解析 argv 并分发到子命令。
///
/// # Errors
///
/// 仅当出现无法预期的内部错误（如全局 IO 故障）时返回 `Err`；
/// 用户级错误（参数不合法、定位失败、`--was` 不匹配等）通过返回 `ExitCode` + stderr 表达。
pub fn run<I, T>(args: I) -> Result<ExitCode, anyhow::Error>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let cli = match Cli::try_parse_from(args) {
        Ok(cli) => cli,
        Err(e) => {
            // clap 已经把 help / version / 用户错误格式化好；直接打印并按规范退出
            // - help / version 走 stdout 退 0
            // - 解析错误走 stderr 退 EXIT_USAGE
            return Ok(handle_clap_exit(&e));
        }
    };

    // 冻结全局 flag 供所有命令读取（output::globals）。
    // NO_COLOR 环境变量（https://no-color.org/）也视为开启 --no-color。
    let no_color = cli.no_color || std::env::var_os("NO_COLOR").is_some();
    crate::output::globals::init(crate::output::globals::Flags {
        quiet: cli.quiet,
        no_color,
        trace: cli.trace,
    });
    crate::trace!(
        "cli flags: quiet={} no_color={} trace={}",
        cli.quiet,
        no_color,
        cli.trace
    );

    let Some(command) = cli.command else {
        // 没有子命令 → 打 help 退成功（保持 M0 行为）
        let _ = Cli::try_parse_from(["xnip", "--help"]);
        return Ok(ExitCode::SUCCESS);
    };

    let code = match command {
        Command::Peek(a) => peek::run(a)?,
        Command::Find(a) => find::run(a)?,
        Command::Replace(a) => replace::run(a)?,
        Command::Insert(a) => insert::run(a)?,
        Command::Move(a) => move_op::run(a)?,
        Command::Indent(a) => indent::run(a)?,
        Command::Apply(a) => apply::run(a)?,
        Command::Mcp(a) => mcp::run(a)?,
        Command::Doctor => {
            let stdout = std::io::stdout();
            let mut out = stdout.lock();
            crate::doctor::report(&mut out).map_err(|e| anyhow::anyhow!("doctor: {e}"))?;
            exit::SUCCESS
        }
    };

    Ok(ExitCode::from(code))
}

fn handle_clap_exit(err: &clap::Error) -> ExitCode {
    use clap::error::ErrorKind;
    match err.kind() {
        ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => {
            print!("{err}");
            ExitCode::SUCCESS
        }
        _ => {
            eprint!("{err}");
            ExitCode::from(exit::USAGE)
        }
    }
}
