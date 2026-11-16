# DSH compact tool projections

Measured on Linux, 2026-11-16. [Raw counts](dsh-compact-results.json) record seven official hosts: 0.2.1 rc.1–rc.5 and 0.2.2 rc.1/rc.3. Each host ran four byte-checked scenarios in ordinary and compact modes: 56 read/edit exchanges, 112 registered tool calls, zero model generations.

| Model-visible component | Tokenizer | Ordinary | Compact | Reduction |
|---|---|---:|---:|---:|
| Tool schemas | cl100k_base | 507 | 422 | 16.8% |
| Tool schemas | o200k_base | 509 | 425 | 16.5% |
| Sample edit receipt | cl100k_base | 36 | 11 | 69.4% |
| Sample edit receipt | o200k_base | 37 | 11 | 70.3% |

Both modes use the same argument keys and literal matching semantics. Schemas keep sandbox escalation and justification descriptions. Read content, canonical values, diff metadata, approval and rewind stay in the official pipeline. Compact mode changes the schema descriptions and successful mutation renderer. All seven hosts produced the same component counts in this run.

The fixture selects `read`, `write` and `edit`. Configured 0.2.2 profiles may also advertise `apply_patch`, which keeps its official schema and implementation. Counts tokenize serialized schema objects and content arrays, including their JSON envelopes; these are consistent representation comparisons.

These are component measurements with `js-tiktoken@1.0.21`, using `cl100k_base` and `o200k_base` as explicit proxies. They measure model-facing `content`, rather than internal canonical values and UI metadata. They establish representation costs; model billing, reasoning, retries and task completion need separate model experiments. Ordinary provider replacement keeps the host's model representation and primarily improves byte correctness.

Long file reads dominate an exchange. In this run, the ordinary long-context read/edit results contain 1,305 cl100k tokens; removing 25 receipt tokens saves about 1.9% of those results. The schema reduction applies when schemas are sent. Keep these denominators separate from receipt-only percentages.

Reproduce from the repository root with prepared hosts and a debug binary:

```sh
cargo build --locked
node benches/native/dsh_native_setup.mjs
npm install --prefix target/tokenizer --ignore-scripts --no-audit --no-fund --save-exact js-tiktoken@1.0.21
node benches/native/dsh_compact_bench.mjs
# A subset: append exact prepared versions to the command.
```

The command writes `target/benchmarks/dsh-compact.json`, checks expected UTF-8/BOM/EOL/EOF bytes independently, captures the executable SHA256 and rejects model generation. Installation costs are separate. The optional compact installer captures the configured tool rows in preset overrides; refresh the installation when adopting changed preset configurations.
