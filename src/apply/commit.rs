//! 两阶段提交执行器（PLAN §6.9.5）：
//!
//! 1. **格式识别 + 解析**（在 cli 层完成）→ 内部统一 op 列表
//! 2. **按文件分组**：把 `FilesFrom` 展开后，每个绝对/相对路径独立一组
//! 3. **组内排序**：起始行号降序；锚点定位先 resolve 再排
//! 4. **阶段一**：每文件 read → 顺序应用所有 op → 写同目录 tmpfile
//! 5. **阶段二**：所有 tmpfile OK 后逐个 atomic rename；可选 `--backup`
//!
//! 失败回滚见 PLAN.md §6.9.5。

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use tempfile::NamedTempFile;
use thiserror::Error;

use crate::apply::{IndentKind, MovePosition, Op, OpContent, Target};
use crate::core::content;
use crate::core::location::{Locator, resolve};
use crate::core::ops::indent::{IndentOp, apply_indent};
use crate::core::ops::insert::{Position, insert_at};
use crate::core::ops::move_op::move_lines;
use crate::core::ops::replace::{replace_pattern, replace_range};

/// 执行选项。
#[derive(Debug, Clone, Default)]
pub struct ExecOpts {
    /// `true` 时只跑阶段一（生成 tmpfile）后立即清理，不修改文件。
    pub check: bool,
    /// `true` 时跑阶段一，把 unified diff 写到 stdout，不修改文件。
    pub dry_run: bool,
    /// `true` 时阶段二写 `<file>.bak`。
    pub backup: bool,
    /// `apply` 清单文件的目录（用于解析相对路径 `@<path>`）。
    /// `None` 表示按 cwd 解析（`--from-stdin` 模式）。
    pub manifest_dir: Option<PathBuf>,
    /// 一次性提供给整份清单的 stdin 字节流，用于 op 内 `@-`。
    /// 一份清单中 `@-` 至多出现一次；多次出现报错。
    /// `None` 等价于无 stdin；遇到 `@-` 直接报错。
    pub stdin_bytes: Option<Vec<u8>>,
    /// `apply --parallel <N>`：阶段一并行处理的文件数上限。`None` 或 0/1 → 单线程。
    pub parallel: Option<usize>,
}

#[derive(Debug, Error)]
pub enum ExecError {
    #[error("phase 1 (validation) failed for {path}: {msg}")]
    Phase1 { path: PathBuf, msg: String },
    #[error(
        "phase 2 (commit) failed for {path}: {msg}; affected files (already committed): {committed:?}"
    )]
    Phase2 {
        path: PathBuf,
        msg: String,
        committed: Vec<PathBuf>,
    },
    #[error("io error reading manifest file list: {0}")]
    Io(#[from] std::io::Error),
}

/// 执行 apply 全流程。返回受影响的文件路径列表。
///
/// # Errors
/// 见 [`ExecError`]；用退出码语义：
/// - Phase1 → exit 3（CHECK）
/// - Phase2 → exit 4（PARTIAL）
pub fn execute(ops: Vec<Op>, opts: &ExecOpts) -> Result<Vec<PathBuf>, ExecError> {
    crate::trace!(
        "apply.execute: n_ops={} check={} dry_run={} backup={} parallel={:?}",
        ops.len(),
        opts.check,
        opts.dry_run,
        opts.backup,
        opts.parallel
    );

    // Step 0: 物化所有 `@-` 为 Text；约束至多 1 次
    let ops = materialize_stdin(ops, opts)?;

    // Step 1: 把 FilesFrom 展开
    let expanded = expand_files_from(ops, opts)?;

    // Step 2: 按文件分组（保留组内 op 原顺序）
    let mut groups: BTreeMap<PathBuf, Vec<Op>> = BTreeMap::new();
    for op in expanded {
        let path = match op.target() {
            Target::File(p) => p.clone(),
            Target::FilesFrom(_) => unreachable!("FilesFrom expanded above"),
        };
        groups.entry(path).or_default().push(op);
    }

    // Step 3+4: 阶段一 — 每文件顺序 apply，写 tmpfile
    // 抽成纯函数 `prepare_one`，dry-run 的 diff 文本由它返回，串行打到 stdout 保序。
    let entries: Vec<(PathBuf, Vec<Op>)> = groups.into_iter().collect();

    let prepared: Vec<Phase1Out> = match opts.parallel {
        Some(n) if n >= 2 => {
            use rayon::prelude::*;
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(n)
                .build()
                .map_err(|e| ExecError::Phase1 {
                    path: PathBuf::new(),
                    msg: format!("rayon pool: {e}"),
                })?;
            pool.install(|| {
                entries
                    .par_iter()
                    .map(|(path, ops_for_file)| prepare_one(path, ops_for_file, opts))
                    .collect::<Result<Vec<_>, ExecError>>()
            })?
        }
        _ => entries
            .iter()
            .map(|(path, ops_for_file)| prepare_one(path, ops_for_file, opts))
            .collect::<Result<Vec<_>, ExecError>>()?,
    };

    // dry-run 的 diff 串行打到 stdout，保证顺序与 entries 一致
    if opts.dry_run {
        let colorize = crate::core::diff::should_colorize_stdout();
        for p in &prepared {
            if let Some(diff) = &p.diff {
                if colorize {
                    print!("{}", crate::core::diff::colorize_unified_diff(diff));
                } else {
                    print!("{diff}");
                }
            }
        }
    }

    if opts.check || opts.dry_run {
        return Ok(prepared.into_iter().map(|p| p.path).collect());
    }

    // Step 5: 阶段二 — atomic rename + 可选 .bak（串行，保证错误回滚顺序可控）
    let mut committed: Vec<PathBuf> = Vec::with_capacity(prepared.len());
    for p in prepared {
        let Phase1Out { path, tmp, .. } = p;
        let Some(tmp) = tmp else {
            // check 模式不会走到这里；保险跳过
            continue;
        };
        if opts.backup && path.exists() {
            let bak = crate::core::atomic::bak_path(&path);
            if let Err(e) = std::fs::copy(&path, &bak) {
                return Err(ExecError::Phase2 {
                    path: path.clone(),
                    msg: format!("backup copy failed: {e}"),
                    committed: committed.clone(),
                });
            }
        }
        if let Err(e) = tmp.persist(&path) {
            return Err(ExecError::Phase2 {
                path,
                msg: format!("rename failed: {}", e.error),
                committed,
            });
        }
        committed.push(path);
    }

    Ok(committed)
}

/// 阶段一每文件输出。`tmp` 在 `check` 模式下为 `None`。
struct Phase1Out {
    path: PathBuf,
    tmp: Option<NamedTempFile>,
    diff: Option<String>,
}

/// 阶段一 per-file：read → 排序 → 顺序 apply → 写 tmpfile（除 `check`）→ 生成 diff（仅 `dry_run`）。
///
/// 设计为纯函数（仅访问 `path` 与不可变 `opts`），可在 rayon 中并行执行。
#[allow(clippy::many_single_char_names)]
fn prepare_one(
    path: &std::path::Path,
    ops_for_file: &[Op],
    opts: &ExecOpts,
) -> Result<Phase1Out, ExecError> {
    let bytes = std::fs::read(path).map_err(|e| ExecError::Phase1 {
        path: path.to_path_buf(),
        msg: format!("read failed: {e}"),
    })?;

    let mut ops_with_key: Vec<(usize, &Op)> = ops_for_file
        .iter()
        .map(|op| (op_start_line(op, &bytes).unwrap_or(usize::MAX), op))
        .collect();
    ops_with_key.sort_by_key(|t| std::cmp::Reverse(t.0));

    let mut content_bytes = bytes;
    for (_, op) in &ops_with_key {
        content_bytes = apply_one_op(op, &content_bytes, opts).map_err(|e| ExecError::Phase1 {
            path: path.to_path_buf(),
            msg: format!("op failed: {e}"),
        })?;
    }

    let diff = if opts.dry_run {
        let original = std::fs::read(path).unwrap_or_default();
        Some(crate::core::diff::unified_diff(
            path,
            &original,
            &content_bytes,
        ))
    } else {
        None
    };

    if opts.check {
        return Ok(Phase1Out {
            path: path.to_path_buf(),
            tmp: None,
            diff,
        });
    }

    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map_or_else(
            || std::path::PathBuf::from("."),
            std::path::Path::to_path_buf,
        );
    let tmp = NamedTempFile::new_in(&parent).map_err(|e| ExecError::Phase1 {
        path: path.to_path_buf(),
        msg: format!("tempfile create failed: {e}"),
    })?;
    std::fs::write(tmp.path(), &content_bytes).map_err(|e| ExecError::Phase1 {
        path: path.to_path_buf(),
        msg: format!("tempfile write failed: {e}"),
    })?;
    Ok(Phase1Out {
        path: path.to_path_buf(),
        tmp: Some(tmp),
        diff,
    })
}

/// 把所有 `OpContent::Stdin` 替换为 `OpContent::Text(stdin_bytes)`，
/// 同时校验：一份清单中 `@-` 至多出现一次。
fn materialize_stdin(ops: Vec<Op>, opts: &ExecOpts) -> Result<Vec<Op>, ExecError> {
    let mut seen = false;
    let mut out = Vec::with_capacity(ops.len());
    for op in ops {
        let needs = matches!(content_of(&op), Some(OpContent::Stdin));
        if !needs {
            out.push(op);
            continue;
        }
        if seen {
            return Err(ExecError::Phase1 {
                path: PathBuf::new(),
                msg: "`@-` (stdin content) may appear at most once per manifest".to_string(),
            });
        }
        let bytes = opts.stdin_bytes.clone().ok_or_else(|| ExecError::Phase1 {
            path: PathBuf::new(),
            msg: "`@-` requires stdin to be provided to apply".to_string(),
        })?;
        seen = true;
        out.push(replace_content(op, OpContent::Text(bytes)));
    }
    Ok(out)
}

fn content_of(op: &Op) -> Option<&OpContent> {
    match op {
        Op::Replace { content, .. } | Op::Insert { content, .. } => Some(content),
        Op::Move { .. } | Op::Indent { .. } => None,
    }
}

/// 返回一个用 `new_content` 替换 `content` 字段的新 op；其他字段克隆保留。
fn replace_content(op: Op, new_content: OpContent) -> Op {
    match op {
        Op::Replace {
            target,
            locator,
            was,
            revert,
            ..
        } => Op::Replace {
            target,
            locator,
            content: new_content,
            was,
            revert,
        },
        Op::Insert {
            target,
            locator,
            position,
            revert,
            ..
        } => Op::Insert {
            target,
            locator,
            content: new_content,
            position,
            revert,
        },
        other => other,
    }
}

/// 展开 `FilesFrom` 目标：把 `--files-from <list>` 替换为对每行路径生成一个相同 op。
fn expand_files_from(ops: Vec<Op>, opts: &ExecOpts) -> Result<Vec<Op>, ExecError> {
    let mut out = Vec::with_capacity(ops.len());
    for op in ops {
        match op.target().clone() {
            Target::File(_) => out.push(op),
            Target::FilesFrom(list) => {
                let resolved_list = resolve_relative(&list, opts);
                let content = std::fs::read_to_string(&resolved_list)?;
                for line in content.lines() {
                    let line = line.trim();
                    if line.is_empty() || line.starts_with('#') {
                        continue;
                    }
                    let path = resolve_relative(std::path::Path::new(line), opts);
                    out.push(clone_with_target(&op, Target::File(path)));
                }
            }
        }
    }
    Ok(out)
}

fn resolve_relative(p: &std::path::Path, opts: &ExecOpts) -> PathBuf {
    if p.is_absolute() {
        return p.to_path_buf();
    }
    if let Some(dir) = &opts.manifest_dir {
        return dir.join(p);
    }
    p.to_path_buf()
}

fn clone_with_target(op: &Op, target: Target) -> Op {
    match op {
        Op::Replace {
            locator,
            content,
            was,
            revert,
            ..
        } => Op::Replace {
            target,
            locator: clone_locator(locator),
            content: content.clone(),
            was: was.clone(),
            revert: *revert,
        },
        Op::Insert {
            locator,
            content,
            position,
            revert,
            ..
        } => Op::Insert {
            target,
            locator: clone_locator(locator),
            content: content.clone(),
            position: *position,
            revert: *revert,
        },
        Op::Move {
            from,
            to_line,
            position,
            revert,
            ..
        } => Op::Move {
            target,
            from: clone_locator(from),
            to_line: *to_line,
            position: *position,
            revert: *revert,
        },
        Op::Indent {
            locator,
            kind,
            revert,
            ..
        } => Op::Indent {
            target,
            locator: clone_locator(locator),
            kind: *kind,
            revert: *revert,
        },
    }
}

fn clone_locator(loc: &Locator) -> Locator {
    match loc {
        Locator::Lines { start, end } => Locator::Lines {
            start: *start,
            end: *end,
        },
        Locator::MatchLine { regex, occurrence } => Locator::MatchLine {
            regex: regex.clone(),
            occurrence: *occurrence,
        },
        Locator::Between {
            start,
            end,
            start_occ,
            end_occ,
            inclusive,
        } => Locator::Between {
            start: start.clone(),
            end: end.clone(),
            start_occ: *start_occ,
            end_occ: *end_occ,
            inclusive: *inclusive,
        },
        Locator::BetweenRe {
            start,
            end,
            start_occ,
            end_occ,
            inclusive,
        } => Locator::BetweenRe {
            start: start.clone(),
            end: end.clone(),
            start_occ: *start_occ,
            end_occ: *end_occ,
            inclusive: *inclusive,
        },
        Locator::Pattern { regex, count } => Locator::Pattern {
            regex: regex.clone(),
            count: *count,
        },
    }
}

/// 解析 op 的起始行号（用于组内排序）。
fn op_start_line(op: &Op, bytes: &[u8]) -> Option<usize> {
    let loc = match op {
        Op::Replace { locator, .. } | Op::Insert { locator, .. } | Op::Indent { locator, .. } => {
            locator
        }
        Op::Move { from, .. } => from,
    };
    if matches!(loc, Locator::Pattern { .. }) {
        return None; // pattern op 不参与行号排序
    }
    resolve(loc, bytes).ok().map(|r| r.start_line)
}

/// 把单个 op 应用到 `bytes`，返回新内容。
fn apply_one_op(op: &Op, bytes: &[u8], opts: &ExecOpts) -> Result<Vec<u8>> {
    match op {
        Op::Replace {
            locator,
            content,
            was,
            revert,
            ..
        } => apply_replace(bytes, locator, content, was.as_deref(), *revert, opts),
        Op::Insert {
            locator,
            content,
            position,
            revert,
            ..
        } => apply_insert(bytes, locator, content, *position, *revert, opts),
        Op::Move {
            from,
            to_line,
            position,
            revert,
            ..
        } => apply_move(bytes, from, *to_line, *position, *revert),
        Op::Indent {
            locator,
            kind,
            revert,
            ..
        } => apply_indent_op(bytes, locator, *kind, *revert),
    }
}

fn apply_replace(
    bytes: &[u8],
    locator: &Locator,
    content: &OpContent,
    was: Option<&[u8]>,
    revert: bool,
    opts: &ExecOpts,
) -> Result<Vec<u8>> {
    if let Locator::Pattern { regex, count } = locator {
        let repl = match content {
            OpContent::Repl(s) => s.clone(),
            _ => bail!("replace --pattern requires `repl` content"),
        };
        let (effective_pat, effective_repl) = if revert {
            crate::core::revert::invert_pattern_replacement(regex.as_str(), &repl)
        } else {
            (regex.as_str().to_string(), repl)
        };
        let re = regex::bytes::Regex::new(&effective_pat)
            .with_context(|| format!("invalid regex (after revert): {effective_pat}"))?;
        let (new_bytes, _n) = replace_pattern(bytes, &re, &effective_repl, *count);
        return Ok(new_bytes);
    }

    if revert {
        // range-locator revert: 仅在 `--lines` 且 `was` 提供时可逆；
        // 等价于把 text 与 was 互换后走 forward 流程。
        if !matches!(locator, Locator::Lines { .. }) {
            bail!(
                "range-locator replace --revert requires `lines` locator (other locators may not exist after forward)"
            );
        }
        let was_bytes = was.ok_or_else(|| anyhow::anyhow!(
            "range-locator replace --revert requires `was`/`was-file` (the original content to restore)"
        ))?.to_vec();
        let r = resolve(locator, bytes).context("locator resolution failed")?;
        // 当前区段应等于 forward 的 text
        let actual = extract_lines_with_newline(bytes, r.start_line, r.end_line);
        let forward_text = load_op_content(content, opts)?;
        let strict = actual == forward_text;
        let lax = !forward_text.ends_with(b"\n")
            && actual.len() == forward_text.len() + 1
            && actual.starts_with(&forward_text)
            && actual.last() == Some(&b'\n');
        if !(strict || lax) {
            bail!(
                "--revert pre-condition failed at lines {}-{}: expected current content == text",
                r.start_line,
                r.end_line
            );
        }
        return replace_range(bytes, r.start_line, r.end_line, &was_bytes)
            .map_err(|e| anyhow::anyhow!("replace failed: {e}"));
    }

    let r = resolve(locator, bytes).context("locator resolution failed")?;

    if let Some(expected) = was {
        let actual = extract_lines_with_newline(bytes, r.start_line, r.end_line);
        if actual != expected {
            bail!(
                "`was` mismatch at lines {}-{}: expected {} bytes, got {} bytes",
                r.start_line,
                r.end_line,
                expected.len(),
                actual.len()
            );
        }
    }

    let payload = load_op_content(content, opts)?;
    replace_range(bytes, r.start_line, r.end_line, &payload)
        .map_err(|e| anyhow::anyhow!("replace failed: {e}"))
}

#[allow(clippy::naive_bytecount)]
fn apply_insert(
    bytes: &[u8],
    locator: &Locator,
    content: &OpContent,
    position: MovePosition,
    revert: bool,
    opts: &ExecOpts,
) -> Result<Vec<u8>> {
    if matches!(locator, Locator::Pattern { .. }) {
        bail!("insert does not accept --pattern locator");
    }
    let r = resolve(locator, bytes).context("locator resolution failed")?;
    if r.start_line != r.end_line {
        bail!(
            "insert requires single-line anchor; got range {}-{}",
            r.start_line,
            r.end_line
        );
    }
    let payload = load_op_content(content, opts)?;
    let pos = match position {
        MovePosition::Before => Position::Before,
        MovePosition::After => Position::After,
    };
    if revert {
        // 仅支持 --lines locator。计算 forward 写入的行区间 + 校验
        if !matches!(locator, Locator::Lines { .. }) {
            bail!("insert --revert requires `lines` locator");
        }
        let normalized = if payload.is_empty() {
            Vec::new()
        } else if payload.last() == Some(&b'\n') {
            payload.clone()
        } else {
            let mut v = payload.clone();
            v.push(b'\n');
            v
        };
        let line_count = normalized.iter().filter(|&&b| b == b'\n').count();
        if line_count == 0 {
            bail!("insert --revert: payload has zero lines, nothing to remove");
        }
        let (del_start, del_end) = match pos {
            Position::After => (r.start_line + 1, r.start_line + line_count),
            Position::Before => (r.start_line, r.start_line + line_count - 1),
        };
        let actual = extract_lines_with_newline(bytes, del_start, del_end);
        if actual != normalized {
            bail!(
                "insert --revert pre-condition failed at lines {del_start}-{del_end}: expected current content == text"
            );
        }
        return replace_range(bytes, del_start, del_end, b"")
            .map_err(|e| anyhow::anyhow!("insert revert failed: {e}"));
    }
    insert_at(bytes, r.start_line, pos, &payload).map_err(|e| anyhow::anyhow!("insert failed: {e}"))
}

#[allow(clippy::many_single_char_names)]
fn apply_move(
    bytes: &[u8],
    from: &Locator,
    to_line: usize,
    position: MovePosition,
    revert: bool,
) -> Result<Vec<u8>> {
    let r = resolve(from, bytes).context("locator resolution failed")?;
    let pos = match position {
        MovePosition::Before => Position::Before,
        MovePosition::After => Position::After,
    };
    if revert {
        let (s, e, t, p) =
            crate::core::ops::move_op::reverse_params(r.start_line, r.end_line, to_line, pos)
                .map_err(|e| anyhow::anyhow!("{e}"))?;
        return move_lines(bytes, s, e, t, p)
            .map_err(|e| anyhow::anyhow!("move revert failed: {e}"));
    }
    move_lines(bytes, r.start_line, r.end_line, to_line, pos)
        .map_err(|e| anyhow::anyhow!("move failed: {e}"))
}

fn apply_indent_op(
    bytes: &[u8],
    locator: &Locator,
    kind: IndentKind,
    revert: bool,
) -> Result<Vec<u8>> {
    let r = resolve(locator, bytes).context("locator resolution failed")?;
    let op = match kind {
        IndentKind::Adjust(n) => {
            if n >= 0 {
                IndentOp::Add(n.unsigned_abs() as usize)
            } else {
                IndentOp::Remove(n.unsigned_abs() as usize)
            }
        }
        IndentKind::TabsToSpaces(n) => IndentOp::TabsToSpaces(n),
        IndentKind::SpacesToTabs(n) => IndentOp::SpacesToTabs(n),
    };
    let effective = if revert {
        match op {
            IndentOp::Add(n) => IndentOp::Remove(n),
            IndentOp::Remove(n) => IndentOp::Add(n),
            IndentOp::TabsToSpaces(n) => IndentOp::SpacesToTabs(n),
            IndentOp::SpacesToTabs(n) => IndentOp::TabsToSpaces(n),
        }
    } else {
        op
    };
    apply_indent(bytes, r.start_line, r.end_line, effective)
        .map_err(|e| anyhow::anyhow!("indent failed: {e}"))
}

/// 加载 `OpContent` 为字节序列。`Repl`/`None` 在调用方已分流，这里不接受。
fn load_op_content(c: &OpContent, opts: &ExecOpts) -> Result<Vec<u8>> {
    match c {
        OpContent::Text(b) => Ok(b.clone()),
        OpContent::Empty => Ok(Vec::new()),
        OpContent::File(p) => {
            let resolved = if p.is_absolute() {
                p.clone()
            } else if let Some(dir) = &opts.manifest_dir {
                dir.join(p)
            } else {
                p.clone()
            };
            content::load_path(&resolved).map_err(|e| anyhow::anyhow!("{e}"))
        }
        OpContent::Stdin => {
            bail!("internal: @- should have been materialized in preflight; this is a bug")
        }
        OpContent::Repl(_) => bail!("internal: replacement content reached load_op_content"),
        OpContent::None => bail!("internal: op without content called load_op_content"),
    }
}

fn extract_lines_with_newline(content: &[u8], start: usize, end: usize) -> Vec<u8> {
    crate::core::location::extract_line_range_with_newline(content, start, end)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::location::Locator;
    use tempfile::tempdir;

    fn write(p: &std::path::Path, b: &[u8]) {
        std::fs::write(p, b).unwrap();
    }
    fn read(p: &std::path::Path) -> Vec<u8> {
        std::fs::read(p).unwrap()
    }

    #[test]
    fn execute_single_replace() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("a.txt");
        write(&p, b"a\nb\nc\n");
        let ops = vec![Op::Replace {
            target: Target::File(p.clone()),
            locator: Locator::Lines { start: 2, end: 2 },
            content: OpContent::Text(b"B".to_vec()),
            was: None,
            revert: false,
        }];
        execute(ops, &ExecOpts::default()).unwrap();
        assert_eq!(read(&p), b"a\nB\nc\n");
    }

    #[test]
    fn execute_multiple_ops_same_file_descending_order() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("a.txt");
        write(&p, b"1\n2\n3\n4\n5\n");
        // 把 line 2 改 X，line 4 改 Y；不依赖输入顺序
        let ops = vec![
            Op::Replace {
                target: Target::File(p.clone()),
                locator: Locator::Lines { start: 2, end: 2 },
                content: OpContent::Text(b"X".to_vec()),
                was: None,
                revert: false,
            },
            Op::Replace {
                target: Target::File(p.clone()),
                locator: Locator::Lines { start: 4, end: 4 },
                content: OpContent::Text(b"Y".to_vec()),
                was: None,
                revert: false,
            },
        ];
        execute(ops, &ExecOpts::default()).unwrap();
        assert_eq!(read(&p), b"1\nX\n3\nY\n5\n");
    }

    #[test]
    fn execute_check_does_not_modify_files() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("a.txt");
        write(&p, b"orig\n");
        let ops = vec![Op::Replace {
            target: Target::File(p.clone()),
            locator: Locator::Lines { start: 1, end: 1 },
            content: OpContent::Text(b"new".to_vec()),
            was: None,
            revert: false,
        }];
        let opts = ExecOpts {
            check: true,
            ..ExecOpts::default()
        };
        execute(ops, &opts).unwrap();
        assert_eq!(read(&p), b"orig\n");
    }

    #[test]
    fn execute_phase1_failure_leaves_file_intact() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("a.txt");
        write(&p, b"orig\n");
        let ops = vec![Op::Replace {
            target: Target::File(p.clone()),
            locator: Locator::Lines {
                start: 100,
                end: 200,
            }, // out of bounds
            content: OpContent::Text(b"x".to_vec()),
            was: None,
            revert: false,
        }];
        let err = execute(ops, &ExecOpts::default()).unwrap_err();
        assert!(matches!(err, ExecError::Phase1 { .. }));
        assert_eq!(
            read(&p),
            b"orig\n",
            "file must remain intact after phase1 failure"
        );
    }

    #[test]
    fn execute_backup_writes_bak() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("a.txt");
        write(&p, b"v1\n");
        let ops = vec![Op::Replace {
            target: Target::File(p.clone()),
            locator: Locator::Lines { start: 1, end: 1 },
            content: OpContent::Text(b"v2".to_vec()),
            was: None,
            revert: false,
        }];
        let opts = ExecOpts {
            backup: true,
            ..ExecOpts::default()
        };
        execute(ops, &opts).unwrap();
        assert_eq!(read(&p), b"v2\n");
        let bak = crate::core::atomic::bak_path(&p);
        assert!(bak.exists());
        assert_eq!(read(&bak), b"v1\n");
    }

    #[test]
    fn execute_was_check_passes() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("a.txt");
        write(&p, b"a\nold\nc\n");
        let ops = vec![Op::Replace {
            target: Target::File(p.clone()),
            locator: Locator::Lines { start: 2, end: 2 },
            content: OpContent::Text(b"new".to_vec()),
            was: Some(b"old\n".to_vec()),
            revert: false,
        }];
        execute(ops, &ExecOpts::default()).unwrap();
        assert_eq!(read(&p), b"a\nnew\nc\n");
    }

    #[test]
    fn execute_was_check_fails() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("a.txt");
        write(&p, b"a\nactual\nc\n");
        let ops = vec![Op::Replace {
            target: Target::File(p.clone()),
            locator: Locator::Lines { start: 2, end: 2 },
            content: OpContent::Text(b"new".to_vec()),
            was: Some(b"expected\n".to_vec()),
            revert: false,
        }];
        let err = execute(ops, &ExecOpts::default()).unwrap_err();
        assert!(matches!(err, ExecError::Phase1 { .. }));
        assert_eq!(read(&p), b"a\nactual\nc\n");
    }

    #[test]
    fn execute_indent_op() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("a.txt");
        write(&p, b"a\nb\n");
        let ops = vec![Op::Indent {
            target: Target::File(p.clone()),
            locator: Locator::Lines { start: 1, end: 2 },
            kind: IndentKind::Adjust(2),
            revert: false,
        }];
        execute(ops, &ExecOpts::default()).unwrap();
        assert_eq!(read(&p), b"  a\n  b\n");
    }

    #[test]
    fn execute_move_op() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("a.txt");
        write(&p, b"1\n2\n3\n");
        let ops = vec![Op::Move {
            target: Target::File(p.clone()),
            from: Locator::Lines { start: 1, end: 1 },
            to_line: 3,
            position: MovePosition::After,
            revert: false,
        }];
        execute(ops, &ExecOpts::default()).unwrap();
        assert_eq!(read(&p), b"2\n3\n1\n");
    }

    #[test]
    fn execute_insert_op() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("a.txt");
        write(&p, b"a\nc\n");
        let ops = vec![Op::Insert {
            target: Target::File(p.clone()),
            locator: Locator::Lines { start: 1, end: 1 },
            content: OpContent::Text(b"b".to_vec()),
            position: MovePosition::After,
            revert: false,
        }];
        execute(ops, &ExecOpts::default()).unwrap();
        assert_eq!(read(&p), b"a\nb\nc\n");
    }

    #[test]
    fn execute_files_from_expands() {
        let dir = tempdir().unwrap();
        let p1 = dir.path().join("x.txt");
        let p2 = dir.path().join("y.txt");
        write(&p1, b"foo\n");
        write(&p2, b"foo\n");
        let list = dir.path().join("list.txt");
        write(
            &list,
            format!("{}\n{}\n", p1.display(), p2.display()).as_bytes(),
        );

        let ops = vec![Op::Replace {
            target: Target::FilesFrom(list),
            locator: Locator::Pattern {
                regex: regex::bytes::Regex::new("foo").unwrap(),
                count: crate::core::location::Count::All,
            },
            content: OpContent::Repl("BAR".to_string()),
            was: None,
            revert: false,
        }];
        execute(ops, &ExecOpts::default()).unwrap();
        assert_eq!(read(&p1), b"BAR\n");
        assert_eq!(read(&p2), b"BAR\n");
    }
}
