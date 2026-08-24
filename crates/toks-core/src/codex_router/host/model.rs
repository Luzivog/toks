use serde::{Deserialize, Serialize};

mod identifiers;

pub use identifiers::BuildId;
pub(crate) use identifiers::RetryId;

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

impl From<GenerationId> for crate::codex_router::handoff::GenerationId {
    fn from(generation: GenerationId) -> Self {
        Self::new(generation.get())
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
