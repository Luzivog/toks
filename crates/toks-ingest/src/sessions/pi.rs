//! Pi (badlogic/pi-mono) session parser
//!
//! Parses JSONL files from `~/.pi/agent/sessions/<encoded-cwd>/*.jsonl` (and,
//! via the `pi` client's OMP scan root, `~/.omp/agent/sessions/...`). Current
//! OMP builds write a `title` metadata record before the `session` header in
//! newly-created session files; see [`PRE_SESSION_METADATA_TYPES`].
//!
//! Pi descendants reuse this record layout verbatim, so [`parse_pi_format_file`]
//! is shared: see `sessions::senpi` for Senpi (OmO Native).

use super::utils::{file_modified_timestamp_ms, lossy_lines};
use super::{normalize_workspace_key, workspace_label_from_key, UnifiedMessage};
use crate::provider_identity::inferred_provider_from_model;
use crate::TokenBreakdown;
use serde::Deserialize;
use std::io::BufReader;
use std::path::Path;

/// Pi session header (first line of JSONL)
#[derive(Debug, Deserialize)]
pub struct PiSessionHeader {
    #[serde(rename = "type")]
    pub entry_type: String,
    pub id: String,
    #[allow(dead_code)]
    pub timestamp: Option<String>,
    #[allow(dead_code)]
    pub cwd: Option<String>,
    #[serde(rename = "parentSession")]
    pub parent_session: Option<String>,
    #[serde(rename = "rlmDepth")]
    pub rlm_depth: Option<u32>,
}

/// Loose type-only probe for a JSONL line, used to identify pre-session
/// metadata records without requiring their full schema.
#[derive(Debug, Deserialize)]
struct PiEntryTypeProbe {
    #[serde(rename = "type")]
    entry_type: String,
}

/// Record types OMP may write before the `session` header (e.g. an
/// auto-generated-title record). The parser skips these while looking for
/// `session` rather than discarding the whole file. Any other unrecognized
/// type before `session` is still treated as a malformed file.
const PRE_SESSION_METADATA_TYPES: &[&str] = &["title"];

/// Pi session entry (subsequent lines of JSONL)
#[derive(Debug, Deserialize)]
pub struct PiSessionEntry {
    #[serde(rename = "type")]
    pub entry_type: String,
    #[allow(dead_code)]
    pub id: Option<String>,
    #[serde(rename = "parentId")]
    #[allow(dead_code)]
    pub parent_id: Option<String>,
    pub timestamp: Option<String>,
    pub message: Option<PiMessage>,
    pub name: Option<String>,
    #[serde(rename = "targetId")]
    pub target_id: Option<String>,
    #[serde(rename = "childUsage")]
    pub child_usage: Option<PiUsage>,
    #[serde(rename = "aggregateUsage")]
    pub aggregate_usage: Option<PiUsage>,
}

#[derive(Debug, Deserialize)]
pub struct PiMessage {
    pub role: Option<String>,
    pub usage: Option<PiUsage>,
    pub model: Option<String>,
    pub provider: Option<String>,
    #[serde(rename = "responseId")]
    pub response_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PiUsage {
    pub input: Option<i64>,
    pub output: Option<i64>,
    pub cache_read: Option<i64>,
    pub cache_write: Option<i64>,
    #[allow(dead_code)]
    pub total_tokens: Option<i64>,
    /// Parsed so the omission below is a real decision rather than an accident
    /// of the schema, but never summed: see the note at the emit site.
    #[allow(dead_code)]
    pub reasoning: Option<i64>,
}

fn is_generated_id(value: &str) -> bool {
    (value.len() == 8 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
        || (value.len() == 36
            && value.bytes().enumerate().all(|(index, byte)| {
                if matches!(index, 8 | 13 | 18 | 23) {
                    byte == b'-'
                } else {
                    byte.is_ascii_hexdigit()
                }
            }))
}

fn strip_generated_id(value: &str) -> Option<&str> {
    for id_len in [36, 8] {
        if value.len() <= id_len || value.as_bytes()[value.len() - id_len - 1] != b'-' {
            continue;
        }
        let id = &value[value.len() - id_len..];
        if is_generated_id(id) {
            return Some(&value[..value.len() - id_len - 1]);
        }
    }
    None
}

fn pi_subagent_name(session_name: &str) -> Option<String> {
    let name = session_name.strip_prefix("subagent-")?;
    let without_id = strip_generated_id(name).or_else(|| {
        let (without_index, index) = name.rsplit_once('-')?;
        if index.is_empty() || !index.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        strip_generated_id(without_index)
    })?;

    (!without_id.is_empty()).then(|| without_id.to_string())
}

/// Parse a Pi JSONL session file
pub fn parse_pi_file(path: &Path) -> Vec<UnifiedMessage> {
    parse_pi_format_file(path, "pi", "pi")
}

/// Parse a JSONL session file written in the Pi record format.
///
/// `client` is the Toks client id stamped on every emitted message, and
/// `fallback_provider` is used only when the message carries no provider and
/// the model name is not recognizable.
pub(crate) fn parse_pi_format_file(
    path: &Path,
    client: &str,
    fallback_provider: &'static str,
) -> Vec<UnifiedMessage> {
    let mut observer = NoopPiFormatObserver;
    parse_pi_format_file_inner(
        path,
        client,
        fallback_provider,
        None,
        false,
        false,
        &mut observer,
    )
}

/// Parse a Pi-format session and retain message ids in namespaced dedup keys.
/// Pi-compatible clients that need cross-file deduplication can opt into this
/// without changing the historical output of the shared Pi and Senpi parsers.
pub(crate) fn parse_pi_format_file_with_dedup(
    path: &Path,
    client: &str,
    fallback_provider: &'static str,
) -> Vec<UnifiedMessage> {
    let mut observer = NoopPiFormatObserver;
    parse_pi_format_file_inner(
        path,
        client,
        fallback_provider,
        Some(client),
        false,
        false,
        &mut observer,
    )
}

/// Receives already-decoded Pi records while the shared parser walks a file.
///
/// Prime Agent uses this hook to derive its fork/child accounting metadata in
/// the same pass that emits messages. The emitted message is supplied only for
/// an assistant record that passed the shared parser's validation.
pub(crate) trait PiFormatObserver {
    fn observe_header(&mut self, _header: &PiSessionHeader) {}

    fn observe_entry(&mut self, _entry: &PiSessionEntry, _emitted: Option<&UnifiedMessage>) {}
}

struct NoopPiFormatObserver;

impl PiFormatObserver for NoopPiFormatObserver {}

/// Parse a Pi-format session whose `session_info.name` identifies an RLM
/// subagent when the session header has `rlmDepth > 0`.
///
/// Deduplication is intentionally cross-session: Prime Agent forks copy prior
/// message entries into a file with a new session id. Provider response ids are
/// preferred; the message id plus immutable event fields is the fallback.
pub(crate) fn parse_pi_format_rlm_file_with_observer(
    path: &Path,
    client: &str,
    fallback_provider: &'static str,
    observer: &mut impl PiFormatObserver,
) -> Vec<UnifiedMessage> {
    parse_pi_format_file_inner(
        path,
        client,
        fallback_provider,
        Some(client),
        true,
        true,
        observer,
    )
}

fn parse_pi_format_file_inner(
    path: &Path,
    client: &str,
    fallback_provider: &'static str,
    dedup_namespace: Option<&str>,
    rlm_session_name_as_agent: bool,
    cross_session_dedup: bool,
    observer: &mut impl PiFormatObserver,
) -> Vec<UnifiedMessage> {
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return Vec::new(),
    };

    let fallback_timestamp = file_modified_timestamp_ms(path);

    let reader = BufReader::new(file);
    let mut messages: Vec<UnifiedMessage> = Vec::with_capacity(64);
    let mut buffer = Vec::with_capacity(4096);

    let mut session_id: Option<String> = None;
    let mut workspace_key: Option<String> = None;
    let mut workspace_label: Option<String> = None;
    let mut agent: Option<String> = None;
    let mut is_rlm_subagent = false;
    for line in lossy_lines(reader) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if session_id.is_none() {
            buffer.clear();
            buffer.extend_from_slice(trimmed.as_bytes());
            let entry_type = match simd_json::from_slice::<PiEntryTypeProbe>(&mut buffer) {
                Ok(probe) => probe.entry_type,
                Err(_) => return Vec::new(),
            };

            if entry_type != "session" {
                if PRE_SESSION_METADATA_TYPES.contains(&entry_type.as_str()) {
                    continue;
                }
                return Vec::new();
            }

            buffer.clear();
            buffer.extend_from_slice(trimmed.as_bytes());
            let header = match simd_json::from_slice::<PiSessionHeader>(&mut buffer) {
                Ok(h) => h,
                Err(_) => return Vec::new(),
            };

            observer.observe_header(&header);
            session_id = Some(header.id.clone());
            workspace_key = header.cwd.as_deref().and_then(normalize_workspace_key);
            workspace_label = workspace_key.as_deref().and_then(workspace_label_from_key);
            is_rlm_subagent = header.rlm_depth.unwrap_or(0) > 0;
            continue;
        }

        buffer.clear();
        buffer.extend_from_slice(trimmed.as_bytes());
        let entry = match simd_json::from_slice::<PiSessionEntry>(&mut buffer) {
            Ok(e) => e,
            Err(_) => continue,
        };

        if entry.entry_type == "session_info" {
            agent = if rlm_session_name_as_agent && is_rlm_subagent {
                entry
                    .name
                    .as_ref()
                    .filter(|name| !name.trim().is_empty())
                    .cloned()
            } else {
                entry.name.as_deref().and_then(pi_subagent_name)
            };
            observer.observe_entry(&entry, None);
            continue;
        }

        if entry.entry_type != "message" {
            observer.observe_entry(&entry, None);
            continue;
        }

        let Some(message) = entry.message.as_ref() else {
            observer.observe_entry(&entry, None);
            continue;
        };

        if message.role.as_deref() != Some("assistant") {
            observer.observe_entry(&entry, None);
            continue;
        }

        let Some(usage) = message.usage.as_ref() else {
            observer.observe_entry(&entry, None);
            continue;
        };

        let Some(model) = message.model.as_deref() else {
            observer.observe_entry(&entry, None);
            continue;
        };

        // A missing/blank provider field is recoverable: infer it from the
        // model name (e.g. a Pi "gpt-5" message with no provider maps to
        // "openai"), falling back to "pi" only when inference can't
        // identify the model, rather than dropping a message that carries
        // valid tokens.
        let provider = match message.provider.as_deref() {
            Some(provider) if !provider.is_empty() => provider.to_string(),
            _ => inferred_provider_from_model(model)
                .unwrap_or(fallback_provider)
                .to_string(),
        };

        let recorded_timestamp = entry
            .timestamp
            .as_deref()
            .and_then(|timestamp| chrono::DateTime::parse_from_rfc3339(timestamp).ok())
            .map(|timestamp| timestamp.timestamp_millis());
        let timestamp = recorded_timestamp.unwrap_or(fallback_timestamp);

        // `usage.reasoning` is read but deliberately not mapped onto
        // `TokenBreakdown::reasoning`. In the Pi format reasoning tokens are a
        // subset of `output` (Pi's own `totalTokens` excludes them), whereas
        // Toks totals `reasoning` as its own additive bucket. Mapping it
        // through would double count.
        let mut unified = UnifiedMessage::new_with_agent(
            client,
            model,
            provider.as_str(),
            session_id.clone().unwrap_or_else(|| "unknown".to_string()),
            timestamp,
            TokenBreakdown {
                input: usage.input.unwrap_or(0).max(0),
                output: usage.output.unwrap_or(0).max(0),
                cache_read: usage.cache_read.unwrap_or(0).max(0),
                cache_write: usage.cache_write.unwrap_or(0).max(0),
                reasoning: 0,
            },
            0.0,
            agent.clone(),
        );
        if let Some(namespace) = dedup_namespace {
            if cross_session_dedup {
                unified.dedup_key = message
                    .response_id
                    .as_deref()
                    .filter(|id| !id.trim().is_empty())
                    .map(|id| format!("{namespace}:response:{id}"))
                    .or_else(|| {
                        entry
                            .id
                            .as_deref()
                            .filter(|id| !id.trim().is_empty())
                            .map(|id| {
                                let stable_timestamp = recorded_timestamp
                                    .map(|timestamp| timestamp.to_string())
                                    .unwrap_or_else(|| "missing".to_string());
                                format!(
                                    "{namespace}:message:{id}:{stable_timestamp}:{provider}:{model}:{}:{}:{}:{}",
                                    unified.tokens.input,
                                    unified.tokens.output,
                                    unified.tokens.cache_read,
                                    unified.tokens.cache_write,
                                )
                            })
                    });
            } else if let Some(message_id) = entry.id.as_deref().filter(|id| !id.trim().is_empty())
            {
                let session_id = session_id.as_deref().unwrap_or("unknown");
                unified.dedup_key = Some(format!("{namespace}:{session_id}:{message_id}"));
            }
        }
        unified.set_workspace(workspace_key.clone(), workspace_label.clone());
        observer.observe_entry(&entry, Some(&unified));
        messages.push(unified);
    }

    messages
}

#[cfg(test)]
mod pi_tests;
