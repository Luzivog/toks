# Repository Guidelines

## Project Structure & Module Organization

Tokscope is a Rust 2021 workspace. `crates/tokscope/` is the GPUI desktop app,
`crates/tokscope-core/` owns account, limit, and history behavior, and
`crates/tokscope-ingest/` contains the repository-owned scanning, parsing,
caching, and pricing engine. Integration tests live under each crate's `tests/`
directory; focused unit tests may sit beside their module. Desktop assets are in
`assets/`, repository checks in `scripts/quality/`, and CI in `.github/workflows/`.

## Build, Test, and Development Commands

- `cargo run -p tokscope --locked` starts the Linux desktop app.
- `cargo run -p tokscope-core --bin tokscope-dump --locked -- history` prints a
  headless JSON snapshot; use `limits` for account limits.
- `cargo test --workspace --all-targets --locked` runs the test suite.
- `cargo fmt --all --check` verifies formatting.
- `cargo clippy --workspace --all-targets --locked -- -D warnings` runs linting.
- `cargo build --workspace --release --locked` creates release artifacts.
- `scripts/quality/check-repository.sh` checks source-size, import, and
  portability rules.

Install the Linux packages listed in `README.md`; do not add machine-specific
linker paths or home directories to repository files.

## Coding Style & Naming Conventions

Use default `rustfmt` formatting and four-space indentation. Follow Rust naming:
`snake_case` modules/functions/tests, `CamelCase` types, and
`SCREAMING_SNAKE_CASE` constants. Prefer typed structs and enums at crate seams.
Keep hand-written production files below 200 lines and test files below 500.
Split by coherent responsibility, not arbitrary line ranges. Avoid internal
wildcard imports and re-exports; generated exceptions require both a generated
path/name and an explicit generated-file marker. The imported ingest parser
foundation has an exact CI debt ratchet; those entries may shrink, never grow.

## Testing Guidelines

Use descriptive tests such as `codex_windows_discovered_structurally`. Build
fixtures from temporary directories and synthetic JSON, never real credentials
or home-directory state. Parser changes should cover current, legacy, malformed,
and unknown-schema inputs. Test behavior through the module's public interface.

## Commits, Pull Requests, and Security

Use short, sentence-case commit summaries and keep commits focused. Pull
requests should describe user-visible behavior, list validation commands, link
issues, and include screenshots for GPUI changes. Call out filesystem, cache,
credential, or network changes. Never log or commit tokens, account identifiers,
rollout contents, or local cache files. Preserve the Tokscale MIT attribution in
`NOTICE` and `crates/tokscope-ingest/LICENSE`.
