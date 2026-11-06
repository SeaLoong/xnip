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

For DSH's real bridge tests, follow [the runtime/binary environment setup](dsh.md#verification) and run `python tests/dsh_adapter_contract.py` plus `node --test tests/dsh_integration_live.cjs`. Installed-package checks additionally use `XNIP_DSH_TEST_INSTALL` for the consumer prefix and `XNIP_PI_TEST_PACKAGE` for its installed `node_modules/xnip-cli`; DSH's actual host dependency packages are required. Then run `node --test tests/dsh_packaged_live.cjs tests/pi_contract.mjs`. Optional suites report a skip when their prepared runtime, real CLI or installed package is missing; the Ubuntu CI job prepares all of them and rejects skipped Node checks. Source acquisition and dependency installation are setup costs. These worker suites execute actual tools with model generation idle.

Native editor runs and fixture selection are documented in [benchmarks](benchmarks/README.md). Installer and ordinary test children disable automatic updates. Enabled updater contracts use copied binaries, isolated state and loopback release fixtures; they do not install into the user's normal xnip path.

## Native host E2E

Use Node 24, Python 3.13, a current Rust toolchain and pnpm 11.25.0 for the full
matrix. DSH's official
plugin manager invokes pnpm; keep that version on `PATH`. A local installation
and complete dependency preparation from the repository root are:

```sh
npm install --prefix target/native-tools --ignore-scripts --no-audit --no-fund --save-exact pnpm@11.25.0
export PATH="$PWD/target/native-tools/node_modules/.bin:$PATH"
cargo fetch --locked
cargo test --locked --test dsh_native_lifecycle_contract --no-run
node benches/native/dsh_native_setup.mjs
node benches/native/dsh_native_setup.mjs --check
node --input-type=module -e "import { prepare } from './benches/native/pi_adapter.mjs'; await prepare();"
node benches/native/gemini_native_setup.mjs
node benches/native/gemini_native_setup.mjs --check
node benches/native/cline_native_setup.mjs
node benches/native/cline_native_setup.mjs --check
python3 benches/native/openhands_setup.py
python3 benches/native/openhands_setup.py --check
node benches/native/opencode_native_setup.mjs
node benches/native/opencode_native_setup.mjs --check
python3 benches/native/aider_native_setup.py
python3 benches/native/aider_native_setup.py --check
python3 benches/native/python_agents_setup.py
python3 benches/native/python_agents_setup.py --check
python3 benches/native/agent_sdks_setup.py
python3 benches/native/agent_sdks_setup.py --check
python3 benches/native/smolagents_setup.py
python3 benches/native/smolagents_setup.py --check
python3 benches/native/google_adk_setup.py
python3 benches/native/google_adk_setup.py --check
python3 benches/native/crewai_setup.py
python3 benches/native/crewai_setup.py --check
python3 benches/native/host_profiles_setup.py
python3 benches/native/host_profiles_setup.py --check
python3 benches/native/goose_adapter.py --setup
python3 benches/native/goose_adapter.py --check
python3 tests/native_e2e.py
python3 tests/native_byte_contract.py
```

The runner uses `--offline` for its Cargo build and lifecycle execution; the
fetch and compilation above prepare those dependencies. Pi's setup verifies
its pinned source and locked SDK under `target/native-bench/pi-v1.3.0`.
Gemini core 0.27.2 and Cline SDK/core 0.2.0 use frozen npm dependencies under
`target/native-hosts/gemini-0.27.2` and `cline-0.2.0`. OpenHands SDK/tools 1.60.1
uses a local `.venv` under `target/native-hosts/openhands-1.60.1`, with pip's
hashed requirements lock. The full matrix selects that prepared interpreter.

Each Python dependency family has its own virtual environment. OpenHands
supports Python 3.12+; the pinned aider snapshot uses Python 3.13. The
`python-agents` group uses MCP 2.3, while OpenAI Agents/AutoGen and smolagents
use their separate MCP 1.30 environments. Preparation scripts install their
own frozen locks and record the interpreter and package hashes. Installation
of Python adapters for native IO includes local `integrations/native-python` and the
selected adapter; official MCP SDK packages use their own transport.
Google ADK has its own MCP 2.3 environment; CrewAI has its own MCP 1.30
environment. Each SDK keeps its dedicated lock and interpreter.
The client-profile setup prepares Codex, Claude Code and Copilot CLI plus the
Continue schema. Goose's official client lane uses its separately prepared
Rust worker; both its setup and check appear above.

The DSH setup installs locked local dependencies under
`target/native-hosts/dsh-0.2.1-rc.2` and `dsh-0.2.1-rc.5`, including
`dsh-rewind-plugin@0.19.2` for profiles that select rewind. `--version
0.2.1-rc.2` or `--version 0.2.1-rc.5` selects one DSH version for setup and E2E.
The setup's `--destination /absolute/directory` selects an isolated destination
with one `--version`; `native-host.json` records its checked dependency/input
hashes and Node/npm versions. Its preparation contracts run actual fresh npm
installs, original host tools, drift detection and restoration:

```sh
node --test-reporter=tap tests/dsh_native_setup.mjs
```

The historical `dsh_setup.mjs` builds the pinned benchmark worker runtime;
`dsh_native_setup.mjs` prepares the full npm hosts for profile-loader checks.
Each preparation uses its own cache root: historical workers under
`target/native-bench`, full native hosts under `target/native-hosts`.
The E2E runner's `--hosts-dir /absolute/host-parent` selects another parent
containing the versioned `dsh-<version>` directories.

The full matrix completes 369 checks across the [support catalog](support.md),
with zero failed or skipped cases:

| Runtime group | Checks | Tool and lifecycle boundary |
|---|---:|---|
| DSH rc.2 and rc.5 | 40 | Official provider/tools, approval/diff/rewind, profile composition and installation failure phases |
| Pi | 24 | SDK loader, original tool factories, hooks, packaged coexistence and builtin restoration |
| Gemini SDK | 24 | Local `Config`, original registry, per-invocation snapshots, approval and service restoration |
| Cline SDK | 24 | Official patch parsing/executor wrapper, actual agent policies/hooks and original-tool restoration |
| OpenHands SDK | 24 | Registered `file_editor`, matching/history, SDK events/hooks/confirmation and factory restoration |
| OpenCode | 25 | Actual CLI/plugin registration, four additional xnip tools, permissions, process lifetime and packaged loading |
| aider | 24 | Pinned Coder formats, authorization and original InputOutput restoration |
| DeepAgents / LangGraph / PydanticAI | 49 | Filesystem backend, real ToolNode and official MCPToolset |
| OpenAI Agents | 21 | Actual SDK discovery, FunctionTool invocation and stdio-session lifecycle |
| AutoGen | 21 | Official MCP workbench, agent tool registry and session lifecycle |
| smolagents | 22 | Actual ToolCallingAgent tool dispatch and owned MCP sessions |
| Google ADK | 20 | McpToolset, ToolContext, filters, confirmation and owned sessions |
| CrewAI | 18 | Official MCP resolver, agent BaseTools and fresh file-tool invocations |
| Client profiles | 33 | Fourteen configurations, isolated CLI inspection, configured MCP launches and official Goose client calls |

The runner pins one current xnip executable for every suite, probes native
capabilities including UTF-8, starts real processes and verifies actual test
counts with zero skipped cases. Registered tools execute in idle host contexts
with zero model generations. Shared fixtures retain each route's documented
matching, EOL and format policy. Completed test counts, selected groups and
executable identity remain in `target/native-e2e/results.json`, alongside
per-suite logs.

`--host` selects the groups listed by `--help`; repeat it for several groups.
`--version` narrows DSH versions. For example:

```sh
python3 tests/native_e2e.py --host dsh --version 0.2.1-rc.2
python3 tests/native_e2e.py --host pi
python3 tests/native_e2e.py --host gemini --host cline --host openhands
```

Focused DSH Node suites default to the rc.5 host. `XNIP_DSH_NATIVE_HOST` selects a
prepared host directory and `XNIP_TEST_BINARY` selects a built executable:

```sh
node tests/dsh_native_contract.mjs
node tests/dsh_native_bytes.mjs
node tests/dsh_native_rewind.mjs
node tests/dsh_native_packaged.mjs
cargo test --locked --test dsh_native_lifecycle_contract -- --ignored --nocapture
```

The real-host lifecycle test is opt-in during ordinary `cargo test`; the unified
runner executes it against each selected host. Transaction and installer fixture
tests remain part of the standard Rust workspace suite. See [native
adapters](native.md) for installation, restoration and protocol details.

Pi's [native extension](pi-native.md) suites also run individually:

```sh
node --test --test-concurrency=1 --test-reporter=tap tests/pi_native_contract.mjs tests/pi_native_bytes.mjs tests/pi_native_packaged.mjs
```

They check same-name schema/prompt/renderer equality, reload restoration, real
permission/result hooks, official matching and argument handling, cancellation,
mutation order, read/image presentation, symlinks and all 13 original byte
fixtures. The private-tarball check loads the default packaged extension and
bundled executable through the actual SDK and session lifecycle. It also
checks native/MCP tool selection, an independent MCP server and reload
restoration of the previous active tools. Model generation remains idle.

The [Gemini](gemini-native.md), [Cline](cline-native.md) and
[OpenHands](openhands-native.md) guides describe each SDK factory and its
focused suites. Ordinary SDK session execution retains approval and hooks;
the fixtures enter actual tool pipelines with model generation idle. Direct
service/executor calls use the authority of the embedding application.

The [CI harness job](../.github/workflows/ci.yml) prepares these dependencies,
executes the native host/SDK matrix, two fresh-install DSH preparation contracts
and provider benchmark contracts. [Scope selection](../.github/scripts/ci_scope.py)
includes native adapters, shared transport, suites and measured runners.

## Contract map

| Suite | Observable contracts |
|---|---|
| [core plan contracts](../tests/core_plan_contract.rs) | Nonwriting plans; exact before/after snapshots; absent versus empty; stale and governance rejection; cancellation with rollback; shared planning and recovery authorization |
| [native protocol](../tests/native_protocol_contract.rs) / [byte contracts](../tests/native_byte_contract.py) | Session plan consumption, explicit authority, request correlation, EOF completion; the original 13 scenarios over CLI, MCP and native transport |
| [native installer](../tests/native_installer_contract.rs) | Phase rollback, snapshots, user-change preservation, interrupted recovery, profile locking and restoration |
| [DSH native host preparation](../tests/dsh_native_setup.mjs) | Frozen full-host dependency installs; exact versions and hashes; actual original tool execution; drift detection and repeatable restoration |
| [DSH provider](../tests/dsh_native_contract.mjs) / [rewind](../tests/dsh_native_rewind.mjs) | Official filesystem tools, observations, sandbox, stale versions, approval audit, diff projection and compatible rewind plugin |
| [native E2E matrix](../tests/native_e2e.py) | 369 checks across catalog backends, plugins, framework SDKs and client profiles; one pinned executable and capability probe; actual registries, processes, file results, restoration and lifecycle phases; zero skipped cases and model generations |
| [E2E result gates](../tests/native_e2e_contract.py) | Ten stdlib contracts require complete TAP/unittest summaries and exact counts; exercise CRLF, duplicate/truncated summaries, failures, skips/cancellations/TODOs, application `OK` diagnostics and unittest expected-failure/unexpected-success outcomes; counted separately from SDK E2E |
| [Pi native extension](../tests/pi_native_contract.mjs) / [byte fixtures](../tests/pi_native_bytes.mjs) / [installed package](../tests/pi_native_packaged.mjs) | Actual official loader/session/runner and default private-tarball entry; same-name host tools; hooks, matching, queue, cancellation, rendering, binary reads, original byte scenarios and reload restoration |
| [Gemini SDK](../tests/gemini_native_contract.mjs) / [byte fixtures](../tests/gemini_native_bytes.mjs) / [installed package](../tests/gemini_native_packaged.mjs) | Original `Config` and tools, per-tool snapshots/concurrency, approval, host byte policy, local npm entry and service restoration |
| [Cline SDK](../tests/cline_native_contract.mjs) / [byte fixtures](../tests/cline_native_bytes.mjs) / [installed package](../tests/cline_native_packaged.mjs) | Official patch parsing and wrappers; actual agent policies/hooks/events, batch guards, private-package entry and original-tool restoration |
| [OpenHands SDK](../tests/openhands_native_contract.py) | Public registered definition and executor; original fixtures/matching/history, SDK events/hooks/confirmation, factory isolation and process closure |
| [OpenCode](../tests/opencode_native_contract.mjs) / [byte fixtures](../tests/opencode_native_bytes.mjs) / [installed package](../tests/opencode_native_packaged.mjs) | Actual CLI plugin loading, additional xnip tools, permission checks, native cancellation, byte goals and builtin/MCP restoration |
| [aider](../tests/aider_native_contract.py) / [Python frameworks](../tests/python_agents_contract.py) | Original Coder IO formats, DeepAgents backends, registered LangGraph tools and PydanticAI MCPToolset; raw guards, host newline policy, real local package loading and restoration |
| [OpenAI Agents](../tests/agents_sdk_contract.py) / [AutoGen](../tests/autogen_contract.py) / [smolagents](../tests/smolagents_contract.py) / [Google ADK](../tests/google_adk_contract.py) / [CrewAI](../tests/crewai_contract.py) | Official MCP SDK discovery and invocation, public tool metadata, specific remote errors, lifecycle and model-call counters |
| [Client profiles](../tests/host_profiles_contract.py) / [catalog](../tests/integration_catalog_contract.py) | Fourteen configuration artifacts, actual CLI readers, stdio tool calls, official Goose client, catalog guides and packaged entry consistency |
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
| [DSH provider benchmark](../tests/native_provider_bench_contract.mjs) / [Pi provider benchmark](../tests/pi_provider_bench_contract.mjs) | Actual host tools and persistent native processes; sample-derived byte/outcome checks, tool arguments/results, timing boundaries and report validation |
| [Gemini provider benchmark](../tests/gemini_provider_bench_contract.mjs) / [Cline provider benchmark](../tests/cline_provider_bench_contract.mjs) | Original SDK tool dispatch and governance; actual results versus independent storage goals, extended matching/patch cases, zero model calls and report validation |
| [OpenCode benchmark](../tests/opencode_provider_bench_contract.mjs) / [OpenHands benchmark](../tests/openhands_provider_bench_contract.py) / [aider benchmark](../tests/aider_provider_bench_contract.py) / [Python framework benchmark](../tests/python_agents_bench_contract.py) | Real tool pipelines, complete file goals, host-policy compatibility, captured argument/result counts and timing scopes |
| [MCP SDK benchmarks](../tests/agent_sdk_bench_contract.py) / [ADK benchmark](../tests/google_adk_bench_contract.py) / [CrewAI benchmark](../tests/crewai_provider_bench_contract.py) / [profile benchmark](../tests/host_profiles_bench_contract.py) | Actual SDK/client calls and direct-MCP reference, specific remote-error gates, public projections, process lifecycle and report integrity |

Release contracts require Python 3.11+ and Git:

```sh
python3 tests/release_contract.py
python3 tests/publish_release_contract.py
```

The planner uses temporary Git histories to verify complete push ranges, path
eligibility, version derivation and isolated Cargo stamping. npm preparation
inputs and bundled integrations qualify as release inputs alongside the seven
`package.sh` integration archives. Source-only adapters and benchmark fixtures
retain their separate scope. Scope contracts select npm package and real-harness
checks for the corresponding JavaScript/Python routes. The CI reuse contract
requires the expanded full-verification marker. The publisher uses a fake
GitHub adapter to verify asset/checksum validation, tag conflicts, draft resume
and publication order; it never contacts GitHub. Validate workflow syntax with
actionlint when changing jobs or expressions.

Updater policy, delivery and coordinator contracts are described in [update testing](update-testing.md). Their authoritative cases are colocated under `src/update/`; inspect those tests when changing scheduling, routes or replacement rules.

## Oracles and boundaries

Core edits compare expected untouched byte slices rather than rebuilding content through the same line helpers being tested. Small move matrices use an independent sequence model and then check inverse restoration. Format tests run equivalent operations through real parsers, including compact whole writes, ordered pairs and native quoted multiline payloads. Whole-file guards distinguish absence from existing empty bytes. Literal insertion compares independently specified before/after output, including CRLF and unterminated EOF, and rejects empty/ambiguous anchors and incompatible flags. CLI tests execute the binary; MCP tests initialize a real stdio server and submit public requests.

Transaction fault injection uses a private test seam because reliably failing a particular commit step is otherwise platform-dependent. Assertions include unchanged targets and parent paths after failed validation, retained recovery state after commit failure, and refusal to overwrite external changes.

Unix-only permissions/symlink cases and Windows-only readonly/case-alias cases are gated for their platforms. A local pass validates only the cases compiled on that platform; CI runs Linux, macOS and Windows. Updater subprocess fixtures require a debug build because production ignores test release endpoints.

These contracts do not establish power-loss durability, cross-file atomic visibility, arbitrary external-writer isolation or full agent/model efficiency. The [architecture limits](architecture.md#persistence-and-limits) and benchmark methodology define those boundaries.
