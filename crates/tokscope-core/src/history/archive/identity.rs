use sha2::{Digest, Sha256};
use tokscope_ingest::sessions::{DurableIdentityScheme, IdentityStrength, UnifiedMessage};

use super::candidate::Candidate;

pub(super) const IDENTITY_VERSION: i64 = 1;

pub(super) struct Identity {
    pub hash: String,
    pub scheme: &'static str,
    pub version: i64,
    pub confidence: i64,
}

impl Identity {
    pub fn for_observation(message: &UnifiedMessage, candidate: &Candidate) -> Self {
        if let Some(durable) = &message.durable_identity {
            let scheme = durable_scheme(&durable.scheme);
            let confidence = match durable.scheme {
                DurableIdentityScheme::ClaudeProviderResponse => match durable.strength {
                    IdentityStrength::Strong => 2,
                    IdentityStrength::SessionStable => 1,
                },
                DurableIdentityScheme::CodexSessionTimestampOccurrence
                | DurableIdentityScheme::CodexSessionRecordSequence => 1,
            };
            return Self {
                hash: durable_hash(scheme, durable.version, &durable.value),
                scheme,
                version: i64::from(durable.version),
                confidence,
            };
        }

        if let Some(key) = message
            .dedup_key
            .as_deref()
            .filter(|key| legacy_claude_stable(message, key))
        {
            return Self {
                hash: durable_hash("claude-provider-response", 1, key),
                scheme: "claude-provider-response",
                version: IDENTITY_VERSION,
                confidence: 2,
            };
        }

        if let Some(key) = message
            .dedup_key
            .as_deref()
            .filter(|key| claude_path_scoped(message, key))
        {
            return Self {
                hash: hash_parts(["event", "claude-path-v1", &source_hash(message), key]),
                scheme: "claude-path-scoped-tool-result",
                version: IDENTITY_VERSION,
                confidence: 0,
            };
        }

        Self {
            hash: hash_parts([
                "event",
                "weak-v1",
                &source_hash(message),
                candidate.accounting_hash.as_str(),
            ]),
            scheme: "canonical-fact",
            version: IDENTITY_VERSION,
            confidence: 0,
        }
    }
}

fn durable_hash(scheme: &str, version: u32, value: &str) -> String {
    hash_parts(["event", "durable-v1", scheme, &version.to_string(), value])
}

pub(super) fn accounting_alias_hash(scheme: &str, version: u32, value: &str) -> String {
    hash_parts([
        "accounting-alias",
        "v1",
        scheme,
        &version.to_string(),
        value,
    ])
}

fn legacy_claude_stable(message: &UnifiedMessage, key: &str) -> bool {
    claude_dedup_domain(&message.client) && !(key.is_empty() || key.contains(":tool_result:"))
}

fn claude_path_scoped(message: &UnifiedMessage, key: &str) -> bool {
    claude_dedup_domain(&message.client) && key.contains(":tool_result:")
}

fn claude_dedup_domain(client: &str) -> bool {
    client == "claude" || client.starts_with("cc-mirror/")
}

fn durable_scheme(scheme: &DurableIdentityScheme) -> &'static str {
    match scheme {
        DurableIdentityScheme::ClaudeProviderResponse => "claude-provider-response",
        DurableIdentityScheme::CodexSessionTimestampOccurrence => {
            "codex-session-timestamp-occurrence"
        }
        DurableIdentityScheme::CodexSessionRecordSequence => "codex-session-record-sequence",
    }
}

pub(super) fn source_hash(message: &UnifiedMessage) -> String {
    hash_parts(["source", "v1", &message.client, &message.session_id])
}

pub(super) fn event_id(identity_hash: &str) -> String {
    hash_parts(["event-row", "v1", identity_hash])
}

pub(super) fn fact_hash(parts: impl IntoIterator<Item = String>) -> String {
    hash_owned_parts(parts)
}

fn hash_owned_parts(parts: impl IntoIterator<Item = String>) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hash_part(&mut hasher, part.as_bytes());
    }
    format!("{:x}", hasher.finalize())
}

fn hash_parts<'a>(parts: impl IntoIterator<Item = &'a str>) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hash_part(&mut hasher, part.as_bytes());
    }
    format!("{:x}", hasher.finalize())
}

fn hash_part(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update((bytes.len() as u64).to_be_bytes());
    hasher.update(bytes);
}
