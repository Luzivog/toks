# Repository Guidelines

## Project Structure & Module Organization

Toks is a Rust workspace. `crates/toks/` is the GPUI app,
`crates/toks-core/` owns account, limit, and history behavior, and
`crates/toks-ingest/` contains repository-owned parsing, caching, and
pricing. Tests live beside modules or under each crate's `tests/`; desktop
assets are in `assets/`, and repository checks are in `scripts/quality/`.

## Build, Test, and Development Commands

- `cargo run -p toks --locked` starts the Linux desktop app.
- `cargo run -p toks-core --bin toks-dump --locked -- history` prints a
  headless snapshot; use `limits` for account limits.
- `cargo check --locked -p toks` is the default UI compile check.
- `cargo test --locked -p toks --lib <filter>` runs focused unit tests.
- `cargo test --locked -p toks --test <target> <filter>` runs one integration
  target with only required features.
- `cargo clippy --locked -p toks --lib -- -D warnings` lints the app.
- `cargo fmt --all --check` verifies formatting.
- `scripts/quality/check-repository.sh` checks source-size, import, and
  portability rules.

Run workspace tests and Clippy once as final gates, not after each edit. Build
release only for release, installation, or packaging. Before broad/all-target or
release-LTO commands, check free disk and active Cargo, rustc, and linker work;
wait instead of competing or writing into unsafe free space. Keep Cargo's native
parallelism and iterative incremental compilation. Use `CARGO_INCREMENTAL=0`
only for a coordinated one-shot gate, and `--profile debugging` only for full
debug symbols. Without explicit approval, do not clean/delete target artifacts,
change a shared target directory, or add build caches/linkers. Never add
machine-specific linker paths or home directories.

## Coding Style & Naming Conventions

Use `rustfmt` and Rust naming: `snake_case` items, `CamelCase` types,
and `SCREAMING_SNAKE_CASE` constants. Prefer typed structs and enums at crate
seams. Keep hand-written production files below 200 lines and tests below 500;
split by responsibility. Avoid internal wildcard imports/re-exports. Generated
exceptions require a generated path/name and marker. Existing ingest debt is
ratcheted: entries may shrink, never grow.
Use Rust 2018 module roots: `foo.rs` with an optional `foo/` directory; do not
add `mod.rs` files. Put a module's tests in sibling `foo_tests.rs`, or use
`foo/tests.rs` plus `foo/tests/*.rs` for multi-file suites; do not define inline
test modules in production files.

## Testing Guidelines

Use descriptive names such as `codex_windows_discovered_structurally`. Use
temporary directories and synthetic JSON, never real credentials. Cover
current, legacy, malformed, and unknown parser inputs through public interfaces.
Group GPUI tests when process sharing is safe; isolate process-global mutation.

## Implementation completion

For each implementation request, complete the full delivery path unless the
user sets a narrower stopping point such as local-only, no commit, no push, or
no install:

1. Run the focused checks, then the repository's final gates.
2. Commit every task-scoped change. Keep unrelated pre-existing changes out of
   the commit.
3. Integrate the commit into local `main`, then push `main` to `origin`.
4. Verify that local `main` and remote `origin/main` resolve to the same commit.
5. Build the release binaries and run `./install.sh`.
6. Restart Toks and each affected service. Verify that the installed and
   running executable hashes match the release binaries, then exercise the
   changed behavior through its user-facing path.

The task is complete only when GitHub `main`, the local `main` checkout, the
installed application, and the running application contain the requested
change. If any delivery step is blocked, report the blocker and leave the
completed work intact for retry.

## Commits, Pull Requests, and Security

Use focused, sentence-case commits. Pull requests should explain visible
behavior, validation, linked issues, and include screenshots for GPUI changes.
Call out filesystem, cache, credential, or network changes. Never commit tokens,
account identifiers, rollout contents, or local caches. Preserve Tokscale MIT
attribution in `NOTICE` and `crates/toks-ingest/LICENSE`.
