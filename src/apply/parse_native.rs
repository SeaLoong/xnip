//! 原生紧凑格式 parser（PLAN §6.9.2）。
//!
//! 一行一操作，注释 `#` 行首，空行忽略。
//!
//! 操作行结构：`<op> <file | --files-from path> <定位> [<修饰>...] [<内容>] [<命名修饰>...]`
//!
//! 词法（PLAN §6.9.2）：
//! - 双引号 `"..."` 包裹含空格/特殊字符；内部识别 C-style 转义 `\n` `\t` `\r` `\\` `\"`
//! - 不带引号的 token 按空格切分，不解析转义
//! - `@<path>` 从外部文件读
//! - `@-` 从 apply 的 stdin 顺序读
//! - `@@` 表示字面 `@`
//! - `""` 空字符串
//!
//! 实现策略：手写递归下降；先 tokenize 一行为字符串列表（带"是否原始引号包裹"标志），
//! 再按 op 类型解析字段顺序。
//!
//! `clippy::many_single_char_names` / `unnecessary_wraps` / `single_match_else` / `if_same_then_else`
//! 在手写 parser 的上下文里反而会伤可读性，本文件明确 allow。
#![allow(
    clippy::many_single_char_names,
    clippy::unnecessary_wraps,
    clippy::needless_pass_by_value,
    clippy::if_same_then_else,
    clippy::single_match_else,
    clippy::single_match,
    clippy::redundant_closure,
    clippy::wildcard_in_or_patterns,
    clippy::doc_markdown,
    clippy::match_wildcard_for_single_variants,
    clippy::option_if_let_else,
    clippy::manual_let_else
)]

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use regex::Regex;
use regex::bytes::Regex as ByteRegex;

use crate::apply::{IndentKind, MovePosition, Op, OpContent, Target};
use crate::core::location::{Count, Locator};

/// 解析整个原生格式清单文本，返回 op 列表。
///
/// # Errors
/// 词法或语法错误时返回 `Err`，错误信息含行号。
pub fn parse(src: &str) -> Result<Vec<Op>> {
    let mut ops = Vec::new();
    for (idx, raw_line) in src.lines().enumerate() {
        let line_no = idx + 1;
        let trimmed = raw_line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let tokens =
            tokenize(raw_line).with_context(|| format!("line {line_no}: tokenize failed"))?;
        if tokens.is_empty() {
            continue;
        }
        let op = parse_op_line(&tokens).with_context(|| format!("line {line_no}: {raw_line}"))?;
        ops.push(op);
    }
    Ok(ops)
}

/// Token：内容字节串 + 是否被引号包裹。
///
/// 引号包裹的 token 含义：是字面字符串（解析过转义），不再二次解读为锚点/数字等。
/// 不被引号包裹的 token 在语义层会被尝试识别为定位/修饰；若不匹配则作为内容字面串。
///
/// `between_literal` 仅在源文本形如 `"A".."B"` 或 `"A".."B"i` 时由 tokenizer 设置；
/// 此时 `bytes` 留空、`quoted` 为 false，parser 直接消费 between_literal 构造 `Locator::Between`。
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Token {
    pub bytes: Vec<u8>,
    pub quoted: bool,
    pub between_literal: Option<BetweenLiteral>,
}

/// 字面 between 锚点：起始锚字节、结束锚字节、是否 inclusive (`/i` 后缀)。
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct BetweenLiteral {
    pub start: Vec<u8>,
    pub end: Vec<u8>,
    pub inclusive: bool,
}

/// 把单行切分为 tokens。
///
/// 词法特例（在 PLAN 词法原则上加了实用性拓展）：
/// - `=/REGEX/N?`、`~/A/..~/B/i?`、`s/PAT/REPL/FLAGS?` 是「原子 token」，不被内部空格切断
/// - 不带引号的 token 中遇到 `"` 则进入引号子模式，字节加到当前 token（使 `was="..."` 能拼为单 token）
///
/// # Errors
/// 引号未闭合返回 `Err`。
pub fn tokenize(line: &str) -> Result<Vec<Token>> {
    let bytes = line.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    let n = bytes.len();
    while i < n {
        // skip whitespace
        while i < n && (bytes[i] == b' ' || bytes[i] == b'\t') {
            i += 1;
        }
        if i >= n {
            break;
        }
        if bytes[i] == b'"' {
            // 扫 quoted 子段；如果后接 `.."..."` 则识别为 between literal
            let (first_buf, after_first) = scan_quoted(bytes, i)?;
            if after_first + 1 < n && bytes[after_first] == b'.' && bytes[after_first + 1] == b'.' {
                // 可能是 between literal：".".."."[i]
                let after_dots = after_first + 2;
                if after_dots < n && bytes[after_dots] == b'"' {
                    let (second_buf, after_second) = scan_quoted(bytes, after_dots)?;
                    let mut end_idx = after_second;
                    let mut inclusive = false;
                    // 可选 `i`：后面紧贴 `i` 且后面是空白或结束
                    if end_idx < n && bytes[end_idx] == b'i' {
                        let next = end_idx + 1;
                        if next == n || bytes[next] == b' ' || bytes[next] == b'\t' {
                            inclusive = true;
                            end_idx = next;
                        }
                    }
                    out.push(Token {
                        bytes: Vec::new(),
                        quoted: false,
                        between_literal: Some(BetweenLiteral {
                            start: first_buf,
                            end: second_buf,
                            inclusive,
                        }),
                    });
                    i = end_idx;
                    continue;
                }
            }
            // 普通 quoted token
            out.push(Token {
                bytes: first_buf,
                quoted: true,
                between_literal: None,
            });
            i = after_first;
            continue;
        }
        // 检测原子定位语法：=/.../N?  或  ~/A/..~/B/i?  或  s/.../.../FLAGS?
        if bytes[i] == b'=' && i + 1 < n && bytes[i + 1] == b'/' {
            let (buf, next) = scan_match_line_atom(bytes, i)?;
            out.push(Token {
                bytes: buf,
                quoted: false,
                between_literal: None,
            });
            i = next;
            continue;
        }
        if bytes[i] == b'~' && i + 1 < n && bytes[i + 1] == b'/' {
            let (buf, next) = scan_between_re_atom(bytes, i)?;
            out.push(Token {
                bytes: buf,
                quoted: false,
                between_literal: None,
            });
            i = next;
            continue;
        }
        if bytes[i] == b's' && i + 1 < n && bytes[i + 1] == b'/' {
            let (buf, next) = scan_subst_atom(bytes, i)?;
            out.push(Token {
                bytes: buf,
                quoted: false,
                between_literal: None,
            });
            i = next;
            continue;
        }
        // 不带引号 token：扫到下一个空白，遇到 " 进入引号子模式拼接
        let mut buf = Vec::new();
        while i < n && bytes[i] != b' ' && bytes[i] != b'\t' {
            if bytes[i] == b'"' {
                let (q, next) = scan_quoted(bytes, i)?;
                buf.extend_from_slice(&q);
                i = next;
            } else {
                buf.push(bytes[i]);
                i += 1;
            }
        }
        out.push(Token {
            bytes: buf,
            quoted: false,
            between_literal: None,
        });
    }
    Ok(out)
}

/// 扫描引号 token。假设 `bytes[start] == '"'`。返回（解转义后的字节串, 下个位置）。
fn scan_quoted(bytes: &[u8], start: usize) -> Result<(Vec<u8>, usize)> {
    let mut i = start + 1;
    let n = bytes.len();
    let mut buf = Vec::new();
    while i < n {
        let b = bytes[i];
        if b == b'"' {
            return Ok((buf, i + 1));
        }
        if b == b'\\' && i + 1 < n {
            let esc = bytes[i + 1];
            match esc {
                b'n' => buf.push(b'\n'),
                b't' => buf.push(b'\t'),
                b'r' => buf.push(b'\r'),
                b'\\' => buf.push(b'\\'),
                b'"' => buf.push(b'"'),
                other => {
                    buf.push(b'\\');
                    buf.push(other);
                }
            }
            i += 2;
        } else {
            buf.push(b);
            i += 1;
        }
    }
    bail!("unclosed quoted string")
}

/// 扫描 `=/REGEX/N?` 原子 token。返回原始字节串（不拆解）。
fn scan_match_line_atom(bytes: &[u8], start: usize) -> Result<(Vec<u8>, usize)> {
    // start 处是 `=`，start+1 是 `/`。在该 `/` 之后扫到下一个未转义 `/`，后面可选数字
    let n = bytes.len();
    let mut i = start + 2;
    while i < n {
        if bytes[i] == b'\\' && i + 1 < n {
            i += 2;
            continue;
        }
        if bytes[i] == b'/' {
            i += 1; // include closing /
            break;
        }
        i += 1;
    }
    // optional digits
    while i < n && bytes[i].is_ascii_digit() {
        i += 1;
    }
    Ok((bytes[start..i].to_vec(), i))
}

/// 扫描 `~/A/..~/B/i?` 原子 token。
fn scan_between_re_atom(bytes: &[u8], start: usize) -> Result<(Vec<u8>, usize)> {
    // 必须够 `~/A/..~/B/`
    // 扫两个 regex 主体
    let n = bytes.len();
    let mut i = start + 2; // skip ~/
    // first regex
    while i < n {
        if bytes[i] == b'\\' && i + 1 < n {
            i += 2;
            continue;
        }
        if bytes[i] == b'/' {
            i += 1;
            break;
        }
        i += 1;
    }
    // expect ..~/
    if i + 4 > n || &bytes[i..i + 4] != b"..~/" {
        bail!("expected `..~/` after first regex in between-re");
    }
    i += 4;
    // second regex
    while i < n {
        if bytes[i] == b'\\' && i + 1 < n {
            i += 2;
            continue;
        }
        if bytes[i] == b'/' {
            i += 1;
            break;
        }
        i += 1;
    }
    // optional `i`
    if i < n && bytes[i] == b'i' {
        i += 1;
    }
    Ok((bytes[start..i].to_vec(), i))
}

/// 扫描 `s/PAT/REPL/FLAGS?` 原子 token。
fn scan_subst_atom(bytes: &[u8], start: usize) -> Result<(Vec<u8>, usize)> {
    let n = bytes.len();
    let mut i = start + 2; // skip s/
    // pat
    while i < n {
        if bytes[i] == b'\\' && i + 1 < n {
            i += 2;
            continue;
        }
        if bytes[i] == b'/' {
            i += 1;
            break;
        }
        i += 1;
    }
    // repl
    while i < n {
        if bytes[i] == b'\\' && i + 1 < n {
            i += 2;
            continue;
        }
        if bytes[i] == b'/' {
            i += 1;
            break;
        }
        i += 1;
    }
    // flags: g 或 数字
    while i < n && (bytes[i] == b'g' || bytes[i].is_ascii_digit()) {
        i += 1;
    }
    Ok((bytes[start..i].to_vec(), i))
}

/// 解析已 tokenize 的一行。
fn parse_op_line(tokens: &[Token]) -> Result<Op> {
    if tokens.is_empty() {
        bail!("empty op line");
    }

    // 先解析全局开关 token：第一个 token 可能是 op 名（不带引号）
    let op_tok = &tokens[0];
    if op_tok.quoted {
        bail!("first token must be op name, not a quoted string");
    }
    let op_name = std::str::from_utf8(&op_tok.bytes).context("op name not UTF-8")?;

    // 第二个 token：file 或 --files-from <path>
    if tokens.len() < 2 {
        bail!("missing target");
    }
    let (target, mut idx) = parse_target(tokens, 1)?;

    // revert 修饰可能出现在定位之前（如 `replace src/Foo.vue revert s/OLD/NEW/g`）
    let mut revert = false;
    if idx < tokens.len() && !tokens[idx].quoted && tokens[idx].bytes == b"revert" {
        revert = true;
        idx += 1;
    }

    // 接下来是定位
    if idx >= tokens.len() {
        bail!("missing locator");
    }
    let (locator, inline_repl, mut idx2) = parse_locator(tokens, idx)?;

    // 修饰 + 内容 + 命名修饰
    let mut position = MovePosition::After;
    let mut indent_kind: Option<IndentKind> = None;
    let mut to_line: Option<usize> = None;
    let mut content: Option<OpContent> = inline_repl.map(OpContent::Repl);
    let mut was: Option<Vec<u8>> = None;

    while idx2 < tokens.len() {
        let t = &tokens[idx2];
        let s = std::str::from_utf8(&t.bytes).unwrap_or("");

        if !t.quoted {
            // before / after / inclusive 在 locator 前已消耗了部分；这里是修饰语境
            match s {
                "before" => {
                    position = MovePosition::Before;
                    idx2 += 1;
                    continue;
                }
                "after" => {
                    position = MovePosition::After;
                    idx2 += 1;
                    continue;
                }
                "revert" => {
                    revert = true;
                    idx2 += 1;
                    continue;
                }
                _ => {}
            }
            // +N / -N / t2s:N / s2t:N
            if let Some(rest) = s.strip_prefix('+') {
                let n: i64 = rest
                    .parse()
                    .with_context(|| format!("invalid indent +N: {s}"))?;
                indent_kind = Some(IndentKind::Adjust(n));
                idx2 += 1;
                continue;
            }
            if let Some(rest) = s.strip_prefix('-') {
                let n: i64 = rest
                    .parse()
                    .with_context(|| format!("invalid indent -N: {s}"))?;
                indent_kind = Some(IndentKind::Adjust(-n));
                idx2 += 1;
                continue;
            }
            if let Some(rest) = s.strip_prefix("t2s:") {
                let n: usize = rest.parse().with_context(|| format!("invalid t2s: {s}"))?;
                indent_kind = Some(IndentKind::TabsToSpaces(n));
                idx2 += 1;
                continue;
            }
            if let Some(rest) = s.strip_prefix("s2t:") {
                let n: usize = rest.parse().with_context(|| format!("invalid s2t: {s}"))?;
                indent_kind = Some(IndentKind::SpacesToTabs(n));
                idx2 += 1;
                continue;
            }
            // was=... / was=@<path>
            if let Some(rest) = s.strip_prefix("was=") {
                if let Some(p) = rest.strip_prefix('@') {
                    if p == "@" {
                        // was=@@ 表示字面 was=@（不太可能；但保持一致）
                        was = Some(b"@".to_vec());
                    } else {
                        let bytes =
                            std::fs::read(p).with_context(|| format!("failed to read was=@{p}"))?;
                        was = Some(bytes);
                    }
                } else {
                    was = Some(rest.as_bytes().to_vec());
                }
                idx2 += 1;
                continue;
            }
            // move 的目标行号（数字）
            if let Ok(n) = s.parse::<usize>()
                && matches!(op_name, "move")
                && to_line.is_none()
            {
                to_line = Some(n);
                idx2 += 1;
                continue;
            }
            // 内容：未引号的 @path / @- / @@
            if let Some(p) = s.strip_prefix('@') {
                content = Some(parse_at_token(p)?);
                idx2 += 1;
                continue;
            }
            // 落到这里：把整 token 当作字面内容
            content = Some(parse_inline_content(t));
            idx2 += 1;
            continue;
        }

        // quoted token：通常是内容；命名修饰 was="..." 已经被未引号 prefix 路径处理
        if content.is_none() {
            content = Some(parse_inline_content(t));
        } else {
            bail!("unexpected extra token: {s}");
        }
        idx2 += 1;
    }

    // 组装为具体 op
    match op_name {
        "replace" => {
            let allow_repl = matches!(locator, Locator::Pattern { .. });
            let final_content = match content {
                Some(c) => c,
                None => {
                    // pattern 模式由 s/pat/repl/ 直接生成 repl，从 locator 内嵌；走特殊路径
                    if let Locator::Pattern { .. } = &locator {
                        // s/pat/repl/g 的 repl 已嵌在 locator 外的 content；如果没显式 content
                        // 但 locator 是 Pattern 加 inline_repl 字段，需要拿出来。
                        // 这里 parse_locator 对 s/.../.../ 已经构造了 Locator::Pattern + repl 通过
                        // 一个特殊通道：放到 Token 流后续跟 OpContent::Repl，所以正常路径会走 content=Some。
                        // 走到这里说明缺 content。
                        bail!("replace --pattern requires `repl` content");
                    }
                    bail!("replace requires content");
                }
            };
            // 对 pattern 模式，content 必须是 Repl 类型；其它模式不能是 Repl
            if allow_repl {
                if !matches!(final_content, OpContent::Repl(_)) {
                    bail!("replace --pattern requires `repl` content (use s/pat/repl/g)");
                }
            } else if matches!(final_content, OpContent::Repl(_)) {
                bail!("non-pattern replace cannot use `repl`");
            }
            Ok(Op::Replace {
                target,
                locator,
                content: final_content,
                was,
                revert,
            })
        }
        "insert" => {
            let final_content = content.context("insert requires content")?;
            if matches!(final_content, OpContent::Repl(_)) {
                bail!("insert cannot use `repl`");
            }
            Ok(Op::Insert {
                target,
                locator,
                content: final_content,
                position,
                revert,
            })
        }
        "move" => {
            let to = to_line.context("move requires target line number")?;
            Ok(Op::Move {
                target,
                from: locator,
                to_line: to,
                position,
                revert,
            })
        }
        "indent" => {
            let kind = indent_kind.context("indent requires +N / -N / t2s:N / s2t:N")?;
            Ok(Op::Indent {
                target,
                locator,
                kind,
                revert,
            })
        }
        other => bail!("unknown op: {other}"),
    }
}

fn parse_target(tokens: &[Token], i: usize) -> Result<(Target, usize)> {
    let t = &tokens[i];
    if !t.quoted && t.bytes == b"--files-from" {
        if i + 1 >= tokens.len() {
            bail!("--files-from requires a path argument");
        }
        let p = &tokens[i + 1];
        let path = std::str::from_utf8(&p.bytes)
            .context("--files-from path not UTF-8")?
            .to_string();
        return Ok((Target::FilesFrom(PathBuf::from(path)), i + 2));
    }
    let path = std::str::from_utf8(&t.bytes).context("file path not UTF-8")?;
    Ok((Target::File(PathBuf::from(path)), i + 1))
}

/// 解析定位 token。返回 (locator, inline_repl, next_idx)。
///
/// `inline_repl` 仅在 `s/pat/repl/g` 语法下为 `Some`，调用方应将其设为 `Op::Replace.content` 的 `Repl`。
fn parse_locator(tokens: &[Token], i: usize) -> Result<(Locator, Option<String>, usize)> {
    let t = &tokens[i];
    if let Some(bl) = &t.between_literal {
        return Ok((
            Locator::Between {
                start: bl.start.clone(),
                end: bl.end.clone(),
                start_occ: 1,
                end_occ: 1,
                inclusive: bl.inclusive,
            },
            None,
            i + 1,
        ));
    }
    if t.quoted {
        bail!(
            "quoted token cannot be a locator; use `\"A\"..\"B\"` for literal between, or use lines/match-line/pattern"
        );
    }
    let s = std::str::from_utf8(&t.bytes).context("locator not UTF-8")?;

    if s.starts_with("s/") {
        let (loc, repl) = parse_subst_locator(s)?;
        return Ok((loc, Some(repl), i + 1));
    }

    // =/regex/[N]：match-line
    if let Some(rest) = s.strip_prefix("=/") {
        let (pat, occ) = split_regex_with_occ(rest)?;
        return Ok((
            Locator::MatchLine {
                regex: Regex::new(&pat).with_context(|| format!("invalid regex: {pat}"))?,
                occurrence: occ,
            },
            None,
            i + 1,
        ));
    }

    // ~/start/..~/end/[i]：between-re
    if s.starts_with("~/") {
        let (sre, ere, inclusive) = parse_between_re_token(s)?;
        return Ok((
            Locator::BetweenRe {
                start: Regex::new(&sre).with_context(|| format!("invalid start regex: {sre}"))?,
                end: Regex::new(&ere).with_context(|| format!("invalid end regex: {ere}"))?,
                start_occ: 1,
                end_occ: 1,
                inclusive,
            },
            None,
            i + 1,
        ));
    }

    // 数字 / 数字-数字
    if let Some((a, b)) = s.split_once('-') {
        let a: usize = a
            .parse()
            .with_context(|| format!("invalid start line: {a:?}"))?;
        let b: usize = b
            .parse()
            .with_context(|| format!("invalid end line: {b:?}"))?;
        return Ok((Locator::Lines { start: a, end: b }, None, i + 1));
    }
    if let Ok(n) = s.parse::<usize>() {
        return Ok((Locator::Lines { start: n, end: n }, None, i + 1));
    }

    bail!("unrecognized locator token: {s}");
}
/// 把 `=/regex/N` 的 `regex/N` 部分（去掉 `=/` 前缀后）切分为 (pattern, occurrence)。
fn split_regex_with_occ(rest: &str) -> Result<(String, usize)> {
    // 找最后一个 `/`，它后面可选数字
    let last = rest.rfind('/').context("expected closing / in =/.../")?;
    let pat = &rest[..last];
    let tail = &rest[last + 1..];
    let occ = if tail.is_empty() {
        1
    } else {
        tail.parse::<usize>()
            .with_context(|| format!("invalid occurrence: {tail}"))?
            .max(1)
    };
    Ok((pat.to_string(), occ))
}

/// 解析 `~/start/..~/end/[i]`。
fn parse_between_re_token(s: &str) -> Result<(String, String, bool)> {
    // 必须是 ~/A/..~/B/ 或 ~/A/..~/B/i
    let rest = s.strip_prefix("~/").context("expected ~/...")?;
    let mid_idx = rest.find("/..~/").context("expected /..~/ separator")?;
    let a = &rest[..mid_idx];
    let after_mid = &rest[mid_idx + "/..~/".len()..];
    // after_mid 是 B/ 或 B/i
    let inclusive = after_mid.ends_with("/i");
    let trimmed = if inclusive {
        after_mid.strip_suffix("/i").unwrap()
    } else {
        after_mid.strip_suffix('/').context("expected closing /")?
    };
    Ok((a.to_string(), trimmed.to_string(), inclusive))
}

/// 解析 `s/pat/repl/[gN]`。返回（`Locator::Pattern`, `repl_string`）。
///
/// flags：`g` = `Count::All`；数字 N = `Count::First(N)`；不带 = `Count::All`。
fn parse_subst_locator(s: &str) -> Result<(Locator, String)> {
    // 逐字节扫描并处理转义，找 3 个未转义的 `/`
    let bytes = s.as_bytes();
    if bytes.len() < 4 || &bytes[..2] != b"s/" {
        bail!("expected s/PAT/REPL/FLAGS");
    }
    // 收集不转义的 / 位置
    let mut slashes = Vec::with_capacity(3);
    let mut i = 2;
    while i < bytes.len() && slashes.len() < 3 {
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            i += 2;
            continue;
        }
        if bytes[i] == b'/' {
            slashes.push(i);
        }
        i += 1;
    }
    if slashes.len() < 2 {
        bail!("expected at least s/PAT/REPL/");
    }
    let pat = unescape_subst(&s[2..slashes[0]]);
    let (repl, flags_start) = if slashes.len() == 3 {
        (
            unescape_subst(&s[slashes[0] + 1..slashes[1]]),
            slashes[1] + 1,
        )
    } else {
        (
            unescape_subst(&s[slashes[0] + 1..slashes[1]]),
            slashes[1] + 1,
        )
    };
    let flags = &s[flags_start..];
    let count = parse_subst_flags(flags)?;
    let regex =
        ByteRegex::new(&pat).with_context(|| format!("invalid pattern in s/.../...: {pat}"))?;
    Ok((Locator::Pattern { regex, count }, repl))
}

/// 解 `\\/` 为 `/`，其他转义原样保留。
fn unescape_subst(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut iter = s.chars().peekable();
    while let Some(c) = iter.next() {
        if c == '\\' {
            if let Some(&next) = iter.peek()
                && next == '/'
            {
                out.push('/');
                iter.next();
                continue;
            }
            out.push('\\');
        } else {
            out.push(c);
        }
    }
    out
}

fn parse_subst_flags(flags: &str) -> Result<Count> {
    if flags.is_empty() {
        return Ok(Count::All);
    }
    if flags == "g" {
        return Ok(Count::All);
    }
    let n: usize = flags
        .parse()
        .with_context(|| format!("invalid s/.../.../flags: {flags:?}"))?;
    if n == 0 {
        bail!("s/.../.../N must be >= 1");
    }
    Ok(Count::First(n))
}

/// 解析 `@<path>` / `@-` / `@@`。输入是去掉首个 `@` 后的字符串。
fn parse_at_token(rest: &str) -> Result<OpContent> {
    if rest == "-" {
        return Ok(OpContent::Stdin);
    }
    if rest == "@" {
        // `@@` 字面 `@` —— 当作 inline 字面字符串
        return Ok(OpContent::Text(b"@".to_vec()));
    }
    Ok(OpContent::File(PathBuf::from(rest)))
}

/// 把 token 转为内容（字面）。
fn parse_inline_content(t: &Token) -> OpContent {
    if t.quoted && t.bytes.is_empty() {
        OpContent::Empty
    } else {
        OpContent::Text(t.bytes.clone())
    }
}

#[cfg(test)]
#[allow(clippy::single_match, clippy::needless_raw_string_hashes)]
mod tests {
    use super::*;

    #[test]
    fn tokenize_simple() {
        let toks = tokenize(r#"replace src/Foo.vue 30 "const X = 1;""#).unwrap();
        assert_eq!(toks.len(), 4);
        assert_eq!(toks[0].bytes, b"replace");
        assert!(!toks[0].quoted);
        assert_eq!(toks[3].bytes, b"const X = 1;");
        assert!(toks[3].quoted);
    }

    #[test]
    fn tokenize_escape_sequences() {
        let toks = tokenize(r#"x "a\nb\tc\\d\"e""#).unwrap();
        assert_eq!(toks.len(), 2);
        assert_eq!(toks[1].bytes, b"a\nb\tc\\d\"e");
    }

    #[test]
    fn tokenize_unclosed_quote_errors() {
        assert!(tokenize(r#"x "unclosed"#).is_err());
    }

    #[test]
    fn parse_replace_lines_basic() {
        let ops = parse(r#"replace a.txt 30 "X""#).unwrap();
        assert_eq!(ops.len(), 1);
        match &ops[0] {
            Op::Replace {
                target,
                locator,
                content,
                ..
            } => {
                assert!(matches!(target, Target::File(p) if p == &PathBuf::from("a.txt")));
                assert!(matches!(locator, Locator::Lines { start: 30, end: 30 }));
                assert!(matches!(content, OpContent::Text(b) if b == b"X"));
            }
            _ => panic!(),
        }
    }

    #[test]
    fn parse_replace_range_with_empty_text() {
        let ops = parse(r#"replace a.txt 30-45 """#).unwrap();
        match &ops[0] {
            Op::Replace { content, .. } => assert!(matches!(content, OpContent::Empty)),
            _ => panic!(),
        }
    }

    #[test]
    fn parse_replace_external_text_file() {
        let ops = parse(r#"replace a.txt 30-45 @./snippets/x.txt"#).unwrap();
        if let Op::Replace { content, .. } = &ops[0]
            && let OpContent::File(p) = content
        {
            assert_eq!(p, &PathBuf::from("./snippets/x.txt"));
        } else {
            panic!();
        }
    }

    #[test]
    fn parse_replace_match_line() {
        let ops = parse(r#"replace a.txt =/^const PORT/ "X""#).unwrap();
        if let Op::Replace { locator, .. } = &ops[0]
            && let Locator::MatchLine {
                regex,
                occurrence: 1,
            } = locator
        {
            assert_eq!(regex.as_str(), "^const PORT");
        } else {
            panic!();
        }
    }

    #[test]
    fn parse_replace_match_line_occurrence() {
        let ops = parse(r#"replace a.txt =/^foo/2 "X""#).unwrap();
        if let Op::Replace { locator, .. } = &ops[0]
            && let Locator::MatchLine { occurrence, .. } = locator
        {
            assert_eq!(*occurrence, 2);
        } else {
            panic!();
        }
    }

    #[test]
    fn parse_replace_between_re() {
        let ops = parse(r#"replace a.txt ~/^function foo/..~/^}/ """#).unwrap();
        if let Op::Replace { locator, .. } = &ops[0]
            && let Locator::BetweenRe {
                start,
                end,
                inclusive,
                ..
            } = locator
        {
            assert_eq!(start.as_str(), "^function foo");
            assert_eq!(end.as_str(), "^}");
            assert!(!inclusive);
        } else {
            panic!();
        }
    }

    #[test]
    fn parse_replace_between_re_inclusive() {
        let ops = parse(r#"replace a.txt ~/^function foo/..~/^}/i """#).unwrap();
        if let Op::Replace { locator, .. } = &ops[0]
            && let Locator::BetweenRe { inclusive, .. } = locator
        {
            assert!(inclusive);
        } else {
            panic!();
        }
    }

    #[test]
    fn parse_replace_was_inline() {
        let ops = parse(r#"replace a.txt 30 "new" was="old""#).unwrap();
        match &ops[0] {
            Op::Replace { was, .. } => assert_eq!(was.as_deref(), Some(b"old".as_ref())),
            _ => panic!(),
        }
    }

    #[test]
    fn parse_insert_with_position() {
        let ops = parse(r#"insert a.txt 5 after "import X""#).unwrap();
        match &ops[0] {
            Op::Insert {
                position, content, ..
            } => {
                assert!(matches!(position, MovePosition::After));
                assert!(matches!(content, OpContent::Text(b) if b == b"import X"));
            }
            _ => panic!(),
        }
    }

    #[test]
    fn parse_insert_before() {
        let ops = parse(r#"insert a.txt 5 before "X""#).unwrap();
        match &ops[0] {
            Op::Insert { position, .. } => assert!(matches!(position, MovePosition::Before)),
            _ => panic!(),
        }
    }

    #[test]
    fn parse_move_basic() {
        let ops = parse(r#"move a.txt 10-20 100"#).unwrap();
        match &ops[0] {
            Op::Move {
                from,
                to_line,
                position,
                ..
            } => {
                assert!(matches!(from, Locator::Lines { start: 10, end: 20 }));
                assert_eq!(*to_line, 100);
                assert!(matches!(position, MovePosition::After));
            }
            _ => panic!(),
        }
    }

    #[test]
    fn parse_indent_add() {
        let ops = parse(r#"indent a.txt 30-45 +2"#).unwrap();
        match &ops[0] {
            Op::Indent { kind, .. } => assert!(matches!(kind, IndentKind::Adjust(2))),
            _ => panic!(),
        }
    }

    #[test]
    fn parse_indent_remove() {
        let ops = parse(r#"indent a.txt 30-45 -2"#).unwrap();
        match &ops[0] {
            Op::Indent { kind, .. } => assert!(matches!(kind, IndentKind::Adjust(-2))),
            _ => panic!(),
        }
    }

    #[test]
    fn parse_indent_t2s() {
        let ops = parse(r#"indent a.txt 1-99 t2s:4"#).unwrap();
        match &ops[0] {
            Op::Indent { kind, .. } => assert!(matches!(kind, IndentKind::TabsToSpaces(4))),
            _ => panic!(),
        }
    }

    #[test]
    fn parse_indent_s2t() {
        let ops = parse(r#"indent a.txt 1-99 s2t:4"#).unwrap();
        match &ops[0] {
            Op::Indent { kind, .. } => assert!(matches!(kind, IndentKind::SpacesToTabs(4))),
            _ => panic!(),
        }
    }

    #[test]
    fn parse_files_from() {
        let ops = parse(r#"replace --files-from list.txt 1-2 """#).unwrap();
        if let Op::Replace { target, .. } = &ops[0]
            && let Target::FilesFrom(p) = target
        {
            assert_eq!(p, &PathBuf::from("list.txt"));
        } else {
            panic!();
        }
    }

    #[test]
    fn parse_comments_and_blank_lines_skipped() {
        let src = "# this is a comment\n\nreplace a.txt 1 \"X\"\n# another\n";
        let ops = parse(src).unwrap();
        assert_eq!(ops.len(), 1);
    }

    #[test]
    fn parse_multiple_ops_one_per_line() {
        let src = "replace a 1 \"X\"\ninsert a 1 after \"Y\"\nindent a 1-2 +2\n";
        let ops = parse(src).unwrap();
        assert_eq!(ops.len(), 3);
    }

    #[test]
    fn parse_unknown_op_errors() {
        assert!(parse(r#"frobnicate a.txt 1"#).is_err());
    }

    #[test]
    fn parse_missing_locator_errors() {
        assert!(parse(r#"replace a.txt"#).is_err());
    }

    #[test]
    fn parse_subst_syntax_basic() {
        let ops = parse(r#"replace --files-from list.txt s/OLD/NEW/g"#).unwrap();
        match &ops[0] {
            Op::Replace {
                locator, content, ..
            } => {
                match locator {
                    Locator::Pattern { regex, count } => {
                        assert_eq!(regex.as_str(), "OLD");
                        assert_eq!(*count, Count::All);
                    }
                    _ => panic!(),
                }
                assert!(matches!(content, OpContent::Repl(s) if s == "NEW"));
            }
            _ => panic!(),
        }
    }

    #[test]
    fn parse_subst_with_count_flag() {
        let ops = parse(r#"replace a.txt s/x/y/3"#).unwrap();
        if let Op::Replace { locator, .. } = &ops[0]
            && let Locator::Pattern { count, .. } = locator
        {
            assert_eq!(*count, Count::First(3));
        } else {
            panic!();
        }
    }

    #[test]
    fn parse_revert_keyword_before_locator() {
        let ops = parse(r#"replace a.txt revert s/OLD/NEW/g"#).unwrap();
        match &ops[0] {
            Op::Replace { revert, .. } => assert!(*revert),
            _ => panic!(),
        }
    }

    #[test]
    fn parse_replace_between_literal() {
        let ops = parse(r#"replace a.txt "// BEGIN".."// END" """#).unwrap();
        match &ops[0] {
            Op::Replace {
                locator, content, ..
            } => {
                match locator {
                    Locator::Between {
                        start,
                        end,
                        inclusive,
                        ..
                    } => {
                        assert_eq!(start, b"// BEGIN");
                        assert_eq!(end, b"// END");
                        assert!(!inclusive);
                    }
                    _ => panic!("expected Between, got {locator:?}"),
                }
                assert!(matches!(content, OpContent::Empty));
            }
            _ => panic!(),
        }
    }

    #[test]
    fn parse_replace_between_literal_inclusive() {
        let ops = parse(r#"replace a.txt "BEGIN".."END"i """#).unwrap();
        if let Op::Replace { locator, .. } = &ops[0]
            && let Locator::Between {
                start,
                end,
                inclusive,
                ..
            } = locator
        {
            assert_eq!(start, b"BEGIN");
            assert_eq!(end, b"END");
            assert!(*inclusive);
        } else {
            panic!();
        }
    }

    #[test]
    fn tokenize_between_literal_emits_single_token() {
        let toks = tokenize(r#""A".."B""#).unwrap();
        assert_eq!(toks.len(), 1);
        let bl = toks[0].between_literal.as_ref().unwrap();
        assert_eq!(bl.start, b"A");
        assert_eq!(bl.end, b"B");
        assert!(!bl.inclusive);
    }
}
