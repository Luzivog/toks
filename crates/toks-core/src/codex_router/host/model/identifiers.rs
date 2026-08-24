use serde::{Deserialize, Serialize};

use super::DeployError;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BuildId(String);

impl BuildId {
    pub fn new(value: impl Into<String>) -> Result<Self, DeployError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(DeployError::InvalidBuildId);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub(crate) struct RetryId(String);

const LEGACY_RETRY_ID: &str = "legacy-v1";

impl Default for RetryId {
    fn default() -> Self {
        Self(LEGACY_RETRY_ID.into())
    }
}

impl RetryId {
    pub(crate) fn new(value: impl Into<String>) -> Result<Self, DeployError> {
        let value = value.into();
        if !Self::valid(&value) {
            return Err(DeployError::InvalidRetryId);
        }
        Ok(Self(value))
    }

    pub(crate) fn fresh() -> Self {
        Self(uuid::Uuid::new_v4().hyphenated().to_string())
    }

    pub(crate) fn is_valid(&self) -> bool {
        Self::valid(&self.0)
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }

    fn valid(value: &str) -> bool {
        value == LEGACY_RETRY_ID
            || uuid::Uuid::parse_str(value)
                .ok()
                .is_some_and(|uuid| uuid.hyphenated().to_string() == value)
    }

    #[cfg(test)]
    pub(crate) fn for_test(value: u128) -> Self {
        Self(uuid::Uuid::from_u128(value).hyphenated().to_string())
    }
}
