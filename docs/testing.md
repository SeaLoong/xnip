# Testing xnip

Tests cover observable bytes, commit state, rejected writes, output budgets and protocol behavior. Matrices share independently specified expected results across input formats and transports. Test-function counts are not coverage percentages.

## Run the checks

```sh
cargo test --workspace --locked
cargo test --workspace --doc --locked
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo +1.89.0 check --workspace --all-targets --locked
cargo deny check
cargo bench --no-run --locked
cargo doc --workspace --no-deps --locked
cargo package --locked
cargo run --locked -p xtask -- sync-integrations
```

Use `RUSTDOCFLAGS=-D warnings` for documentation checks in CI. Integration generation must leave committed outputs unchanged. See [contributing](../CONTRIBUTING.md) for tool prerequisites and packaging from a dirty working tree.

Platform installers have separate offline suites:

```sh
# Linux/macOS
python3 tests/install_sh.py
# Windows: first build target/debug/xnip.exe
cargo build --locked
powershell -NoProfile -File tests/install_ps1.ps1
```

Benchmark plumbing can be checked without installing native editor dependencies:

```sh
python3 tests/native_benchmark_contract.py
```

Native editor runs and fixture selection are documented in [benchmarks](benchmarks/README.md). Installer and ordinary test children disable automatic updates. Enabled updater contracts use copied binaries, isolated state and loopback release fixtures; they do not install into the user's normal xnip path.

## Contract map

| Suite | Observable contracts |
|---|---|
| [core contracts](../src/core/contract_tests.rs) | Inclusive byte ranges; LF/CRLF/unterminated EOF/Unicode/invalid UTF-8; locators and empty-anchor rejection; literal/regex replacement; insert/move/indent; diff; content/revert; atomic staging, backups and permissions; shared bounded reads and writer failures |
| [apply contracts](../src/apply/contract_tests.rs) | Native/JSON/YAML equivalence; malformed/unknown fields; detection; serial/parallel planning; stdin and external payload resolution; lifecycle coalescing; overlapping-plan rejection; revert and patch planning |
| [transaction contracts](../src/core/transaction.rs) | Fault injection at commit steps; rollback; interrupted journals; integrity verification; repeat recovery; conflict refusal at existing and newly created paths |
| [edit CLI](../tests/edit_contract.rs) | Literal old/text bytes and ambiguity; preview/check/no-op; create/delete/rename; no-clobber; backups; line/pattern edits; inverses; platform filesystem rules |
| [batch CLI](../tests/batch_contract.rs) | Format and ordering equivalence; canonical aliases; sequential dependencies; no partial writes on validation failure; stdin/path rules; commit errors and recovery receipts |
| [patch CLI](../tests/patch_contract.rs) | Unified/apply_patch lifecycle; exact context; CRLF/EOF; quoted UTF-8 paths; explicit patch root; malformed hunk rejection; path confinement |
| [read CLI](../tests/read_contract.rs) | Selection/context; item/byte caps; zero unlimited; giant lines and UTF-8 boundaries; byte columns; input ordering; late IO errors |
| [read selection matrix](../tests/read_matrix_contract.rs) | Exhaustive small selections/context/EOF/UTF-8 cases; dense bounded reads and late IO failures |
| [CLI presentation](../tests/cli_contract.rs) | Discovery and exit categories; quiet/trace/color; compact/JSON/NDJSON; raw legacy previews; explicit truncation; executable documentation examples |
| [live MCP](../tests/transport_contract.rs) | Stdio initialization and tool discovery; default/full toolsets; input schema rejection; preview/check/structured receipts; direct batches; reader budgets; recovery |
| [updater subprocesses](../tests/update_contract.rs) | Nonblocking CLI, singleton workers, MCP leases, deferred/forced installation, detached IO and platform replacement |
| [POSIX installer](../tests/install_sh.py) | Local archive/checksum handling; pinned/latest selection; failed download/checksum preserves installation |
| [PowerShell installer](../tests/install_ps1.ps1) | Local archive/checksum handling; target/version normalization; failed requests and destination cleanup |
| [benchmark plumbing](../tests/native_benchmark_contract.py) | Adapter output/error separation, byte and EOF comparisons, cost-field interpretation |

Release contracts require Python 3.11+ and Git:

```sh
python3 tests/release_contract.py
python3 tests/publish_release_contract.py
```

The planner uses temporary Git histories to verify complete push ranges, path eligibility, version derivation and isolated Cargo stamping. The publisher uses a fake GitHub adapter to verify asset/checksum validation, tag conflicts, draft resume and publication order; it never contacts GitHub. Validate workflow syntax with actionlint when changing jobs or expressions.

Updater policy, delivery and coordinator contracts are described in [update testing](update-testing.md). Their authoritative cases are colocated under `src/update/`; inspect those tests when changing scheduling, routes or replacement rules.

## Oracles and boundaries

Core edits compare expected untouched byte slices rather than rebuilding content through the same line helpers being tested. Small move matrices use an independent sequence model and then check inverse restoration. Format tests run equivalent operations through real parsers. CLI tests execute the binary; MCP tests initialize a real stdio server and submit public requests.

Transaction fault injection uses a private test seam because reliably failing a particular commit step is otherwise platform-dependent. Assertions include unchanged targets and parent paths after failed validation, retained recovery state after commit failure, and refusal to overwrite external changes.

Unix-only permissions/symlink cases and Windows-only readonly/case-alias cases are gated for their platforms. A local pass validates only the cases compiled on that platform; CI runs Linux, macOS and Windows. Updater subprocess fixtures require a debug build because production ignores test release endpoints.

These contracts do not establish power-loss durability, cross-file atomic visibility, arbitrary external-writer isolation or full agent/model efficiency. The [architecture limits](architecture.md#persistence-and-limits) and benchmark methodology define those boundaries.
