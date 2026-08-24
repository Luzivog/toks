//! Parser for Reasonix's authoritative append-only statistics records.
//!
//! Reasonix writes one JSON object per provider request to
//! `<REASONIX_HOME>/stats/YYYY-MM-DD.jsonl`. Session transcript JSONL is not
//! scanned: it has no authoritative usage counters and would overlap stats.

use super::utils::{lossy_lines, parse_timestamp_value};
use super::UnifiedMessage;
use crate::provider_identity::{canonical_provider, inferred_provider_from_model};
use crate::TokenBreakdown;
use serde::Deserialize;
use std::io::BufReader;
use std::path::Path;

#[derive(Debug, Deserialize)]
struct ReasonixStat {
    ts: serde_json::Value,
    #[serde(default)]
    model: String,
    #[serde(default)]
    prompt: i64,
    #[serde(default)]
    completion: i64,
    #[serde(default)]
    reasoning: i64,
    #[serde(default)]
    cache_hit: i64,
    cache_miss: Option<i64>,
    #[serde(default)]
    total: i64,
    #[serde(default)]
    requests: i64,
    #[serde(default)]
    turn: bool,
}

fn split_model_ref(model_ref: &str) -> (String, String) {
    let model_ref = model_ref.trim();
    if let Some((provider, model)) = model_ref.split_once('/') {
        let provider = canonical_provider(provider).unwrap_or_else(|| provider.to_string());
        return (provider, model.to_string());
    }
    let provider = inferred_provider_from_model(model_ref)
        .unwrap_or("reasonix")
        .to_string();
    (provider, model_ref.to_string())
}

fn non_negative(value: i64) -> i64 {
    value.max(0)
}

pub fn parse_reasonix_file(path: &Path) -> Vec<UnifiedMessage> {
    let Ok(file) = std::fs::File::open(path) else {
        return Vec::new();
    };

    lossy_lines(BufReader::new(file))
        .enumerate()
        .filter_map(|(line_index, line)| {
            let record: ReasonixStat = serde_json::from_str(line.trim()).ok()?;
            if record.turn
                || record.model.trim().is_empty()
                || (record.total <= 0 && record.requests <= 0)
            {
                return None;
            }
            let timestamp = parse_timestamp_value(&record.ts)?;
            let (provider_id, model_id) = split_model_ref(&record.model);
            let cache_read = non_negative(record.cache_hit);
            let raw_input = non_negative(record.prompt);
            // An explicit nonzero cache miss is Reasonix's authoritative
            // ordinary-input bucket. Older records omit it, so derive that
            // bucket from prompt tokens and cache hits in that case.
            let input = match record.cache_miss {
                Some(cache_miss) if cache_miss != 0 => non_negative(cache_miss),
                _ => raw_input.saturating_sub(cache_read),
            };
            let reasoning = non_negative(record.reasoning).min(non_negative(record.completion));
            let tokens = TokenBreakdown {
                input,
                output: non_negative(record.completion).saturating_sub(reasoning),
                cache_read,
                cache_write: 0,
                reasoning,
            };
            let mut message = UnifiedMessage::new_with_dedup(
                "reasonix",
                model_id,
                provider_id,
                format!("reasonix-stats:{}", path.display()),
                timestamp,
                tokens,
                0.0,
                Some(format!(
                    "reasonix:{}:{}:{}:{}",
                    path.display(),
                    line_index,
                    record.requests,
                    record.total
                )),
            );
            message.message_count = record.requests.clamp(1, i64::from(i32::MAX)) as i32;
            Some(message)
        })
        .collect()
}

#[cfg(test)]
mod reasonix_tests;
