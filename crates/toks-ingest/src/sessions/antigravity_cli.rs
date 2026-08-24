//! Antigravity CLI session parser
//!
//! The Antigravity CLI (the terminal agent, distinct from the Antigravity IDE)
//! stores each conversation as a SQLite database under
//! `~/.gemini/antigravity-cli/conversations/<uuid>.db`. Unlike the IDE-backed
//! [`super::antigravity`] source — which depends on a *running* language server
//! reachable over RPC and caches JSONL under the config dir — the CLI usage is
//! already on disk and can be read directly. No RPC, no `antigravity sync`.
//!
//! Each `gen_metadata` row is one generation encoded as the same
//! `GeneratorMetadata` protobuf the IDE returns over
//! `GetCascadeTrajectoryGeneratorMetadata`. The repository has no `.proto` /
//! prost decoder (the IDE path receives JSON because the language server does
//! the proto→JSON conversion), so this module ships a tiny wire-format reader
//! and pulls only the few fields it needs. The field numbers below were
//! reverse-engineered from real databases and cross-checked across 6 sessions
//! / 140 turns (`#9 + #10 == #3`, i.e. output + thinking == total output;
//! `#5`/cacheRead only appears once a cached prefix exists and grows with the
//! conversation):
//!
//! - `gen_metadata.#1`            → chatModel message
//!   - `#19` (string, optional)  → responseModel (e.g. `gemini-3-flash-a`)
//!   - `#21` (string, optional)  → model display label (`Gemini 3.6 Flash (High)`)
//!   - `#9.#4` = `{#1: seconds, #2: nanos}` → per-generation wall-clock time
//!   - `#4`                      → usage message
//!     - `#1` (varint, const)    → fixed system-prompt tokens (≈1132)
//!     - `#2` (varint)           → newly-processed (non-cached) input tokens
//!     - `#5` (varint)           → cacheRead tokens
//!     - `#9` (varint)           → output (text) tokens
//!     - `#10` (varint)          → thinking / reasoning tokens
//!     - `#11` (string)          → responseId (dedup key)
//! - `trajectory_metadata_blob.#2` = `{#1: seconds, #2: nanos}` → created-at
//! - `trajectory_metadata_blob.#1.#1` (string)                  → workspace URI
//!
//! `#19` is optional in practice: some continuation turns omit it while still
//! writing `#21`. [`SessionModels`] recovers the machine id for those rows from
//! the rest of the same conversation. `#21` was present on every row observed so
//! far, including the ones missing `#19`, but nothing here requires it — a row
//! carrying neither field is handled too. `#21` serves only as a join key
//! between rows of one file and is never used as a pricing key: it is a
//! server-supplied name that gets renamed (`Gemini 3 Flash` → `Gemini 3.5 Flash
//! (High)`) and could be localized.

use super::utils::open_readonly_sqlite;
use super::{normalize_workspace_key, workspace_label_from_key, UnifiedMessage};
use crate::{pricing, provider_identity, TokenBreakdown};
use rusqlite::Connection;
use std::collections::{HashMap, HashSet};
use std::path::Path;

pub fn parse_antigravity_cli_file(path: &Path) -> Vec<UnifiedMessage> {
    let Some(conn) = open_readonly_sqlite(path) else {
        return Vec::new();
    };

    let session_id = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("unknown")
        .to_string();

    let (timestamp, workspace_key, workspace_label) = read_trajectory_meta(&conn, path);

    let mut stmt = match conn.prepare("SELECT data FROM gen_metadata ORDER BY idx") {
        Ok(stmt) => stmt,
        // Not an Antigravity CLI database (table missing) — nothing to count.
        Err(_) => return Vec::new(),
    };
    let rows = match stmt.query_map([], |row| row.get::<_, Vec<u8>>(0)) {
        Ok(rows) => rows,
        Err(_) => return Vec::new(),
    };

    // Buffered rather than streamed so a row missing its own `#19` can borrow
    // attribution from anywhere in the conversation, not just from rows that
    // happen to precede it.
    let blobs: Vec<Vec<u8>> = rows.flatten().collect();
    let session_models = SessionModels::from_blobs(&blobs);

    let mut messages = Vec::new();
    let mut seen_response_ids: HashSet<String> = HashSet::new();
    for blob in &blobs {
        // `timestamp` is the session-created fallback; each row prefers its own
        // per-generation wall-clock stamp (see `parse_gen_metadata`).
        if let Some(mut message) = parse_gen_metadata(
            blob,
            &session_id,
            timestamp,
            &session_models,
            &mut seen_response_ids,
        ) {
            if workspace_key.is_some() {
                message.set_workspace(workspace_key.clone(), workspace_label.clone());
            }
            messages.push(message);
        }
    }

    messages
}

/// Model attribution recovered from the conversation as a whole, for rows whose
/// `chatModel.#19` (responseModel) is missing.
///
/// Antigravity CLI leaves `#19` out of some generations — observed on
/// continuation / tool turns, which also carry a large `cacheRead` and a tiny
/// output — while still writing the `#21` display label. Those rows are not
/// information-poor: sibling rows in the same database carry the machine id for
/// the very same label, so the id is recoverable from the file itself. Without
/// this the rows resolve to `antigravity/unknown`, which has no price, and a
/// single such row aborts the whole submission.
///
/// The label is only ever a join key between rows of one file; the value handed
/// back is always a `#19` machine id observed in that same file. Display labels
/// are never used as pricing keys — see the alias table's note on renamed and
/// localizable labels.
#[derive(Default)]
struct SessionModels {
    /// `#21` display label → the `#19` machine id seen alongside it. A label
    /// that appears with two ids naming *different* priced models is dropped:
    /// an ambiguous label is no better evidence than no label at all. Ids that
    /// differ only in spelling but share an alias target (Antigravity's
    /// `gemini-pro-default` / `gemini-pro-agent`) are not ambiguous, and the
    /// first id observed is kept.
    by_display: HashMap<String, String>,
    /// The file's only `#19` value — set only when every row carrying one
    /// agrees *and* every label in the file was identified by some row. Serves
    /// rows that have neither `#19` nor `#21`.
    sole_model: Option<String>,
}

impl SessionModels {
    fn from_blobs(blobs: &[Vec<u8>]) -> Self {
        let mut by_display: HashMap<&str, Option<&str>> = HashMap::new();
        let mut distinct: HashSet<&str> = HashSet::new();
        let mut unresolved_labels: Vec<&str> = Vec::new();

        for blob in blobs {
            let Some(chat_model) = message_field(blob, 1) else {
                continue;
            };
            let label = non_empty_string_field(chat_model, 21);
            let Some(model) = non_empty_string_field(chat_model, 19) else {
                // Kept so `sole_model` below can tell whether this row was
                // identified by some other row carrying the same label.
                unresolved_labels.extend(label);
                continue;
            };
            distinct.insert(model);
            if let Some(label) = label {
                by_display
                    .entry(label)
                    .and_modify(|resolved| {
                        if let Some(existing) = *resolved {
                            // Antigravity swaps between several `#19` machine ids
                            // under one display label within a single conversation
                            // (`gemini-pro-default` and `gemini-pro-agent` both
                            // appear as "Gemini 3.1 Pro (High)"). Those are the same
                            // priced model, so comparing the raw strings reports a
                            // false ambiguity and drops the label — which later
                            // leaves `#19`-less rows as `unknown` and aborts
                            // `submit`. Compare canonical alias targets instead, so
                            // only a genuinely different model clears the mapping.
                            if existing != model {
                                let existing_canon =
                                    pricing::aliases::resolve_alias(existing).unwrap_or(existing);
                                let new_canon =
                                    pricing::aliases::resolve_alias(model).unwrap_or(model);
                                if existing_canon != new_canon {
                                    *resolved = None;
                                }
                            }
                        }
                    })
                    .or_insert(Some(model));
            }
        }

        let by_display: HashMap<String, String> = by_display
            .into_iter()
            .filter_map(|(label, model)| Some((label.to_string(), model?.to_string())))
            .collect();

        // A label that no row ever identified is proof the conversation ran a
        // model this file never names — one *identified* model is not one
        // model. Counting only the ids would let an unlabelled row inherit the
        // single named id and bill a model switch under the wrong model, so
        // withhold the fallback entirely and let those rows stay `unknown`.
        let every_label_identified = unresolved_labels
            .iter()
            .all(|label| by_display.contains_key(*label));
        let sole_model = match (distinct.len(), every_label_identified) {
            (1, true) => distinct.iter().next().map(|model| (*model).to_string()),
            _ => None,
        };

        Self {
            by_display,
            sole_model,
        }
    }

    /// Best available `#19` for a row that has none of its own.
    fn recover(&self, chat_model: &[u8]) -> Option<&str> {
        match non_empty_string_field(chat_model, 21) {
            // A label joined elsewhere in this file resolves to its machine id.
            // A label that never appeared next to a `#19` is positive evidence
            // of a model this file never identified, so falling through to
            // another row's model would be a guess against the evidence —
            // `unknown` is the honest answer there.
            Some(label) => self.by_display.get(label).map(String::as_str),
            None => self.sole_model.as_deref(),
        }
    }
}

fn parse_gen_metadata(
    blob: &[u8],
    session_id: &str,
    session_timestamp: i64,
    session_models: &SessionModels,
    seen_response_ids: &mut HashSet<String>,
) -> Option<UnifiedMessage> {
    let chat_model = message_field(blob, 1)?;
    let usage = message_field(chat_model, 4)?;

    // Per-generation wall-clock time: `chatModel.#9.#4` is an absolute
    // `{#1: seconds, #2: nanos}` Timestamp for this turn (same shape as the
    // session-created stamp), so each turn is dated when it actually happened
    // rather than at conversation start. Fall back to the session-created
    // `session_timestamp` when the field is absent or zero (older databases or
    // malformed rows).
    let timestamp = message_field(chat_model, 9)
        .and_then(|gen| message_field(gen, 4))
        .and_then(proto_timestamp_ms)
        .filter(|&ms| ms > 0)
        .unwrap_or(session_timestamp);

    // input = fixed system prompt (#1) + newly-processed input (#2). The
    // constant #1 is, to the best of our reverse-engineering, the agent's fixed
    // system prompt and counts as billable input; if an official schema later
    // contradicts this, only the input total needs revisiting.
    // Clamp untrusted u64 varints into i64 (a corrupt/malicious blob could
    // encode a value > i64::MAX, which `as i64` would wrap to a negative count)
    // and combine with saturating_add so totals never overflow.
    let to_i64 = |v: u64| i64::try_from(v).unwrap_or(i64::MAX);
    let input = to_i64(varint_field(usage, 1).unwrap_or(0))
        .saturating_add(to_i64(varint_field(usage, 2).unwrap_or(0)));
    let cache_read = to_i64(varint_field(usage, 5).unwrap_or(0));
    let output = to_i64(varint_field(usage, 9).unwrap_or(0));
    let reasoning = to_i64(varint_field(usage, 10).unwrap_or(0));
    if input == 0 && output == 0 && cache_read == 0 && reasoning == 0 {
        return None;
    }

    let dedup_key = string_field(usage, 11)
        .filter(|text| !text.trim().is_empty())
        .map(|text| text.to_string());
    if let Some(key) = &dedup_key {
        if !seen_response_ids.insert(key.clone()) {
            return None;
        }
    }

    let model_raw = non_empty_string_field(chat_model, 19)
        .or_else(|| session_models.recover(chat_model))
        .unwrap_or("unknown");
    let model_id = pricing::aliases::resolve_alias(model_raw)
        .unwrap_or(model_raw)
        .to_string();
    let provider_id = provider_identity::inferred_provider_from_model(&model_id)
        .unwrap_or("antigravity")
        .to_string();

    Some(UnifiedMessage::new_with_dedup(
        "antigravity-cli",
        model_id,
        provider_id,
        session_id,
        timestamp,
        TokenBreakdown {
            input,
            output,
            cache_read,
            cache_write: 0,
            reasoning,
        },
        0.0,
        dedup_key,
    ))
}

/// Read the session-level created-at timestamp and workspace from the single
/// `trajectory_metadata_blob` row. This timestamp dates the conversation as a
/// whole and is the per-row fallback for any `gen_metadata` row missing its own
/// `#9.#4` wall-clock stamp. Falls back to the file mtime when the blob is
/// absent or undecodable.
fn read_trajectory_meta(conn: &Connection, path: &Path) -> (i64, Option<String>, Option<String>) {
    let blob: Option<Vec<u8>> = conn
        .query_row(
            "SELECT data FROM trajectory_metadata_blob LIMIT 1",
            [],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .ok();

    let mut timestamp = None;
    let mut workspace_key = None;
    let mut workspace_label = None;

    if let Some(blob) = &blob {
        timestamp = session_created_ms(blob).filter(|&ms| ms > 0);

        if let Some(uri) = message_field(blob, 1).and_then(|folder| string_field(folder, 1)) {
            if let Some(path_str) = file_uri_to_path(uri) {
                workspace_key = normalize_workspace_key(&path_str);
                workspace_label = workspace_key.as_deref().and_then(workspace_label_from_key);
            }
        }
    }

    let timestamp = timestamp.unwrap_or_else(|| file_modified_ms(path));
    (timestamp, workspace_key, workspace_label)
}

fn session_created_ms(blob: &[u8]) -> Option<i64> {
    proto_timestamp_ms(message_field(blob, 2)?)
}

/// Decode a protobuf `{#1: seconds, #2: nanos}` Timestamp message to epoch ms.
/// Shared by the session-created stamp and the per-generation `#9.#4` stamp.
///
/// `seconds` is an unbounded wire varint, so a malformed blob can carry a value
/// whose `* 1000` overflows `i64` and panics in debug builds. Use checked
/// arithmetic and return `None` on overflow to keep the module's
/// "malformed data degrades to `None`, never panics" contract.
///
/// `nanos` is range-validated against the protobuf Timestamp spec (must be
/// `0..=999_999_999`); an out-of-range or negative `nanos` marks the whole
/// stamp as malformed (`None`) so the caller's `ms > 0` filter and
/// session-timestamp fallback take over instead of producing a skewed time.
fn proto_timestamp_ms(ts: &[u8]) -> Option<i64> {
    let seconds = varint_field(ts, 1)? as i64;
    let nanos = i64::try_from(varint_field(ts, 2).unwrap_or(0)).ok()?;
    if !(0..=999_999_999).contains(&nanos) {
        return None;
    }
    seconds.checked_mul(1000)?.checked_add(nanos / 1_000_000)
}

fn file_modified_ms(path: &Path) -> i64 {
    std::fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .map(|time| chrono::DateTime::<chrono::Utc>::from(time).timestamp_millis())
        .unwrap_or(0)
}

/// Convert a `file://` URI to a filesystem path, percent-decoding UTF-8 escapes
/// (workspace paths on cloud drives can be percent-encoded CJK). After the
/// scheme the remainder is `authority + path`; the three shapes RFC 8089 (and
/// Antigravity) produces are handled:
/// - `file:///C:/x`        → `C:/x`            (empty authority, Windows drive: drop the leading slash)
/// - `file:///home/x`      → `/home/x`         (empty authority, POSIX absolute: keep as-is)
/// - `file://host/share/x` → `//host/share/x`  (non-empty authority → UNC: restore the leading `//`)
fn file_uri_to_path(uri: &str) -> Option<String> {
    let decoded = percent_decode(uri.strip_prefix("file://")?);
    let bytes = decoded.as_bytes();
    let path = if bytes.first() == Some(&b'/') {
        // Empty authority. Drop the slash before a Windows drive letter
        // (`/C:/...`); keep POSIX absolute paths untouched.
        if bytes.len() >= 3 && bytes[2] == b':' {
            decoded[1..].to_string()
        } else {
            decoded
        }
    } else {
        // Non-empty authority (`host/share/...`) is a UNC path; restore the
        // leading `//` so `normalize_workspace_key` preserves the UNC prefix
        // instead of collapsing it into the path body.
        format!("//{decoded}")
    };
    Some(path)
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(hi), Some(lo)) = (hex_value(bytes[i + 1]), hex_value(bytes[i + 2])) {
                out.push((hi << 4) | lo);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Minimal protobuf wire-format reader (no prost / schema dependency).
// ---------------------------------------------------------------------------

enum Wire<'a> {
    Varint(u64),
    Len(&'a [u8]),
    Fixed64,
    Fixed32,
}

struct ProtoReader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> ProtoReader<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    fn read_varint(&mut self) -> Option<u64> {
        let mut result: u64 = 0;
        let mut shift = 0u32;
        loop {
            let byte = *self.buf.get(self.pos)?;
            self.pos += 1;
            result |= u64::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return Some(result);
            }
            shift += 7;
            if shift >= 64 {
                return None;
            }
        }
    }

    /// Yield the next `(field_number, value)` pair, or `None` at end-of-buffer
    /// or on a malformed/unsupported wire type. Group wire types (3/4) are
    /// deprecated and never appear here; we stop rather than risk desync.
    fn next_field(&mut self) -> Option<(u64, Wire<'a>)> {
        if self.pos >= self.buf.len() {
            return None;
        }
        let tag = self.read_varint()?;
        let field = tag >> 3;
        let wire = match tag & 0x7 {
            0 => Wire::Varint(self.read_varint()?),
            1 => {
                self.pos = self.pos.checked_add(8).filter(|&p| p <= self.buf.len())?;
                Wire::Fixed64
            }
            2 => {
                let len = self.read_varint()? as usize;
                let end = self.pos.checked_add(len).filter(|&p| p <= self.buf.len())?;
                let bytes = &self.buf[self.pos..end];
                self.pos = end;
                Wire::Len(bytes)
            }
            5 => {
                self.pos = self.pos.checked_add(4).filter(|&p| p <= self.buf.len())?;
                Wire::Fixed32
            }
            _ => return None,
        };
        Some((field, wire))
    }
}

/// First length-delimited (sub-message / string / bytes) value for `field`.
fn message_field(buf: &[u8], field: u64) -> Option<&[u8]> {
    let mut reader = ProtoReader::new(buf);
    while let Some((found, wire)) = reader.next_field() {
        if found == field {
            if let Wire::Len(bytes) = wire {
                return Some(bytes);
            }
        }
    }
    None
}

/// First varint value for `field`.
fn varint_field(buf: &[u8], field: u64) -> Option<u64> {
    let mut reader = ProtoReader::new(buf);
    while let Some((found, wire)) = reader.next_field() {
        if found == field {
            if let Wire::Varint(value) = wire {
                return Some(value);
            }
        }
    }
    None
}

/// First UTF-8 string value for `field`.
fn string_field(buf: &[u8], field: u64) -> Option<&str> {
    message_field(buf, field).and_then(|bytes| std::str::from_utf8(bytes).ok())
}

/// [`string_field`], treating a blank value as absent. Antigravity writes the
/// model fields either fully or not at all, but a blank string must not be
/// mistaken for a usable model id or display label.
fn non_empty_string_field(buf: &[u8], field: u64) -> Option<&str> {
    string_field(buf, field).filter(|text| !text.trim().is_empty())
}

#[cfg(test)]
mod tests;
