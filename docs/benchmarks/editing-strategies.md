# Editing strategies and the remaining tradeoffs

The [current native run](results.json), [derived statistics](relative-results.json) and [readable results](results.md) compare real editing executors horizontally. The original literal xnip-edit profile remains alongside relative-path, adaptive, CLI and text-receipt variants; it is not replaced by the best new result. Native profiles likewise keep their original representations, failures and byte-incompatible outcomes.

The relevant target is less complete observable edit exchange under a stated contract. The smallest old/new block, a shared-file batch and a whole-file rewrite are useful in different situations. Changing a persistence algorithm after receiving a large payload cannot recover tokens already transmitted.

## What the current measurements show

The current 0.5.0 run executes 41 editor profiles on the same 13 scenarios, three times each, plus six separate host→xnip bridge profiles on those tasks. All original 31 profiles remain. DSH observation/filesystem policies receive separate frontiers; they and the host bridge lanes are excluded from the pooled native minimum. Only outputs that match expected file bytes in every repetition qualify for the best-observed frontier. Native and xnip use symmetric candidate permissions; snapshot-enabled includes guarded candidates on both sides.

| xnip transport / permitted operations | Geometric mean native best / xnip best input | xnip shorter / tied / native shorter |
|---|---:|---:|
| MCP guarded | 1.23x | 13 / 0 / 0 |
| MCP snapshot-enabled | 1.26x | 12 / 1 / 0 |
| CLI stdin guarded | 1.55x | 13 / 0 / 0 |
| CLI stdin snapshot-enabled | 1.80x | 13 / 0 / 0 |

CLI minima span native programs and compact maps, with invocation cost reported separately; this table is executor input, not total shell/MCP session usage. The original profiles remain visible beside the added configurations. Goose supplies the smallest guarded native input on 12 tasks, while OpenCode wins the global-replace native input. Across these 41 measured profiles, the MCP guarded input ratio is 1.23x.

Five short-file cases exposed missing candidate strategies rather than an unavoidable JSON lower bound. The current run executes the alternatives through existing interfaces and verifies the goal bytes in every repetition:

| Task | Native best input tokens | Best xnip MCP input tokens | Selected xnip representation |
|---|---:|---:|---|
| multiline-function | 26 | 25 | Original-coordinate line replace |
| no-final-newline | 14 | 13 | Unique literal 808 → 300, preserving the final 0 |
| disambiguated-block | 24 | 17 | Line 5 replace |
| same-file-batch | 20 | 18 | Ordered unique pairs 1 → 10 and 3 → 30 |
| insert-after-anchor | 18 | 15 | Line 1 insert, default after |

Whole-file JSON has wrapper and escaping costs, but that does not establish a lower bound for other eligible JSON inputs. Known line ranges can omit unchanged content; a complete known file can prove that a shorter old substring remains unique. The snapshot-only generator now considers these strategies before execution, retaining nonempty overlapping-match uniqueness and each ordered virtual revision. Original guarded profiles and required-old probes keep their supplied old bytes.

The same complete-file trimming opportunity is provided to accepted-relative OpenCode and Gemini literal profiles. Both new profiles are byte-exact on all thirteen tasks. Their permission cohort is snapshot-enabled, while their selected unique literal edits retain an actual guarded contract. This prevents crediting xnip for a one-sided input policy.

CLI snapshot policies reuse the same trim helper and actual counter; native programs also reuse the line candidates, while compact maps receive trimmed pairs. On `large-local-change`, OpenCode's known-file trim costs **21** tokens, xnip MCP's trim costs **16**, and the native CLI trim costs **13**. All three retain the complete relative path and a unique old literal. Existing native literal manifests also reduce the guarded global replacement to **14** MCP input tokens and the multi-file batch to **29**; the known-file long-line candidate costs **11**. The complete `manifest_text` JSON wrapper is included. Every current input frontier task favors xnip except snapshot-enabled MCP `delete-block`, which ties at **12 / 12**; guarded deletion still favors xnip at **19 / 18**. These are best observed inputs among the supplied strategies, not proof that any interface has reached its shortest possible encoding. No failing or inconvenient profile is removed.

Aider's final returned text and Goose's provider-rendered output are unknown, so their input counts cannot be turned into fabricated complete-exchange wins or losses. The known-return frontier names its own eligible winner independently.

The matched structured/text receipt experiment has 26 byte-exact pairs with identical actual inputs and returned text. Canonical full JSON-RPC reply counts sum to **1,600 versus 702 tokens**, saving 898 at that captured layer; the equal-pair text/default geometric mean is **0.440x**. The same five-tool compact catalog is **2,303 structured versus 1,754 text-receipt tokens**. These measured reply/catalog differences do not stand in for provider billing. [Current results](results.md), [raw evidence](results.json), [current catalogs and prompts](representation-results.md)

## What the added harnesses change

All ten added native profiles and all six real MCP bridge profiles meet the byte oracle on all thirteen tasks in all three repetitions. Their matching/IO behavior is still different: the mixed-EOL probe normalizes untouched line endings under DSH fs edit and Pi, while the raw str editor and Goose preserve them. Pi's original-coordinate multi-edit rejects a dependent start→middle→done batch; sequential DSH/Goose calls accept it. Late multi-file failures retain earlier independent writes. Those outcomes remain in the report rather than being silently repaired.

Fixed supplied-input profiles relative to the original absolute-path xnip-edit baseline have input geometric means of **0.959x for DSH fs-bare edit**, **1.534x for fs-observed edit including its real read**, **1.252x for Pi edit**, and **0.989x for Goose edit**. These are distinct path/policy/input configurations, not complete-agent rankings. DSH bare and Goose can supply less input than that original literal xnip baseline; xnip's compact/adaptive profiles are separate measured alternatives.

DSH's read-before-edit, stale-after-read and new-session probes show the actual observation policy rejecting mutations that bare kernels accept. The observed policy's additional real read is not a gratuitous serialization cost and cannot be removed while claiming the same freshness contract. Its policy-specific MCP guarded exchange comparison is **2.215x input / 5.737x input plus known text**, versus **1.384x / 2.063x** for fs-bare. These compare captured edit exchange with an external xnip process, not identical filesystem authority. See [policy-specific results](results.md#dsh-policy-specific-exchange-frontiers).

The [host exchange records](host-exchange-results.json) separate actual native/editor catalogs from genuine host→xnip MCP catalogs. For tiny-config, DSH fs-observed's captured model schema projection costs **508 tokens**, versus **1,544** for its five-tool xnip bridge; Pi's native edit/write projection costs **323**, versus **1,544** for its bridge. DSH prompt contributions and Pi instructions/snippets are additional separately captured layers. These are editor-only/bridge-only fixture compositions, not a measured default AgentLoop or coexistence catalog.

Thus a shorter edit/receipt does not establish a smaller complete session: the xnip tool catalog can add setup context. Goose's raw API catalog and raw MCP DTO are retained, while its provider projection stays unknown. No hypothetical request count, cache behavior or paid token savings is invented.

## Why a native payload can be smaller

### A short final file can cost less than its old/new edit

Aider WholeFileCoder receives the final complete file and writes it. Its pinned apply_edits does not require an old matching block in the message; the whole-file prompt explicitly asks for the updated file. A deletion-heavy rewrite can therefore omit much of the old content. [Pinned implementation](https://github.com/Aider-AI/aider/blob/5dc9490bb35f9729ef2c95d00a19ccd30c26339c/aider/coders/wholefile_coder.py#L124-L128), [whole-file prompt](https://github.com/Aider-AI/aider/blob/5dc9490bb35f9729ef2c95d00a19ccd30c26339c/aider/coders/wholefile_prompts.py)

This is an operation choice, not evidence that literal replacement has a broken implementation. A complete small final file can be shorter than the old block plus its replacement. A large file with a tiny local change reverses that relationship. The current run measures both and checks CRLF/final-newline bytes.

Xnip now has explicit whole-file write/upsert operations, with an optional whole-old-file precondition. Snapshot-enabled policies may use an unguarded whole write when the complete intended snapshot is already supplied; guarded policies retain old-content checks. The ordinary exact edit continues to reject missing or ambiguous old text. [Current edit contract](../edit.md), [strategy methodology](native-methodology.md)

### One file and several changes can share structure

A patch can state the path once and carry multiple hunks or a merged hunk. A SEARCH/REPLACE block can similarly combine nearby changes. Separate literal calls repeat path and argument structure and return once per call.

The new xnip compact files map shares each file path while preserving ordered old/text pairs. Both the original independent-call profile and the grouped strategies are measured, so batching is not credited to the matching algorithm. A single larger block is not automatically cheapest: added unchanged context can outweigh the saved wrappers. [Files-map contract](../apply-format.md), [actual plan generation](../../benches/native_tools.py)

### JSON, raw programs and invocation are different boundaries

MCP argument objects carry field names, quotes and escaped strings. Codex's raw patch body and aider's edit message use different envelopes. The CLI native program can use safe bare tokens and literal quoted multiline content, while the compact CLI format accepts a map directly.

The new CLI profiles count the complete program delivered to stdin, including every operation, file and payload. They separately count the executed invocation argv. A raw-program input advantage cannot be presented as a lower complete shell-tool cost without that extra boundary. Full serialized MCP replies are likewise separate from captured tool text. [Cost-layer definitions](native-methodology.md#cost-layers-and-timing)

The native encoder considers equivalent safe bare/quoted and raw-multiline/escaped choices before execution. MCP reuses these complete programs through the existing `manifest_text` argument and counts the JSON wrapper, while CLI sends the program directly. The optional final record newline is another candidate. Reserved payload and locator prefixes remain quoted. This removes redundant syntax without interpreting user text as a regex or dropping byte checks.

## Path configuration is not an editing algorithm

The original xnip literal baseline starts one MCP process at an outer directory and sends absolute paths for each inner fixture. New MCP profiles start at the actual case root and send relative slash paths. They never mutate the parent process cwd.

Relative paths already belong to xnip's file interface; the improvement here is binding the workspace correctly. OpenCode's pinned edit code also joins relative arguments to its instance directory, despite its absolute-path description. Gemini 0.27.2 validates and corrects relative arguments against its target directory. Separate accepted-relative native profiles execute those real paths, so xnip cannot claim a one-sided workspace advantage. [Xnip working-directory contract](../mcp.md#working-directory-and-trust), [pinned OpenCode resolution](https://github.com/anomalyco/opencode/blob/e3c4eb242585dcd77be1a867391b7c25fd47ce52/packages/opencode/src/tool/edit.ts#L47-L83), [Gemini versioned edit source](https://github.com/google-gemini/gemini-cli/blob/v0.27.2/packages/core/src/tools/edit.ts)

Actual source/configuration fingerprints and both original and native-corrected arguments remain in raw JSON. No inferred path-string saving is substituted for a real execution.

## Guarded and snapshot-enabled comparisons

The guarded frontier permits guarded candidates only. The snapshot-enabled frontier permits guarded plus snapshot candidates **symmetrically on both sides**. Allowing a complete overwrite does not force a native tool into whole-file output on a large local edit while allowing xnip to use a short literal replacement.

Winning records retain their actual profile contract. Xnip MCP and CLI minima stay separate; native candidates can span their own transports, with the winning transport identified. Matching strictness still differs among native parsers; separate probes record those differences.

Required-old probes preserve the exact supplied stale/ambiguous text. An adaptive snapshot-capable profile cannot replace a failed old match with an unconditional write, regenerate its guard from the current file or silently select a different target. Smaller inputs with incompatible bytes never qualify for the frontier.

## Selection precedes execution

Adaptive candidates are generated from the known source, intended result and supplied edits. Their complete serialized inputs are counted, then the smallest candidate is selected before any editor call. Ties prefer fewer calls, a guarded contract and deterministic names. The raw record retains candidate costs, hashes, selected operation and selected contract.

Snapshot-only line candidates call the existing default `xnip_apply` with a complete native `manifest_text`, or send that complete native program directly to CLI stdin. LF-only diff groups use original coordinates; source-derived simulation checks CRLF, EOF, empty deletions and inserted blank lines before admitting the candidate. Line payloads remain quoted to prevent literal modifier-looking text from becoming syntax. Literal trims enumerate shared-context offsets, with bounded offsets on large blocks, and retain the smallest serialized pair before complete argument-plan scoring. Neither search is a global optimality proof. [Candidate contracts](native-methodology.md#xnip-contracts-and-pre-execution-selection), [existing line and manifest interfaces](../apply-format.md)

The fixture already supplies complete before/after files to every generator. A real agent that only knows one block may need a read or additional generation before a full-file write is eligible; that work is not free. The benchmark's post-run best-observed frontier is a descriptive lower envelope, not a hidden policy that retries candidates and selects the output afterward.

The original three-line context profiles remain. Added Codex/aider unique-context profiles enumerate symmetric 0..3-line contexts and widen only when needed for uniqueness. Aider's pinned instructions call for enough context to disambiguate, rather than a mandatory fixed three-line budget. These are named candidate sets, not proof of a globally minimal native message. [Generator](../../benches/native_tools.py), [pinned block rules](https://github.com/Aider-AI/aider/blob/5dc9490bb35f9729ef2c95d00a19ccd30c26339c/aider/coders/editblock_prompts.py#L134-L144)

Aider EOF-aware whole profiles are measured separately from canonical closed-fence profiles. They use an unclosed last fence only for a known unterminated goal without trailing whitespace; the native non-final response accessor would otherwise strip the tail. Newline-terminated goals use a closing fence, and other trailing-whitespace goals are not claimed as preserved by the unclosed encoding. Platform and LF writes also remain separate. A generator-specific failure is not attributed to every possible invocation of the tool.

## Compact text and complete replies

The current default text MCP receipt omits structuredContent and outputSchema while retaining written/unchanged/check/preview/journal text and errors. Explicit --receipts structured keeps machine-readable change metadata; the original benchmark counterparts select it deliberately. This changes the captured full reply/catalog boundary without changing per-edit arguments.

The report pairs the same adaptive strategy under structured and text-only receipt modes. It calculates a receipt-mode reduction only when both outputs are byte-exact and the actual inputs and returned text are identical. Equal token counts alone cannot establish identical content. Catalogs are discovered from real running servers under each advertised mode. [MCP receipt contract](../mcp.md), [paired receipt statistics](../../benches/native_report.py)

Captured returned text and the full serialized JSON-RPC object are not interchangeable. A host may inject only text or expose additional structured fields. The report shows both observed layers without converting a wire reduction into a claim about a provider's billed usage.

Aider's final model-facing return and Goose's provider projection remain unknown. Empty editing-kernel stdout or a raw internal MCP DTO does not become a zero-token complete response, and an unknown value cannot win a combined input/return comparison.

## Interpreting the current results

Use the [README frontier](charts/readme-frontier.svg) for four input-only permission/transport groups and [per-task frontier](charts/frontier.svg) for absolute winners and known-text exchanges. The [full report](results.md) retains every profile, candidate contract, byte result, invocation cost and captured MCP layer.

The useful questions are concrete:

- Does a local edit avoid unchanged source content?
- Can several edits share one path and one response?
- Is a whole-file write eligible under the required precondition?
- Does the native parser accept a shorter valid encoding?
- Does a smaller input remain smaller after its known returned text?
- Which additional host/transport layers are actually captured?

The aim is to reduce duplicated information while preserving the chosen edit contract and correct file bytes. A full agent study still needs model/harness controls, initial reads, tool selection, retries, caching and provider usage. No paid inference is performed by these executor suites.
