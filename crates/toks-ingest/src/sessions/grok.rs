//! Grok Build session parser.
//!
//! Grok Build writes JSON-RPC session updates under
//! `~/.grok/sessions/<urlencoded-workspace>/<session-id>/updates.jsonl`.
//! Session rollups also land in sibling `signals.json` (including
//! `totalTokensBeforeCompaction` and `contextTokensUsed`). Legacy update logs
//! expose cumulative `totalTokens` counters without a stable input/output
//! split, so this parser records per-turn positive total-token deltas as input
//! tokens and reconciles any remaining `signals.json` total so compacted
//! sessions are not under-counted. Recent Grok Build releases additionally
//! write per-inference token breakdowns to `~/.grok/logs/unified.jsonl`.

use super::utils::{
    extract_i64, extract_string, file_modified_timestamp_ms, lossy_lines, parse_timestamp_value,
    read_file_or_none,
};
use super::{normalize_workspace_key, workspace_label_from_key, UnifiedMessage};
use crate::TokenBreakdown;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

const CLIENT_ID: &str = "grok";
const PROVIDER_ID: &str = "xai";
const UNKNOWN_MODEL: &str = "grok-unknown";
const UNIFIED_LOG_DEDUP_PREFIX: &str = "grok-unified:";

type UnifiedGeneration = u64;
type UnifiedProcessKey = (i64, UnifiedGeneration);
type UnifiedProcessSessionKey = (i64, UnifiedGeneration, String);
type UnifiedSessionTree = Vec<(PathBuf, Vec<PathBuf>)>;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct UnifiedChildScope {
    pid: i64,
    generation: UnifiedGeneration,
    session_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum UnifiedModelEvidence {
    Unique(String),
    Conflict,
}

#[derive(Debug, Default)]
struct UnifiedChildEvidence {
    known_scopes: HashSet<UnifiedChildScope>,
    child_models: HashMap<UnifiedChildScope, UnifiedModelEvidence>,
    terminal_scopes: HashSet<UnifiedChildScope>,
    terminal_models: HashMap<UnifiedChildScope, UnifiedModelEvidence>,
    child_session_ids: HashSet<String>,
}

fn authoritative_model(value: Option<&Value>) -> Option<String> {
    extract_string(value).and_then(|model| {
        let model = model.trim();
        (!model.is_empty() && model != UNKNOWN_MODEL).then(|| model.to_string())
    })
}

fn record_model_evidence(
    evidence: &mut HashMap<UnifiedChildScope, UnifiedModelEvidence>,
    scope: &UnifiedChildScope,
    model: String,
) {
    match evidence.entry(scope.clone()) {
        std::collections::hash_map::Entry::Vacant(entry) => {
            entry.insert(UnifiedModelEvidence::Unique(model));
        }
        std::collections::hash_map::Entry::Occupied(mut entry) => match entry.get() {
            UnifiedModelEvidence::Unique(existing) if existing == &model => {}
            UnifiedModelEvidence::Unique(_) | UnifiedModelEvidence::Conflict => {
                entry.insert(UnifiedModelEvidence::Conflict);
            }
        },
    }
}

fn current_unified_generation(
    generations: &mut HashMap<i64, UnifiedGeneration>,
    pid: i64,
) -> UnifiedGeneration {
    *generations.entry(pid).or_insert(0)
}

fn advance_unified_generation(generations: &mut HashMap<i64, UnifiedGeneration>, pid: i64) {
    let generation = generations.entry(pid).or_insert(0);
    *generation = generation.saturating_add(1);
}

fn unified_subagent_id(value: &Value) -> Option<String> {
    extract_string(value.get("ctx")?.get("subagent_id")).filter(|id| !id.trim().is_empty())
}

fn unified_child_scope(
    value: &Value,
    generations: &mut HashMap<i64, UnifiedGeneration>,
) -> Option<UnifiedChildScope> {
    let pid = required_non_negative_i64(value.get("pid"))?;
    Some(UnifiedChildScope {
        pid,
        generation: current_unified_generation(generations, pid),
        session_id: unified_subagent_id(value)?,
    })
}

fn unified_spawn_model(value: &Value) -> Option<String> {
    let context = value.get("ctx")?;
    authoritative_model(context.get("effective_model"))
        .or_else(|| authoritative_model(context.get("effective_model_raw")))
}

fn unified_terminal_model(value: &Value) -> Option<String> {
    authoritative_model(value.get("ctx")?.get("effective_model"))
}

fn unique_child_model<'a>(
    evidence: &'a UnifiedChildEvidence,
    scope: &UnifiedChildScope,
) -> Option<&'a str> {
    let UnifiedModelEvidence::Unique(model) = evidence.child_models.get(scope)? else {
        return None;
    };
    Some(model)
}

fn unique_terminal_model<'a>(
    evidence: &'a UnifiedChildEvidence,
    scope: &UnifiedChildScope,
) -> Option<&'a str> {
    if !evidence.terminal_scopes.contains(scope) {
        return None;
    }
    let UnifiedModelEvidence::Unique(terminal_model) = evidence.terminal_models.get(scope)? else {
        return None;
    };
    let child_model = unique_child_model(evidence, scope)?;
    (terminal_model == child_model).then_some(child_model)
}

fn has_conflicting_child_evidence(
    evidence: &UnifiedChildEvidence,
    scope: &UnifiedChildScope,
) -> bool {
    matches!(
        evidence.child_models.get(scope),
        Some(UnifiedModelEvidence::Conflict)
    ) || matches!(
        evidence.terminal_models.get(scope),
        Some(UnifiedModelEvidence::Conflict)
    )
}

#[derive(Debug, Clone)]
struct GrokMetadata {
    session_id: String,
    model_id: Option<String>,
    timestamp: i64,
    workspace_key: Option<String>,
    workspace_label: Option<String>,
}

#[derive(Debug, Clone)]
struct ActiveTurn {
    baseline_total: i64,
    max_total: i64,
    timestamp: i64,
    model_id: String,
    turn_index: usize,
}

#[derive(Debug, Clone)]
struct GrokUsage {
    tokens: TokenBreakdown,
}

impl GrokUsage {
    fn from_update(value: &Value) -> Option<Self> {
        let usage = get_path(value, &["params", "update", "usage"])?;
        let raw_input = usage_value(usage, &["inputTokens", "input_tokens", "promptTokens"]);
        let raw_output = usage_value(
            usage,
            &["outputTokens", "output_tokens", "completionTokens"],
        );
        let cache_read = usage_value(
            usage,
            &[
                "cachedReadTokens",
                "cacheReadTokens",
                "cache_read_input_tokens",
            ],
        );
        let cache_write = usage_value(
            usage,
            &[
                "cachedWriteTokens",
                "cacheWriteTokens",
                "cacheCreationTokens",
                "cache_creation_input_tokens",
            ],
        );
        let reasoning = usage_value(
            usage,
            &["reasoningTokens", "thoughtTokens", "thinkingTokens"],
        );
        let reported_total = usage
            .get("totalTokens")
            .or_else(|| usage.get("total_tokens"))
            .and_then(|value| extract_i64(Some(value)))
            .map(|value| value.max(0));

        if raw_input == 0
            && raw_output == 0
            && cache_read == 0
            && cache_write == 0
            && reasoning == 0
        {
            return None;
        }

        // Grok's `inputTokens` includes the `cachedReadTokens` subset, and its
        // `outputTokens` includes the `reasoningTokens` subset. The reported
        // total is input + output, so split those overlaps before handing the
        // values to TokenBreakdown, whose buckets are additive.
        let inclusive_total = raw_input.saturating_add(raw_output);
        // The surrounding Grok usage contract treats input/output as inclusive
        // buckets even when the optional aggregate total is absent. Do not
        // require the redundant total field before removing the nested cache
        // and reasoning buckets.
        let reported_total_is_inclusive =
            reported_total.is_none() || reported_total == Some(inclusive_total);

        Some(Self {
            tokens: TokenBreakdown {
                input: if reported_total_is_inclusive {
                    raw_input.saturating_sub(cache_read)
                } else {
                    raw_input
                },
                output: if reported_total_is_inclusive {
                    raw_output.saturating_sub(reasoning)
                } else {
                    raw_output
                },
                cache_read,
                cache_write,
                reasoning,
            },
        })
    }
}

fn usage_value(value: &Value, keys: &[&str]) -> i64 {
    keys.iter()
        .find_map(|key| extract_i64(value.get(*key)))
        .unwrap_or(0)
        .max(0)
}

fn message_from_tokens(
    metadata: &GrokMetadata,
    model_id: String,
    timestamp: i64,
    tokens: TokenBreakdown,
    dedup_key: String,
    is_turn_start: bool,
) -> UnifiedMessage {
    let mut message = UnifiedMessage::new_with_dedup(
        CLIENT_ID,
        if model_id.trim().is_empty() {
            UNKNOWN_MODEL.to_string()
        } else {
            model_id
        },
        PROVIDER_ID,
        metadata.session_id.clone(),
        timestamp,
        tokens,
        0.0,
        Some(dedup_key),
    );
    message.set_workspace(
        metadata.workspace_key.clone(),
        metadata.workspace_label.clone(),
    );
    message.is_turn_start = is_turn_start;
    message
}

impl ActiveTurn {
    fn new(baseline_total: i64, timestamp: i64, model_id: String, turn_index: usize) -> Self {
        Self {
            baseline_total,
            max_total: baseline_total,
            timestamp,
            model_id,
            turn_index,
        }
    }

    fn observe_total(&mut self, total: i64, timestamp: i64) {
        if total > self.max_total {
            self.max_total = total;
            self.timestamp = timestamp;
        }
    }

    fn into_message(self, metadata: &GrokMetadata) -> Option<UnifiedMessage> {
        let token_delta = self.max_total.saturating_sub(self.baseline_total);
        if token_delta <= 0 {
            return None;
        }

        let model_id = if self.model_id.trim().is_empty() {
            UNKNOWN_MODEL.to_string()
        } else {
            self.model_id
        };

        Some(message_from_tokens(
            metadata,
            model_id,
            self.timestamp,
            TokenBreakdown {
                input: token_delta,
                output: 0,
                cache_read: 0,
                cache_write: 0,
                reasoning: 0,
            },
            format!("grok:{}:{}", metadata.session_id, self.turn_index),
            true,
        ))
    }
}

pub fn parse_grok_updates_file(path: &Path) -> Vec<UnifiedMessage> {
    if path.file_name().and_then(|name| name.to_str()) != Some("updates.jsonl") {
        return Vec::new();
    }

    let metadata = read_metadata(path);
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(_) => return Vec::new(),
    };

    let mut fallback_messages = Vec::new();
    let mut usage_messages = Vec::new();
    let mut current_model = metadata
        .model_id
        .clone()
        .unwrap_or_else(|| UNKNOWN_MODEL.to_string());
    let mut last_total: Option<i64> = None;
    let mut last_total_timestamp = metadata.timestamp;
    let mut active_turn: Option<ActiveTurn> = None;
    let mut turn_index = 0usize;
    let mut usage_index = 0usize;

    for line in lossy_lines(BufReader::new(file)) {
        if line.trim().is_empty() {
            continue;
        }

        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };

        if let Some(model_id) = extract_model_id(&value) {
            current_model = model_id;
            if let Some(turn) = active_turn.as_mut() {
                if turn.model_id == UNKNOWN_MODEL {
                    turn.model_id = current_model.clone();
                }
            }
        }

        let timestamp = extract_timestamp_ms(&value).unwrap_or(metadata.timestamp);
        if is_user_message_chunk(&value) {
            if let Some(turn) = active_turn.take() {
                if let Some(message) = turn.into_message(&metadata) {
                    fallback_messages.push(message);
                }
            }

            active_turn = Some(ActiveTurn::new(
                last_total.unwrap_or(0),
                timestamp,
                current_model.clone(),
                turn_index,
            ));
            turn_index = turn_index.saturating_add(1);
        }

        if let Some(usage) = GrokUsage::from_update(&value) {
            let model_id = if current_model != UNKNOWN_MODEL {
                current_model.clone()
            } else {
                get_path(&value, &["params", "update", "usage", "modelUsage"])
                    .and_then(Value::as_object)
                    .and_then(|models| (models.len() == 1).then(|| models.keys().next().cloned()))
                    .flatten()
                    .or_else(|| metadata.model_id.clone())
                    .unwrap_or_else(|| UNKNOWN_MODEL.to_string())
            };
            let event_id = get_path(&value, &["params", "_meta", "eventId"])
                .and_then(|value| extract_string(Some(value)))
                .unwrap_or_else(|| format!("turn-{usage_index}"));
            // `eventId` is not unique: Grok reuses it across usage records, so
            // keying on it alone gave distinct turns byte-identical keys. The
            // Grok lane in `lib.rs` does not collapse duplicate keys today —
            // it only runs `prefer_unified_log_messages` — so this is not
            // currently load-bearing, but a per-record-unique key is correct on
            // its own merits and cheap insurance against any consumer that does
            // key on it. The position of the record within the file
            // disambiguates them and stays stable across re-parses of an
            // unchanged file, which the on-disk message cache this key feeds
            // requires. Note the key is only unique within one file; it is not
            // a global identity.
            usage_messages.push(message_from_tokens(
                &metadata,
                model_id,
                timestamp,
                usage.tokens,
                format!(
                    "grok:{}:usage:{usage_index}:{event_id}",
                    metadata.session_id
                ),
                true,
            ));
            usage_index = usage_index.saturating_add(1);
        }

        let Some(total_tokens) = extract_total_tokens(&value) else {
            continue;
        };
        if total_tokens < 0 {
            continue;
        }

        match last_total {
            Some(previous) if total_tokens < previous => {
                // Grok sometimes repeats or rewinds intermediate counters while
                // streaming tool updates. Treat cumulative totals as monotonic.
                continue;
            }
            Some(previous) if total_tokens == previous => {
                last_total_timestamp = timestamp;
            }
            Some(previous) => {
                if active_turn.is_none() {
                    active_turn = Some(ActiveTurn::new(
                        previous,
                        timestamp,
                        current_model.clone(),
                        turn_index,
                    ));
                    turn_index = turn_index.saturating_add(1);
                }
                if let Some(turn) = active_turn.as_mut() {
                    turn.observe_total(total_tokens, timestamp);
                }
                last_total_timestamp = timestamp;
                last_total = Some(total_tokens);
            }
            None => {
                if let Some(turn) = active_turn.as_mut() {
                    turn.observe_total(total_tokens, timestamp);
                }
                last_total_timestamp = timestamp;
                last_total = Some(total_tokens);
            }
        }
    }

    if let Some(turn) = active_turn {
        if let Some(message) = turn.into_message(&metadata) {
            fallback_messages.push(message);
        }
    }

    if fallback_messages.is_empty() && usage_messages.is_empty() {
        if let Some(total_tokens) = last_total.filter(|tokens| *tokens > 0) {
            let aggregate_turn = ActiveTurn {
                baseline_total: 0,
                max_total: total_tokens,
                timestamp: last_total_timestamp,
                model_id: current_model.clone(),
                turn_index: 0,
            };
            if let Some(message) = aggregate_turn.into_message(&metadata) {
                fallback_messages.push(message);
            }
        }
    }

    if usage_messages.is_empty() {
        append_signals_reconciliation(path, &metadata, &mut fallback_messages, &current_model);
        return fallback_messages;
    }

    // A usage record is emitted when a turn completes. Keep only cumulative
    // counter activity newer than the latest completed turn as a best-effort
    // representation of a currently running turn; older fallback messages are
    // the same work already covered by authoritative usage records.
    let latest_usage_timestamp = usage_messages
        .iter()
        .map(|message| message.timestamp)
        .max()
        .unwrap_or(0);
    usage_messages.extend(
        fallback_messages
            .into_iter()
            .filter(|message| message.timestamp > latest_usage_timestamp),
    );
    usage_messages
}

/// Parse Grok Build's append-only unified log. Each
/// `shell.turn.inference_done` record reports a prompt total that includes
/// cached prompt tokens and a completion total that includes reasoning tokens.
/// Store the non-overlapping component buckets so the breakdown remains
/// additive and the source totals are preserved.
pub fn parse_grok_unified_log_file(path: &Path) -> Vec<UnifiedMessage> {
    if path.file_name().and_then(|name| name.to_str()) != Some("unified.jsonl") {
        return Vec::new();
    }

    let mut file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(_) => return Vec::new(),
    };
    let prefix_len = file.metadata().map(|metadata| metadata.len()).unwrap_or(0);
    parse_grok_unified_log_snapshot(path, &mut file, prefix_len)
}

#[cfg(test)]
fn parse_grok_unified_log_file_with_prefix(path: &Path, prefix_len: u64) -> Vec<UnifiedMessage> {
    let mut file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(_) => return Vec::new(),
    };
    parse_grok_unified_log_snapshot(path, &mut file, prefix_len)
}

fn parse_grok_unified_log_snapshot(
    path: &Path,
    file: &mut std::fs::File,
    prefix_len: u64,
) -> Vec<UnifiedMessage> {
    let fallback_timestamp = file_modified_timestamp_ms(path);
    let evidence = collect_unified_child_evidence(file, prefix_len);
    if file.seek(SeekFrom::Start(0)).is_err() {
        return Vec::new();
    }

    let metadata_by_session = read_unified_session_metadata(path);
    let mut generations = HashMap::new();
    let mut fallback_model_by_pid: HashMap<UnifiedProcessKey, String> = HashMap::new();
    let mut model_by_pid_and_session: HashMap<UnifiedProcessSessionKey, String> = HashMap::new();
    let mut model_by_session = HashMap::new();
    let mut seen = HashSet::new();
    let mut messages = Vec::new();

    for line in lossy_lines(BufReader::new(file).take(prefix_len)) {
        if line.trim().is_empty() {
            continue;
        }

        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };

        if let Some(pid) = unified_log_process_start_pid(&value) {
            // The unified log survives process restarts, so an OS-reused PID
            // must not inherit model authority from the previous process.
            advance_unified_generation(&mut generations, pid);
            continue;
        }

        let message_name = value.get("msg").and_then(Value::as_str);
        match message_name {
            Some("subagent read parent config (live)") => {
                if let Some((pid, model_id)) = unified_log_parent_model(&value) {
                    let generation = current_unified_generation(&mut generations, pid);
                    fallback_model_by_pid.insert((pid, generation), model_id);
                }
                continue;
            }
            Some("subagent model resolved") => {
                if let Some((pid, model_id)) = unified_log_parent_model(&value) {
                    let generation = current_unified_generation(&mut generations, pid);
                    fallback_model_by_pid.insert((pid, generation), model_id);
                    continue;
                }
            }
            Some("subagent spawn credentials") => {
                if let Some((pid, model_id)) = unified_log_parent_model(&value) {
                    let generation = current_unified_generation(&mut generations, pid);
                    fallback_model_by_pid.insert((pid, generation), model_id);
                }
                if let Some(scope) = unified_child_scope(&value, &mut generations) {
                    if let Some(model_id) = unified_spawn_model(&value) {
                        if unique_child_model(&evidence, &scope) == Some(model_id.as_str()) {
                            model_by_pid_and_session
                                .entry((scope.pid, scope.generation, scope.session_id))
                                .or_insert(model_id);
                        }
                    }
                }
                continue;
            }
            Some("subagent completed") | Some("subagent failed") => {
                if let Some(scope) = unified_child_scope(&value, &mut generations) {
                    if let Some(model_id) = unified_terminal_model(&value) {
                        if unique_terminal_model(&evidence, &scope) == Some(model_id.as_str()) {
                            // A terminal record is fallback evidence, never a
                            // rewrite of a model established by an earlier
                            // exact event.
                            model_by_pid_and_session
                                .entry((scope.pid, scope.generation, scope.session_id))
                                .or_insert(model_id);
                        }
                    }
                }
                continue;
            }
            _ => {}
        }

        if let Some((pid, session_id, model_id)) = unified_log_model_change(&value) {
            match (pid, session_id) {
                (Some(pid), Some(session_id)) => {
                    let generation = current_unified_generation(&mut generations, pid);
                    model_by_pid_and_session.insert((pid, generation, session_id), model_id);
                }
                (None, Some(session_id)) => {
                    model_by_pid_and_session.retain(|key, _| {
                        key.2 != session_id || evidence.child_session_ids.contains(&key.2)
                    });
                    model_by_session.insert(session_id, model_id);
                }
                (Some(pid), None) => {
                    let generation = current_unified_generation(&mut generations, pid);
                    fallback_model_by_pid.insert((pid, generation), model_id);
                }
                (None, None) => {}
            }
            continue;
        }

        if message_name != Some("shell.turn.inference_done") {
            continue;
        }

        let Some(session_id) =
            extract_string(value.get("sid")).filter(|session_id| !session_id.trim().is_empty())
        else {
            continue;
        };
        let Some(context) = value.get("ctx") else {
            continue;
        };
        let Some(prompt_tokens) = required_non_negative_i64(context.get("prompt_tokens")) else {
            continue;
        };
        let Some(completion_tokens) = required_non_negative_i64(context.get("completion_tokens"))
        else {
            continue;
        };
        let Some(mut cached_prompt_tokens) =
            optional_non_negative_i64(context.get("cached_prompt_tokens"))
        else {
            continue;
        };
        let Some(reasoning_tokens) = optional_non_negative_i64(context.get("reasoning_tokens"))
        else {
            continue;
        };
        cached_prompt_tokens = cached_prompt_tokens.min(prompt_tokens);

        let loop_index = match context.get("loop_index") {
            Some(value) => {
                let Some(loop_index) = required_non_negative_i64(Some(value)) else {
                    continue;
                };
                loop_index
            }
            None => 1,
        };
        let Some(pid) = optional_non_negative_i64(value.get("pid")) else {
            continue;
        };
        let timestamp = value
            .get("ts")
            .and_then(parse_timestamp_value)
            .unwrap_or(fallback_timestamp);
        let reasoning = reasoning_tokens.min(completion_tokens);
        let dedup_key = unified_log_dedup_key(&session_id, &value);
        if !seen.insert(dedup_key.clone()) {
            continue;
        }

        let metadata = metadata_by_session
            .get(&session_id)
            .cloned()
            .unwrap_or_else(|| fallback_unified_metadata(&session_id, fallback_timestamp));
        let generation = current_unified_generation(&mut generations, pid);
        let child_scope = value.get("pid").map(|_| UnifiedChildScope {
            pid,
            generation,
            session_id: session_id.clone(),
        });
        let known_scope = child_scope
            .as_ref()
            .is_some_and(|scope| evidence.known_scopes.contains(scope));
        let model_attribution_conflicted = child_scope
            .as_ref()
            .is_some_and(|scope| has_conflicting_child_evidence(&evidence, scope));
        let known_child_session = evidence.child_session_ids.contains(&session_id);
        let exact_model = model_by_pid_and_session
            .get(&(pid, generation, session_id.clone()))
            .cloned();
        let model_id = if model_attribution_conflicted {
            UNKNOWN_MODEL.to_string()
        } else if let Some(model_id) = exact_model {
            model_id
        } else if known_scope {
            child_scope
                .as_ref()
                .and_then(|scope| unique_terminal_model(&evidence, scope))
                .map(str::to_string)
                .unwrap_or_else(|| UNKNOWN_MODEL.to_string())
        } else if known_child_session {
            UNKNOWN_MODEL.to_string()
        } else {
            model_by_session
                .get(&session_id)
                .or_else(|| fallback_model_by_pid.get(&(pid, generation)))
                .cloned()
                .or_else(|| metadata.model_id.clone())
                .unwrap_or_else(|| UNKNOWN_MODEL.to_string())
        };
        let mut message = message_from_tokens(
            &metadata,
            model_id,
            timestamp,
            TokenBreakdown {
                input: prompt_tokens.saturating_sub(cached_prompt_tokens),
                output: completion_tokens.saturating_sub(reasoning),
                cache_read: cached_prompt_tokens,
                cache_write: 0,
                reasoning,
            },
            dedup_key,
            loop_index == 1,
        );
        message.model_attribution_conflicted = model_attribution_conflicted;
        message.session_id = session_id;
        message.message_count = i32::from(message.is_turn_start);
        messages.push(message);
    }

    messages
}

fn collect_unified_child_evidence(
    file: &mut std::fs::File,
    prefix_len: u64,
) -> UnifiedChildEvidence {
    let mut evidence = UnifiedChildEvidence::default();
    let mut generations = HashMap::new();

    for line in lossy_lines(BufReader::new(file).take(prefix_len)) {
        if line.trim().is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if let Some(pid) = unified_log_process_start_pid(&value) {
            advance_unified_generation(&mut generations, pid);
            continue;
        }

        let message_name = value.get("msg").and_then(Value::as_str);
        let is_spawn = message_name == Some("subagent spawn credentials");
        let is_terminal = matches!(message_name, Some("subagent completed" | "subagent failed"));
        if !is_spawn && !is_terminal {
            continue;
        }
        let Some(subagent_id) = unified_subagent_id(&value) else {
            continue;
        };
        evidence.child_session_ids.insert(subagent_id);
        let Some(scope) = unified_child_scope(&value, &mut generations) else {
            continue;
        };
        evidence.known_scopes.insert(scope.clone());
        if is_terminal {
            evidence.terminal_scopes.insert(scope.clone());
        }

        let model_id = if is_spawn {
            unified_spawn_model(&value)
        } else {
            unified_terminal_model(&value)
        };
        let Some(model_id) = model_id else {
            continue;
        };
        record_model_evidence(&mut evidence.child_models, &scope, model_id.clone());
        if is_terminal {
            record_model_evidence(&mut evidence.terminal_models, &scope, model_id);
        }
    }

    evidence
}

/// Dispatch between Grok's legacy per-session updates and its newer unified
/// log without accepting unrelated JSONL files under the Grok home directory.
pub fn parse_grok_file(path: &Path) -> Vec<UnifiedMessage> {
    match path.file_name().and_then(|name| name.to_str()) {
        Some("updates.jsonl") => parse_grok_updates_file(path),
        Some("unified.jsonl") => parse_grok_unified_log_file(path),
        _ => Vec::new(),
    }
}

/// Return the files and directories that can affect metadata attached to a
/// unified-log message. The unified parser reads every session under the Grok
/// home, so the root, workspace/session directories, and metadata siblings all
/// participate in its source fingerprint. Legacy update files only need their
/// own sibling metadata.
pub(crate) fn grok_related_paths(path: &Path) -> Vec<(String, PathBuf)> {
    if path.file_name().and_then(|name| name.to_str()) != Some("unified.jsonl") {
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        return ["signals.json", "summary.json", "events.jsonl"]
            .into_iter()
            .map(|name| (name.to_string(), parent.join(name)))
            .collect();
    }

    let Some(grok_home) = path.parent().and_then(Path::parent) else {
        return Vec::new();
    };
    let sessions_root = grok_home.join("sessions");
    let mut related = vec![("sessions-directory".to_string(), sessions_root.clone())];

    let Some((_, workspaces)) = unified_session_tree(path) else {
        return related;
    };
    for (workspace_dir, session_dirs) in workspaces {
        let workspace_suffix = cache_path_suffix(grok_home, &workspace_dir);
        related.push((
            format!("sessions-workspace:{workspace_suffix}"),
            workspace_dir.clone(),
        ));
        for session_dir in session_dirs {
            let session_suffix = cache_path_suffix(grok_home, &session_dir);
            related.push((
                format!("sessions-session:{session_suffix}"),
                session_dir.clone(),
            ));
            for file_name in [
                "updates.jsonl",
                "summary.json",
                "events.jsonl",
                "signals.json",
            ] {
                related.push((
                    format!("sessions-file:{session_suffix}/{file_name}"),
                    session_dir.join(file_name),
                ));
            }
        }
    }

    related
}

/// Uses the richer, per-inference unified log for sessions it covers. Legacy
/// updates remain a fallback for sessions absent from that log, avoiding an
/// additive merge of two representations of the same activity.
pub fn prefer_unified_log_messages(mut messages: Vec<UnifiedMessage>) -> Vec<UnifiedMessage> {
    let unified_sessions: HashSet<String> = messages
        .iter()
        .filter(|message| is_unified_log_message(message))
        .map(|message| message.session_id.clone())
        .collect();

    if unified_sessions.is_empty() {
        return messages;
    }

    let mut legacy_models = HashMap::new();
    let mut legacy_workspaces = HashMap::new();
    for message in messages
        .iter()
        .filter(|message| !is_unified_log_message(message))
    {
        if message.model_id != UNKNOWN_MODEL {
            match legacy_models.entry(message.session_id.clone()) {
                std::collections::hash_map::Entry::Vacant(entry) => {
                    entry.insert(Some(message.model_id.clone()));
                }
                std::collections::hash_map::Entry::Occupied(mut entry) => {
                    if entry.get().as_ref() != Some(&message.model_id) {
                        entry.insert(None);
                    }
                }
            }
        }

        let workspace = (
            message.workspace_key.clone(),
            message.workspace_label.clone(),
        );
        if workspace == (None, None) {
            continue;
        }

        match legacy_workspaces.entry(message.session_id.clone()) {
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(Some(workspace));
            }
            std::collections::hash_map::Entry::Occupied(mut entry) => {
                if entry.get().as_ref() != Some(&workspace) {
                    entry.insert(None);
                }
            }
        }
    }

    for message in messages
        .iter_mut()
        .filter(|message| is_unified_log_message(message))
    {
        if message.model_id == UNKNOWN_MODEL && !message.model_attribution_conflicted {
            if let Some(Some(model_id)) = legacy_models.get(&message.session_id) {
                message.model_id = model_id.clone();
            }
        }
        if message.workspace_key.is_none() && message.workspace_label.is_none() {
            if let Some(Some((workspace_key, workspace_label))) =
                legacy_workspaces.get(&message.session_id)
            {
                message.set_workspace(workspace_key.clone(), workspace_label.clone());
            }
        }
    }

    // A unified row only proves that one legacy activity row is covered when
    // both representations agree on the session, timestamp, and inclusive
    // token total. Retain every unmatched legacy row so a partially migrated
    // session cannot lose its older history.
    let mut covered_activity = HashMap::new();
    let mut covered_fallback_timestamps = HashMap::new();
    for message in messages
        .iter()
        .filter(|message| is_unified_log_message(message))
    {
        *covered_activity
            .entry((
                message.session_id.clone(),
                message.timestamp,
                message.tokens.total(),
            ))
            .or_insert(0usize) += 1;
        *covered_fallback_timestamps
            .entry((message.session_id.clone(), message.timestamp))
            .or_insert(0usize) += 1;
    }

    let mut selected = Vec::with_capacity(messages.len());
    for message in messages {
        if is_unified_log_message(&message) {
            selected.push(message);
            continue;
        }

        let key = (
            message.session_id.clone(),
            message.timestamp,
            message.tokens.total(),
        );
        let covered = covered_activity.get_mut(&key).is_some_and(|count| {
            if *count == 0 {
                false
            } else {
                *count -= 1;
                true
            }
        }) || (is_legacy_fallback_message(&message)
            && covered_fallback_timestamps
                .get_mut(&(message.session_id.clone(), message.timestamp))
                .is_some_and(|count| {
                    if *count == 0 {
                        false
                    } else {
                        *count -= 1;
                        true
                    }
                }));
        if !covered {
            selected.push(message);
        }
    }

    selected
}

fn is_unified_log_message(message: &UnifiedMessage) -> bool {
    message
        .dedup_key
        .as_deref()
        .is_some_and(|key| key.starts_with(UNIFIED_LOG_DEDUP_PREFIX))
}

fn is_legacy_fallback_message(message: &UnifiedMessage) -> bool {
    let Some(key) = message.dedup_key.as_deref() else {
        return false;
    };
    key.starts_with("grok:") && !key.contains(":usage:") && !key.ends_with(":signals")
}

fn unified_log_process_start_pid(value: &Value) -> Option<i64> {
    if value.get("msg").and_then(Value::as_str) != Some("AuthManager::new") {
        return None;
    }
    required_non_negative_i64(value.get("pid"))
}

fn unified_log_parent_model(value: &Value) -> Option<(i64, String)> {
    let pid = required_non_negative_i64(value.get("pid"))?;
    let context = value.get("ctx")?;
    let model_id = match value.get("msg").and_then(Value::as_str)? {
        "subagent read parent config (live)" => {
            authoritative_model(context.get("session_model_id"))
                .or_else(|| authoritative_model(context.get("parent_model")))
                .or_else(|| authoritative_model(context.get("global_model_id")))
        }
        "subagent model resolved" | "subagent spawn credentials" => {
            authoritative_model(context.get("parent_model"))
        }
        _ => None,
    }?;
    Some((pid, model_id))
}

fn unified_log_model_change(value: &Value) -> Option<(Option<i64>, Option<String>, String)> {
    let pid = match value.get("pid") {
        Some(value) => Some(required_non_negative_i64(Some(value))?),
        None => None,
    };
    let context = value.get("ctx")?;
    let model_id = match value.get("msg").and_then(Value::as_str)? {
        "model changed" => authoritative_model(context.get("model")),
        "model catalog: notifying clients" => authoritative_model(context.get("current_model_id")),
        "backend_search: model switch" => authoritative_model(context.get("new_model"))
            .or_else(|| authoritative_model(context.get("model")))
            .or_else(|| authoritative_model(context.get("current_model_id"))),
        "subagent model resolved" => authoritative_model(context.get("model_id"))
            .or_else(|| authoritative_model(context.get("model"))),
        _ => None,
    }?;

    let session_id =
        extract_string(value.get("sid")).filter(|session_id| !session_id.trim().is_empty());
    (pid.is_some() || session_id.is_some()).then_some((pid, session_id, model_id))
}

fn required_non_negative_i64(value: Option<&Value>) -> Option<i64> {
    extract_i64(value).filter(|value| *value >= 0)
}

fn optional_non_negative_i64(value: Option<&Value>) -> Option<i64> {
    match value {
        Some(value) => required_non_negative_i64(Some(value)),
        None => Some(0),
    }
}

fn unified_log_dedup_key(session_id: &str, value: &Value) -> String {
    let event_id = [
        &["event_id"][..],
        &["eventId"][..],
        &["id"][..],
        &["uuid"][..],
        &["ctx", "event_id"][..],
        &["ctx", "eventId"][..],
        &["ctx", "id"][..],
        &["ctx", "uuid"][..],
    ]
    .into_iter()
    .find_map(|path| {
        get_path(value, path)
            .and_then(|value| extract_string(Some(value)))
            .filter(|id| !id.trim().is_empty())
    });

    let identity = event_id.map_or_else(
        || {
            // Without a source event ID, the complete normalized row is the
            // stable discriminator. Exact duplicate rows still collapse, but
            // rows that happen to share timestamp and token fields do not.
            format!(
                "row:{}",
                serde_json::to_string(value).unwrap_or_else(|_| String::new())
            )
        },
        |event_id| format!("id:{event_id}"),
    );
    format!("{UNIFIED_LOG_DEDUP_PREFIX}{session_id}:{identity}")
}

fn fallback_unified_metadata(session_id: &str, timestamp: i64) -> GrokMetadata {
    GrokMetadata {
        session_id: session_id.to_string(),
        model_id: None,
        timestamp,
        workspace_key: None,
        workspace_label: None,
    }
}

fn read_unified_session_metadata(path: &Path) -> HashMap<String, GrokMetadata> {
    let Some((_, workspaces)) = unified_session_tree(path) else {
        return HashMap::new();
    };

    let mut metadata_by_session = HashMap::new();
    for (workspace_dir, session_dirs) in workspaces {
        let workspace_key = workspace_dir
            .file_name()
            .and_then(|name| name.to_str())
            .map(percent_decode_lossy)
            .and_then(|decoded| normalize_workspace_key(&decoded));
        let workspace_label = workspace_key.as_deref().and_then(workspace_label_from_key);

        for session_dir in session_dirs {
            let Some(session_id) = session_dir
                .file_name()
                .and_then(|name| name.to_str())
                .filter(|id| !id.trim().is_empty())
            else {
                continue;
            };

            let updates_path = session_dir.join("updates.jsonl");
            let metadata = if updates_path.is_file() {
                read_metadata(&updates_path)
            } else {
                let mut metadata =
                    fallback_unified_metadata(session_id, file_modified_timestamp_ms(&session_dir));
                metadata.workspace_key = workspace_key.clone();
                metadata.workspace_label = workspace_label.clone();
                read_summary_metadata(&session_dir.join("summary.json"), &mut metadata);
                read_events_metadata(&session_dir.join("events.jsonl"), &mut metadata);
                read_signals_metadata(&session_dir.join("signals.json"), &mut metadata);
                metadata
            };
            metadata_by_session.insert(session_id.to_string(), metadata);
        }
    }

    metadata_by_session
}

fn unified_session_tree(path: &Path) -> Option<(PathBuf, UnifiedSessionTree)> {
    let grok_home = path.parent().and_then(Path::parent)?;
    let sessions_root = grok_home.join("sessions");
    let mut workspaces = Vec::new();
    let Ok(entries) = std::fs::read_dir(&sessions_root) else {
        return Some((sessions_root, workspaces));
    };

    for entry in entries.flatten() {
        let workspace_dir = entry.path();
        if !workspace_dir.is_dir() {
            continue;
        }
        let mut session_dirs = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&workspace_dir) {
            for entry in entries.flatten() {
                let session_dir = entry.path();
                if session_dir.is_dir() {
                    session_dirs.push(session_dir);
                }
            }
        }
        session_dirs.sort_unstable();
        workspaces.push((workspace_dir, session_dirs));
    }
    workspaces.sort_by(|left, right| left.0.cmp(&right.0));

    Some((sessions_root, workspaces))
}

fn cache_path_suffix(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn non_negative_i64(value: Option<&Value>) -> i64 {
    extract_i64(value).unwrap_or(0).max(0)
}

fn effective_total_from_signals(value: &Value) -> i64 {
    let before = non_negative_i64(value.get("totalTokensBeforeCompaction"));
    let total = non_negative_i64(value.get("totalTokens"));
    match value.get("contextTokensUsed") {
        None => before.saturating_add(total),
        Some(ctx) => total.max(before.saturating_add(non_negative_i64(Some(ctx)))),
    }
}

fn model_id_from_signals(value: &Value) -> Option<String> {
    extract_string(value.get("primaryModelId")).or_else(|| {
        value
            .get("modelsUsed")
            .and_then(|models| models.as_array())
            .and_then(|models| models.first())
            .and_then(|model| extract_string(Some(model)))
    })
}

fn append_signals_reconciliation(
    updates_path: &Path,
    metadata: &GrokMetadata,
    messages: &mut Vec<UnifiedMessage>,
    fallback_model: &str,
) {
    let signals_path = match sibling(updates_path, "signals.json") {
        Some(path) => path,
        None => return,
    };
    let data = match read_file_or_none(&signals_path) {
        Some(data) => data,
        None => return,
    };
    let value: Value = match serde_json::from_slice(&data) {
        Ok(value) => value,
        Err(_) => return,
    };

    let signals_total = effective_total_from_signals(&value);
    if signals_total <= 0 {
        return;
    }

    let updates_total: i64 = messages.iter().map(|message| message.tokens.input).sum();
    let extra = signals_total.saturating_sub(updates_total);
    if extra <= 0 {
        return;
    }

    let model_id = model_id_from_signals(&value)
        .filter(|model| !model.trim().is_empty())
        .or_else(|| metadata.model_id.clone())
        .unwrap_or_else(|| fallback_model.to_string());
    // Anchor the reconciliation delta to the last recorded update activity rather
    // than signals.json's mtime. The mtime advances every time Grok rewrites the
    // rollup for a live session, which would migrate this whole (potentially
    // multi-million-token) extra to a new day on each rescan and retroactively
    // shrink the prior day's total. The last update timestamp only moves when
    // genuine new activity is recorded, so the delta stays put across rescans.
    let timestamp = messages
        .iter()
        .map(|message| message.timestamp)
        .max()
        .unwrap_or(metadata.timestamp);

    let mut message = UnifiedMessage::new_with_dedup(
        CLIENT_ID,
        model_id,
        PROVIDER_ID,
        metadata.session_id.clone(),
        timestamp,
        TokenBreakdown {
            input: extra,
            output: 0,
            cache_read: 0,
            cache_write: 0,
            reasoning: 0,
        },
        0.0,
        Some(format!("grok:{}:signals", metadata.session_id)),
    );
    message.set_workspace(
        metadata.workspace_key.clone(),
        metadata.workspace_label.clone(),
    );
    messages.push(message);
}

fn read_metadata(path: &Path) -> GrokMetadata {
    let session_dir = path.parent();
    let session_id = session_dir
        .and_then(|dir| dir.file_name())
        .and_then(|name| name.to_str())
        .filter(|id| !id.trim().is_empty())
        .unwrap_or("unknown")
        .to_string();

    let workspace_key = session_dir
        .and_then(|dir| dir.parent())
        .and_then(|workspace_dir| workspace_dir.file_name())
        .and_then(|name| name.to_str())
        .map(percent_decode_lossy)
        .and_then(|decoded| normalize_workspace_key(&decoded));
    let workspace_label = workspace_key.as_deref().and_then(workspace_label_from_key);

    let fallback_timestamp = file_modified_timestamp_ms(path);
    let mut metadata = GrokMetadata {
        session_id,
        model_id: None,
        timestamp: fallback_timestamp,
        workspace_key,
        workspace_label,
    };

    if let Some(summary_path) = sibling(path, "summary.json") {
        read_summary_metadata(&summary_path, &mut metadata);
    }
    if let Some(events_path) = sibling(path, "events.jsonl") {
        read_events_metadata(&events_path, &mut metadata);
    }
    if let Some(signals_path) = sibling(path, "signals.json") {
        read_signals_metadata(&signals_path, &mut metadata);
    }

    metadata
}

fn read_signals_metadata(path: &Path, metadata: &mut GrokMetadata) {
    let Some(data) = read_file_or_none(path) else {
        return;
    };
    let Ok(value) = serde_json::from_slice::<Value>(&data) else {
        return;
    };

    if metadata.model_id.is_none() {
        metadata.model_id = model_id_from_signals(&value);
    }
}

fn read_summary_metadata(path: &Path, metadata: &mut GrokMetadata) {
    let Some(data) = read_file_or_none(path) else {
        return;
    };
    let Ok(value) = serde_json::from_slice::<Value>(&data) else {
        return;
    };

    if metadata.model_id.is_none() {
        metadata.model_id = extract_string(value.get("current_model_id"))
            .or_else(|| extract_string(value.get("model_id")));
    }

    if let Some(timestamp) = value
        .get("updated_at")
        .or_else(|| value.get("created_at"))
        .and_then(parse_timestamp_value)
    {
        metadata.timestamp = timestamp;
    }
}

fn read_events_metadata(path: &Path, metadata: &mut GrokMetadata) {
    let Ok(file) = std::fs::File::open(path) else {
        return;
    };

    for line in lossy_lines(BufReader::new(file)).take(500) {
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };

        if metadata.model_id.is_none() {
            metadata.model_id = extract_string(value.get("model_id"));
        }
        if metadata.session_id == "unknown" {
            if let Some(session_id) = extract_string(value.get("session_id")) {
                metadata.session_id = session_id;
            }
        }
        if let Some(timestamp) = value.get("ts").and_then(parse_timestamp_value) {
            metadata.timestamp = timestamp;
        }

        if metadata.model_id.is_some() && metadata.session_id != "unknown" {
            break;
        }
    }
}

fn sibling(path: &Path, file_name: &str) -> Option<PathBuf> {
    Some(path.parent()?.join(file_name))
}

fn extract_model_id(value: &Value) -> Option<String> {
    for path in [
        &["params", "update", "_meta", "modelId"][..],
        &["params", "_meta", "modelId"][..],
        &["params", "modelId"][..],
        &["model_id"][..],
        &["modelId"][..],
        &["model"][..],
    ] {
        if let Some(model_id) = get_path(value, path).and_then(|value| extract_string(Some(value)))
        {
            if !model_id.trim().is_empty() {
                return Some(model_id);
            }
        }
    }
    None
}

fn extract_total_tokens(value: &Value) -> Option<i64> {
    for path in [
        &["params", "_meta", "totalTokens"][..],
        &["params", "update", "_meta", "totalTokens"][..],
        &["params", "update", "totalTokens"][..],
        &["params", "totalTokens"][..],
        &["usage", "totalTokens"][..],
        &["totalTokens"][..],
    ] {
        if let Some(total) = get_path(value, path).and_then(|value| extract_i64(Some(value))) {
            return Some(total);
        }
    }
    None
}

fn extract_timestamp_ms(value: &Value) -> Option<i64> {
    for path in [
        &["params", "_meta", "agentTimestampMs"][..],
        &["params", "update", "_meta", "agentTimestampMs"][..],
        &["params", "timestamp"][..],
        &["timestamp"][..],
        &["ts"][..],
    ] {
        if let Some(timestamp) = get_path(value, path).and_then(parse_timestamp_value) {
            return Some(timestamp);
        }
    }
    None
}

fn is_user_message_chunk(value: &Value) -> bool {
    get_path(value, &["params", "update", "sessionUpdate"]).and_then(|value| value.as_str())
        == Some("user_message_chunk")
}

fn get_path<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    path.iter()
        .try_fold(value, |current, key| current.get(*key))
}

fn percent_decode_lossy(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0usize;

    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(high), Some(low)) = (hex_value(bytes[i + 1]), hex_value(bytes[i + 2])) {
                decoded.push((high << 4) | low);
                i += 3;
                continue;
            }
        }

        decoded.push(bytes[i]);
        i += 1;
    }

    String::from_utf8_lossy(&decoded).into_owned()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
