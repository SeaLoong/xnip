# Builds and releases

CI checks every configured branch push and pull request. A successful push to the repository's default branch also creates a development prerelease when release inputs changed. Tag releases and manual build rehearsals use the same six-platform [artifact workflow](../.github/workflows/build-release.yml).

## Automatic development prereleases

The [release planner](../.github/scripts/release.py) compares every path in the complete push range, including renamed and deleted paths. It does not rely on GitHub's limited webhook file list or workflow path filters. Initial pushes inspect the full initial tree. A missing comparison commit fails the plan rather than guessing that nothing changed.

| Input changes | Automatic prerelease |
|---|---|
| Application files under `src/` | Yes, except dedicated `contract_tests.rs` files |
| Cargo manifests/lockfile, build script, Rust toolchain, Cross and `.cargo/` configuration | Yes |
| Installers, licenses and third-party notices | Yes |
| Integration packages, editing guide and its six packaged references, `xtask` generator | Yes |
| Release planning/publication scripts and release/artifact workflows | Yes |
| General README/docs, tests, benchmarks, CI-check-only changes | No; README content is included with the next qualifying release |
| Pull requests, including forks | Checks only; no publication |
| Nondefault branch pushes | No publication |

The exact allowlist lives in `release_relevant` and its constants, with [offline contracts](../tests/release_contract.py). CI still runs its checks for changes that do not publish.

For a stable Cargo version `0.3.1`, CI generates `0.3.2-dev.<run_number>.g<commit12>`. An already-prerelease Cargo version retains its major/minor/patch core. Version, tag and binary `--version` agree: only the isolated build checkout's root manifest and matching lockfile entry are stamped. The source branch is never rewritten. Rerunning a workflow keeps the same tag; `run_attempt` is excluded.

Publication starts only after formatting, linting, all platform tests, dependency policy, MSRV, package/integration checks and release contracts pass. Six build jobs produce Linux musl, macOS and Windows archives for x86_64 and ARM64. Native runner smoke tests check the embedded version and doctor output.

A [publisher](../.github/scripts/publish_release.py) verifies all 21 expected assets and six archive SHA256 sidecars before creating or completing a draft. It publishes only after all uploads are present. Development releases set `prerelease=true` and `make_latest=false`; they never replace stable `latest`.

The publisher serializes calls for the same version. An existing tag must point to the same source commit. A published complete release is left intact on rerun; a draft can resume. A conflicting tag or incomplete published release fails instead of replacing published assets. Different versions can build concurrently; completion order is not commit order.

Install a preview by its explicit tag:

```sh
XNIP_VERSION=v0.3.2-dev.123.gabcdef123456 sh install.sh
```

```powershell
$env:XNIP_VERSION = 'v0.3.2-dev.123.gabcdef123456'
./install.ps1
```

The default installers and [silent updater](updates.md) remain on stable releases. A preview based on the next patch cannot be replaced by the previous stable version.

## Stable release procedure

1. Update the root package version in `Cargo.toml` and regenerate its `Cargo.lock` entry. Keep `xtask` unpublished.
2. Move Unreleased entries into a dated `CHANGELOG.md` section and update pinned installation examples. Use the project's Asia/Shanghai timezone for the release date.
3. Review user-facing contracts and compatibility limits. Run `cargo run --locked -p xtask -- sync-integrations` and commit generated integration files too.
4. Run [contributor checks](../CONTRIBUTING.md), commit and push. Wait for all CI checks on that exact commit.

A manual rehearsal runs checks and all release builds without publishing:

```sh
gh workflow run release.yml --ref master
gh run list --workflow release.yml --limit 3
```

Inspect all artifacts, then create an annotated tag matching Cargo:

```sh
git tag -a v0.3.2 -m "xnip v0.3.2"
git push --atomic origin master v0.3.2
```

The example assumes Cargo has been set to that new unpublished version. Never move an existing published tag. Push the release commit and tag atomically: the planner sees a matching stable tag at the same commit and skips a duplicate development prerelease while retaining all branch checks. The tag workflow verifies Cargo/tag equality, repeats checks, builds, and publishes from a complete draft. Stable release notes use the matching, nonempty changelog section. Tags containing a SemVer prerelease identifier are marked prerelease and do not become latest.

Archives contain the executable, README, MIT license and third-party notices. Assets comprise six binary archives, six checksums, two installers and seven integration archives. Build commit metadata comes from `XNIP_BUILD_COMMIT`; `Cross.toml` forwards it into Linux containers.

## Verify publication

- Confirm the tag resolves to the tested commit and all 21 assets exist.
- Check prerelease/latest flags for the intended channel.
- Download the archive and checksum, verify SHA256, and run `--version` and `doctor`.
- Exercise an installer in an isolated directory. Stable releases should work with both the explicit tag and default latest channel.
- Record the CI/release URLs with the validation results.

If uploading fails, retry the same run to resume its draft. A published immutable release is never repaired by overwriting its assets; use a new version. Ordinary default-branch publication uses the publish job's `contents: write` token. [GitHub requires additional workflow permission](https://docs.github.com/en/rest/releases/releases?apiVersion=latest) when an older target commit changes workflow files relative to the current default branch. This can happen after a concurrent workflow update. An optional repository secret `RELEASE_TOKEN`, scoped to Contents and Workflows write for this repository, is passed only to the publishing step; without it that exceptional request fails explicitly and can be retried later. No extra secret is needed for the ordinary current-default-HEAD case.

Creating a tag with `GITHUB_TOKEN` does not start another release workflow, so CI calls the artifact workflow directly in the same run. The tag workflow also excludes the reserved `-dev.*` namespace, preventing duplicate release runs when an optional token does generate a tag event. See [GitHub's trigger rules](https://docs.github.com/en/actions/how-tos/write-workflows/choose-when-workflows-run/trigger-a-workflow).

## Optional crates.io publication

crates.io publishing is separate and is not performed by these workflows. Verify ownership/name availability, run `cargo publish --dry-run --locked`, then publish with the appropriate credential only when that distribution is intended. Document `cargo install xnip --locked` only after verifying it. Never place registry tokens in release notes or the repository.
