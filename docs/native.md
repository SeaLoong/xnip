# Native host adapters

Provider and IO adapters reuse existing host tools or edit formats while
calling the xnip editing engine. Plugin and framework tool adapters register
their explicit `xnip_*` surfaces. Native commits call
`core::transaction::Plan` in the single `xnip` Rust crate. The
[support matrix](support.md) lists every backend, SDK toolset and client profile;
CLI, MCP and skills remain available.

| Host | Native adapter | Verified versions |
|---|---|---|
| DSH | L3 filesystem provider; transactional profile installer below | `0.2.1-rc.2`, `0.2.1-rc.5` |
| Pi | L2′ native tool override; explicit [extension setup](pi-native.md) | `1.3.0` |
| Gemini SDK | Local `Config` filesystem service; [per-tool execution scope](gemini-native.md) | `@google/gemini-cli-core` `0.27.2` |
| Cline SDK | `apply_patch` executor with official parsing and tools; [SDK setup](cline-native.md) | `@cline/sdk` `0.2.0` |
| OpenHands SDK | `file_editor` executor with official matching/history; [Python package](openhands-native.md) | SDK/tools `1.60.1` |
| aider | Eight official Coder formats through the [IO adapter](aider-native.md) | Source `5dc9490`, Python 3.13 |
| DeepAgents | FilesystemMiddleware [file backend](deepagents.md) | `0.7.34` |

The additional-tool routes expose their own xnip schemas:

| Host / framework | Registered tools | Setup |
|---|---|---|
| OpenCode `1.19.3` | `xnip_edit`, `xnip_write`, `xnip_apply`, `xnip_read` | [OpenCode plugin API](opencode.md) |
| LangGraph `1.2.20` | `xnip_read`, `xnip_edit`, `xnip_write` | [StructuredTools and ToolNode](langgraph.md) |

OpenCode's builtin editors retain their original formatter and LSP behavior
beside the plugin tools. MCP SDKs and client profiles use the
[MCP integration routes](support.md#official-mcp-sdk-toolsets).

Pi uses official `registerTool` same-name replacement and the original tool
factories' `operations` IO extension points. Its registered `read`, `edit` and
`write` tools keep host matching, hooks, presentation and mutation ordering;
extension removal plus session reload restores the builtins. It uses the same
OS authority as Pi's builtin tools.

The SDK adapters attach when the application constructs its host session.
Gemini uses a local `Config` and a per-tool execution scope; Cline selects the
official `apply_patch` tool route; OpenHands selects an SDK factory that retains
the model-facing `file_editor` definition. Each guide includes the application
setup and restoration lifecycle. The profile installer below applies to DSH.

## DSH profile setup

The local [`integrations/dsh-native`](../integrations/dsh-native/) package keeps
DSH's `read`, `write` and `edit` tools. It delegates writes to `xnip native serve`
and retains host path resolution, observations, sandbox, approval, diff and
rewind services. The same provider has been verified with official DSH
`0.2.1-rc.2` and `0.2.1-rc.5`; profiles that select rewind use
`dsh-rewind-plugin@0.19.2`.

Use Node 24 and pnpm 11.25.0 on `PATH` for DSH's official plugin manager;
[dependency setup](testing.md#native-host-e2e) includes a local pnpm installation.
Build xnip and select an existing DSH profile directory and installed DSH
executable:

```sh
cargo build --locked
target/debug/xnip native install --host dsh \
  --profile /home/me/.dsh/profiles/dev \
  --plugin /work/xnip/integrations/dsh-native --dsh /path/to/dsh
target/debug/xnip native verify --host dsh \
  --profile /home/me/.dsh/profiles/dev \
  --plugin /work/xnip/integrations/dsh-native --dsh /path/to/dsh
```

On Windows, use `target/debug/xnip.exe`. `--profile` takes the directory itself;
DSH's profile-name syntax belongs to the DSH CLI. State defaults to
`PROFILE/.xnip-native`; `--state-dir` chooses another location bound to this
profile. `--node` selects the lifecycle driver's Node executable.

Installation stages a local package with the current xnip executable's absolute
path in package-owned configuration. Its bundle selects one `xnip-native-fs`
provider, disables `fs-sandbox` and an existing `xnip-mcp` entry, and preserves
the profile user's patch. Verification starts the official host composition,
checks its tool registry and performs a real edit. Restart the selected
profile's running sessions to load the new provider.

The standalone [DSH MCP integration](dsh.md) remains available as its own
registration. Removing the native bundle restores the earlier profile
composition, including an MCP bridge that was previously selected.

## Restoration and retained state

Use the same host arguments for removal and restoration verification:

```sh
xnip native uninstall --host dsh --profile /home/me/.dsh/profiles/dev \
  --plugin /work/xnip/integrations/dsh-native --dsh /path/to/dsh
xnip native verify --restored --host dsh --profile /home/me/.dsh/profiles/dev \
  --plugin /work/xnip/integrations/dsh-native --dsh /path/to/dsh
xnip native doctor --state-dir /home/me/.dsh/profiles/dev/.xnip-native
xnip native clean --state-dir /home/me/.dsh/profiles/dev/.xnip-native --purge-user-data
```

Restart running sessions after removal as well. `native recover` takes the
same host arguments and restores an interrupted installation or removal.
Known baseline, installed and completed-removal states resume automatically.
Third-party changes remain available for reconciliation against the retained
snapshots.

The installer snapshots declared configuration and dependency paths, hashes
stored blobs, observes user configuration and locks both the profile and its
state directory. Failed phases restore the baseline; interrupted operations
retain a recovery manifest. Post-installation user changes receive a conflict
report with retained snapshots. Successful restoration keeps those snapshots
until `native clean --purge-user-data` removes this installation's `blobs`,
`stage` and manifest after checking the restored baseline.

Editing journals and `.bak` files keep their own [recovery lifecycle](recovery.md).
The native cleaner's scope is the selected installation state. Use
`xnip doctor --native --state-dir ...` to combine ordinary xnip diagnostics with
that profile's retained verification evidence.

When a successful commit returns a retained recovery journal, the shared
JavaScript and Python clients log its path for the operator. SDK tool results
keep their host presentation. The [recovery guide](recovery.md) describes
inspection of that retained evidence.

## DSH behavior and shared limits

The DSH provider uses the host's per-target lock and observed version, then
rechecks storage during core commit. It returns the host's exact
`dev:ino:size:mtimeNs:ctimeNs` version from the committed file. Tool-owned
observation, approval and rewind events continue through their existing host
services. The lifecycle verifier exercises rewind when the selected profile
enables it.

Storage preserves BOM, CRLF and EOF bytes. Host-normalized multiline matches are
translated to guarded raw byte spans. Global replacements spanning mixed newline
spellings use a guarded whole-file write. Diff presentation follows the host's
normalization rules.

Each serialized operation request has a 32 MiB limit. DSH's small edits on larger
files send the changed spans; whole-file operations include their full payload
in that request. Plan snapshots and file loading use memory proportional to
the affected files. Authority, durability and concurrent-writer boundaries are
described in [architecture](architecture.md#persistence-and-limits).

## Rust and child-process APIs

Rust hosts call `xnip::core::transaction::Plan` directly. Plans expose
`snapshots()`, `preview()`, validation and commit. `apply::commit::prepare`
constructs the same plan from parsed batches while preserving their ordering
rules. Before/after snapshots distinguish an absent file from empty bytes.

`Plan::commit_controlled` lets a host perform approval, sandbox, version and undo
snapshot checks around the existing transaction. Its cancellation token is
checked at persistence checkpoints; failures use the shared rollback and
recovery-journal implementation. Host schemas and result translation belong to
the adapter. CLI and MCP retain their existing argument, output and error
contracts.

`xnip native serve` supplies a persistent JSON-lines frontend for subprocess
hosts. Requests contain `id`, `protocol: 1` and an `action`. Responses preserve
the ID and contain `result` or `error: {code, message}`. `capabilities` reports
the API version, supported operations and resource limits. The session holds
up to 64 pending plans; each plan is committed or aborted within its creating
process.

```json
{"id":1,"protocol":1,"action":"capabilities"}
{"id":2,"protocol":1,"action":"plan","operations":[{"op":"exact_replace","path":"/work/file.txt","old":[111,108,100],"text":[110,101,119]}]}
{"id":3,"protocol":1,"action":"commit","plan_id":1,"policy":{"read_only":false,"writable_roots":["/work"]}}
```

Operations include exact replacement and insertion, whole writes,
create/delete/rename, line replacement/insertion, move, indent and text patches.
Other actions provide bounded byte reads, abort/cancellation and authorized
journal recovery.

| Content field or mode | Wire behavior |
|---|---|
| Byte-valued operation fields such as `old`, `text`, `anchor`, `was` | Accept byte arrays or UTF-8 strings; strings encode directly to bytes |
| `read` / `plan` `content_encoding: "bytes"` (default) | Reads return `bytes`; before/after snapshots contain byte arrays |
| `read` / `plan` `content_encoding: "utf8"` | Valid UTF-8 returns `text` for reads and strings in snapshots; binary content uses byte arrays |
| `commit` content | Retains the creating plan's encoding; absent snapshots remain `null` |

The host supplies an explicit commit policy after its approval checks.
`writable_roots` selects write authority: a path list confines it to those
roots, `[]` permits zero write targets, and `null` requests the process's full
filesystem authority. OS process permissions apply in every case.

## Verification

Follow [native host E2E setup](testing.md#native-host-e2e), then run:

```sh
python3 tests/native_e2e.py
python3 tests/native_byte_contract.py
```

The unified E2E entry point pins one xnip executable, probes native
capabilities, starts real tool processes and checks registries, schemas and
file results. Each suite follows its provider, plugin, SDK or MCP lifecycle.
The [test matrix](testing.md#native-host-e2e) and [support catalog](support.md)
record each route's verified versions, counts and execution boundary. Idle
host contexts record zero model generations.

Per-suite logs, executable identity and completed counts remain under
`target/native-e2e`. Repeat `--host` to select routes listed by the runner's
`--help`. `--version` selects DSH versions; `--hosts-dir` selects the parent of
the prepared DSH directories:

```sh
python3 tests/native_e2e.py --host dsh --version 0.2.1-rc.2
python3 tests/native_e2e.py --host pi
python3 tests/native_e2e.py --host gemini --host cline --host openhands
```

The byte-contract suite uses CLI, real MCP and native JSONL on the same
independently specified fixtures. Rust tests separately cover transaction
control, protocol state, installer rollback, corruption, drift and locking.
See the [contract map](testing.md#contract-map) for focused suite entry points.
