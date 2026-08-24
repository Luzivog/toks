//! Cline task parser
//!
//! Cline is the upstream project that Roo Code and Kilo forked from, so it
//! shares the same VS Code globalStorage task-log format and reuses the same
//! parser helper.

use super::roocode::parse_roo_kilo_file;
use super::utils::{extract_f64, extract_i64, file_modified_timestamp_ms, parse_timestamp_value};
use super::{normalize_workspace_key, workspace_label_from_key, UnifiedMessage};
use crate::TokenBreakdown;
use serde::Deserialize;
use serde_json::Value;
use std::path::{Path, PathBuf};

pub fn parse_cline_file(path: &Path) -> Vec<UnifiedMessage> {
    if is_cline_cli_messages_path(path) {
        return parse_cline_cli_file(path);
    }

    parse_roo_kilo_file(path, "cline")
}

#[derive(Debug, Deserialize)]
struct ClineCliMessagesFile {
    #[serde(rename = "sessionId")]
    session_id: Option<String>,
    agent: Option<String>,
    messages: Option<Vec<ClineCliMessage>>,
}

#[derive(Debug, Deserialize)]
struct ClineCliMessage {
    id: Option<String>,
    role: Option<String>,
    ts: Option<Value>,
    content: Option<Vec<Value>>,
    #[serde(rename = "modelInfo")]
    model_info: Option<ClineCliModelInfo>,
    metrics: Option<ClineCliMetrics>,
}

fn is_human_user_prompt(content: Option<&[Value]>) -> bool {
    let Some(content) = content else {
        return false;
    };

    let mut has_text_block = false;
    for block in content {
        match block.get("type").and_then(Value::as_str) {
            Some("tool_result") => return false,
            Some("text") => has_text_block = true,
            _ => {}
        }
    }
    has_text_block
}

#[derive(Debug, Deserialize)]
struct ClineCliModelInfo {
    id: Option<String>,
    provider: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ClineCliMetrics {
    #[serde(rename = "inputTokens")]
    input_tokens: Option<Value>,
    #[serde(rename = "outputTokens")]
    output_tokens: Option<Value>,
    #[serde(rename = "cacheReadTokens")]
    cache_read_tokens: Option<Value>,
    #[serde(rename = "cacheWriteTokens")]
    cache_write_tokens: Option<Value>,
    cost: Option<Value>,
}

#[derive(Debug, Default, Deserialize)]
struct ClineCliManifest {
    session_id: Option<String>,
    provider: Option<String>,
    model: Option<String>,
    cwd: Option<String>,
    workspace_root: Option<String>,
    metadata: Option<Value>,
}

pub(crate) fn is_cline_cli_messages_path(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with(".messages.json"))
}

pub(crate) fn cline_cli_manifest_path(path: &Path) -> PathBuf {
    let stem = path
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let session_stem = stem.strip_suffix(".messages").unwrap_or(stem);
    path.parent()
        .unwrap_or_else(|| Path::new("."))
        .join(format!("{session_stem}.json"))
}

fn read_cline_cli_manifest(path: &Path) -> ClineCliManifest {
    let manifest_path = cline_cli_manifest_path(path);
    let Ok(mut bytes) = std::fs::read(manifest_path) else {
        return ClineCliManifest::default();
    };

    simd_json::from_slice(&mut bytes).unwrap_or_default()
}

fn non_empty_string(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn extract_non_negative_finite_f64(value: Option<&Value>) -> Option<f64> {
    extract_f64(value).filter(|value| value.is_finite() && *value >= 0.0)
}

/// Parse Cline CLI's persisted assistant messages from
/// `~/.cline/data/sessions/<session>/<session>.messages.json`.
pub fn parse_cline_cli_file(path: &Path) -> Vec<UnifiedMessage> {
    let Some(data) = super::utils::read_file_or_none(path) else {
        return Vec::new();
    };

    let mut bytes = data;
    let Ok(file) = simd_json::from_slice::<ClineCliMessagesFile>(&mut bytes) else {
        return Vec::new();
    };
    let manifest = read_cline_cli_manifest(path);

    let session_id = file
        .session_id
        .or(manifest.session_id)
        .or_else(|| {
            path.file_stem()
                .and_then(|name| name.to_str())
                .map(|name| name.trim_end_matches(".messages").to_string())
        })
        .unwrap_or_else(|| "unknown".to_string());
    let fallback_timestamp = file_modified_timestamp_ms(path);
    let workspace_key = manifest
        .workspace_root
        .as_deref()
        .or(manifest.cwd.as_deref())
        .and_then(normalize_workspace_key);
    let workspace_label = workspace_key.as_deref().and_then(workspace_label_from_key);
    let session_title = manifest
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("title"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);

    let mut current_model =
        non_empty_string(manifest.model.as_deref()).unwrap_or_else(|| "unknown".to_string());
    let mut current_provider =
        non_empty_string(manifest.provider.as_deref()).unwrap_or_else(|| "unknown".to_string());
    let mut pending_turn_start = false;
    let mut assistant_index = 0usize;
    let mut messages = Vec::new();

    for entry in file.messages.unwrap_or_default() {
        if entry.role.as_deref() == Some("user") {
            if is_human_user_prompt(entry.content.as_deref()) {
                pending_turn_start = true;
            }
            continue;
        }
        if entry.role.as_deref() != Some("assistant") {
            continue;
        }

        if let Some(model_info) = entry.model_info.as_ref() {
            if let Some(model) = non_empty_string(model_info.id.as_deref()) {
                current_model = model;
            }
            if let Some(provider) = non_empty_string(model_info.provider.as_deref()) {
                current_provider = provider;
            }
        }

        let Some(metrics) = entry.metrics else {
            continue;
        };
        let input_tokens = extract_i64(metrics.input_tokens.as_ref())
            .unwrap_or(0)
            .max(0);
        let output = extract_i64(metrics.output_tokens.as_ref())
            .unwrap_or(0)
            .max(0);
        let cache_read = extract_i64(metrics.cache_read_tokens.as_ref())
            .unwrap_or(0)
            .max(0);
        let cache_write = extract_i64(metrics.cache_write_tokens.as_ref())
            .unwrap_or(0)
            .max(0);
        let input = input_tokens
            .saturating_sub(cache_read)
            .saturating_sub(cache_write);
        let reported_cost = extract_non_negative_finite_f64(metrics.cost.as_ref());
        let cost = reported_cost.unwrap_or(0.0);

        let total_tokens = input
            .saturating_add(output)
            .saturating_add(cache_read)
            .saturating_add(cache_write);
        if total_tokens == 0 && reported_cost.is_none() {
            continue;
        }

        let timestamp = entry
            .ts
            .as_ref()
            .and_then(parse_timestamp_value)
            .unwrap_or(fallback_timestamp);
        let dedup_key = entry
            .id
            .filter(|id| !id.trim().is_empty())
            .map(|id| format!("cline-cli:{session_id}:{id}"))
            .unwrap_or_else(|| format!("cline-cli:{session_id}:{assistant_index}"));
        let mut message = UnifiedMessage::new_with_agent(
            "cline",
            current_model.clone(),
            current_provider.clone(),
            session_id.clone(),
            timestamp,
            TokenBreakdown {
                input,
                output,
                cache_read,
                cache_write,
                reasoning: 0,
            },
            cost,
            file.agent.clone(),
        );
        message.dedup_key = Some(dedup_key);
        message.is_turn_start = pending_turn_start;
        message.session_title = session_title.clone();
        message.set_workspace(workspace_key.clone(), workspace_label.clone());
        if reported_cost.is_some() {
            message.mark_provider_reported_cost();
        }
        messages.push(message);
        assistant_index += 1;
        pending_turn_start = false;
    }

    messages
}

#[cfg(test)]
mod cline_tests;
