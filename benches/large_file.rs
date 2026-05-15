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
//! PLAN §7.8 性能目标：100MB 文件 single op < 2s（macOS M 系；其它平台放宽）。

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use regex::bytes::Regex as ByteRegex;
use xnip::core::diff::unified_diff;
use xnip::core::location::Count;
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

criterion_group!(
    benches,
    bench_replace_range,
    bench_replace_pattern,
    bench_indent,
    bench_diff
);
criterion_main!(benches);
