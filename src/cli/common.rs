//! 各 CLI 子命令复用的 args / 转换工具。
//!
//! 部分函数仅被尚未实现的子命令使用。M2 后续填实后可移除 allow。
#![allow(dead_code)]

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::Args;
use regex::Regex;
use regex::bytes::Regex as ByteRegex;

use crate::core::content::Content;
use crate::core::location::{Count, Locator};

/// 写命令公用的 5 个定位维度（至少且最多 1 个有效）。
///
/// PLAN.md §6.4 / §6.9.2。
#[derive(Debug, Args, Default, Clone)]
pub struct LocatorArgs {
    /// 行号区间（1-based 闭区间），如 `--lines 30` 或 `--lines 30-45`。
    #[arg(long, value_name = "RANGE")]
    pub lines: Option<String>,

    /// 匹配整行的正则，如 `--match-line '^const PORT'`。
    #[arg(long, value_name = "REGEX")]
    pub match_line: Option<String>,

    /// `--match-line` / `--between` 的命中序号，1-based，默认 1。
    #[arg(long, default_value_t = 1, value_name = "N")]
    pub occurrence: usize,

    /// 字面锚点：`--between '// BEGIN'..'// END'`。
    #[arg(long, value_name = "START..END")]
    pub between: Option<String>,

    /// 正则锚点：`--between-re '^function foo'..'^\}'`。
    #[arg(long, value_name = "START..END")]
    pub between_re: Option<String>,

    /// `--between` 是否包含锚点行（默认 false）。
    #[arg(long, default_value_t = false)]
    pub inclusive: bool,

    /// 仅 `replace` 模式：命中正则的位置。
    #[arg(long, value_name = "REGEX")]
    pub pattern: Option<String>,

    /// 仅 `--pattern` 模式：替换前 N 处或全部（`all` / 数字）。
    #[arg(long, value_name = "N|all")]
    pub count: Option<String>,
}

impl LocatorArgs {
    /// 把命令行 args 转为单个 `Locator`。
    ///
    /// 校验规则：
    /// - 5 种定位互斥；多于 1 个 → error
    /// - 0 个 → error（写命令必须给定位）
    /// - `--inclusive` 仅当配 `--between*` 才有意义；这里不强制（`peek` 也用这个结构但不太需要）
    pub fn into_locator(self) -> Result<Locator> {
        let count = count_kinds(&self);
        if count == 0 {
            bail!(
                "missing locator: one of --lines / --match-line / --between / --between-re / --pattern is required"
            );
        }
        if count > 1 {
            bail!(
                "conflicting locators: --lines / --match-line / --between / --between-re / --pattern are mutually exclusive"
            );
        }

        if let Some(s) = &self.lines {
            let (start, end) = parse_line_range(s)?;
            return Ok(Locator::Lines { start, end });
        }
        if let Some(re) = &self.match_line {
            let r = Regex::new(re).with_context(|| format!("invalid --match-line regex: {re}"))?;
            return Ok(Locator::MatchLine {
                regex: r,
                occurrence: self.occurrence.max(1),
            });
        }
        if let Some(s) = &self.between {
            let (start, end) = parse_between_literal(s)?;
            return Ok(Locator::Between {
                start,
                end,
                start_occ: self.occurrence.max(1),
                end_occ: 1,
                inclusive: self.inclusive,
            });
        }
        if let Some(s) = &self.between_re {
            let (sre, ere) = parse_between_regex(s)?;
            return Ok(Locator::BetweenRe {
                start: sre,
                end: ere,
                start_occ: self.occurrence.max(1),
                end_occ: 1,
                inclusive: self.inclusive,
            });
        }
        if let Some(p) = &self.pattern {
            let r = ByteRegex::new(p).with_context(|| format!("invalid --pattern regex: {p}"))?;
            let count = parse_count(self.count.as_deref())?;
            return Ok(Locator::Pattern { regex: r, count });
        }
        unreachable!("count_kinds was > 0 but no branch matched");
    }
}

fn count_kinds(a: &LocatorArgs) -> usize {
    let mut n = 0;
    if a.lines.is_some() {
        n += 1;
    }
    if a.match_line.is_some() {
        n += 1;
    }
    if a.between.is_some() {
        n += 1;
    }
    if a.between_re.is_some() {
        n += 1;
    }
    if a.pattern.is_some() {
        n += 1;
    }
    n
}

/// 解析 `30` / `30-45`。
pub fn parse_line_range(s: &str) -> Result<(usize, usize)> {
    if let Some((a, b)) = s.split_once('-') {
        let a = a
            .trim()
            .parse::<usize>()
            .with_context(|| format!("invalid start line: {a:?}"))?;
        let b = b
            .trim()
            .parse::<usize>()
            .with_context(|| format!("invalid end line: {b:?}"))?;
        Ok((a, b))
    } else {
        let n = s
            .trim()
            .parse::<usize>()
            .with_context(|| format!("invalid line number: {s:?}"))?;
        Ok((n, n))
    }
}

fn parse_between_literal(s: &str) -> Result<(Vec<u8>, Vec<u8>)> {
    let (a, b) = s
        .split_once("..")
        .with_context(|| format!("--between expected `START..END`, got: {s:?}"))?;
    Ok((a.as_bytes().to_vec(), b.as_bytes().to_vec()))
}

fn parse_between_regex(s: &str) -> Result<(Regex, Regex)> {
    let (a, b) = s
        .split_once("..")
        .with_context(|| format!("--between-re expected `START..END`, got: {s:?}"))?;
    let ar = Regex::new(a).with_context(|| format!("invalid --between-re start regex: {a:?}"))?;
    let br = Regex::new(b).with_context(|| format!("invalid --between-re end regex: {b:?}"))?;
    Ok((ar, br))
}

fn parse_count(s: Option<&str>) -> Result<Count> {
    match s {
        None => Ok(Count::All),
        Some(v) => {
            let v = v.trim();
            if v.eq_ignore_ascii_case("all") {
                Ok(Count::All)
            } else {
                let n = v.parse::<usize>().with_context(|| {
                    format!("invalid --count value: {v:?}; expected `all` or N")
                })?;
                if n == 0 {
                    bail!("--count must be >= 1, or `all`");
                }
                Ok(Count::First(n))
            }
        }
    }
}

/// 写命令公用的内容来源（互斥）。
#[derive(Debug, Args, Default, Clone)]
pub struct ContentArgs {
    /// 字面字符串。
    #[arg(long, value_name = "TEXT", conflicts_with_all = ["text_stdin", "text_file", "repl"])]
    pub text: Option<String>,

    /// 从 stdin 读取。
    #[arg(long, default_value_t = false)]
    pub text_stdin: bool,

    /// 从外部文件读取。
    #[arg(long, value_name = "PATH", conflicts_with_all = ["text", "text_stdin", "repl"])]
    pub text_file: Option<PathBuf>,

    /// 仅 `--pattern` 模式：替换字符串（支持 `$1` 反向引用）。
    #[arg(long, value_name = "STR", conflicts_with_all = ["text", "text_stdin", "text_file"])]
    pub repl: Option<String>,
}

impl ContentArgs {
    /// 转为 [`Content`]。`require_some == true` 时缺失内容报错；否则返回 `Ok(None)`。
    ///
    /// 注意：删除（即 `--text ""`）也算一种"提供了内容"。
    pub fn into_content(self, require_some: bool) -> Result<Option<Content>> {
        let mut count = 0;
        if self.text.is_some() {
            count += 1;
        }
        if self.text_stdin {
            count += 1;
        }
        if self.text_file.is_some() {
            count += 1;
        }
        if self.repl.is_some() {
            count += 1;
        }
        if count == 0 {
            if require_some {
                bail!(
                    "missing content: one of --text / --text-stdin / --text-file / --repl is required"
                );
            }
            return Ok(None);
        }
        if count > 1 {
            bail!("conflicting content sources are mutually exclusive");
        }
        if let Some(s) = self.text {
            return Ok(Some(Content::Inline(s.into_bytes())));
        }
        if self.text_stdin {
            return Ok(Some(Content::Stdin));
        }
        if let Some(p) = self.text_file {
            return Ok(Some(Content::File(p)));
        }
        if let Some(r) = self.repl {
            return Ok(Some(Content::Replacement(r)));
        }
        unreachable!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_line_range_single() {
        assert_eq!(parse_line_range("30").unwrap(), (30, 30));
    }

    #[test]
    fn parse_line_range_pair() {
        assert_eq!(parse_line_range("30-45").unwrap(), (30, 45));
    }

    #[test]
    fn parse_line_range_invalid() {
        assert!(parse_line_range("abc").is_err());
        assert!(parse_line_range("1-x").is_err());
    }

    #[test]
    fn parse_count_default_is_all() {
        assert_eq!(parse_count(None).unwrap(), Count::All);
    }

    #[test]
    fn parse_count_all_keyword() {
        assert_eq!(parse_count(Some("all")).unwrap(), Count::All);
        assert_eq!(parse_count(Some("ALL")).unwrap(), Count::All);
    }

    #[test]
    fn parse_count_number() {
        assert_eq!(parse_count(Some("3")).unwrap(), Count::First(3));
    }

    #[test]
    fn parse_count_zero_invalid() {
        assert!(parse_count(Some("0")).is_err());
    }
}
