INSERT INTO usage_rollups (
  period, bucket_start_ms, client, provider, model, cost_source, long_context,
  input_tokens, output_tokens, cache_read_tokens, cache_write_tokens,
  reasoning_tokens, message_count, turn_count, cost_nanos, event_count,
  input_b0, input_b1, input_b2, input_b3, input_b4,
  output_b0, output_b1, output_b2, output_b3, output_b4,
  cache_read_b0, cache_read_b1, cache_read_b2, cache_write_b0, cache_write_b1
)
WITH pending AS (
  SELECT e.*, r.accounting_projection_version FROM events e
  JOIN event_revisions r
    ON r.event_id=e.event_id AND r.fact_hash=e.canonical_fact_hash
  LEFT JOIN projection_events p ON p.event_id=e.event_id
  WHERE p.event_id IS NULL
), periods(period) AS (VALUES (0), (1)), priced AS (
  SELECT *,
    CASE WHEN input_tokens + cache_read_tokens + cache_write_tokens > 272000
      THEN 1 ELSE 0 END AS long_context,
    CASE
      WHEN client='codex' AND accounting_projection_version<2
        THEN MAX(output_tokens-reasoning_tokens, 0)
      ELSE output_tokens
    END AS normalized_output
  FROM pending
), normalized AS (
  SELECT *, normalized_output + reasoning_tokens AS priced_output FROM priced
)
SELECT
  period,
  CASE period WHEN 0 THEN 0 ELSE (timestamp_ms / 60000) * 60000 END,
  client, provider, model, cost_source, long_context,
  SUM(input_tokens), SUM(normalized_output), SUM(cache_read_tokens),
  SUM(cache_write_tokens), SUM(reasoning_tokens), SUM(message_count),
  SUM(is_turn_start), SUM(cost_nanos), COUNT(*),
  SUM(MIN(input_tokens, 128000)),
  SUM(MAX(MIN(input_tokens, 200000) - 128000, 0)),
  SUM(MAX(MIN(input_tokens, 256000) - 200000, 0)),
  SUM(MAX(MIN(input_tokens, 272000) - 256000, 0)),
  SUM(MAX(input_tokens - 272000, 0)),
  SUM(MIN(priced_output, 128000)),
  SUM(MAX(MIN(priced_output, 200000) - 128000, 0)),
  SUM(MAX(MIN(priced_output, 256000) - 200000, 0)),
  SUM(MAX(MIN(priced_output, 272000) - 256000, 0)),
  SUM(MAX(priced_output - 272000, 0)),
  SUM(MIN(cache_read_tokens, 200000)),
  SUM(MAX(MIN(cache_read_tokens, 272000) - 200000, 0)),
  SUM(MAX(cache_read_tokens - 272000, 0)),
  SUM(MIN(cache_write_tokens, 200000)),
  SUM(MAX(cache_write_tokens - 200000, 0))
FROM normalized CROSS JOIN periods
GROUP BY period, 2, client, provider, model, cost_source, long_context
ON CONFLICT(
  period, bucket_start_ms, client, provider, model, cost_source, long_context
)
DO UPDATE SET
  input_tokens=input_tokens+excluded.input_tokens,
  output_tokens=output_tokens+excluded.output_tokens,
  cache_read_tokens=cache_read_tokens+excluded.cache_read_tokens,
  cache_write_tokens=cache_write_tokens+excluded.cache_write_tokens,
  reasoning_tokens=reasoning_tokens+excluded.reasoning_tokens,
  message_count=message_count+excluded.message_count,
  turn_count=turn_count+excluded.turn_count,
  cost_nanos=cost_nanos+excluded.cost_nanos,
  event_count=event_count+excluded.event_count,
  input_b0=input_b0+excluded.input_b0, input_b1=input_b1+excluded.input_b1,
  input_b2=input_b2+excluded.input_b2, input_b3=input_b3+excluded.input_b3,
  input_b4=input_b4+excluded.input_b4,
  output_b0=output_b0+excluded.output_b0, output_b1=output_b1+excluded.output_b1,
  output_b2=output_b2+excluded.output_b2, output_b3=output_b3+excluded.output_b3,
  output_b4=output_b4+excluded.output_b4,
  cache_read_b0=cache_read_b0+excluded.cache_read_b0,
  cache_read_b1=cache_read_b1+excluded.cache_read_b1,
  cache_read_b2=cache_read_b2+excluded.cache_read_b2,
  cache_write_b0=cache_write_b0+excluded.cache_write_b0,
  cache_write_b1=cache_write_b1+excluded.cache_write_b1;

UPDATE projection_state SET
  strong_events=strong_events + (
    SELECT COUNT(*) FROM events e LEFT JOIN projection_events p ON p.event_id=e.event_id
    WHERE p.event_id IS NULL AND e.confidence=2
  ),
  weak_events=weak_events + (
    SELECT COUNT(*) FROM events e LEFT JOIN projection_events p ON p.event_id=e.event_id
    WHERE p.event_id IS NULL AND e.confidence<2
  ),
  conflicts=conflicts + (
    SELECT COUNT(*) FROM events e LEFT JOIN projection_events p ON p.event_id=e.event_id
    WHERE p.event_id IS NULL AND e.conflicted=1
  )
WHERE singleton=1;

INSERT INTO projection_events
SELECT e.event_id, e.canonical_fact_hash, r.accounting_projection_version,
  e.client, e.provider, e.model,
  e.timestamp_ms, e.input_tokens, e.output_tokens, e.cache_read_tokens,
  e.cache_write_tokens, e.reasoning_tokens, e.message_count, e.is_turn_start,
  e.confidence, e.conflicted, e.cost_source, e.cost_nanos
FROM events e
JOIN event_revisions r
  ON r.event_id=e.event_id AND r.fact_hash=e.canonical_fact_hash
LEFT JOIN projection_events p ON p.event_id=e.event_id
WHERE p.event_id IS NULL;

UPDATE projection_state SET complete=1 WHERE singleton=1;
