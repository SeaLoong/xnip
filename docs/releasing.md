# Release procedure

The release workflow builds six OS/architecture archives and publishes a GitHub Release only after its validation and build jobs succeed. GitHub Releases are the supported distribution channel. crates.io publishing is a separate action and is not performed by this workflow.

## Prepare the release commit

1. Update the root package version in `Cargo.toml` and regenerate `Cargo.lock` if the version changes. Keep `xtask` unpublished.
2. Move Unreleased entries into a dated `CHANGELOG.md` release section and update version-pinned installation examples. Keep the release date in the project's Asia/Shanghai timezone.
3. Review README, CLI examples, manifest rules, MCP differences, compatibility limits and installer asset names. Run `cargo run --locked -p xtask -- sync-integrations` and commit generated files too.
4. Run the checks in [CONTRIBUTING.md](../CONTRIBUTING.md), commit and push. Wait for every job in the `ci` workflow on that exact commit, including Linux, macOS and Windows tests.

Do not move an existing published version tag to another commit. If a released version needs a fix, increment the version and publish another release.

## Rehearse and tag

Use the manual workflow to test all release builds without publishing:

```sh
gh workflow run release.yml --ref master
gh run list --workflow release.yml --limit 3
```

Inspect the run and its artifacts. Once checks and all target builds pass on the intended commit, create an annotated tag matching Cargo's version:

```sh
git tag -a v0.1.1 -m "xnip v0.1.1"
git push origin v0.1.1
```

The tag workflow checks the tag/version match and repeats validation. It packages executable, README and license; generates archive SHA256 sidecars; and uploads six binary archives, six sidecars, two installers and six integration archives. Build commit metadata comes from `XNIP_BUILD_COMMIT`; `Cross.toml` passes it into Linux build containers.

Use the release notes to describe capabilities, installation, compatibility and limitations. Treat v0.x API/behavior changes explicitly; do not promise universal undo or multi-file transactions.

## Verify publication

- Confirm the release is neither draft nor prerelease, its tag resolves to the tested commit, and all 20 expected assets exist.
- Download the actual binary archive and checksum, verify the hash, and run `--version` and `doctor`.
- Exercise the published installer with an isolated installation directory. Verify both the explicit version and default `latest` channel. Stop running Windows MCP processes before replacing a normal installation.
- Check the latest-download links and version-pinned source install command. Record the CI/release URLs in the completion report.

For a failure before publication, fix the branch, push, and rerun the rehearsal. If a tag build fails before publishing, prefer a corrected version rather than rewriting a tag that users may already have fetched. Never delete successful unrelated releases or bypass required checks.

## Optional crates.io publication

If the maintainer separately chooses to distribute through crates.io, verify ownership/name availability, run `cargo publish --dry-run --locked`, then publish with the appropriate account credential. Add `cargo install xnip --locked` to installation instructions only after that distribution has been verified. Do not place registry tokens in the repository or release notes.
