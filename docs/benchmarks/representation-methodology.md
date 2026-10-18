# Supplementary representation and xnip measurements

This separate experiment measures serialized edit representations and current xnip interfaces. Its outputs are [representation-results.md](representation-results.md) and [representation-results.json](representation-results.json); the runner is [edit_cost.py](../../benches/edit_cost.py). The [primary native benchmark](README.md) runs third-party native editors.

## What it measures

The same 13 deterministic fixtures cover small configuration changes, a local change in a 2,000-line file, multiline code, Unicode, CRLF, missing EOF newline, repeated text, explicit global replacement, same/multiple-file changes, a 40,000-character line, insertion and deletion. Separate cases exercise file lifecycle and rejected edits followed by a read and corrected retry.

Every xnip execution checks expected file bytes. The suite exercises single MCP edits, direct MCP batches, line manifests, native literal manifests and imported patches where supported. It captures actual default/full MCP catalogs, current integration instructions, bounded/unlimited output and CLI output modes.

Other columns serialize whole-file JSON, old/new JSON, aider-style blocks, unified diff and apply_patch-style text. They measure representation size only. They do not establish third-party success rates, matching behavior, latency or model quality. Aider-like serialization in this script does not replace the actual aider parser measured by the primary native suite.

| Cost | Measurement |
|---|---|
| Advertised tools | Actual initialize instructions plus tools/list catalogs |
| Agent instructions | Current integration entry points; installed roles differ |
| Initial context | Known old blocks or full fixture files, as separate assumptions |
| Edit payload | Complete serialized arguments or textual format |
| Result | Actual normalized xnip MCP result or serialized CLI stdout/stderr |
| Retry | Rejected xnip call, actual read, corrected call and responses |
| Local runtime | Median xnip execution time with its call/process scope |

Known-block and full-file context are alternatives. Content blocks and independent calls are counted separately, so unrelated strings cannot merge token boundaries. MCP result counting includes the serialized result object in this experiment; the native primary report counts captured returned text separately. Their numbers have different accounting boundaries.

Native manifest payloads use raw CLI stdin. Passing the same content as an MCP manifest_text string adds JSON quoting; direct edits arrays avoid that nesting. The simple patch/block example generator excludes CRLF and missing-EOF fixtures because it does not encode their special newline semantics. That restriction is a generator limit, not a claim about every native editor.

## Reproduce

~~~sh
cargo build --locked
python benches/edit_cost.py --repeats 3 --json target/edit-cost/representation-results.json --markdown target/edit-cost/representation-results.md
~~~

Python 3.10+ and a built xnip suffice for exact UTF-8 bytes and character counts. Tokens stay null without a tokenizer. No bytes-divided-by-four token estimates are used.

~~~sh
python -m pip install --target target/benchmark-python tiktoken==0.14.0
python benches/edit_cost.py --encoding o200k_base --repeats 3 --json docs/benchmarks/representation-results.json --markdown docs/benchmarks/representation-results.md
~~~

The recorded encoding is o200k_base. Counts are exact for that encoding and captured serialization; they exclude provider billing, hidden prompts, caching, reasoning and host framing. A first vocabulary load may use the network; cache it for offline runs.

Every xnip subprocess receives XNIP_AUTO_UPDATE=0 without changing the host environment. Temporary fixture paths are normalized to /workspace before output counting. Warm MCP and CLI process timings differ in scope and are not a speed ranking.

## Provenance and interpretation

JSON retains fixtures, expected bytes, payloads, real xnip responses, schema snapshots, instruction text/hashes/counts and retry transcripts. A source fingerprint covers tracked and untracked Rust/build/instruction inputs, Cargo manifests/lockfile and the runner; binary SHA-256 is checked before and after execution. A source or binary change during measurement prevents publication of a mixed report.

The suite compares current interfaces horizontally. It does not retain a historical binary baseline. The captured source revision and dirty-tree hashes identify the measured working tree rather than asserting that the data represents every future build.

Whole-file output can be shortest on small files. Line-based context can dominate a tiny substring edit in a very long line. A single patch can batch several changes. The relevant choice depends on known context, file semantics and tool contracts; payload size alone cannot predict model retries or correctness.

The format choices are grounded in upstream documentation: [Codex apply-patch source](https://github.com/openai/codex/tree/main/codex-rs/apply-patch), [aider edit formats](https://aider.chat/docs/more/edit-formats.html), [OpenCode tools](https://opencode.ai/docs/tools/) and [Gemini CLI source](https://github.com/google-gemini/gemini-cli/tree/main/packages/core/src/tools). These references explain formats; exact tested versions, native sources and observed behavior belong to [the native method](native-methodology.md) and its raw provenance. No invented Claude Code or Cursor executor is scored.

A complete agent experiment must additionally retain the model, harness version, prompts, tool catalog, mandatory reads, all attempts/results, provider usage, task assertions and independent repeated runs. Neither offline suite ran paid inference or measures total agent savings.
