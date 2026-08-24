use crate::clients::ClientId;

pub(crate) fn parser_version(client: ClientId) -> u32 {
    match client {
        // These clients accumulated parser-only invalidations under the old
        // global schema. Their independent counters start from those histories
        // so future changes have an obvious local version to increment.
        // v6->v7: token_count rows now carry source-native durable accounting
        // identities and fork-replay aliases, and Codex incremental state
        // retains identity occurrence counters. Existing Codex shards must
        // reparse to seed all three.
        // v7->v8: compact the occurrence tracker wire shape. Existing v7
        // bincode shards are stale because bincode cannot migrate an untagged
        // map representation safely; JSON accounting checkpoints migrate.
        // v8->v9: session scans now recover after invalid UTF-8.
        ClientId::Codex => 9,
        // v4->v5: jcode's assistant-message timestamp is now back-calculated
        // to the turn start (timestamp - tool_duration_ms) instead of using
        // the recorded (end-anchored) timestamp directly. Follow-up to #890.
        // v5->v6: OpenAI-style Jcode usage now removes cache-read overlap from
        // input_tokens before pricing and aggregation.
        // v6->v7: snapshot and journal message arrays are now parsed
        // leniently (a single wrong-typed token_usage no longer drops the
        // whole session or its journal line), and a journal replay of an
        // already-seen user message id no longer re-arms pending_turn_start
        // and mints a spurious turn.
        // v7->v8: journal scans now recover after invalid UTF-8.
        ClientId::Jcode => 8,
        // v5->v6: merge same-dedup-key Copilot spans before emitting messages.
        // v6->v7: all-zero trace/span ids (the W3C sentinel for "no recording
        // span context") are now treated as absent instead of as a real,
        // shared identity, and a valid span_id alone (no trace_id) is now a
        // stable dedup key instead of falling through to the line-index key.
        // v7->v8: stabilize duplicate agent attribution and partial timing boundaries.
        // v8->v9: scans now recover useful records containing invalid UTF-8.
        ClientId::Copilot => 9,
        // Pi subagent sessions derive agent attribution from session_info names;
        // version-1 caches carry those messages without agent metadata. Kimchi
        // v2 added namespaced dedup keys. Their shared reader now decodes
        // invalid UTF-8 lossily, so both advance to v3.
        ClientId::Pi | ClientId::Kimchi => 3,
        // Devin CLI v1 could stop at a malformed chat_message. v2->v3:
        // message timestamp is now back-calculated to the turn start
        // (created_at - total_time_ms) instead of the recorded (end-anchored)
        // created_at. Follow-up to #890.
        ClientId::DevinCli => 3,
        // Desktop v1 parsed a non-ACP shape and did not track its CLI title
        // lookup. v2->v3: scans now recover after invalid UTF-8.
        ClientId::DevinDesktop => 3,
        // WARNING — bumping this discards data that is not recoverable by
        // re-parsing. Claude Code rewrites a transcript in place on
        // resume/compact, and since #994 a Claude entry deliberately carries
        // assistant turns the live file no longer contains (see
        // `HistoryRetention::RetainObserved`). A bump drops every entry, and
        // the cold rebuild that follows sees only the compacted file — so it
        // silently retires those turns from every user's totals and
        // reintroduces the exact drift #994 reported. The lossy parent-agent
        // lookup changes attribution, not token totals, so keep v2 and retain
        // observed turns; fresh or changed sources receive the new attribution.
        ClientId::Claude => 2,
        // Junie's usage-event timestamp is now back-calculated to the call
        // start (timestampMs - usage.time) instead of the recorded
        // (end-anchored) timestampMs. Follow-up to #890. v2->v3: preserve
        // provider-reported cost provenance, including explicit zeroes, so
        // strict submission does not reject valid cached unknown-model usage.
        // v3->v4: scans now recover after invalid UTF-8.
        ClientId::Junie => 4,
        // zcode's model_usage timestamp now prefers `started_at` over
        // `completed_at`. Follow-up to #890. v2->v3: rows with a NULL
        // `started_at` now back-calculate `completed_at - duration_ms`
        // instead of staying end-anchored at `completed_at`, and
        // `is_turn_start` is now assigned to the earliest-STARTED request
        // per turn instead of the first one seen in completed_at order.
        // Second-round follow-up to #890.
        // v3->v4: scans now recover after invalid UTF-8.
        ClientId::Zcode => 4,
        // opencodereview's llm_response timestamp is now back-calculated to
        // the call start (timestamp - duration_ms) instead of the recorded
        // (end-anchored) timestamp. Follow-up to #890. v2->v3: records without
        // their own `timestamp` now carry a line-number discriminator in the
        // dedup key, so distinct calls sharing a model and token counts no
        // longer collapse under the shared file-mtime fallback. v3->v4:
        // scans now recover after invalid UTF-8.
        ClientId::OpenCodeReview => 4,
        // Kiro's structured messages.jsonl turns now back-calculate the
        // start anchor from `turn_end - elapsedTime` when the user prompt's
        // own timestamp is missing/unparseable, instead of falling through
        // to the (end-anchored) turn_end timestamp. Second-round follow-up
        // to #890. v2->v3: message scans recover after invalid UTF-8.
        ClientId::Kiro => 3,
        // Kimi v2 checks token buckets without an overflowing sum. v2->v3:
        // symbolic usage-record models now resolve from the latest llm.request.
        // v3->v4: non-positive wire timestamps (kimi-cli `timestamp`,
        // kimi-code `time`) now fall back to the file mtime instead of
        // anchoring the message in a pre-epoch bucket. v4->v5 recovers invalid UTF-8.
        ClientId::Kimi => 5,
        // v1->v2: standalone Cline messages subtract cache buckets from gross
        // input tokens, reject non-finite costs, and preserve zero-cost reports.
        // v2->v3: content-aware Cline CLI turn-start classification now
        // recognizes user tool-result records as continuations instead of
        // beginning a new turn, so cached turns must be reparsed.
        ClientId::Cline => 3,
        // v1->v2: per-model token attribution now comes from
        // session_model_usage instead of crediting the whole session to
        // sessions.model, and dedup keys are namespaced per (session, model).
        ClientId::Hermes => 2,
        // v2 added per-turn usage records. v3 adds the canonical unified log,
        // non-overlapping output/cache/reasoning buckets, and session metadata.
        // v4 scopes unified model attribution by PID generation and exact child
        // session, so the same source can now produce different model IDs.
        // v5 preserves distinct unified events when timestamps and token
        // buckets repeat, and fingerprints the complete sessions metadata tree.
        // v6 persists whether an unknown unified model was deliberately
        // fail-closed due to conflicting child attribution evidence.
        // v6->v7: session files are now parsed past undecodable lines instead
        // of stopping at the first one, and usage dedup keys carry the record's
        // file position. Both change the parse of byte-identical input, so
        // cached entries hold truncated and under-deduplicated output (#1031).
        ClientId::Grok => 7,
        // v1 retained MiMo's embedded `cost` value but did not preserve its
        // provider-reported provenance. Reparse cached rows so strict submit
        // validation does not reject valid unknown-model MiMo usage offline.
        // v2->v3: duplicate merging now upgrades the retained row when a later
        // copy carries an explicit cost, including zero.
        ClientId::MiMoCode => 3,
        // v1->v2: file-backed line readers recover after invalid UTF-8.
        // Reasonix v1 introduced sampled append-only fingerprints; CommandCode
        // also treats empty string content as zero tokens.
        ClientId::Droid
        | ClientId::OpenClaw
        | ClientId::Qwen
        | ClientId::Gjc
        | ClientId::CommandCode
        | ClientId::CodeBuddy
        | ClientId::WorkBuddy
        | ClientId::Senpi
        | ClientId::Reasonix
        | ClientId::PrimeAgent => 2,
        _ => 1,
    }
}
