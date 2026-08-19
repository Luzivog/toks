CREATE TABLE IF NOT EXISTS events (
  event_id TEXT PRIMARY KEY,
  identity_scheme TEXT NOT NULL,
  identity_version INTEGER NOT NULL,
  confidence INTEGER NOT NULL CHECK (confidence BETWEEN 0 AND 2),
  canonical_fact_hash TEXT NOT NULL,
  canonical_accounting_hash TEXT NOT NULL,
  conflicted INTEGER NOT NULL DEFAULT 0 CHECK (conflicted IN (0, 1)),
  client TEXT NOT NULL,
  provider TEXT NOT NULL,
  model TEXT NOT NULL,
  timestamp_ms INTEGER NOT NULL,
  input_tokens INTEGER NOT NULL CHECK (input_tokens >= 0),
  output_tokens INTEGER NOT NULL CHECK (output_tokens >= 0),
  cache_read_tokens INTEGER NOT NULL CHECK (cache_read_tokens >= 0),
  cache_write_tokens INTEGER NOT NULL CHECK (cache_write_tokens >= 0),
  reasoning_tokens INTEGER NOT NULL CHECK (reasoning_tokens >= 0),
  duration_ms INTEGER,
  message_count INTEGER NOT NULL CHECK (message_count >= 0),
  is_turn_start INTEGER NOT NULL CHECK (is_turn_start IN (0, 1)),
  model_conflicted INTEGER NOT NULL CHECK (model_conflicted IN (0, 1)),
  cost_nanos INTEGER NOT NULL CHECK (cost_nanos >= 0),
  cost_source INTEGER NOT NULL CHECK (cost_source IN (0, 2))
) STRICT;

CREATE TABLE IF NOT EXISTS identities (
  identity_hash TEXT PRIMARY KEY,
  scheme TEXT NOT NULL,
  version INTEGER NOT NULL,
  confidence INTEGER NOT NULL CHECK (confidence BETWEEN 0 AND 2),
  event_id TEXT NOT NULL REFERENCES events(event_id) ON DELETE CASCADE
) STRICT;

CREATE TABLE IF NOT EXISTS accounting_aliases (
  alias_hash TEXT PRIMARY KEY,
  scheme TEXT NOT NULL,
  version INTEGER NOT NULL,
  event_id TEXT NOT NULL REFERENCES events(event_id) ON DELETE CASCADE,
  conflicted INTEGER NOT NULL DEFAULT 0 CHECK (conflicted IN (0, 1))
) STRICT;

CREATE TABLE IF NOT EXISTS event_revisions (
  event_id TEXT NOT NULL REFERENCES events(event_id) ON DELETE CASCADE,
  fact_hash TEXT NOT NULL,
  accounting_hash TEXT NOT NULL,
  accounting_projection_version INTEGER NOT NULL,
  client TEXT NOT NULL,
  provider TEXT NOT NULL,
  model TEXT NOT NULL,
  timestamp_ms INTEGER NOT NULL,
  input_tokens INTEGER NOT NULL,
  output_tokens INTEGER NOT NULL,
  cache_read_tokens INTEGER NOT NULL,
  cache_write_tokens INTEGER NOT NULL,
  reasoning_tokens INTEGER NOT NULL,
  duration_ms INTEGER,
  message_count INTEGER NOT NULL,
  is_turn_start INTEGER NOT NULL,
  model_conflicted INTEGER NOT NULL,
  cost_nanos INTEGER NOT NULL,
  cost_source INTEGER NOT NULL,
  first_observed_generation INTEGER NOT NULL CHECK (first_observed_generation > 0),
  PRIMARY KEY (event_id, fact_hash)
) STRICT;

CREATE TABLE IF NOT EXISTS event_sources (
  event_id TEXT NOT NULL REFERENCES events(event_id) ON DELETE CASCADE,
  source_hash TEXT NOT NULL,
  first_seen_generation INTEGER NOT NULL CHECK (first_seen_generation > 0),
  last_seen_generation INTEGER NOT NULL CHECK (last_seen_generation > 0),
  PRIMARY KEY (event_id, source_hash)
) STRICT;

CREATE TABLE IF NOT EXISTS archive_state (
  singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
  captured_since_ms INTEGER NOT NULL,
  captured_through_ms INTEGER NOT NULL
) STRICT;

CREATE TABLE IF NOT EXISTS archive_clock (
  singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
  last_scan_generation INTEGER NOT NULL CHECK (last_scan_generation > 0)
) STRICT;

CREATE TABLE IF NOT EXISTS archive_pending (
  singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
  scan_hash TEXT NOT NULL,
  scan_generation INTEGER NOT NULL CHECK (scan_generation > 0)
) STRICT;

-- A source checkpoint is committed in the same transaction as every event
-- accepted from that source. Raw source names and revisions never enter the
-- archive; callers provide transient values that are domain-separated and
-- hashed before these rows are written.
CREATE TABLE IF NOT EXISTS source_checkpoints (
  source_hash TEXT PRIMARY KEY,
  revision_hash TEXT NOT NULL,
  captured_through_ms INTEGER NOT NULL,
  backfill_complete INTEGER NOT NULL CHECK (backfill_complete IN (0, 1))
) STRICT;

-- Explicit user exclusions are durable intent, unlike provider-log deletion.
-- Keeping only timestamps avoids persisting transcript or account metadata.
CREATE TABLE IF NOT EXISTS forgotten_ranges (
  start_ms INTEGER NOT NULL,
  end_ms INTEGER NOT NULL CHECK (end_ms > start_ms),
  PRIMARY KEY (start_ms, end_ms)
) STRICT;

-- projection_events is the durable before-image for rollup maintenance. It
-- lets v2 migration and live reconciliation interleave without double adding
-- an event, even when a canonical revision changes during migration.
CREATE TABLE IF NOT EXISTS projection_events (
  event_id TEXT PRIMARY KEY REFERENCES events(event_id) ON DELETE CASCADE,
  fact_hash TEXT NOT NULL,
  accounting_projection_version INTEGER NOT NULL,
  client TEXT NOT NULL,
  provider TEXT NOT NULL,
  model TEXT NOT NULL,
  timestamp_ms INTEGER NOT NULL,
  input_tokens INTEGER NOT NULL,
  output_tokens INTEGER NOT NULL,
  cache_read_tokens INTEGER NOT NULL,
  cache_write_tokens INTEGER NOT NULL,
  reasoning_tokens INTEGER NOT NULL,
  message_count INTEGER NOT NULL,
  is_turn_start INTEGER NOT NULL,
  confidence INTEGER NOT NULL CHECK (confidence BETWEEN 0 AND 2),
  conflicted INTEGER NOT NULL CHECK (conflicted IN (0, 1)),
  cost_source INTEGER NOT NULL CHECK (cost_source IN (0, 2)),
  cost_nanos INTEGER NOT NULL
) STRICT;

CREATE TABLE IF NOT EXISTS usage_rollups (
  period INTEGER NOT NULL CHECK (period BETWEEN 0 AND 1),
  bucket_start_ms INTEGER NOT NULL,
  client TEXT NOT NULL,
  provider TEXT NOT NULL,
  model TEXT NOT NULL,
  cost_source INTEGER NOT NULL CHECK (cost_source IN (0, 2)),
  long_context INTEGER NOT NULL CHECK (long_context IN (0, 1)),
  input_tokens INTEGER NOT NULL,
  output_tokens INTEGER NOT NULL,
  cache_read_tokens INTEGER NOT NULL,
  cache_write_tokens INTEGER NOT NULL,
  reasoning_tokens INTEGER NOT NULL,
  message_count INTEGER NOT NULL,
  turn_count INTEGER NOT NULL,
  cost_nanos INTEGER NOT NULL,
  event_count INTEGER NOT NULL,
  input_b0 INTEGER NOT NULL, input_b1 INTEGER NOT NULL, input_b2 INTEGER NOT NULL,
  input_b3 INTEGER NOT NULL, input_b4 INTEGER NOT NULL,
  output_b0 INTEGER NOT NULL, output_b1 INTEGER NOT NULL, output_b2 INTEGER NOT NULL,
  output_b3 INTEGER NOT NULL, output_b4 INTEGER NOT NULL,
  cache_read_b0 INTEGER NOT NULL, cache_read_b1 INTEGER NOT NULL,
  cache_read_b2 INTEGER NOT NULL,
  cache_write_b0 INTEGER NOT NULL, cache_write_b1 INTEGER NOT NULL,
  PRIMARY KEY (
    period, bucket_start_ms, client, provider, model, cost_source, long_context
  )
) STRICT;

CREATE TABLE IF NOT EXISTS projection_state (
  singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
  complete INTEGER NOT NULL CHECK (complete IN (0, 1)),
  strong_events INTEGER NOT NULL,
  weak_events INTEGER NOT NULL,
  conflicts INTEGER NOT NULL
) STRICT;

INSERT INTO projection_state
SELECT 1, CASE WHEN EXISTS (SELECT 1 FROM events) THEN 0 ELSE 1 END, 0, 0, 0
WHERE NOT EXISTS (SELECT 1 FROM projection_state WHERE singleton = 1);

CREATE INDEX IF NOT EXISTS revisions_by_event ON event_revisions(event_id);
CREATE INDEX IF NOT EXISTS sources_by_event ON event_sources(event_id);
CREATE INDEX IF NOT EXISTS revisions_by_accounting
  ON event_revisions(accounting_hash, event_id);
CREATE INDEX IF NOT EXISTS sources_by_source
  ON event_sources(source_hash, event_id);
CREATE INDEX IF NOT EXISTS projection_events_by_fact
  ON projection_events(fact_hash, event_id);
