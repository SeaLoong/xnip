# Contributing to xnip

Use Rust stable, Cargo, a native linker and Git. The minimum supported Rust version is 1.89; the edition is 2024. Python 3 runs POSIX installer and benchmark-plumbing tests; Python 3.11+ runs the release planner and publisher contracts; PowerShell runs Windows installer tests.

## Build and verify

```sh
cargo build --locked
cargo test --workspace --locked
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo +1.89.0 check --workspace --all-targets --locked
cargo deny check
cargo bench --no-run --locked
cargo doc --workspace --no-deps --locked
cargo package --locked
```

Install the minimum toolchain with `rustup toolchain install 1.89.0 --profile minimal` and cargo-deny with `cargo install cargo-deny --locked` when needed. CI runs docs with warnings denied. Package from a clean checkout; local validation may use `--allow-dirty`.

Run `python3 tests/install_sh.py` on Linux/macOS. On Windows, build the debug executable and run `./tests/install_ps1.ps1`. Installer tests use local fixtures and preserve the user's installed binary. Run `python3 tests/native_benchmark_contract.py` for offline benchmark plumbing. Release changes also require `python3 tests/release_contract.py` and `python3 tests/publish_release_contract.py`; both are offline and never publish. The full suite map and platform limits are in [testing](docs/testing.md); updater cases have a separate [map](docs/update-testing.md).

## Make a change

Keep byte transformations in `src/core/ops`, guarded exact edits in `src/core/edit.rs`, shared read behavior in `src/core/read.rs`, persistence/recovery in `src/core/atomic.rs` and `transaction.rs`, and batch planning in `src/apply`. CLI and MCP adapt these contracts. See [architecture](docs/architecture.md).

Verify changed semantics through the relevant public interface, including preserved bytes, CRLF/EOF, ambiguous targets, preconditions, nonwriting modes and failure boundaries. Use child-process working directories rather than changing the process-wide cwd in parallel tests. Test background updates with copied binaries and isolated offline fixtures. Keep install-test candidates newer than the crate version (the fixtures use `999.0.0`), and test older/equal SemVer rejection separately so a release bump cannot invalidate installation coverage.

New dependencies must support the Rust minimum and pass `cargo deny check`. Keep unavoidable generated-code lint exceptions scoped and explained.

## Documentation

Update both READMEs for user-visible behavior. Current contracts live in `docs/`; historical release changes belong in [CHANGELOG.md](CHANGELOG.md). Avoid duplicating defaults that are already clear from `--help` unless they explain a workflow.

After changing `docs/SKILL.md` or its editing references, regenerate the Codex guide, generic/GitHub/Claude skills and copied references:

```sh
cargo run --locked -p xtask -- sync-integrations
```

Commit generated files with their canonical source. Short project instruction templates under `integrations/` are maintained separately. Load detailed rules through operation-specific pointers; keep always-loaded instructions small.

The [native benchmark](docs/benchmarks/README.md) compares actual editor implementations. Report paired relative costs together with byte correctness and captured-response limits; representation-only measurements are supplementary.

## Review and release

Explain the original problem, resulting behavior, validation and material compatibility changes. The [CI workflow contract](docs/ci.md) defines check scopes and workflow ownership. The required `ci` gate covers formatting, Clippy, Linux/macOS/Windows contracts, minimum Rust, advisories and package/integration consistency when selected; every release commit requires full verification. Maintainers follow the [release procedure](docs/releasing.md).
