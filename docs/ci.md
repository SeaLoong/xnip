# CI workflow contract

The repository has exactly three workflows. Keep verification in `ci.yml`, release entry-point policy in `release.yml`, and artifact delivery in `build-release.yml`. Do not add a workflow for an individual command or platform.

| Workflow | Entry points | Work and boundary |
|---|---|---|
| [ci.yml](../.github/workflows/ci.yml) | Pull requests targeting `master`/`main`, pushes to those branches, or a reusable call from `release.yml` | Select checks from Git history, verify them, and expose the `ci` gate. Only a qualifying default-branch push may request a development release after full verification. |
| [release.yml](../.github/workflows/release.yml) | A `v*` tag excluding reserved `v*-dev.*` tags, or a manual dispatch | Validate the Cargo/tag version, reuse complete checks for the exact commit or request full checks, then call the artifact workflow. Tags publish; manual runs only rehearse. |
| [build-release.yml](../.github/workflows/build-release.yml) | `workflow_call` from the two entry points above | Build six release binaries, smoke-test runnable targets, package checksums and integration assets, and optionally publish a complete draft. It does not decide which commits are eligible or repeat source checks. |

Ordinary PR verification never publishes and receives no release secret. Verification jobs explicitly use `contents: read`; only artifact-call jobs and the actual publisher request `contents: write`. The reusable-check caller in `release.yml` must declare the upper permission bound needed by the development-publisher declaration, but its tag/manual event cannot enable that publisher.

## Required check and work selection

Configure branch protection to require the single check named **`ci`**. Individual jobs may legitimately skip; the aggregate gate always runs and fails on a failed or cancelled dependency, failed scope detection, or a selected job that did not run successfully. Cancelling an entire PR run cannot produce a successful gate. No workflow-level path filter can leave a required check pending.

[`ci_scope.py`](../.github/scripts/ci_scope.py) compares the complete Git diff, including deleted paths and both sides of renames. Pushes compare `before..GITHUB_SHA`; PRs compare the base/head merge-base with the checked-out synthetic merge commit. Initial pushes compare against the empty tree. Invalid or missing comparison history fails verification. Webhook file lists and their file-count limits are not used.

| Changed inputs | Verification |
|---|---|
| Rust source/tests/benchmarks, Cargo/build/toolchain/`.cargo`/Cross/dependency policy, installers and their tests, or an unrecognized non-document input | Rust quality, tests on Linux/macOS/Windows, MSRV and dependency policy |
| `docs/cli.md`, `docs/mcp.md`, `docs/apply-format.md`, `docs/examples.md` | Rust checks too: CLI contracts execute examples embedded from these files |
| `docs/SKILL.md`, its six packaged references, `integrations/`, GitHub skills or `xtask/` | Regenerate integrations and require a clean diff; `xtask/` also enables Rust checks |
| Any workflow, the scope selector or the release-verification helper (including their tests) | Full verification |
| General README/docs, benchmark reports/Python helpers, or ordinary Python contract tests | Cheap automation/installer contracts; Rust is skipped unless another input requires it |
| Default-branch push requiring Rust checks | Full verification, including integration consistency, so the exact commit can be reused for a tagged release |
| Default-branch push eligible for a development release | Full verification before any artifact build, even for a release-script-only change |
| Tag/manual reusable verification or `force_all: true` | Full verification regardless of changed paths |

The six packaged reference documents are `apply-format.md`, `error-codes.md`, `examples.md`, `patch.md`, `recovery.md` and `edit.md`. The selector constants are authoritative; update their boundary contracts when adding a generated input or a document consumed by Rust tests. Release eligibility remains a separate policy in [releasing](releasing.md).

## Job contents

| Job | Required work |
|---|---|
| `scope` | Determine changed inputs and default-branch release eligibility once |
| `quality` | On Rust inputs: rustfmt, Clippy with all targets, rustdoc with warnings denied, and verified crate packaging. On integration inputs: regenerate and compare the four generated integration trees |
| `test` | One `cargo test --workspace --locked` per OS; the Windows leg also runs offline PowerShell installer contracts using the already-built CLI |
| `compatibility` | Rust 1.89 check for all workspace targets, then cargo-deny |
| `contracts` | actionlint plus offline scope, release-verification, release-planner, publisher, native-benchmark plumbing and POSIX installer contracts; shell syntax check |
| `ci` | Require every selected job to pass; emit the `Full verification` marker only after all checks in full mode pass |
| `prerelease` | After the gate, delegate an eligible default-branch development release to the artifact workflow |

`cargo test` already includes doctests ([Cargo reference](https://doc.rust-lang.org/cargo/commands/cargo-test.html)); do not run a second `--doc` pass in CI. Clippy and the MSRV check compile-check benchmark targets. Optimized `cargo bench --no-run`, cost measurements and native editor benchmarks are local/manual investigations, outside the required CI path. See [benchmark instructions](benchmarks/README.md). The cheap benchmark-plumbing contracts remain required.

Rust checks keep `--locked` and warnings denied. Installer tests use local fixtures. Automatic updates are disabled throughout CI and release builds.

## Release verification and publication

`release.yml` looks up `ci.yml` runs by the peeled checkout commit SHA and accepts only a `push` from the same repository's current default branch. It inspects the latest attempt's `ci` job and requires both a successful gate and a successful `Full verification` step. A PR/fork run, a different commit, or scoped verification cannot authorize a release.

If the trusted run is still checking, release waits up to 30 minutes for its gate. It can proceed while development artifact builds are still running; their success is not used as evidence of source correctness. A failed/cancelled gate blocks release. No trusted run, an old run without the gate, or a successful scoped gate causes one reusable full verification instead. Manual rehearsal follows the same policy but always sets `publish: false`.

An atomic default-branch/code/version/tag push receives full branch verification while the planner suppresses the duplicate development release. The tag workflow can reuse that same gate. Different workflow entry points have separate concurrency groups, avoiding a nested caller cancelling or waiting on itself.

PR updates cancel superseded runs. Default-branch commits and releases are not cancelled by later commits. Publishing the same version stays serialized; published assets remain immutable and incomplete drafts can resume. Keep all six supported target archives and the existing checksum/draft validation.

Job timeouts bound stalled work: scope/contracts 5 minutes, quality/compatibility 15, platform tests 20, gate 2, release identity/prior-check lookup 35, artifact builds 45 and publication 15. These are ceilings rather than expected durations.

## Changing CI

1. Change the workflow with the responsibility above; do not introduce duplicate push/tag entry points.
2. Update the scope classifier and its contracts for new inputs. Unknown non-document paths conservatively enable Rust checks.
3. Run `node --test tests/verified_ci_contract.cjs`, `python tests/ci_scope_contract.py`, `python tests/release_contract.py`, `python tests/publish_release_contract.py` and `actionlint`. The verification helper uses the Node runtime already available on the hosted Ubuntu runner; no npm dependencies are needed. Run installer contracts on their native OS when changing those jobs.
4. Check docs-only PRs, source PRs, packaged-guide changes, default-branch development publication, atomic stable tags and manual rehearsal. A new skip condition must never create a full-verification marker without running its checks.
5. Review the first remote runs after merging; local syntax and mocked policy checks cannot verify runner availability or release credentials.
