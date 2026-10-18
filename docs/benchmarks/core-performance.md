# xnip core performance

These measurements compare execution strategies in the same working-tree build.
They are separate from [native editor input costs](README.md): no agent, model or
token usage is measured. The build reports `xnip 0.3.1`; source and executable
hashes identify the measured implementation.

## In-process strategy comparisons

[Criterion estimates and samples](core-criterion-results.json) record the
benchmark executable, compiler, CPU, source hashes and 95% confidence intervals.
The host was Windows 11, AMD64 Family 25 Model 33, with 16 logical CPUs. Each case
used 10 samples after a one-second warmup and a two-second measurement request.

| Operation | Mean | 95% confidence interval |
|---|---:|---:|
| Replace one middle line, byte-span copy, 10,000 lines | 48.21 µs | 46.87–49.59 µs |
| Same result, reference line splitting/joining | 273.06 µs | 268.28–277.53 µs |
| Independent replacement check, 1 edit, 100,000 lines | 3.28 ms | 3.25–3.31 ms |
| Independent replacement check, 8 edits, 100,000 lines | 3.90 ms | 3.87–3.94 ms |
| Independent replacement check, 32 edits, 100,000 lines | 6.77 ms | 6.53–7.06 ms |
| Eight exact edits, owned read then update, 100,000 lines | 18.81 ms | 18.54–19.09 ms |
| Same edits, borrowed plan transformation | 11.47 ms | 11.27–11.67 ms |
| 128 equal-length exact edits, fresh output buffers, 100,000 lines | 156.49 ms | 153.95–159.41 ms |
| Same equal-length edits, copy-on-write reuse | 45.38 ms | 44.69–46.10 ms |
| 128 growing/shrinking exact edits, fresh output buffers, 100,000 lines | 229.82 ms | 224.81–235.24 ms |
| Same growing/shrinking edits, copy-on-write reuse | 82.71 ms | 81.39–84.10 ms |
| 128 identical replacements, fresh output buffers, 100,000 lines | 157.66 ms | 155.35–160.18 ms |
| Same validated no-ops, copy-on-write path | 45.84 ms | 44.57–47.43 ms |

In this run, reference line joining took 5.66 times the byte-span time. Owned
read/update took 1.64 times the borrowed transformation time. For 128 edits,
fresh-buffer/COW time ratios were 3.45 for equal lengths, 2.78 for growing/shrinking
content and 3.44 for validated no-ops. These compare current strategies in the
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
  --sample-size 10 --warm-up-time 1 --measurement-time 2
```

Criterion writes estimates and raw samples under `target/criterion/NAME/new/`.
The committed JSON copies those estimates; historical `change/` comparisons from
unrelated cached runs are excluded.

## Whole-process planning probes

[CLI samples and provenance](core-performance-results.json) come from
[core_perf.py](../../benches/core_perf.py). The script runs the actual release
binary with updates disabled and five repetitions of `apply --check --quiet`.
It includes startup, manifest parsing and cached local reads.

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
| 65,536 | Grouped lines | 1 | 96.65 ms | 88.80 ms | 19.61 MiB |
| 65,536 | Ordered exact, shorter | 1 | 97.94 ms | 92.11 ms | 15.62 MiB |
| 65,536 | Grouped lines | 8 | 94.43 ms | 89.10 ms | 19.61 MiB |
| 65,536 | Ordered exact, shorter | 8 | 93.65 ms | 90.96 ms | 15.60 MiB |
| 65,536 | Grouped lines | 32 | 93.00 ms | 90.24 ms | 19.65 MiB |
| 65,536 | Ordered exact, shorter | 32 | 103.14 ms | 101.33 ms | 15.62 MiB |
| 65,536 | Grouped lines | 128 | 100.16 ms | 97.55 ms | 19.68 MiB |
| 65,536 | Ordered exact, shorter | 128 | 190.14 ms | 180.58 ms | 15.71 MiB |
| 65,536 | Ordered exact, equal length | 128 | 160.78 ms | 155.37 ms | 15.67 MiB |
| 65,536 | Ordered exact, growth/shrinkage | 128 | 187.37 ms | 174.99 ms | 19.71 MiB |
| 65,536 | Ordered exact, validated no-op | 128 | 161.71 ms | 154.60 ms | 11.65 MiB |
| 2,097,152 | Grouped lines | 1 | 105.44 ms | 101.85 ms | 19.62 MiB |
| 2,097,152 | Grouped lines | 32 | 100.03 ms | 97.55 ms | 19.65 MiB |

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
