# DSH compatibility validation — source candidate

Validated on Linux x86_64, 2026-11-16. The current checkout remains a source candidate based on 0.6.4. [Machine-readable results](dsh-validation.json) record exact executable identities and selected checks.

| Scope | Result |
|---|---|
| DSH 0.2.1 rc.1, rc.2, rc.3, rc.4, rc.5 | 24 checks per version passed |
| DSH 0.2.2 rc.1, rc.3 | 24 checks per version passed |
| Cross-version upgrade and compact tools | 8 passed |
| Rust workspace | 280 passed; one opt-in lifecycle test ignored by the ordinary suite and executed separately for each DSH host |
| Minimum Rust 1.89 | Workspace/all-target check passed |
| Formatting, Clippy, documentation | Passed |
| Local Cargo package verification | Passed |
| npm preparation and consumer contracts | 20 + 13 passed |

The DSH lane completed **176 checks with zero failures and zero skips**. Registered tools and idle official AgentLoop instances exercised disk bytes, observations, sandbox, denied escalation, diff metadata, rewind, profile boot and shutdown. Every version exercised a consumer-installed npm artifact through fresh, ordinary and linked dependency layouts. Each layout performed installation, real verification, removal, restored-provider verification and cleanup while checking neighboring profiles and shared dependencies. Five injected installation failure phases plus the retained name-based installation route ran through the real host for each version.

The upgrade case installs against 0.2.1-rc.5 with rewind 0.19.2, switches to the real 0.2.2-rc.1 host and rewind 0.21.2 dependency tree, then verifies and removes the adapter. Current user package fields, lock bytes, root dependency links, file permissions and the user patch remain preserved. A modified adapter file leaves installation evidence intact and produces an integrity conflict. A symlinked temporary-directory fixture exercises canonical-path verification.

The matrix pins one executable for all hosts. A subsequent permission-only restoration correction was checked on the final executable with the full eight upgrade/compact scenarios and a direct permission-only verify/uninstall journey; the JSON records both SHA256 values. Rust regression, Clippy, minimum-version and package verification cover the final Rust source. Focused selection was also checked with only one selected compact host case.

[Token measurements](benchmarks/dsh-compact-results.md) cover 112 real registered tool calls across seven hosts with zero model generations. Counts compare serialized schemas and successful mutation receipts under two tokenizer proxies; long reads retain their full content.

## Reproduce

```sh
source /workspace/.xnip-tools/activate.sh
cd /workspace/xnip
cargo test --workspace --locked
python3 tests/native_e2e.py --host dsh
node benches/native/dsh_compact_bench.mjs
```

Raw matrix evidence remains under `target/native-e2e`; the tokenizer output is `target/benchmarks/dsh-compact.json`. The reusable cloud configuration saves installation and startup instructions separately from publishing the environment snapshot.

## Boundaries

These checks establish the Linux DSH lane. Other Agent SDK lanes retain their separately recorded evidence; macOS and Windows DSH journeys need their own platform runs. Runtime feature checks govern additional compatible releases; the recorded version list is the measured set. Preset tool configuration overrides in compact mode are captured at installation and refreshed through reinstallation. DSH 0.2.2's optional `apply_patch` retains the official host implementation and is exercised as such; the xnip-backed byte-editing routes here are `read`, `write` and `edit`.

## 0.6.5 release follow-up

The release review added a ninth upgrade/compact check, bringing the selected DSH gate to 177 checks (the full catalog defines 506). Nine local upgrade/compact cases passed with no failures or skips. The historical fixture contains the exact adapter sources from commit `45230e595efaf9c449ef7dd3a55f2776c127ef99`, with SHA256 provenance and the original manifest shape. Its ordinary offline case uses a compatible current server at a separate installation path.

A separate run supplied the actual public `xnip-linux-x64-musl@0.6.4` payload, verified its binary SHA256 and version, installed with the original CLI/adapter, switched to DSH 0.2.2-rc.1 and rewind 0.21.2, and completed current-CLI verification, uninstall and restored verification. It also checked that losing the old bound executable fails verification while preserving installation evidence, then succeeds when that executable returns. The profile's upgraded package fields and lock bytes remain preserved.

The review corrected verification to use the profile-bound executable and propagated the prepared-host parent into focused upgrade/compact tests. An empty custom parent fails; a parent containing only the selected rc.3 host passes one case, with no skips. The release wait window also now covers the configured full CI horizon; remote CI and six-platform publication acceptance remain the release gates.
