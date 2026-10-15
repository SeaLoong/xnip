# Contributing to xnip

Use Rust stable, Cargo, a native linker, and Git. The minimum supported Rust version is 1.89 and the edition is 2024. Python 3 runs the POSIX installer contract tests; PowerShell runs the Windows installer tests.

## Build and check

```sh
cargo build --locked
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo +1.89.0 check --workspace --all-targets --locked
cargo deny check
cargo package --locked
```

Install the minimum toolchain with `rustup toolchain install 1.89.0 --profile minimal` and cargo-deny with `cargo install cargo-deny --locked` if needed. Run `cargo package` from a clean checkout; local investigation may use `--allow-dirty`, but CI packages committed sources.

On Linux/macOS, run `python3 tests/install_sh.py`. On Windows, first build the debug executable, then run `./tests/install_ps1.ps1`. The tests use local archives and mocked downloads. They do not install globally. `cargo bench --bench large_file` runs core-function benchmarks; process startup and disk IO are not included.

## Make a change

Keep byte transformations in `src/core/ops`, file persistence in `src/core/atomic.rs`, and batch planning in `src/apply`. CLI and MCP are separate frontends to these shared operations. See [architecture](docs/architecture.md) for contracts and remaining limits.

Include focused regression coverage for changed edit semantics, especially CRLF, unterminated EOF, empty files, preconditions, backups and failures before commit. Exercise both CLI and MCP when their inputs or responses change. Avoid process-wide changes such as switching the working directory inside parallel unit tests; use child-process working directories instead.

Do not relax safety checks or dependency advisories merely to make CI green. Macro-generated lint exceptions should stay scoped and explained. New dependencies must support the declared Rust minimum and pass `cargo deny check`.

## Documentation

Update both READMEs for installation, compatibility and user-visible behavior. Detailed references live in `docs/`; `PLAN.md` is a historical proposal, not the current contract.

After changing the agent guide or references, run:

```sh
cargo run --locked -p xtask -- sync-integrations
```

This generates the generic and GitHub skills and copied references. Other integration templates are maintained in their own styles; keep their command examples consistent with the CLI. Add changes to the Unreleased section of `CHANGELOG.md`.

## Pull requests and releases

Explain the original problem, new behavior, validation performed and compatibility changes. CI checks formatting, strict Clippy, all tests on Linux/macOS/Windows, minimum Rust, dependency advisories and package/integration consistency. Fix failures before merging. Maintainers publish using the [release procedure](docs/releasing.md).
