use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct GenerationId(u64);

impl GenerationId {
    pub(crate) const fn from_raw(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GenerationStatus {
    Staged,
    Active,
    Draining,
    Retired,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ActivationPhase {
    Prepared,
    PreviousPaused,
    TargetAccepting,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeployPlan {
    StageTarget {
        target: GenerationId,
        build: BuildId,
    },
    PauseAdmissions {
        previous: Option<GenerationId>,
        target: GenerationId,
    },
    StartAccepting {
        target: GenerationId,
    },
    ResumeAdmissions {
        previous: GenerationId,
        failed_target: GenerationId,
    },
    Retire {
        generation: GenerationId,
    },
    Settled {
        active: Option<GenerationId>,
    },
    Unavailable {
        failed_target: GenerationId,
        active: Option<GenerationId>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeploymentEvent {
    Prepared {
        target: GenerationId,
    },
    PreviousPaused {
        target: GenerationId,
    },
    TargetAccepting {
        target: GenerationId,
    },
    AdmissionsResumed {
        failed_target: GenerationId,
    },
    ConnectionsObserved {
        generation: GenerationId,
        active: u64,
    },
    Retired {
        generation: GenerationId,
    },
    Failed {
        generation: GenerationId,
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenerationSnapshot {
    pub id: GenerationId,
    pub build: BuildId,
    pub status: GenerationStatus,
    pub active_connections: Option<u64>,
    pub failure: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivationSnapshot {
    pub target: GenerationId,
    pub previous: Option<GenerationId>,
    pub phase: ActivationPhase,
    pub failure: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeploymentSnapshot {
    pub generations: Vec<GenerationSnapshot>,
    pub activation: Option<ActivationSnapshot>,
    pub last_rollback: Option<GenerationId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeployError {
    InvalidBuildId,
    InvalidRetryId,
    DeploymentBusy,
    UnknownGeneration(GenerationId),
    InvalidTransition(&'static str),
    InvalidPersistedState(&'static str),
    GenerationIdsExhausted,
}

impl std::fmt::Display for DeployError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for DeployError {}
