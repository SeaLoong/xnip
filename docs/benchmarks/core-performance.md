# xnip core performance

These measurements compare execution strategies in the same working-tree build.
They are separate from [native editor input costs](README.md): no agent, model or
token usage is measured. The measured crate version is `0.4.0`; source commits
and executable hashes identify the measured implementation.

## In-process strategy comparisons

[Criterion estimates and samples](core-criterion-results.json) record the
benchmark executable, compiler, CPU, source hashes and 95% confidence intervals.
The host was Windows 11, AMD64 Family 25 Model 33, with 16 logical CPUs. Each case
used 10 samples after a one-second warmup and a two-second measurement request.

| Operation | Mean | 95% confidence interval |
|---|---:|---:|
| Replace one middle line, byte-span copy, 10,000 lines | 47.33 µs | 46.70–48.15 µs |
| Same result, reference line splitting/joining | 272.39 µs | 263.90–281.94 µs |
| Independent replacement check, 1 edit, 100,000 lines | 3.99 ms | 3.76–4.26 ms |
| Independent replacement check, 8 edits, 100,000 lines | 4.68 ms | 4.57–4.80 ms |
| Independent replacement check, 32 edits, 100,000 lines | 7.84 ms | 7.35–8.43 ms |
| Eight exact edits, owned read then update, 100,000 lines | 20.93 ms | 20.19–21.68 ms |
| Same edits, borrowed plan transformation | 11.51 ms | 11.09–11.97 ms |
| 128 equal-length exact edits, fresh output buffers, 100,000 lines | 181.23 ms | 169.66–195.49 ms |
| Same equal-length edits, copy-on-write reuse | 46.10 ms | 44.48–47.91 ms |
| 128 growing/shrinking exact edits, fresh output buffers, 100,000 lines | 225.23 ms | 217.24–235.23 ms |
| Same growing/shrinking edits, copy-on-write reuse | 86.27 ms | 84.69–87.70 ms |
| 128 identical replacements, fresh output buffers, 100,000 lines | 179.31 ms | 170.38–186.79 ms |
| Same validated no-ops, copy-on-write path | 60.77 ms | 55.55–68.30 ms |

In this run, reference line joining took 5.75 times the byte-span time. Owned
read/update took 1.82 times the borrowed transformation time. For 128 edits,
fresh-buffer/COW time ratios were 3.93 for equal lengths, 2.61 for growing/shrinking
content and 2.95 for validated no-ops. These compare current strategies in the
same benchmark executable; they are not before/after release speedups.

The line-join reference handles the fixed LF-terminated single-line fixture and
is checked against the optimized result before timing. It is a reference
algorithm inside the benchmark, not an older xnip executable.

The exact-edit comparisons use the same matcher and edits on both paths. The
128-edit fresh-buffer strategy uses the public borrowed transformation interface
and constructs a new output after each validated edit. COW uses the dedicated
exact-edit interface and reuses the mutable current buffer. Expected bytes are
assembled independently from the known replacement records and untouched suffix;
both strategies must match that oracle before timing and leave disk bytes
unchanged. Fixture loading is outside the measured region, but the initial
current buffer still shares its original snapshot, so the first required COW
copy is timed. Growing/shrinking edits are near the beginning of the fixture and
therefore move most of the tail; this is not a cheap append-only case.

Batch checks include cached filesystem reads and plan validation. These
in-process groups exclude commits, journal writes and diff rendering.

Reproduce from a stable source tree:

```sh
XNIP_AUTO_UPDATE=0 cargo bench --bench large_file --locked -- \
  'replace_range_10k|batch_check_100k|plan_exact_100k|plan_exact_reuse_100k' \
  --sample-size 10 --warm-up-time 1 --measurement-time 2 \
  --save-baseline release-0.4.0 --noplot
```

Criterion writes estimates and raw samples under `target/criterion/NAME/new/`.
The committed JSON copies the current estimates and samples, including outliers.
Source and benchmark-executable hashes were stable across the run, and all
sample files were regenerated during this measurement. The named baseline keeps
the run separate from historical `change/` comparisons.

## Whole-process planning probes

[CLI samples and provenance](core-performance-results.json) come from
[core_perf.py](../../benches/core_perf.py). The script runs the actual `xnip 0.4.0`
release binary with updates disabled and five repetitions of `apply --check --quiet`.
Its source commit matches the Criterion report. Measurements include startup,
manifest parsing and cached local reads.

Every case first performs an unmeasured execution and compares all resulting
bytes with an independent expected result, including unchanged bytes for the
no-op fixture. Timed checks must leave the original bytes unchanged. Inputs are
fresh temporary files, and no installed xnip is replaced.

The standard fixture is 4 MiB / 65,536 lines, with 1, 8, 32 and 128 independent
changes. Grouped line edits and shorter ordered exact replacements produce the
same intended final bytes, with different matching and ordering contracts.
Three additional 128-edit cases preserve replacement lengths, alternate growth
and shrinkage, or replace text with itself. These have different expected results
and characterize distinct workloads. The dense fixture is the same 4 MiB split
into 2,097,152 short lines, with 1 or 32 grouped changes.

| Lines | Strategy | Edits | Median | Minimum | Maximum observed working set |
|---:|---|---:|---:|---:|---:|
| 65,536 | Grouped lines | 1 | 90.54 ms | 83.49 ms | 19.62 MiB |
| 65,536 | Ordered exact, shorter | 1 | 90.92 ms | 88.16 ms | 15.61 MiB |
| 65,536 | Grouped lines | 8 | 94.63 ms | 87.50 ms | 19.69 MiB |
| 65,536 | Ordered exact, shorter | 8 | 91.96 ms | 88.80 ms | 15.65 MiB |
| 65,536 | Grouped lines | 32 | 89.14 ms | 87.70 ms | 19.64 MiB |
| 65,536 | Ordered exact, shorter | 32 | 107.55 ms | 104.56 ms | 15.64 MiB |
| 65,536 | Grouped lines | 128 | 109.47 ms | 106.57 ms | 19.71 MiB |
| 65,536 | Ordered exact, shorter | 128 | 172.78 ms | 168.19 ms | 15.71 MiB |
| 65,536 | Ordered exact, equal length | 128 | 156.36 ms | 151.43 ms | 15.67 MiB |
| 65,536 | Ordered exact, growth/shrinkage | 128 | 186.71 ms | 173.28 ms | 19.69 MiB |
| 65,536 | Ordered exact, validated no-op | 128 | 162.23 ms | 151.31 ms | 11.72 MiB |
| 2,097,152 | Grouped lines | 1 | 97.74 ms | 91.54 ms | 19.61 MiB |
| 2,097,152 | Grouped lines | 32 | 97.04 ms | 96.05 ms | 19.65 MiB |

Each CLI sample combines a fresh process start, manifest parsing, cached file
reads and validation. Those costs are not measured separately, so no portion of
these times can be assigned to startup or an individual module. Small
nonmonotonic differences are not a scalability claim. Criterion's in-process
microseconds and these whole-process milliseconds have different scopes and
must not be compared as if they measured the same operation.

Ordered exact cases reuse the current buffer after protecting the original
snapshot. Equal lengths copy the matched span; growth and shrinkage move the
tail; validated no-ops avoid mutation. Every ordered edit still scans for global
uniqueness, including no-ops. Grouped replacements validate original coordinates
and assemble one final output. The table compares current workloads and does
not establish a historical speedup. All per-run observations remain in the JSON.

Windows working-set values are cumulative OS peak counters sampled every
millisecond; the final interval before process exit can be missed. Other
platforms record memory as unavailable. These are whole-process working sets,
not allocator-specific peak heap measurements.

```sh
cargo build --release --locked
python benches/core_perf.py --xnip target/release/xnip \
  --output docs/benchmarks/core-performance-results.json --repeats 5
```

On Windows, pass `target/release/xnip.exe`. The script records CPU, OS, version,
binary SHA256 and hashes of Rust/Cargo/benchmark inputs, and refuses to write a
report if an input or the binary changes during measurement.

## Windows path identity checks

A separate temporary-directory probe compared `CharUpperW`,
`CompareStringOrdinal(ignoreCase)` and actual filesystem alias lookup on this
Windows host. All three agreed:

| Names | Same target |
|---|---|
| Ä / ä | Yes |
| Σ / σ | Yes |
| Σ / ς | No |
| ſ / S | No |
| ß / SS | No |
| K / k | No |

The transaction module and compact manifest use one OS-mapped UTF-16 identity.
Rust Unicode full folding would incorrectly merge some distinct names above.
Contracts cover new-target aliases, ordered virtual updates, backup collisions,
duplicate recovery targets and case-aliased paths inside the recovery journal.
An unpaired-surrogate case verifies that identities preserve raw Windows path
units rather than substituting replacement characters. Case-sensitive Windows
directories retain xnip's existing conservative case-insensitive contract.

## What changed and what remains

Line replacement locates the selected byte span and copies the unchanged prefix
and suffix once. It no longer allocates a view and newline flag for every line.
Independent grouped replacements retain only requested line boundaries, validate
all original-byte preconditions and overlapping ranges, then assemble one output.
Anchor locators still resolve against the original contents; mixed operations,
pattern replacements and dependent sequential plans keep their existing path.

The transaction plan shares immutable original bytes with its initial virtual
state. Ordered unique exact edits first validate missing text and ambiguity,
including overlapping matches, against the current virtual contents. The first
real change separates the current buffer from the original snapshot; later
equal-length replacements copy only the matched span, while length changes use
a splice that moves the tail. A complete-file match can build directly from the
new payload. Identical replacements still validate and count matches but avoid
cloning file contents. Non-no-op `all` replacements retain the fresh-output
algorithm and its non-overlapping match semantics.

The owned `read` and borrowed `transform` interfaces remain available. Contract
tests independently check bytes, growth, shrinkage, deletion, binary/UTF-8
payloads, no-op/overlap rules, late failures and snapshots shared across rename.
Stage-time and commit-time external-change checks, per-target validation,
recovery snapshots and rollback remain in place. A second file sync is skipped
only when staging already synced the requested permissions; restoring a
different snapshot mode still updates and syncs that metadata.

Reuse removes repeated full-output allocation/copying, not matching: every exact
edit still scans the current contents to establish global uniqueness. Ordered
exact editing can therefore still take O(edits × file size) search work, and
resizing can move a large tail per edit. A current file buffer and protected
original snapshot still need memory; inputs are loaded in full and requested
previews still build diffs. These fixtures do not establish commit throughput,
power-loss durability, cross-file atomic visibility or complete-agent latency.
