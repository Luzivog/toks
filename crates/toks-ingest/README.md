# Toks Ingest

First-party workspace crate for local Claude Code and Codex discovery, parsing,
deduplication, incremental source caching, and API-rate cost estimation. Its
runtime state stays in the established backward-compatible local namespace, and
it has no Tokscale installation, process, repository, or cache dependency.

The parser foundation was originally derived from Tokscale v4.13.0
(`94a02774`) under the MIT license. Toks now owns this fork and its product
surface; retain `LICENSE` and review any deliberately imported upstream change.
