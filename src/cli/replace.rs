//! `xnip replace <file> [Locator] [Content] [--was|--was-file] [--dry-run] [--check] [--backup] [--revert]`
//!
//! PLAN.md §6.7.3。

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::Args as ClapArgs;

use crate::core::atomic;
use crate::core::content;
use crate::core::location::{Locator, resolve};
use crate::core::ops::replace::{replace_pattern, replace_range};
use crate::output::exit;

use super::common::{ContentArgs, LocatorArgs};

#[derive(Debug, ClapArgs)]
#[allow(clippy::struct_excessive_bools)]
pub struct Args {
    /// 目标文件路径。
    pub file: PathBuf,

    #[command(flatten)]
    pub locator: LocatorArgs,

    #[command(flatten)]
    pub content: ContentArgs,

    /// 校验：原区段必须等于此字符串字面（保护并发改动）。
    #[arg(long, value_name = "TEXT", conflicts_with = "was_file")]
    pub was: Option<String>,

    /// 校验：原区段必须等于此文件内容。
    #[arg(long, value_name = "PATH")]
    pub was_file: Option<PathBuf>,

    /// 仅打印新内容到 stdout，不写文件。
    #[arg(long, default_value_t = false)]
    pub dry_run: bool,

    /// 仅做参数与定位检查，不写也不输出新内容。
    #[arg(long, default_value_t = false)]
    pub check: bool,

    /// 写文件前先备份为 `<file>.bak`。
    #[arg(long, default_value_t = false)]
    pub backup: bool,

    /// 反向执行（仅 pattern 模式时支持自动反向；range 模式必须配 `--was`）。
    #[arg(long, default_value_t = false)]
    pub revert: bool,
}

#[allow(clippy::needless_pass_by_value)]
pub fn run(a: Args) -> Result<u8> {
    // 1) 读原文件
    let bytes = content::load_path(&a.file).map_err(|e| anyhow::anyhow!("{e}"))?;

    // 2) 解析 Locator
    let loc = a.locator.into_locator()?;

    // 3) 准备 content
    let content_src = a
        .content
        .into_content(true)?
        .expect("require_some=true ensures Some");

    // 4) 分两路：pattern 模式 vs range 模式
    match loc {
        Locator::Pattern { regex, count } => {
            // pattern 模式必须配 --repl
            let repl = content_src.as_replacement().ok_or_else(|| {
                anyhow::anyhow!(
                    "`--pattern` requires `--repl`; got --text/--text-file/--text-stdin"
                )
            })?;

            // revert：将 pattern/repl 反转（参数对称）
            let (effective_pat, effective_repl) = if a.revert {
                let (p, r) = crate::core::revert::invert_pattern_replacement(regex.as_str(), repl);
                (p, r)
            } else {
                (regex.as_str().to_string(), repl.to_string())
            };
            let re = regex::bytes::Regex::new(&effective_pat)
                .with_context(|| format!("invalid regex (after revert): {effective_pat}"))?;

            let (new_bytes, n_replaced) = replace_pattern(&bytes, &re, &effective_repl, count);

            if a.check {
                crate::note!("xnip replace: {n_replaced} match(es) would be replaced");
                return Ok(if n_replaced > 0 {
                    exit::SUCCESS
                } else {
                    exit::CHECK
                });
            }

            if n_replaced == 0 {
                eprintln!("xnip replace: pattern did not match anything");
                return Ok(exit::CHECK);
            }

            commit_or_dry_run(&a.file, &new_bytes, a.dry_run, a.backup)
        }
        loc => {
            // range 模式
            // revert 语义（PLAN §6.8）：与 forward 完全互逆。
            //
            // 实现：当 `--revert` + `--was` 同时给定时，等价于"以 was 内容回写 locator 区域，
            // 同时校验当前 locator 区域确实是 --text 给的内容"——即把 text 与 was 互换。
            //
            // 不可逆条件 → 报错：
            //   - locator 不是 `--lines`（match-line/between/between-re 在 forward 后内容里
            //     不一定还能定位到原区域）
            //   - 缺 `--was`（无法表达"反向后的内容应是什么"）
            if a.revert {
                if !matches!(loc, Locator::Lines { .. }) {
                    bail!(
                        "`--revert` with range locator requires `--lines a-b`; \
                         match-line/between/between-re cannot be safely inverted \
                         because the anchor may not exist after forward execution"
                    );
                }
                let was_bytes = expected_was(a.was.as_ref(), a.was_file.as_ref())?
                    .ok_or_else(|| anyhow::anyhow!(
                        "`--revert` with range locator requires `--was` or `--was-file` (the original content to restore)"
                    ))?;
                if content_src.as_replacement().is_some() {
                    bail!("`--repl` is only valid with `--pattern`");
                }
                let forward_text =
                    content::load(&content_src).map_err(|e| anyhow::anyhow!("{e}"))?;
                // revert pre-condition: 当前 locator 区域应等于 forward 写入的 text
                let r = resolve(&loc, &bytes).map_err(|e| anyhow::anyhow!("{e}"))?;
                let actual = extract_lines(&bytes, r.start_line, r.end_line);
                // forward_text 在 forward 路径里被作为 payload，可能不带换行；
                // 写入文件后会在区段尾部由 replace_range 接合换行。
                // 这里允许 actual 比 forward_text 多 1 个尾随 \n。
                let matches_strict = actual == forward_text;
                let matches_lax = !forward_text.ends_with(b"\n")
                    && actual.len() == forward_text.len() + 1
                    && actual.starts_with(&forward_text)
                    && actual.last() == Some(&b'\n');
                if !(matches_strict || matches_lax) {
                    eprintln!(
                        "xnip replace: --revert pre-condition failed at lines {}-{} \
                         (expected current content == --text)",
                        r.start_line, r.end_line
                    );
                    return Ok(exit::CHECK);
                }
                let new_bytes = replace_range(&bytes, r.start_line, r.end_line, &was_bytes)
                    .map_err(|e| anyhow::anyhow!("{e}"))?;
                if a.check {
                    return Ok(exit::SUCCESS);
                }
                return commit_or_dry_run(&a.file, &new_bytes, a.dry_run, a.backup);
            }

            let r = resolve(&loc, &bytes).map_err(|e| anyhow::anyhow!("{e}"))?;

            // --was / --was-file 校验
            if let Some(expected) = expected_was(a.was.as_ref(), a.was_file.as_ref())? {
                let actual = extract_lines(&bytes, r.start_line, r.end_line);
                if actual != expected {
                    eprintln!(
                        "xnip replace: --was check failed at lines {}-{}",
                        r.start_line, r.end_line
                    );
                    return Ok(exit::CHECK);
                }
            }

            // 加载 payload（不允许 --repl）
            if content_src.as_replacement().is_some() {
                bail!("`--repl` is only valid with `--pattern`");
            }
            let payload = content::load(&content_src).map_err(|e| anyhow::anyhow!("{e}"))?;

            let new_bytes = replace_range(&bytes, r.start_line, r.end_line, &payload)
                .map_err(|e| anyhow::anyhow!("{e}"))?;

            if a.check {
                return Ok(exit::SUCCESS);
            }

            commit_or_dry_run(&a.file, &new_bytes, a.dry_run, a.backup)
        }
    }
}

/// 提取 `[start, end]` 行（含其行尾 `\n`，最后一行若不带 `\n` 则不补）作为字节串。
/// 用于 `--was` 字面比较。委托至 `core::location` 的公共实现，避免重复。
fn extract_lines(content: &[u8], start: usize, end: usize) -> Vec<u8> {
    crate::core::location::extract_line_range_with_newline(content, start, end)
}

/// 解析 `--was` / `--was-file`，返回期望的原区段字节。
fn expected_was(was: Option<&String>, was_file: Option<&PathBuf>) -> Result<Option<Vec<u8>>> {
    match (was, was_file) {
        (Some(_), Some(_)) => bail!("--was and --was-file are mutually exclusive"),
        (Some(s), None) => Ok(Some(s.clone().into_bytes())),
        (None, Some(p)) => {
            Ok(Some(std::fs::read(p).with_context(|| {
                format!("failed to read --was-file: {}", p.display())
            })?))
        }
        (None, None) => Ok(None),
    }
}
fn commit_or_dry_run(
    path: &std::path::Path,
    new_bytes: &[u8],
    dry_run: bool,
    backup: bool,
) -> Result<u8> {
    if dry_run {
        use std::io::Write;
        std::io::stdout()
            .lock()
            .write_all(new_bytes)
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        return Ok(exit::SUCCESS);
    }
    atomic::atomic_write(path, new_bytes, backup).map_err(|e| anyhow::anyhow!("{e}"))?;
    crate::note!(
        "xnip replace: wrote {} ({} byte(s))",
        path.display(),
        new_bytes.len()
    );
    Ok(exit::SUCCESS)
}
