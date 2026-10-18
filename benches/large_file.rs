//! 大文件性能基准。
//!
//! 测量 xnip 对大文件（10K / 100K 行）执行典型操作的吞吐：
//! - `replace_range`：行级替换
//! - `replace_pattern`：regex 全文替换
//! - `apply_indent`：批量缩进
//! - `unified_diff`：diff 生成
//!
//! 跑：`cargo bench`
//!
//! Core operation groups exclude process startup and filesystem IO.
//! Batch checks include cached reads and validation; plan groups load fixtures outside timing.
//! Neither group commits writes.

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use regex::bytes::Regex as ByteRegex;
use std::hint::black_box;
use xnip::core::diff::unified_diff;
use xnip::core::location::Count;
use xnip::core::ops::find::{FindMode, FindOpts, scan};
use xnip::core::ops::indent::{IndentOp, apply_indent};
use xnip::core::ops::replace::{replace_pattern, replace_range};

fn make_lines(n: usize) -> Vec<u8> {
    let mut buf = Vec::with_capacity(n * 32);
    for i in 0..n {
        buf.extend_from_slice(format!("line {i}: lorem ipsum dolor sit amet\n").as_bytes());
    }
    buf
}

fn bench_replace_range(c: &mut Criterion) {
    let body = make_lines(10_000);
    let mut g = c.benchmark_group("replace_range_10k");
    g.throughput(Throughput::Bytes(body.len() as u64));
    let expected = reference_line_join(&body, 5_000, b"REPLACED");
    assert_eq!(
        replace_range(&body, 5_000, 5_000, b"REPLACED").unwrap(),
        expected
    );
    g.bench_function("middle", |b| {
        b.iter(|| black_box(replace_range(black_box(&body), 5_000, 5_000, b"REPLACED").unwrap()));
    });
    g.bench_function("reference_line_join", |b| {
        b.iter(|| black_box(reference_line_join(black_box(&body), 5_000, b"REPLACED")));
    });
    g.finish();
}

fn bench_replace_pattern(c: &mut Criterion) {
    let body = make_lines(10_000);
    let re = ByteRegex::new("lorem").unwrap();
    let mut g = c.benchmark_group("replace_pattern_10k");
    g.throughput(Throughput::Bytes(body.len() as u64));
    g.bench_function("all", |b| {
        b.iter(|| {
            let _ = replace_pattern(&body, &re, "LOREM", Count::All);
        });
    });
    g.finish();
}

fn bench_indent(c: &mut Criterion) {
    let body = make_lines(10_000);
    let mut g = c.benchmark_group("indent_10k");
    g.throughput(Throughput::Bytes(body.len() as u64));
    g.bench_function("add_2_spaces", |b| {
        b.iter(|| {
            let _ = apply_indent(&body, 1, 10_000, IndentOp::Add(2)).unwrap();
        });
    });
    g.finish();
}

fn bench_diff(c: &mut Criterion) {
    let before = make_lines(1_000);
    let after = replace_range(&before, 500, 500, b"CHANGED LINE").unwrap();
    let mut g = c.benchmark_group("diff_1k");
    g.throughput(Throughput::Bytes(before.len() as u64));
    g.bench_function("unified", |b| {
        b.iter(|| {
            let _ = unified_diff(std::path::Path::new("x.txt"), &before, &after);
        });
    });
    g.finish();
}

fn bench_find(c: &mut Criterion) {
    let body = make_lines(100_000);
    let re = ByteRegex::new("lorem").unwrap();
    let mut group = c.benchmark_group("find_100k");
    for (name, cap) in [("first", Some(1)), ("all", None)] {
        let opts = FindOpts {
            mode: FindMode::Pattern(&re),
            max_matches: cap,
            first_only: false,
        };
        group.bench_function(name, |b| {
            b.iter(|| black_box(scan(black_box(&body), &opts)));
        });
    }
    group.finish();
}

fn bench_literal_edit(c: &mut Criterion) {
    let body = make_lines(10_000);
    let mut group = c.benchmark_group("edit_literal_10k");
    group.bench_function("unique_block", |b| {
        b.iter(|| {
            black_box(
                xnip::core::edit::replace_exact(
                    black_box(&body),
                    b"line 5000: lorem ipsum dolor sit amet\n",
                    b"replacement\n",
                    false,
                )
                .unwrap(),
            );
        });
    });
    group.finish();
}

// A current-process reference strategy, not an older xnip release.
// The fixture is LF-terminated and selects one line; the result is checked above.
fn reference_line_join(content: &[u8], line: usize, payload: &[u8]) -> Vec<u8> {
    let lines: Vec<_> = content.split_inclusive(|byte| *byte == b'\n').collect();
    assert!((1..=lines.len()).contains(&line));
    let mut after = Vec::with_capacity(content.len() + payload.len());
    for bytes in &lines[..line - 1] {
        after.extend_from_slice(bytes);
    }
    after.extend_from_slice(payload);
    if !payload.is_empty() && !payload.ends_with(b"\n") {
        after.push(b'\n');
    }
    for bytes in &lines[line..] {
        after.extend_from_slice(bytes);
    }
    after
}

fn bench_batch_checks(c: &mut Criterion) {
    use xnip::apply::{
        Op, OpContent, Target,
        commit::{ExecOpts, execute_report},
    };
    use xnip::core::location::Locator;
    let body = make_lines(100_000);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("input");
    std::fs::write(&path, &body).unwrap();
    let opts = ExecOpts {
        check: true,
        ..ExecOpts::default()
    };
    let mut group = c.benchmark_group("batch_check_100k");
    group.throughput(Throughput::Bytes(body.len() as u64));
    for count in [1, 8, 32] {
        group.bench_function(format!("independent_lines_{count}"), |b| {
            b.iter(|| {
                let ops = (0..count)
                    .map(|index| Op::Replace {
                        target: Target::File(path.clone()),
                        locator: Locator::Lines {
                            start: (index + 1) * 1000,
                            end: (index + 1) * 1000,
                        },
                        content: OpContent::Text(b"changed".to_vec()),
                        was: None,
                        revert: false,
                    })
                    .collect();
                black_box(execute_report(ops, &opts).unwrap());
            });
        });
    }
    group.finish();
    assert_eq!(std::fs::read(path).unwrap(), body);
}

fn bench_plan_transforms(c: &mut Criterion) {
    use criterion::BatchSize;
    use xnip::core::{edit::replace_exact, transaction::Plan};
    let body = make_lines(100_000);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("input");
    std::fs::write(&path, &body).unwrap();
    let edits: Vec<_> = (0..8)
        .map(|i| format!("line {i}: lorem ipsum dolor sit amet\n").into_bytes())
        .collect();
    let mut group = c.benchmark_group("plan_exact_100k");
    for borrowed in [false, true] {
        group.bench_function(
            if borrowed {
                "borrowed_transform"
            } else {
                "owned_read_update"
            },
            |b| {
                b.iter_batched(
                    || {
                        let mut plan = Plan::default();
                        plan.read(&path).unwrap();
                        plan
                    },
                    |mut plan| {
                        for old in &edits {
                            if borrowed {
                                plan.transform(&path, |before| {
                                    Ok(replace_exact(before, old, b"changed\n", false)?.0)
                                })
                                .unwrap();
                            } else {
                                let before = plan.read(&path).unwrap();
                                let after =
                                    replace_exact(&before, old, b"changed\n", false).unwrap().0;
                                plan.update(&path, after).unwrap();
                            }
                        }
                        black_box(plan);
                    },
                    BatchSize::PerIteration,
                );
            },
        );
    }
    group.finish();
    assert_eq!(std::fs::read(path).unwrap(), body);
}

fn bench_plan_exact_reuse(c: &mut Criterion) {
    use criterion::BatchSize;
    use xnip::core::{edit::replace_exact, transaction::Plan};
    let body = make_lines(100_000);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("input");
    std::fs::write(&path, &body).unwrap();
    let mut group = c.benchmark_group("plan_exact_reuse_100k");
    for shape in ["equal_128", "resize_128", "noop_128"] {
        let edits: Vec<_> = (0..128)
            .map(|index| {
                let old = format!("line {index}: lorem ipsum dolor sit amet\n").into_bytes();
                let new = match shape {
                    "equal_128" => old.to_ascii_uppercase(),
                    "resize_128" if index % 2 == 0 => b"short\n".to_vec(),
                    "resize_128" => format!("expanded {index}: {}\n", "y".repeat(128)).into_bytes(),
                    _ => old.clone(),
                };
                (old, new)
            })
            .collect();
        let original_prefix: usize = edits.iter().map(|(old, _)| old.len()).sum();
        let expected = [
            edits
                .iter()
                .flat_map(|(_, new)| new.iter().copied())
                .collect::<Vec<_>>(),
            body[original_prefix..].to_vec(),
        ]
        .concat();
        for cow in [false, true] {
            let apply = |plan: &mut Plan| {
                for (old, new) in &edits {
                    if cow {
                        plan.replace_exact(&path, old, new, false).unwrap();
                    } else {
                        plan.transform(&path, |before| {
                            Ok(replace_exact(before, old, new, false)?.0)
                        })
                        .unwrap();
                    }
                }
            };
            let mut oracle = Plan::default();
            apply(&mut oracle);
            assert_eq!(oracle.read(&path).unwrap(), expected);
            assert_eq!(oracle.changed_files().is_empty(), shape == "noop_128");
            group.bench_function(
                format!(
                    "{shape}/{}",
                    if cow { "copy_on_write" } else { "fresh_buffer" }
                ),
                |b| {
                    b.iter_batched(
                        || {
                            let mut plan = Plan::default();
                            plan.read(&path).unwrap();
                            plan
                        },
                        |mut plan| {
                            apply(&mut plan);
                            black_box(plan);
                        },
                        BatchSize::PerIteration,
                    );
                },
            );
        }
    }
    group.finish();
    assert_eq!(std::fs::read(path).unwrap(), body);
}

criterion_group!(
    benches,
    bench_replace_range,
    bench_replace_pattern,
    bench_indent,
    bench_diff,
    bench_find,
    bench_literal_edit,
    bench_batch_checks,
    bench_plan_transforms,
    bench_plan_exact_reuse
);
criterion_main!(benches);
