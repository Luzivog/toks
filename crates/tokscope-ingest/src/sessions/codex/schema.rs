use serde::Deserialize;
use serde_json::Value;

/// Codex entry structure from JSONL files.
#[derive(Debug, Deserialize)]
pub struct CodexEntry {
    #[serde(rename = "type")]
    pub entry_type: String,
    pub timestamp: Option<String>,
    pub payload: Option<CodexPayload>,
}

#[derive(Debug, Deserialize)]
pub struct CodexPayload {
    /// Context-dependent upstream id. On events this is the turn `sub_id` and
    /// can repeat across token_count records, so it is not an event identity.
    pub id: Option<String>,
    pub forked_from_id: Option<String>,
    #[serde(rename = "type")]
    pub payload_type: Option<String>,
    pub model: Option<String>,
    pub model_name: Option<String>,
    pub model_info: Option<CodexModelInfo>,
    pub info: Option<CodexInfo>,
    pub turn_id: Option<String>,
    /// Unix seconds from `task_started`. Wrong-typed values decode as absent
    /// rather than rejecting the entire JSONL entry.
    #[serde(default, deserialize_with = "deserialize_lenient_i64")]
    pub started_at: Option<i64>,
    pub source: Option<Value>,
    pub thread_source: Option<String>,
    pub cwd: Option<String>,
    pub model_provider: Option<String>,
    pub agent_nickname: Option<String>,
    pub message: Option<String>,
}

fn deserialize_lenient_i64<'de, D>(deserializer: D) -> Result<Option<i64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<Value>::deserialize(deserializer)?;
    Ok(value.and_then(|value| {
        value
            .as_i64()
            .or_else(|| value.as_u64().map(|number| number as i64))
            .or_else(|| value.as_f64().map(|number| number as i64))
    }))
}

#[derive(Debug, Deserialize)]
pub struct CodexModelInfo {
    pub slug: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CodexInfo {
    pub model: Option<String>,
    pub model_name: Option<String>,
    pub last_token_usage: Option<CodexTokenUsage>,
    pub total_token_usage: Option<CodexTokenUsage>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct CodexTokenUsage {
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub cached_input_tokens: Option<i64>,
    pub cache_read_input_tokens: Option<i64>,
    pub reasoning_output_tokens: Option<i64>,
    pub total_tokens: Option<i64>,
}
