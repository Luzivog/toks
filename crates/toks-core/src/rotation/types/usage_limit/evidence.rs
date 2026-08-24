use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::safe_identifier;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UsageLimitClassification {
    StructuredError,
    ErrorMessage,
}

impl UsageLimitClassification {
    pub const fn label(self) -> &'static str {
        match self {
            Self::StructuredError => "structured error",
            Self::ErrorMessage => "message match",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageLimitEvidence {
    classification: UsageLimitClassification,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    status: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    frame_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    error_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    error_code: Option<String>,
    payload_sha256: String,
}

impl UsageLimitEvidence {
    pub(crate) fn from_upstream(
        classification: UsageLimitClassification,
        status: Option<u16>,
        frame_type: Option<&str>,
        error_type: Option<&str>,
        error_code: Option<&str>,
        payload: &[u8],
    ) -> Self {
        let mut digest = Sha256::new();
        digest.update(b"toks.usage-limit-evidence.v1\0");
        digest.update(payload);
        Self {
            classification,
            status,
            frame_type: safe_identifier(frame_type),
            error_type: safe_identifier(error_type),
            error_code: safe_identifier(error_code),
            payload_sha256: format!("sha256:{:x}", digest.finalize()),
        }
    }

    pub const fn classification(&self) -> UsageLimitClassification {
        self.classification
    }

    pub fn frame_type(&self) -> Option<&str> {
        self.frame_type.as_deref()
    }

    pub const fn status(&self) -> Option<u16> {
        self.status
    }
}
