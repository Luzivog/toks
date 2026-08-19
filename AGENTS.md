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

## Testing Guidelines

Use descriptive names such as `codex_windows_discovered_structurally`. Use
temporary directories and synthetic JSON, never real credentials. Cover
current, legacy, malformed, and unknown parser inputs through public interfaces.
Group GPUI tests when process sharing is safe; isolate process-global mutation.

## Commits, Pull Requests, and Security

Use focused, sentence-case commits. Pull requests should explain visible
behavior, validation, linked issues, and include screenshots for GPUI changes.
Call out filesystem, cache, credential, or network changes. Never commit tokens,
account identifiers, rollout contents, or local caches. Preserve Tokscale MIT
attribution in `NOTICE` and `crates/toks-ingest/LICENSE`.
