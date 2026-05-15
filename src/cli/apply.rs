//! `xnip apply <path>` — 批量应用清单文件。
//!
//! PLAN.md §6.9。

use std::io::Read;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::Args as ClapArgs;

use crate::apply::commit::{ExecError, ExecOpts, execute};
use crate::apply::detect::{Format, parse_auto, parse_format_arg, parse_with};
use crate::apply::{Op, OpContent};
use crate::output::exit;

#[derive(Debug, ClapArgs)]
#[allow(clippy::struct_excessive_bools)]
pub struct Args {
    /// 清单文件路径（与 `--from-stdin` 互斥）。
    #[arg(value_name = "MANIFEST", conflicts_with = "from_stdin")]
    pub path: Option<PathBuf>,

    /// 从 stdin 读取清单。
    #[arg(long, default_value_t = false)]
    pub from_stdin: bool,

    /// 显式指定格式：`native` / `json` / `yaml`。不指定走自动识别。
    #[arg(long, value_name = "FORMAT")]
    pub format: Option<String>,

    /// 仅阶段一校验，不写文件。stdout 打 `OK`。
    #[arg(long, default_value_t = false)]
    pub check: bool,

    /// 阶段一 + 输出 unified diff，不写文件。
    #[arg(long, default_value_t = false)]
    pub dry_run: bool,

    /// 写文件前先备份为 `<file>.bak`（默认不写）。
    #[arg(long, default_value_t = false)]
    pub backup: bool,

    /// 以 NDJSON 事件流输出运行进度（stdout 一行一事件）。
    #[arg(long, default_value_t = false)]
    pub json: bool,

    /// 阶段一并行处理的文件数上限（0/1 = 单线程）。
    #[arg(long, value_name = "N", default_value_t = 0)]
    pub parallel: usize,

    /// 以文件提供 op 内 `@-` 的字节流（默认从本进程 stdin 读）。
    /// `--from-stdin` 模式下必须明确使用该选项，因为本进程 stdin 已被清单占用。
    #[arg(long, value_name = "PATH")]
    pub stdin_file: Option<PathBuf>,
}

#[allow(clippy::needless_pass_by_value)]
pub fn run(a: Args) -> Result<u8> {
    if a.path.is_none() && !a.from_stdin {
        bail!("either MANIFEST path or --from-stdin is required");
    }

    // 读取清单内容
    let (src, manifest_dir, hint_path) = if let Some(p) = &a.path {
        let bytes = std::fs::read(p)
            .with_context(|| format!("failed to read manifest: {}", p.display()))?;
        let s = String::from_utf8(bytes).context("manifest must be UTF-8")?;
        let dir = p.parent().map(std::path::Path::to_path_buf);
        (s, dir, Some(p.clone()))
    } else {
        let mut buf = Vec::new();
        std::io::stdin()
            .read_to_end(&mut buf)
            .context("failed to read stdin")?;
        let s = String::from_utf8(buf).context("stdin must be UTF-8")?;
        (s, None, None)
    };

    // 准备 op 内 `@-` 使用的 stdin 字节流。
    // - 明确指定 `--stdin-file` → 读该文件
    // - 否则 lazy：仅在 op 列表里真的出现 `@-` 时才尝试读进程 stdin；
    //   否则置 None，避免吞掉用户的无关管道输入。
    let op_stdin: Option<Vec<u8>> = if let Some(p) = &a.stdin_file {
        let bytes = std::fs::read(p)
            .with_context(|| format!("failed to read --stdin-file: {}", p.display()))?;
        Some(bytes)
    } else {
        None
    };

    // 解析
    let ops = if let Some(fmt_str) = &a.format {
        let fmt = parse_format_arg(fmt_str)?;
        parse_with(&src, fmt).with_context(|| format!("parse with --format {fmt:?} failed"))?
    } else if a.from_stdin {
        // stdin 默认 native
        parse_with(&src, Format::Native)
            .or_else(|_| parse_auto(&src, None))
            .context("stdin manifest parse failed")?
    } else {
        parse_auto(&src, hint_path.as_deref())?
    };

    // Lazy stdin 读取：仅当 op 列表里实际出现 `@-` 时才读进程 stdin。
    // - 已通过 `--stdin-file` 指定 → 跳过
    // - manifest 自身就是从 stdin 读的（`--from-stdin`）→ stdin 已被占用，不能再读
    let op_stdin: Option<Vec<u8>> = if op_stdin.is_some() {
        op_stdin
    } else if has_stdin_content(&ops) {
        if a.from_stdin {
            bail!(
                "manifest reads from stdin (`--from-stdin`); cannot also consume stdin for op `@-`. \
                 Use `--stdin-file <PATH>` to provide op payload separately."
            );
        }
        let mut buf = Vec::new();
        std::io::stdin()
            .read_to_end(&mut buf)
            .context("failed to read process stdin for op `@-`")?;
        Some(buf)
    } else {
        None
    };

    // 执行
    let opts = ExecOpts {
        check: a.check,
        dry_run: a.dry_run,
        backup: a.backup,
        manifest_dir,
        stdin_bytes: op_stdin,
        parallel: if a.parallel <= 1 {
            None
        } else {
            Some(a.parallel)
        },
    };

    if a.json {
        let stdout = std::io::stdout();
        let mut out = stdout.lock();
        let _ = crate::output::json::emit(
            &mut out,
            &crate::output::json::Event::Start { command: "apply" },
        );
        match execute(ops, &opts) {
            Ok(files) => {
                let _ = crate::output::json::emit(
                    &mut out,
                    &crate::output::json::Event::Done {
                        affected_files: files.iter().map(|p| p.display().to_string()).collect(),
                    },
                );
                if a.check {
                    println!("OK");
                }
                return Ok(exit::SUCCESS);
            }
            Err(e) => {
                let (kind, code) = match &e {
                    ExecError::Phase1 { .. } => ("phase1", exit::CHECK),
                    ExecError::Phase2 { .. } => ("phase2", exit::PARTIAL),
                    ExecError::Io(_) => ("io", exit::IO),
                };
                let _ = crate::output::json::emit(
                    &mut out,
                    &crate::output::json::Event::Error {
                        kind,
                        message: format!("{e}"),
                    },
                );
                return Ok(code);
            }
        }
    }

    match execute(ops, &opts) {
        Ok(files) => {
            if a.check {
                println!("OK");
            } else if !a.dry_run {
                crate::note!("xnip apply: committed {} file(s)", files.len());
            }
            Ok(exit::SUCCESS)
        }
        Err(e @ ExecError::Phase1 { .. }) => {
            eprintln!("xnip apply: {e}");
            Ok(exit::CHECK)
        }
        Err(e @ ExecError::Phase2 { .. }) => {
            eprintln!("xnip apply: {e}");
            Ok(exit::PARTIAL)
        }
        Err(e @ ExecError::Io(_)) => {
            eprintln!("xnip apply: {e}");
            Ok(exit::IO)
        }
    }
}

/// 检查 op 列表中是否有任意 op 的 content 是 `@-`（[`OpContent::Stdin`]）。
fn has_stdin_content(ops: &[Op]) -> bool {
    ops.iter().any(|op| match op {
        Op::Replace { content, .. } | Op::Insert { content, .. } => {
            matches!(content, OpContent::Stdin)
        }
        Op::Move { .. } | Op::Indent { .. } => false,
    })
}
