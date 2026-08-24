//! Kimi CLI / Kimi Code session parser
//!
//! Parses wire.jsonl from both `kimi-cli` and `kimi-code`.
//!
//! ~/.kimi/sessions/[GROUP_ID]/[SESSION_UUID]/wire.jsonl
//!   Token data comes from StatusUpdate messages.
//!
//! ~/.kimi-code/sessions/[WORKSPACE]/[SESSION]/agents/[AGENT]/wire.jsonl
//!   Token data comes from usage.record lines.

use super::utils::{file_modified_timestamp_ms, lossy_lines};
use super::UnifiedMessage;
use crate::TokenBreakdown;
use serde::Deserialize;
use std::collections::HashMap;
use std::io::BufReader;
use std::path::{Path, PathBuf};

/// Top-level wire.jsonl line: either metadata or a timestamped message
#[derive(Debug, Deserialize)]
struct WireLine {
    timestamp: Option<f64>,
    message: Option<WireMessage>,
    #[serde(rename = "type")]
    line_type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WireMessage {
    #[serde(rename = "type")]
    msg_type: String,
    payload: Option<StatusPayload>,
}

#[derive(Debug, Deserialize)]
struct StatusPayload {
    token_usage: Option<TokenUsage>,
    #[allow(dead_code)]
    message_id: Option<String>,
}

/// Token usage counts shared by both wire formats.
///
/// Legacy kimi-cli StatusUpdate payloads use snake_case field names;
/// kimi-code usage.record lines use the camelCase aliases.
#[derive(Debug, Deserialize)]
struct TokenUsage {
    #[serde(alias = "inputOther")]
    input_other: Option<i64>,
    output: Option<i64>,
    #[serde(alias = "inputCacheRead")]
    input_cache_read: Option<i64>,
    #[serde(alias = "inputCacheCreation")]
    input_cache_creation: Option<i64>,
}

impl TokenUsage {
    /// Clamp negative counts to zero and build a breakdown.
    /// Returns `None` when every count is zero so callers can skip the entry.
    fn to_breakdown(&self) -> Option<TokenBreakdown> {
        let input = self.input_other.unwrap_or(0).max(0);
        let output = self.output.unwrap_or(0).max(0);
        let cache_read = self.input_cache_read.unwrap_or(0).max(0);
        let cache_write = self.input_cache_creation.unwrap_or(0).max(0);

        if input == 0 && output == 0 && cache_read == 0 && cache_write == 0 {
            return None;
        }

        Some(TokenBreakdown {
            input,
            output,
            cache_read,
            cache_write,
            // Kimi wire protocols do not expose reasoning tokens; all reasoning included in output
            reasoning: 0,
        })
    }
}

/// Default model name when config.json is not available
const DEFAULT_MODEL: &str = "kimi-for-coding";
const DEFAULT_PROVIDER: &str = "moonshot";

/// Locate the legacy Kimi CLI config consumed by `parse_kimi_file`. Kimi Code
/// embeds model information in each wire record and does not use this file.
pub(crate) fn kimi_config_path(wire_path: &Path) -> Option<PathBuf> {
    let sessions_dir = wire_path.parent()?.parent()?.parent()?;
    Some(sessions_dir.parent()?.join("config.json"))
}

/// Read model name from ~/.kimi/config.json if available
fn read_model_from_config(wire_path: &Path) -> String {
    if let Some(config_path) = kimi_config_path(wire_path) {
        if let Ok(content) = std::fs::read_to_string(&config_path) {
            if let Ok(bytes) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(model) = bytes.get("model").and_then(|v| v.as_str()) {
                    if !model.is_empty() {
                        return model.to_string();
                    }
                }
            }
        }
    }
    DEFAULT_MODEL.to_string()
}

/// Extract session ID from the wire.jsonl path
/// Path format: ~/.kimi/sessions/GROUP_ID/SESSION_UUID/wire.jsonl
fn extract_session_id(path: &Path) -> String {
    path.parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string()
}

/// Check whether a wire.jsonl path belongs to kimi-code.
///
/// kimi-code writes `<root>/sessions/WORKSPACE/SESSION/agents/AGENT/wire.jsonl`
/// while legacy kimi-cli writes `<root>/sessions/GROUP/UUID/wire.jsonl`, so the
/// grandparent directory component (`agents`) distinguishes the formats. The
/// layout under the root is created by kimi-code itself, so this holds for the
/// default `~/.kimi-code` root and custom `KIMI_CODE_HOME` roots alike.
pub fn is_kimi_code_path(path: &Path) -> bool {
    path.parent()
        .and_then(|agent_dir| agent_dir.parent())
        .and_then(|agents_dir| agents_dir.file_name())
        .is_some_and(|name| name == "agents")
}

/// Extract session ID from a kimi-code wire.jsonl path.
/// Path format: ~/.kimi-code/sessions/WORKSPACE/SESSION_UUID/agents/AGENT/wire.jsonl
fn extract_session_id_from_kimi_code_path(path: &Path) -> String {
    // Walk up: wire.jsonl -> AGENT -> agents -> SESSION_UUID -> ...
    path.parent() // AGENT
        .and_then(|p| p.parent()) // agents
        .and_then(|p| p.parent()) // SESSION_UUID
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string()
}

/// Strip the "kimi-code/" prefix from model IDs emitted by kimi-code.
fn normalize_kimi_code_model(model: &str) -> String {
    model
        .strip_prefix("kimi-code/")
        .unwrap_or(model)
        .to_string()
}

/// Normalize a Kimi Code model, excluding symbolic config references such as
/// `__kimi_env_model__` that do not identify the model sent to the provider.
fn concrete_kimi_code_model(model: &str) -> Option<String> {
    let normalized = normalize_kimi_code_model(model.trim());
    let normalized = normalized.trim();
    let symbolic =
        normalized.len() >= 4 && normalized.starts_with("__") && normalized.ends_with("__");
    (!normalized.is_empty() && !symbolic).then(|| normalized.to_string())
}

/// Kimi Code wire.jsonl line structure.
#[derive(Debug, Deserialize)]
struct KimiCodeWireLine {
    #[serde(rename = "type")]
    line_type: String,
    model: Option<String>,
    usage: Option<TokenUsage>,
    #[serde(rename = "usageScope")]
    usage_scope: Option<String>,
    time: Option<i64>,
}

/// Parse a Kimi Code wire.jsonl file.
pub fn parse_kimi_code_file(path: &Path) -> Vec<UnifiedMessage> {
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return Vec::new(),
    };

    let session_id = extract_session_id_from_kimi_code_path(path);
    let fallback_timestamp = file_modified_timestamp_ms(path);

    let reader = BufReader::new(file);
    let mut messages: Vec<UnifiedMessage> = Vec::new();
    let mut latest_request_model: Option<String> = None;

    for line in lossy_lines(reader) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let mut bytes = trimmed.as_bytes().to_vec();
        let wire_line = match simd_json::from_slice::<KimiCodeWireLine>(&mut bytes) {
            Ok(wl) => wl,
            Err(_) => continue,
        };

        // usage.record can contain only a symbolic config reference, while the
        // preceding llm.request records the concrete model sent to the provider.
        if wire_line.line_type == "llm.request" {
            if let Some(model) = wire_line
                .model
                .as_deref()
                .and_then(concrete_kimi_code_model)
            {
                latest_request_model = Some(model);
            }
            continue;
        }

        // Only process usage.record lines.
        // step.end also carries usage, but it duplicates the same usage.record
        // that was emitted in the same turn, so we ignore it to avoid double counting.
        if wire_line.line_type != "usage.record" {
            continue;
        }

        // Only count turn-scoped usage. kimi-code tags every usage.record with
        // usageScope: "turn" for per-step LLM calls made inside a user turn and
        // "session" for non-turn bookkeeping (e.g. context compaction), and its
        // own tooling treats a missing usageScope as session-scoped, so require
        // an explicit "turn" to avoid counting aggregate records.
        if wire_line.usage_scope.as_deref() != Some("turn") {
            continue;
        }

        // Skip entries with zero tokens
        let Some(tokens) = wire_line.usage.as_ref().and_then(TokenUsage::to_breakdown) else {
            continue;
        };

        let model = wire_line
            .model
            .as_deref()
            .and_then(concrete_kimi_code_model)
            .or_else(|| latest_request_model.clone())
            .unwrap_or_else(|| DEFAULT_MODEL.to_string());

        // `time` is Unix milliseconds, so only positivity is checked here —
        // deliberately not routed through `parse_timestamp_value`, whose
        // seconds-vs-milliseconds heuristic would rescale anything below 1e12.
        // This field is never seconds, so rescaling would invent a plausible
        // instant for a value that is simply corrupt; the mtime fallback says
        // "unknown" instead.
        let timestamp_ms = wire_line
            .time
            .filter(|ms| *ms > 0)
            .unwrap_or(fallback_timestamp);

        messages.push(UnifiedMessage::new(
            "kimi",
            model,
            DEFAULT_PROVIDER,
            session_id.clone(),
            timestamp_ms,
            tokens,
            0.0,
        ));
    }

    messages
}

/// Parse a Kimi CLI wire.jsonl file
pub fn parse_kimi_file(path: &Path) -> Vec<UnifiedMessage> {
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return Vec::new(),
    };

    let model = read_model_from_config(path);
    let session_id = extract_session_id(path);
    let fallback_timestamp = file_modified_timestamp_ms(path);

    let reader = BufReader::new(file);
    let mut messages: Vec<UnifiedMessage> = Vec::new();
    let mut timestamp_sources: Vec<TimestampSource> = Vec::new();
    let mut keyed_indices: HashMap<String, usize> = HashMap::new();

    for line in lossy_lines(reader) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let mut bytes = trimmed.as_bytes().to_vec();
        let wire_line = match simd_json::from_slice::<WireLine>(&mut bytes) {
            Ok(wl) => wl,
            Err(_) => continue,
        };

        // Skip metadata lines (first line: {"type": "metadata", ...})
        if wire_line.line_type.as_deref() == Some("metadata") {
            continue;
        }

        let message = match wire_line.message {
            Some(m) => m,
            None => continue,
        };

        // Only process StatusUpdate messages
        if message.msg_type != "StatusUpdate" {
            continue;
        }

        let payload = match message.payload {
            Some(p) => p,
            None => continue,
        };

        let token_usage = match payload.token_usage {
            Some(u) => u,
            None => continue,
        };

        // Convert Unix seconds (float) to milliseconds, falling back to file
        // mtime when the wire value is missing or does not convert to a
        // positive instant. A corrupt `{"timestamp": -1.5}` would otherwise
        // anchor the message in a pre-epoch daily bucket; the float->int cast
        // also collapses NaN to 0, so the same check catches that.
        let (timestamp_ms, timestamp_source) = match wire_line
            .timestamp
            .map(|ts| (ts * 1000.0) as i64)
            .filter(|ms| *ms > 0)
        {
            Some(ms) => (ms, TimestampSource::Wire),
            None => (fallback_timestamp, TimestampSource::FileMtime),
        };

        // Skip entries with zero tokens
        let Some(tokens) = token_usage.to_breakdown() else {
            continue;
        };

        let dedup_key = payload.message_id;

        let message = UnifiedMessage::new_with_dedup(
            "kimi",
            model.clone(),
            DEFAULT_PROVIDER,
            session_id.clone(),
            timestamp_ms,
            tokens,
            0.0,
            dedup_key,
        );
        push_or_replace_status_update(
            &mut messages,
            &mut timestamp_sources,
            &mut keyed_indices,
            message,
            timestamp_source,
        );
    }

    messages
}

fn exact_token_total(tokens: &TokenBreakdown) -> i128 {
    i128::from(tokens.input)
        + i128::from(tokens.output)
        + i128::from(tokens.cache_read)
        + i128::from(tokens.cache_write)
        + i128::from(tokens.reasoning)
}

/// Where a StatusUpdate's anchor came from. The mtime fallback is a guess for a
/// line whose own timestamp was unusable, so it ranks below a real wire value
/// when duplicates are compared.
#[derive(Clone, Copy, PartialEq, Eq)]
enum TimestampSource {
    Wire,
    FileMtime,
}

fn should_replace_status_update(
    existing: (&UnifiedMessage, TimestampSource),
    candidate: (&UnifiedMessage, TimestampSource),
) -> bool {
    let (existing, existing_source) = existing;
    let (candidate, candidate_source) = candidate;
    let existing_total = exact_token_total(&existing.tokens);
    let candidate_total = exact_token_total(&candidate.tokens);

    if candidate_total != existing_total {
        return candidate_total > existing_total;
    }

    // Totals tie, so the anchor decides. Compare provenance before the value:
    // mtime is >= every real timestamp in a file still being written, so a
    // corrupt duplicate that fell back to it would otherwise outrank the good
    // line it duplicates and move the message off its true day.
    if existing_source != candidate_source {
        return candidate_source == TimestampSource::Wire;
    }

    candidate.timestamp >= existing.timestamp
}

fn push_or_replace_status_update(
    messages: &mut Vec<UnifiedMessage>,
    timestamp_sources: &mut Vec<TimestampSource>,
    keyed_indices: &mut HashMap<String, usize>,
    message: UnifiedMessage,
    timestamp_source: TimestampSource,
) {
    let dedup_key = message
        .dedup_key
        .as_ref()
        .filter(|key| !key.is_empty())
        .cloned();

    let Some(dedup_key) = dedup_key else {
        messages.push(message);
        timestamp_sources.push(timestamp_source);
        return;
    };

    if let Some(index) = keyed_indices.get(&dedup_key).copied() {
        if should_replace_status_update(
            (&messages[index], timestamp_sources[index]),
            (&message, timestamp_source),
        ) {
            messages[index] = message;
            timestamp_sources[index] = timestamp_source;
        }
        return;
    }

    let index = messages.len();
    messages.push(message);
    timestamp_sources.push(timestamp_source);
    keyed_indices.insert(dedup_key, index);
}

#[cfg(test)]
mod tests;
