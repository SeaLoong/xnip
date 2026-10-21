[中文](README.zh.md) | English

# xnip

**Precise text edits with less repeated context.** Give your coding agent a known old block and its replacement. xnip edits that block, preserves untouched bytes, and returns a compact result.

- **Reuse what the agent already knows.** Read missing context when needed; a known edit goes straight to the editor.
- **Choose the smallest useful edit.** Exact blocks avoid unchanged content; file groups share paths, anchor insertions avoid repeating the anchor, and explicit whole-file writes suit small complete rewrites.
- **Keep results small.** Short receipts, bounded reads/previews and five default MCP tools keep routine context compact.
- **Keep the edit precise.** Missing or ambiguous blocks fail before writing; an explicit `all` enables global replacement. Exact edits preserve CRLF and final-newline bytes.
- **Use one editor across harnesses.** CLI and stdio MCP share file lifecycle, batch, patch, line and anchor operations.

## Measured editing costs

The benchmark executes real **Codex, Gemini CLI, OpenCode, aider, DSH, Pi and Goose editors** against the 0.5.5 source build: **13 scenarios × 41 editor profiles × 3 repetitions**, plus six separate host→xnip MCP profiles. Every eligible profile produces the expected file bytes in every repetition. Counts use `tiktoken 0.14.0 / o200k_base`.

![Best observed byte-exact editing input](docs/benchmarks/charts/readme-frontier.svg)

Each task compares the **best observed native input / best observed xnip input**; above **1x** means less xnip input. Geometric means give every task equal weight. Guarded pools retain old-content checks; snapshot-enabled pools allow both guarded edits and whole-file writes on **both** sides.

| xnip interface / allowed operations | Native best / xnip best | xnip shorter / tied / native shorter |
|---|---:|---:|
| MCP · guarded | 1.23x | 13 / 0 / 0 |
| MCP · snapshot-enabled | 1.26x | 12 / 1 / 0 |
| CLI stdin · guarded | 1.55x | 13 / 0 / 0 |
| CLI stdin · snapshot-enabled | 1.80x | 13 / 0 / 0 |

**Why use it:** with complete file context, a substring change in the long-line fixture takes **11 MCP input tokens**, versus **18** for the best observed native input (Goose), or **10,037** for Codex's unique-context patch. File groups also avoid repeated paths and calls. The CLI stdin strategies have smaller input in all 13 measured tasks, in both pools; its invocation arguments are an additional cost recorded separately.

**Choose the representation before sending it:** when complete current content is already known and old-content guards are optional, native line manifests and shorter unique literal pairs can beat a whole-file listing. The multiline function takes **25** xnip MCP input tokens versus **26** for aider whole; the same-file batch takes **18** versus **20**. Required old-content checks remain intact. In the snapshot-enabled MCP pool, xnip is smaller in 12 tasks and tied in one.

**Smaller replies:** `xnip mcp --receipts text` reduces full captured JSON-RPC reply tokens by about **56%** across 26 matched comparisons. Inputs, text receipts and final file bytes are identical to the default structured mode. The same five-tool discovery catalog is **23.8% smaller** (2,303 to 1,754 tokens).

**New harness findings:** Goose's short argument shape wins most native guarded input comparisons; Pi batches disjoint edits against the original file. DSH's observed profiles include the real read/view required before editing. Its bare and observed policy groups have separate comparisons and stay outside the pooled frontier above; xnip's external MCP process does not inherit DSH's filesystem policy. [Host exchange evidence](docs/benchmarks/host-exchange-results.json) records actual public names, schema projections, instructions and returned content separately. Goose's provider projection remains unknown.

Native relative-path, tighter-context and equivalent known-file literal-trimming profiles are included. Only byte-exact outcomes qualify for the frontier; all original profiles and incompatible results remain visible. Aider's final returned text is unknown. CLI invocation, serialized MCP request/reply, tool text and startup costs are separate measurements; this is an editor benchmark, not billed or complete agent usage.

[Full results and per-task charts](docs/benchmarks/results.md) · [Raw evidence](docs/benchmarks/results.json) · [Method and reproduction](docs/benchmarks/README.md) · [Core performance](docs/benchmarks/core-performance.md)

## Install

### npm (preferred)

Node **22.14+** is required; Rust is unnecessary for installing a prepared package.
After the `xnip-cli` release is published, install it with:

~~~sh
npm install --global xnip-cli
xnip --version
~~~

Update with `npm install --global xnip-cli@latest`; npm-managed processes disable
native self-update. Restart MCP sessions after upgrading. DSH/Pi activation
remains explicit.

For unpublished development builds or offline verification, install a matching
private host tarball using
`npm install --global --offline --ignore-scripts ./xnip-cli-0.5.5.tgz`.
See the [npm guide](npm/README.md) for platform packages, local preparation and
the CI publication path.

### Standalone binaries and source

Release **0.5.5** includes DSH/Pi/Goose integration and npm distribution.
See the [changelog](CHANGELOG.md). Without Node, use either SHA256-verifying installer:

~~~sh
# macOS / Linux
curl -fsSL https://github.com/SeaLoong/xnip/releases/latest/download/install.sh | sh
~~~

~~~powershell
# Windows
Invoke-RestMethod https://github.com/SeaLoong/xnip/releases/latest/download/install.ps1 | Invoke-Expression
~~~

Or download a binary from [GitHub Releases](https://github.com/SeaLoong/xnip/releases).
See [installation](docs/installation.md) for targets, PATH, pinned versions,
checksums and removal. Source installation needs **Rust 1.89+**:

~~~sh
cargo install --git https://github.com/SeaLoong/xnip --tag v0.5.5 --locked xnip
~~~

## Edit

~~~sh
xnip edit config.txt --old 'PORT=8080' --text 'PORT=3000'
xnip edit generated.txt --write --text-file content.txt
xnip edit imports.py --old 'import os' --text-file new-import.txt --position after
xnip edit src/new.txt --create --text-file content.txt
xnip edit old-name.txt --rename-to new-name.txt
xnip edit obsolete.txt --delete
xnip apply edits.json --dry-run
xnip apply edits.json
~~~

Literal replacement requires a unique nonempty old block. Extend it with surrounding context when ambiguous; use `--all` for an intentional global change. `--dry-run` previews and `--check` validates; both leave files unchanged. For portable multiline content, use payload files or stdin.

`--write` explicitly creates or replaces a complete file; optional `--old` checks the entire existing content. Literal `--position before|after` inserts exactly the supplied bytes beside a unique anchor. Independent edits can share paths in one compact JSON manifest:

~~~json
{"a.txt":[["old","new"],["next","final"]],"b.txt":{"old":"was","text":"now"}}
~~~

Known line ranges, anchors, moves and indentation have dedicated commands. Use `peek` or `find` to fill gaps in known context; repository discovery and semantic analysis use the harness's search/language tools. Start with the [CLI tutorial](docs/cli.md), [edit reference](docs/edit.md) or [examples](docs/examples.md).

Reads default to 200 lines / 100 search hits / 12,000 payload bytes; previews default to 12,000 bytes. Truncation is explicit; zero disables the corresponding limit. `edit/apply/recover` return compact receipts; `--output json` provides complete machine results and `--output human` expands status and previews.

## Agent integration

Use the CLI already on PATH, or configure `xnip mcp` for structured arguments. MCP advertises `edit/apply/peek/find/recover` by default; `--toolset full` adds specialized line tools and diagnostics. `xnip mcp --receipts text` omits structured change metadata and output schemas for smaller agent responses; the default keeps machine-readable results. Several known blocks fit one `xnip_apply({files:{"a.txt":[["old","new"],["next","final"]]}})` call. Direct `edits` arrays remain available for explicit global replacements. Client setup and result fields are in [MCP](docs/mcp.md).

| Client | Guidance | Placement |
|---|---|---|
| Codex | [instructions and guide](docs/codex.md) | Merge AGENTS section; guide under `~/.codex/guides/xnip-editing/` |
| Claude Code | [skill](integrations/claude-code/SKILL.md) | `.claude/skills/xnip/`, including references |
| Cursor | [rule](integrations/cursor/xnip.mdc) | Project rule, `alwaysApply: false` |
| Copilot | [instruction](integrations/copilot/xnip.md) | Project instructions |
| Aider | [convention](integrations/aider/CONVENTIONS.md) | Project conventions |
| DSH | [bundle and MCP plugin](docs/dsh.md) | Explicitly activate the local package bundle; official MCP bridge |
| Pi | [extension](docs/pi.md) | Local package entry registers an MCP server per session |
| Goose | [stdio extension](docs/goose.md) | Merge extension config or use `--with-extension`; xnip 0.5.5+ |
| Other agents | [generic skill](integrations/generic/SKILL.md) | Load for editing; include references |

The [npm distribution](npm/README.md) packages the same native executable and includes both `xnip-cli/dsh` and `xnip-cli/pi`; a separate `dsh-xnip` package is unnecessary for this MCP integration. CI validates six platform packages and publishes them before the parent package. Local host tarballs remain private for offline testing. Node is required only for the npm wrapper and JavaScript host integrations.

The [AGENTS snippet](integrations/agents-md/AGENTS.md) is a short project-level alternative. The canonical [agent guide](docs/SKILL.md) selects operations and loads detailed rules on demand.

## Silent updates

A detached singleton worker checks releases and stages verified binaries without blocking edits or showing update messages. Installation normally waits until earlier usage becomes idle and the next invocation opens an opportunity. CLI uses a 30-minute idle window; MCP follows connection lifetime. Prepared releases have a 24-hour maximum deferral. Existing sessions continue running their loaded image.

See [update scheduling and private state](docs/updates.md). Set `XNIP_AUTO_UPDATE=0` for pinned/offline deployments.

## Limits

- Batches validate all operations before committing, then write one file at a time. Commit failures attempt rollback; inspect the [recovery journal](docs/recovery.md) before retrying after exit 4.
- Preconditions check observed bytes; they do not lock out external writers. Rewritten files preserve ordinary permissions, with [filesystem metadata limits](docs/architecture.md#persistence-and-limits).
- Backups append `.bak` to the complete filename and replace an existing sidecar. Git/backups provide exact recovery; `--revert` has [restricted inverse semantics](docs/apply-format.md#--revert-semantics).
- Line operations split on LF and require the caller to supply CRLF payloads when needed. CLI strings follow shell quoting.
- MCP uses the launching user's permissions and working directory, for trusted local clients.

## Documentation and development

| Need | Reference |
|---|---|
| Installation and client setup | [installation](docs/installation.md), [Codex](docs/codex.md), [MCP](docs/mcp.md) |
| Editing and automation | [CLI](docs/cli.md), [edit](docs/edit.md), [manifests](docs/apply-format.md), [patches](docs/patch.md), [exit codes](docs/error-codes.md) |
| Failure recovery | [recovery](docs/recovery.md) |
| Design and verification | [architecture](docs/architecture.md), [glossary](GLOSSARY.md), [tests](docs/testing.md), [update tests](docs/update-testing.md), [benchmarks](docs/benchmarks/README.md) |
| Contribution and release | [contributing](CONTRIBUTING.md), [release procedure](docs/releasing.md), [changelog](CHANGELOG.md) |

~~~sh
cargo test --workspace --locked
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo run --locked -p xtask -- sync-integrations
~~~

Additional platform, dependency, MSRV and packaging checks are in [CONTRIBUTING.md](CONTRIBUTING.md). MIT; see [LICENSE](LICENSE) and [third-party notices](THIRD_PARTY_LICENSES.md).
