use serde::{Deserialize, Serialize};

use super::ThreadId;

mod evidence;
pub use evidence::{UsageLimitClassification, UsageLimitEvidence};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UsageLimitPhase {
    HttpResponse,
    HttpStream,
    WebSocketHandshake,
    WebSocketFrame,
}

impl UsageLimitPhase {
    pub const fn label(self) -> &'static str {
        match self {
            Self::HttpResponse => "HTTP response",
            Self::HttpStream => "HTTP stream",
            Self::WebSocketHandshake => "WebSocket handshake",
            Self::WebSocketFrame => "WebSocket frame",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UsageLimitTierOrigin {
    Client,
    ToksForcedFast,
    ToksStandardFallback,
    Unspecified,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageLimitTier {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    effective: Option<String>,
    origin: UsageLimitTierOrigin,
}

impl UsageLimitTier {
    pub(crate) fn new(effective: Option<&str>, origin: UsageLimitTierOrigin) -> Self {
        let effective = safe_identifier(effective);
        let origin = if effective.is_none() {
            UsageLimitTierOrigin::Unspecified
        } else {
            origin
        };
        Self { effective, origin }
    }

    pub(crate) fn client(effective: Option<&str>) -> Self {
        Self::new(effective, UsageLimitTierOrigin::Client)
    }

    pub(crate) fn unspecified() -> Self {
        Self::new(None, UsageLimitTierOrigin::Unspecified)
    }

    pub fn effective(&self) -> Option<&str> {
        self.effective.as_deref()
    }

    pub const fn origin(&self) -> UsageLimitTierOrigin {
        self.origin
    }

    pub fn label(&self) -> &str {
        match self.effective.as_deref() {
            Some("fast" | "priority" | "ultrafast") => "Fast",
            Some("default" | "standard") => "Standard",
            Some(tier) => tier,
            None => "unspecified tier",
        }
    }
}

impl UsageLimitTierOrigin {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Client => "client requested",
            Self::ToksForcedFast => "Toks forced Fast",
            Self::ToksStandardFallback => "Toks Standard fallback",
            Self::Unspecified => "origin unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageLimitIncident {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    thread_id: Option<ThreadId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    model: Option<String>,
    tier: UsageLimitTier,
    phase: UsageLimitPhase,
    evidence: UsageLimitEvidence,
}

impl UsageLimitIncident {
    pub(crate) fn new(
        thread_id: Option<ThreadId>,
        model: Option<&str>,
        tier: UsageLimitTier,
        phase: UsageLimitPhase,
        evidence: UsageLimitEvidence,
    ) -> Self {
        Self {
            thread_id,
            model: safe_identifier(model),
            tier,
            phase,
            evidence,
        }
    }

    pub fn thread_id(&self) -> Option<&ThreadId> {
        self.thread_id.as_ref()
    }

    pub fn model(&self) -> Option<&str> {
        self.model.as_deref()
    }

    pub const fn tier(&self) -> &UsageLimitTier {
        &self.tier
    }

    pub const fn phase(&self) -> UsageLimitPhase {
        self.phase
    }

    pub const fn evidence(&self) -> &UsageLimitEvidence {
        &self.evidence
    }
}

fn safe_identifier(value: Option<&str>) -> Option<String> {
    value
        .filter(|value| !value.is_empty() && value.len() <= 128)
        .filter(|value| {
            value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
        })
        .map(str::to_owned)
}
