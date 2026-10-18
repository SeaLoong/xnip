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
//! Measures in-memory core operations; process startup and filesystem IO are excluded.

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
    g.bench_function("middle", |b| {
        b.iter(|| {
            let _ = replace_range(&body, 5_000, 5_000, b"REPLACED").unwrap();
        });
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

criterion_group!(
    benches,
    bench_replace_range,
    bench_replace_pattern,
    bench_indent,
    bench_diff,
    bench_find,
    bench_literal_edit
);
criterion_main!(benches);
