# Builds and releases

CI selects checks for configured branch pushes and pull requests. A successful push to the repository's default branch also creates a development prerelease when release inputs changed. Tag releases and manual build rehearsals use the same six-platform [artifact workflow](../.github/workflows/build-release.yml). The [CI workflow contract](ci.md) defines the three workflows, their event boundaries, required gate and change scopes.

## What the current workflows deliver

The release pipeline delivers **21 GitHub attachments and seven npm packages**
from one verified source commit. Local npm preparation stays private by default;
only explicit, complete public preparation can enter the publishing job.

| Destination / output | Count | Current behavior |
|---|---:|---|
| GitHub native archives | 6 | Linux musl, macOS and Windows, each x64/ARM64 |
| GitHub archive SHA256 sidecars | 6 | One checksum per native archive |
| GitHub installers | 2 | `install.sh`, `install.ps1` |
| GitHub integration archives | 7 | AGENTS, aider, Claude Code, Codex, Copilot, Cursor and generic |
| **GitHub release total** | **21** | Exact inventory validated before draft publication |
| npm registry packages | 7 | Parent package plus six native platforms, with integrity and native-installation checks |
| crates.io | 0 published by CI | Separate optional manual distribution |

The integration archive names are `xnip-agents-md.tar.gz`,
`xnip-aider-conventions.tar.gz`, `xnip-claude-code-skill.tar.gz`,
`xnip-codex-instructions.tar.gz`, `xnip-copilot-prompt.tar.gz`,
`xnip-cursor-rule.tar.gz` and `xnip-generic-skill.tar.gz`.
DSH/Pi entries live in the local npm package; Goose uses source configuration
examples. They are not additional GitHub release attachments.

Manual `workflow_dispatch` runs the same six builds, prepares all 21 GitHub
attachments and seven npm tarballs, then installs the parent and matching native
package on six actual OS/CPU runners. The `delivery-<version>` Actions artifact
contains separate `dist/` and `dist-npm/` trees. Manual runs skip the publishing
job and need no publishing credentials.

## Automatic development prereleases

The [release planner](../.github/scripts/release.py) compares every path in the complete push range, including renamed and deleted paths. It does not rely on GitHub's limited webhook file list or workflow path filters. Initial pushes inspect the full initial tree. A missing comparison commit fails the plan rather than guessing that nothing changed.

| Input changes | Automatic prerelease |
|---|---|
| Application files under `src/` | Yes, except dedicated `contract_tests.rs` files |
| Cargo manifests/lockfile, build script, Rust toolchain, Cross and `.cargo/` configuration | Yes |
| Installers, licenses and third-party notices | Yes |
| The seven shipped integration directories with `package.sh`, editing guide and its six packaged references, `xtask` generator | Yes |
| npm package inputs and DSH/Pi plugin entries | Yes; these are shipped through the npm distribution |
| Goose source configuration/hints and native benchmark fixtures | No; not shipped as additional release assets |
| Release planning/publication scripts and release/artifact workflows | Yes |
| General README/docs, tests, benchmarks, CI-check-only changes | No; README content is included with the next qualifying release |
| Pull requests, including forks | Checks only; no publication |
| Nondefault branch pushes | No publication |

The exact allowlist lives in `release_relevant` and its constants, with [offline contracts](../tests/release_contract.py). CI still runs its checks for changes that do not publish.

For example, with stable Cargo version `0.5.1`, CI generates `0.5.2-dev.<run_number>.g<commit12>`. An already-prerelease Cargo version retains its major/minor/patch core. Version, tag and binary `--version` agree: only the isolated build checkout's root manifest and matching lockfile entry are stamped. The source branch is never rewritten. Rerunning a workflow keeps the same tag; `run_attempt` is excluded.

Every commit selected for publication receives full verification: formatting, linting, all platform tests, dependency policy, MSRV, package/integration checks and release contracts must pass. The successful `ci` gate records a `Full verification (Rust, integrations, npm, harnesses)` marker; scoped verification alone cannot authorize publication. Six build jobs produce Linux musl, macOS and Windows archives for x86_64 and ARM64. Native runner smoke tests check the embedded version and doctor output.

A [publisher](../.github/scripts/publish_release.py) verifies all 21 expected assets and six archive SHA256 sidecars before creating or completing a draft. It publishes only after all uploads are present. Development releases set `prerelease=true` and `make_latest=false`; they never replace stable `latest`.

The publisher serializes calls for the same version. An existing tag must point to the same source commit. A published complete release is left intact on rerun; a draft can resume. A conflicting tag or incomplete published release fails instead of replacing published assets. Different versions can build concurrently; completion order is not commit order.

Install a preview by its explicit tag:

```sh
XNIP_VERSION=v0.5.2-dev.123.gabcdef123456 sh install.sh
```

```powershell
$env:XNIP_VERSION = 'v0.5.2-dev.123.gabcdef123456'
./install.ps1
```

The default installers and [silent updater](updates.md) remain on stable releases. A preview based on the next patch cannot be replaced by the previous stable version.

## Stable release procedure

1. Update the root package version in `Cargo.toml` and regenerate its `Cargo.lock` entry. Keep `xtask` unpublished.
2. Move Unreleased entries into a dated `CHANGELOG.md` section and update pinned installation examples. Use the project's Asia/Shanghai timezone for the release date.
3. Review user-facing contracts and compatibility limits. Run `cargo run --locked -p xtask -- sync-integrations` and commit generated integration files too.
4. Run [contributor checks](../CONTRIBUTING.md) and commit locally. For stable publication, push the release commit and annotated tag atomically as shown below. The tag workflow waits for the full `ci` gate on that exact commit before building the release.

Commit and push the intended source before starting CI: GitHub Actions builds
that pushed ref, not an uncommitted local working tree. A stable release must have
a nonempty matching changelog section before its tag is created. Merely pushing the 0.5.1 source to the default
branch selects a development prerelease; it does not publish stable 0.5.1.

A manual rehearsal runs source checks, complete packaging and six native npm
installation checks without publishing:

```sh
gh workflow run release.yml --ref master
gh run list --workflow release.yml --limit 3
```

When publishing, create an annotated tag matching Cargo:

```sh
git tag -a v0.5.1 -m "xnip v0.5.1"
git push --atomic origin master v0.5.1
```

The example assumes Cargo has been set to that new unpublished version. Never move an existing published tag. Push the release commit and tag atomically: the planner sees a matching stable tag at the same commit and skips a duplicate development prerelease while retaining all branch checks. The tag workflow verifies Cargo/tag equality and reuses the successful full `ci` gate from a same-repository default-branch push for that exact commit, waiting for an in-progress gate if necessary. If no reusable full gate exists, it runs full checks once. It then builds and publishes from a complete draft. Stable release notes use the matching, nonempty changelog section. Tags containing a SemVer prerelease identifier are marked prerelease and do not become latest.

Archives contain the executable, README, MIT license and third-party notices. Assets comprise six binary archives, six checksums, two installers and seven integration archives. Build commit metadata comes from `XNIP_BUILD_COMMIT`; `Cross.toml` forwards it into Linux containers.

## Verify publication

- Confirm the tag resolves to the tested commit and all 21 assets exist.
- Check prerelease/latest flags for the intended channel.
- Download the archive and checksum, verify SHA256, and run `--version` and `doctor`.
- Exercise an installer in an isolated directory. Stable releases should work with both the explicit tag and default latest channel.
- Record the CI/release URLs with the validation results.

If uploading fails, retry the same run to resume its draft. A published immutable release is never repaired by overwriting its assets; use a new version. Ordinary default-branch publication uses the publish job's `contents: write` token. [GitHub requires additional workflow permission](https://docs.github.com/en/rest/releases/releases?apiVersion=latest) when an older target commit changes workflow files relative to the current default branch. This can happen after a concurrent workflow update. An optional repository secret `RELEASE_TOKEN`, scoped to Contents and Workflows write for this repository, is passed only to the publishing step; without it that exceptional request fails explicitly and can be retried later. No extra secret is needed for the ordinary current-default-HEAD case.

Creating a tag with `GITHUB_TOKEN` does not start another release workflow, so CI calls the artifact workflow directly in the same run. The tag workflow also excludes the reserved `-dev.*` namespace, preventing duplicate release runs when an optional token does generate a tag event. See [GitHub's trigger rules](https://docs.github.com/en/actions/how-tos/write-workflows/choose-when-workflows-run/trigger-a-workflow).

## npm publication

The public entry is `xnip`; ordinary users run `npm install --global xnip`.
Six exact-version optional packages provide the Rust executable:

| Package | Platform |
|---|---|
| `xnip-linux-x64-musl` | Linux x64, static musl |
| `xnip-linux-arm64-musl` | Linux ARM64, static musl |
| `xnip-darwin-x64` | macOS Intel |
| `xnip-darwin-arm64` | macOS Apple Silicon |
| `xnip-win32-x64` | Windows x64 |
| `xnip-win32-arm64` | Windows ARM64 |

The parent contains CLI/MCP and DSH/Pi integration; it does not bundle every
platform executable. npm installs the matching optional dependency. A separate
`dsh-xnip` package is unnecessary. Private host-only tarballs bundle one local
payload for offline development and cannot enter public publication.

### Build, verify and publish

```text
full source checks for the exact commit
  -> six version-stamped native builds and source/hash records
  -> collect 21 GitHub attachments and seven npm tarballs
  -> actual packed installation on all six native OS/CPU runners
  -> authenticate and check all existing registry versions
  -> publish six platforms, verify each, publish parent last
  -> verify npm integrity/channel, publish complete GitHub draft
```

Each build emits its archive, SHA256 sidecar and `.build.json` record. The latter
binds version, target, peeled source commit and archive hash. It stays in Actions
artifacts and is not added to the 21-file GitHub inventory. The collection job
stamps its checkout to the same version and runs:

```sh
python npm/prepare.py --public --dist dist-binaries --out dist-npm --pack --source-commit COMMIT_SHA
python .github/scripts/publish_npm.py --prepared dist-npm --source-commit COMMIT_SHA --check
```

Preparation defaults to private. Public mode rejects incomplete target sets,
host-only/GNU-local inputs, wrong binary version/target and inconsistent hashes
or source commits. All seven packages use the same repository identity and
version; the parent has exact optional dependencies. Prepack validates the
actual payload too. Build records prevent accidental input mixups; they are not
independent digital signatures.

The two commands above do not publish. The release job uses:

```sh
python .github/scripts/publish_npm.py --prepared candidate/dist-npm --source-commit COMMIT_SHA --auth token
```

Add `--prerelease` for a development version; authentication may explicitly be
`oidc` instead. Stable uses `latest`, development uses `next`. All existing
versions and channel tags are checked before any write. Platform packages are
published and verified before the parent; only matching immutable integrities
can be skipped on retry. An inconsistent existing version, tag mismatch or
attempt to move a channel backward fails rather than overwriting it.
[`npm publish` channel options](https://docs.npmjs.com/cli/v11/commands/npm-publish/#tag)

Publication is serialized per stable/development channel. After a partial
failure, **rerun failed jobs**, keeping the same verified `delivery-<version>`
artifact. Rebuilding every job can change native bytes or package integrity;
an already-published version cannot be replaced. Tag promotion is a separate
explicit operation, not an implicit retry repair. GitHub publication runs after
npm verifies and can resume its draft independently.

### First publication and Trusted Publisher transition

The default authentication mode follows the existing repository convention:
an Actions secret `NPM_TOKEN` is passed to npm as `NODE_AUTH_TOKEN`.
`NODE_AUTH_TOKEN` is also accepted as the secret name when `NPM_TOKEN` is absent.
Configure a valid publishing credential in repository Actions secrets; never put
its value in a workflow, a package or a committed npm configuration. The publisher
captures npm diagnostics and exposes only operation status and safe error codes.

For seven new names, first complete an authorized initial publication using a
credential allowed to create those packages. Then configure each package's
Trusted Publisher. The official [trust prerequisites](https://docs.npmjs.com/cli/v11/commands/npm-trust/#prerequisites)
require an existing package, write access and account 2FA. Staged creation is an
alternative, but it creates a public `0.0.0-stage` placeholder and is not a local
dry run. [Staging behavior](https://docs.npmjs.com/staged-publishing/)

For OIDC, configure each package for `SeaLoong/xnip` and the actual caller filename:
`release.yml` for stable tags, plus `ci.yml` if development publishing is enabled.
Allow direct publishing when automatic publication is intended. The workflows
already grant `id-token: write` through caller and reusable callee; the publisher
uses GitHub-hosted runners. Set repository Actions variable
`XNIP_NPM_AUTH=oidc` after the trusted publisher configuration is ready. This mode
does not pass the npm credential and does not silently fall back to token mode.
A new trust configuration needs a successful publish within two days.
[Official OIDC configuration and reusable-workflow rules](https://docs.npmjs.com/trusted-publishers/)

Verify the next actual version through OIDC before retiring the bootstrap
credential. Rerunning an already-complete version only checks its integrity and
does not prove that OIDC publication works. Ordinary direct OIDC publication
requires npm 11.5.1+ and Node 22.14+; the hosted publishing job uses Node 24.
The `npm trust` setup CLI and staging require npm 11.15+.
[CLI prerequisites](https://docs.npmjs.com/cli/v11/commands/npm-trust/#prerequisites)

Public-repository/public-package OIDC publication generates provenance
automatically. It complements the native checksum, version, target and commit
checks rather than replacing them.
[Provenance](https://docs.npmjs.com/generating-provenance-statements/)

## Optional crates.io publication

crates.io publishing is separate and is not performed by these workflows. Verify ownership/name availability, run `cargo publish --dry-run --locked`, then publish with the appropriate credential only when that distribution is intended. Document `cargo install xnip --locked` only after verifying it. Never place registry tokens in release notes or the repository.
