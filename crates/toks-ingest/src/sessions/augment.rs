//! Augment Code (Auggie CLI) session parser
//!
//! Parses local session snapshots from `~/.augment/sessions/<sessionId>.json`.
//!
//! ## Token accounting
//!
//! Each completed turn stores one authoritative `token_usage` observation on a
//! `response_nodes[]` entry. Verified against real sessions: turns almost never
//! carry multiple usage nodes. If they do, we take the **last** non-empty usage
//! (final streamed totals) rather than summing — summing would double-count if
//! the format ever repeated cumulative values.
//!
//! Input and cache buckets are reported independently (Anthropic-style split
//! accounting). Do not subtract `cache_read` from `input`.
//!
//! ## Completeness gate
//!
//! Only turns with `completed: true` are counted. Snapshots may retain
//! in-progress or aborted turns that already carry a partial `token_usage`.
//!
//! ## Timestamps
//!
//! Auggie records `finishedAt` only — there is no per-turn start time or
//! duration in the on-disk schema, so messages are **end-anchored**. Cost and
//! token totals are unaffected; duration-based metrics stay empty.
//!
//! ## Cost
//!
//! This parser always emits `cost = 0`. Downstream pricing estimates public
//! model API list prices from the model id. Augment credits /
//! `subAgentCostUsd` are intentionally ignored.
//!
//! ## Turn shape
//!
//! One unified message is emitted per completed turn, so `is_turn_start` is
//! always set. If a future format needs multiple messages per turn, revisit
//! that flag before counting turns.

use super::utils::{file_modified_timestamp_ms, parse_timestamp_str, read_file_or_none};
use super::UnifiedMessage;
use crate::{provider_identity, TokenBreakdown};
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize)]
struct AugmentSession {
    #[serde(rename = "sessionId")]
    session_id: Option<String>,
    #[serde(rename = "agentState")]
    agent_state: Option<AugmentAgentState>,
    #[serde(default, rename = "chatHistory")]
    chat_history: Vec<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct AugmentAgentState {
    #[serde(rename = "modelId")]
    model_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AugmentTurn {
    #[serde(rename = "finishedAt")]
    finished_at: Option<String>,
    /// Only completed turns are counted. Snapshots may retain in-progress or
    /// aborted turns that already carry a partial `token_usage` observation.
    #[serde(default)]
    completed: Option<bool>,
    exchange: Option<AugmentExchange>,
    #[serde(rename = "sequenceId")]
    sequence_id: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct AugmentExchange {
    model_id: Option<String>,
    request_id: Option<String>,
    #[serde(default)]
    response_nodes: Vec<AugmentResponseNode>,
}

/// Response node subset. Extra wire fields are ignored so unknown node shapes
/// do not fail the turn.
#[derive(Debug, Deserialize)]
struct AugmentResponseNode {
    token_usage: Option<AugmentTokenUsage>,
}

#[derive(Debug, Deserialize)]
struct AugmentTokenUsage {
    input_tokens: Option<i64>,
    output_tokens: Option<i64>,
    cache_read_input_tokens: Option<i64>,
    cache_creation_input_tokens: Option<i64>,
}

fn model_id(model: Option<&str>) -> String {
    let model = model.unwrap_or("unknown").trim();
    if model.is_empty() {
        "unknown".to_string()
    } else {
        model.to_string()
    }
}

fn provider_for_model(model: &str) -> String {
    // Unrecognized ids fall back to "augment". Pricing tables will not match
    // that provider, so estimated cost stays $0 while tokens still count.
    provider_identity::inferred_provider_from_model(model)
        .unwrap_or("augment")
        .to_string()
}

fn tokens_from_usage(usage: &AugmentTokenUsage) -> TokenBreakdown {
    TokenBreakdown {
        input: usage.input_tokens.unwrap_or(0).max(0),
        output: usage.output_tokens.unwrap_or(0).max(0),
        cache_read: usage.cache_read_input_tokens.unwrap_or(0).max(0),
        cache_write: usage.cache_creation_input_tokens.unwrap_or(0).max(0),
        reasoning: 0,
    }
}

fn usage_is_nonzero(usage: &AugmentTokenUsage) -> bool {
    tokens_from_usage(usage).total() > 0
}

/// Prefer the last non-empty observation so a later full total wins over an
/// earlier partial if the format ever streams multiple usage nodes.
fn last_token_usage(nodes: &[AugmentResponseNode]) -> Option<&AugmentTokenUsage> {
    nodes
        .iter()
        .rev()
        .find_map(|node| node.token_usage.as_ref().filter(|u| usage_is_nonzero(u)))
}

fn session_id_from_path(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn turn_dedup_key(session_id: &str, turn: &AugmentTurn, index: usize) -> String {
    if let Some(request_id) = turn
        .exchange
        .as_ref()
        .and_then(|e| e.request_id.as_deref())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return format!("augment:{session_id}:req:{request_id}");
    }
    if let Some(seq) = turn.sequence_id.as_ref() {
        let seq_str = match seq {
            serde_json::Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        if !seq_str.is_empty() && seq_str != "null" {
            return format!("augment:{session_id}:seq:{seq_str}");
        }
    }
    format!("augment:{session_id}:turn:{index}")
}

/// Parse an Augment/Auggie session JSON file into unified messages (one per turn).
///
/// Best-effort: unreadable files, invalid JSON, and malformed turns yield no
/// messages for that input rather than hard errors (same contract as peer
/// local session parsers).
pub fn parse_augment_file(path: &Path) -> Vec<UnifiedMessage> {
    let Some(data) = read_file_or_none(path) else {
        return vec![];
    };

    let session: AugmentSession = match serde_json::from_slice(&data) {
        Ok(s) => s,
        Err(_) => return vec![],
    };

    let session_id = session
        .session_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| session_id_from_path(path));

    if session_id.is_empty() {
        return vec![];
    }

    let default_model = model_id(
        session
            .agent_state
            .as_ref()
            .and_then(|s| s.model_id.as_deref()),
    );
    let fallback_ts = file_modified_timestamp_ms(path);

    let mut messages = Vec::with_capacity(session.chat_history.len());
    for (index, raw_turn) in session.chat_history.into_iter().enumerate() {
        let turn: AugmentTurn = match serde_json::from_value(raw_turn) {
            Ok(t) => t,
            Err(_) => continue,
        };

        // Skip incomplete/aborted turns even if a partial token_usage was streamed.
        if turn.completed != Some(true) {
            continue;
        }

        let Some(exchange) = turn.exchange.as_ref() else {
            continue;
        };
        let Some(usage) = last_token_usage(&exchange.response_nodes) else {
            continue;
        };

        let tokens = tokens_from_usage(usage);

        let model = model_id(
            exchange
                .model_id
                .as_deref()
                .filter(|m| !m.trim().is_empty())
                .or(Some(default_model.as_str())),
        );
        let provider = provider_for_model(&model);
        let timestamp = turn
            .finished_at
            .as_deref()
            .and_then(parse_timestamp_str)
            .unwrap_or(fallback_ts);

        let mut msg = UnifiedMessage::new_with_dedup(
            "augment",
            model,
            provider,
            session_id.clone(),
            timestamp,
            tokens,
            0.0,
            Some(turn_dedup_key(&session_id, &turn, index)),
        );
        // One message per completed turn (see module docs).
        msg.is_turn_start = true;
        messages.push(msg);
    }

    messages
}

#[cfg(test)]
mod augment_tests;
