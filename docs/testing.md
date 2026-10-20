# Testing xnip

Tests cover observable bytes, commit state, rejected writes, output budgets and protocol behavior. Matrices share independently specified expected results across input formats and transports. Test-function counts are not coverage percentages.

## Run the checks

```sh
# Keep dev binaries small for npm payload checks; assertions remain enabled.
CARGO_PROFILE_DEV_DEBUG=0 cargo test --workspace --locked
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo +1.89.0 check --workspace --all-targets --locked
cargo deny check
cargo bench --no-run --locked
cargo doc --workspace --no-deps --locked
cargo package --locked
cargo run --locked -p xtask -- sync-integrations
```

Use `RUSTDOCFLAGS=-D warnings` for documentation checks in CI. Integration generation must leave committed outputs unchanged. See [contributing](../CONTRIBUTING.md) for tool prerequisites and packaging from a dirty working tree, and [CI scopes](ci.md) for required checks. `cargo test` includes doctests; performance measurements run on demand.

The private npm package has real local packaging, installation and process contracts. With Node 24.13.0, Python 3.13 and the debug CLI already built:

```sh
python tests/npm_prepare_contract.py
python tests/npm_contract.py
node --test tests/dsh_integration_contract.cjs
```

The preparation-input suite rejects malformed archives, wrong compiled version/target markers and invalid architecture/static-link labels, without executing foreign fixtures. The npm process suite packs private tarballs, installs the bundled host payload with lifecycle scripts disabled and the registry offline, then exercises the actual CLI/MCP process. It uses isolated installation, npm cache and executable cache directories under `target`; it never publishes packages or edits a host profile. The DSH configuration suite is independent of the native editor and does not simulate its matching or filesystem operations.

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

The pinned [DSH](dsh.md), [Pi](pi.md) and [Goose](goose.md) suites also execute real native editors and real MCP bridges without a model. Prepare their isolated dependencies explicitly before testing:

```sh
node benches/native/dsh_setup.mjs
node benches/native/dsh_setup.mjs --check
python benches/native/goose_adapter.py --setup
python benches/native/goose_adapter.py --check
node --test tests/pi_contract.mjs
python tests/goose_native_contract.py
```

For DSH's real bridge tests, follow [the runtime/binary environment setup](dsh.md#verification) and run `python tests/dsh_adapter_contract.py` plus `node --test tests/dsh_integration_live.cjs`. Installed-package checks additionally use `XNIP_DSH_TEST_INSTALL` for the consumer prefix and `XNIP_PI_TEST_PACKAGE` for its installed `node_modules/xnip`; DSH's actual host dependency packages are required. Then run `node --test tests/dsh_packaged_live.cjs tests/pi_contract.mjs`. Optional suites report a skip when their prepared runtime, real CLI or installed package is missing; the Ubuntu CI job prepares all of them and rejects skipped Node checks. Source acquisition and dependency installation are setup costs, and no suite runs a provider or model session.

Native editor runs and fixture selection are documented in [benchmarks](benchmarks/README.md). Installer and ordinary test children disable automatic updates. Enabled updater contracts use copied binaries, isolated state and loopback release fixtures; they do not install into the user's normal xnip path.

## Contract map

| Suite | Observable contracts |
|---|---|
| [core contracts](../src/core/contract_tests.rs) | Inclusive byte ranges; LF/CRLF/unterminated EOF/Unicode/invalid UTF-8; locators and empty-anchor rejection; literal/regex replacement; insert/move/indent; diff; content/revert; atomic staging, backups and permissions; shared bounded reads and writer failures |
| [apply contracts](../src/apply/contract_tests.rs) | Native/JSON/YAML equivalence; malformed/unknown fields; detection; serial/parallel planning; stdin and external payload resolution; lifecycle coalescing; overlapping-plan rejection; revert and patch planning |
| [compact input contracts](../src/apply/compact.rs) | File-map shapes; ordered exact pairs; whole writes and guards; empty groups, duplicate keys, unknown fields and normalized alias rejection |
| [transaction contracts](../src/core/transaction.rs) | Fault injection at commit steps; rollback; interrupted journals; integrity verification; repeat recovery; conflict refusal at existing and newly created paths |
| [edit CLI](../tests/edit_contract.rs) | Literal replacement and before/after insertion; anchor ambiguity and byte preservation; complete-file write and whole-content guards; missing versus empty; symlink refusal; preview/check/no-op; create/delete/rename; backups; line/pattern edits and inverses |
| [batch CLI](../tests/batch_contract.rs) | Native/JSON/YAML ordering; canonical aliases; sequential dependencies; validation-before-write; stdin/path rules; commit errors and recovery receipts |
| [compact CLI](../tests/compact_contract.rs) | Ordered file maps; whole writes and guards; malformed/aliased groups; raw multiline and bare CR literals; CRLF/EOF; late validation leaves files unchanged |
| [patch CLI](../tests/patch_contract.rs) | Unified/apply_patch lifecycle; exact context; CRLF/EOF; quoted UTF-8 paths; explicit patch root; malformed hunk rejection; path confinement |
| [read CLI](../tests/read_contract.rs) | Selection/context; item/byte caps; zero unlimited; giant lines and UTF-8 boundaries; byte columns; input ordering; late IO errors |
| [read selection matrix](../tests/read_matrix_contract.rs) | Exhaustive small selections/context/EOF/UTF-8 cases; dense bounded reads and late IO failures |
| [CLI presentation](../tests/cli_contract.rs) | Discovery and exit categories; quiet/trace/color; compact/JSON/NDJSON; raw legacy previews; explicit truncation; executable documentation examples |
| [live MCP](../tests/transport_contract.rs) | Stdio discovery; default/full toolsets; strict argument and mixed-input rejection; compact file groups and whole writes; exact insertion; default structured and opt-in text receipts; schema constraints and false-default omission; optional success flags; complete parse/regex errors; metadata omission and truthful modes; preview/check/no-op; reads and recovery |
| [MCP lifecycle compatibility](../tests/mcp_discovery_contract.rs) | Typed SDK discovery with 2026 per-request metadata; legacy 2025 initialization; tools/list, stale rejection and retry, unknown-method errors followed by successful edits, text receipts, Unicode paths and EOF shutdown |
| [npm archive/build identity](../tests/npm_prepare_contract.py) | Foreign binary version/target markers; old-version refusal before output creation; architecture/static linking; tar/ZIP inventory, traversal and link rejection |
| [private npm distribution](../tests/npm_contract.py) | Local tarball preparation and offline installation; compiled identity and checksum validation; host-only payloads; byte/exit-code fidelity; concurrent immutable cache publication; tamper refusal; mapped-process lifetime during uninstall; abrupt wrapper death and native input EOF |
| [DSH integration configuration](../tests/dsh_integration_contract.cjs) | Explicit Cordis entry; native resolution; cwd, environment and receipt options; invalid options and official bridge loading failures |
| [DSH native and bridge](../tests/dsh_adapter_contract.py) / [bundle loader](../tests/dsh_integration_live.cjs) / [installed package](../tests/dsh_packaged_live.cjs) | Official ToolRuntime and filesystem executors; observation/stale/read-window policy; literal and str editor behavior; raw values versus model projection; real package patch resolution and xnip stdio |
| [Pi native and bridge](../tests/pi_contract.mjs) | Official argument preparation and mutation queue; original-coordinate multi-edit rejection; fuzzy/EOL/BOM behavior; writes; extension registration; real MCP discovery, projection and disposal |
| [Goose native and bridge](../tests/goose_native_contract.py) | Source integrity; original EditTools matching and IO; edit/write schemas; ambiguity and stale refusal; isolated cwd; complete official GooseClient discovery and legacy stdio profiles |
| [updater subprocesses](../tests/update_contract.rs) | Nonblocking CLI, singleton workers, MCP leases, deferred/forced installation, detached IO and platform replacement |
| [POSIX installer](../tests/install_sh.py) | Local archive/checksum handling; pinned/latest selection; failed download/checksum preserves installation |
| [PowerShell installer](../tests/install_ps1.ps1) | Local archive/checksum handling; target/version normalization; failed requests and destination cleanup |
| [benchmark plumbing](../tests/native_benchmark_contract.py) | Adapter output/error separation; byte/EOF comparisons; snapshot-only unique-context trimming and line candidates; original-guard probe preservation; native/CLI/MCP selection symmetry; native literal manifests and record EOF; bounded counter caching with unchanged per-call totals; permission versus actual-precondition metadata; independent cost layers |

Release contracts require Python 3.11+ and Git:

```sh
python3 tests/release_contract.py
python3 tests/publish_release_contract.py
```

The planner uses temporary Git histories to verify complete push ranges, path eligibility, version derivation and isolated Cargo stamping. Private npm preparation and the DSH/Pi/Goose entries are ineligible for native release publication because they have no corresponding GitHub asset. A contract checks integration eligibility against the actual `package.sh` archive inventory. Scope contracts independently select npm package checks and native harness checks, so a changed JavaScript/Python adapter is not mistaken for documentation. The CI reuse contract requires the expanded full-verification marker; an older marker cannot claim the new npm/harness checks. The publisher uses a fake GitHub adapter to verify asset/checksum validation, tag conflicts, draft resume and publication order; it never contacts GitHub. Validate workflow syntax with actionlint when changing jobs or expressions.

Updater policy, delivery and coordinator contracts are described in [update testing](update-testing.md). Their authoritative cases are colocated under `src/update/`; inspect those tests when changing scheduling, routes or replacement rules.

## Oracles and boundaries

Core edits compare expected untouched byte slices rather than rebuilding content through the same line helpers being tested. Small move matrices use an independent sequence model and then check inverse restoration. Format tests run equivalent operations through real parsers, including compact whole writes, ordered pairs and native quoted multiline payloads. Whole-file guards distinguish absence from existing empty bytes. Literal insertion compares independently specified before/after output, including CRLF and unterminated EOF, and rejects empty/ambiguous anchors and incompatible flags. CLI tests execute the binary; MCP tests initialize a real stdio server and submit public requests.

Transaction fault injection uses a private test seam because reliably failing a particular commit step is otherwise platform-dependent. Assertions include unchanged targets and parent paths after failed validation, retained recovery state after commit failure, and refusal to overwrite external changes.

Unix-only permissions/symlink cases and Windows-only readonly/case-alias cases are gated for their platforms. A local pass validates only the cases compiled on that platform; CI runs Linux, macOS and Windows. Updater subprocess fixtures require a debug build because production ignores test release endpoints.

These contracts do not establish power-loss durability, cross-file atomic visibility, arbitrary external-writer isolation or full agent/model efficiency. The [architecture limits](architecture.md#persistence-and-limits) and benchmark methodology define those boundaries.
