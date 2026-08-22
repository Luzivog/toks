#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RemoteConnectionStatus {
    #[default]
    Off,
    Connecting,
    Connected,
    Errored,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RemoteConnection {
    pub status: RemoteConnectionStatus,
    pub server_name: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteDevice {
    pub client_id: String,
    pub display_name: Option<String>,
    pub device_type: Option<String>,
    pub platform: Option<String>,
    pub os_version: Option<String>,
    pub device_model: Option<String>,
    pub app_version: Option<String>,
    pub last_seen_at: Option<i64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum RemoteDevices {
    #[default]
    NotLoaded,
    Loaded(Vec<RemoteDevice>),
    Failed(String),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RemoteControlSnapshot {
    pub connection: RemoteConnection,
    pub environment_id: Option<String>,
    pub devices: RemoteDevices,
}

#[derive(Clone, PartialEq, Eq)]
pub struct RemotePairing {
    pub(crate) pairing_code: String,
    pub manual_code: String,
    pub environment_id: String,
    pub expires_at: i64,
}

impl std::fmt::Debug for RemotePairing {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RemotePairing")
            .field("pairing_code", &"[REDACTED]")
            .field("manual_code", &"[REDACTED]")
            .field("environment_id", &self.environment_id)
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RemoteControlFailureKind {
    SignInRequired,
    VerificationRequired,
    DisabledByAdministrator,
    CodexUnavailable,
    DaemonUnavailable,
    Retryable,
    Other,
}

#[derive(Debug)]
pub struct RemoteControlFailure {
    pub kind: RemoteControlFailureKind,
    pub detail: String,
}

impl std::fmt::Display for RemoteControlFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.detail.fmt(formatter)
    }
}

impl std::error::Error for RemoteControlFailure {}

impl From<anyhow::Error> for RemoteControlFailure {
    fn from(error: anyhow::Error) -> Self {
        let detail = error.to_string();
        let lower = detail.to_ascii_lowercase();
        let kind = if lower.contains("requires chatgpt authentication")
            || lower.contains("sign in to chatgpt")
        {
            RemoteControlFailureKind::SignInRequired
        } else if lower.contains("multi-factor")
            || lower.contains("mfa")
            || lower.contains("verification")
        {
            RemoteControlFailureKind::VerificationRequired
        } else if lower.contains("managed requirements") || lower.contains("administrator") {
            RemoteControlFailureKind::DisabledByAdministrator
        } else if lower.contains("codex cli was not found") || lower.contains("not executable") {
            RemoteControlFailureKind::CodexUnavailable
        } else if lower.contains("control socket") || lower.contains("no codex home") {
            RemoteControlFailureKind::DaemonUnavailable
        } else if lower.contains("timed out")
            || lower.contains("-32001")
            || lower.contains("overloaded")
        {
            RemoteControlFailureKind::Retryable
        } else {
            RemoteControlFailureKind::Other
        };
        Self { kind, detail }
    }
}

impl RemotePairing {
    pub fn new(
        pairing_code: String,
        manual_code: String,
        environment_id: String,
        expires_at: i64,
    ) -> Self {
        Self {
            pairing_code,
            manual_code,
            environment_id,
            expires_at,
        }
    }

    pub fn has_expired(&self, now_seconds: i64) -> bool {
        now_seconds >= self.expires_at
    }

    pub(crate) fn pairing_code(&self) -> &str {
        &self.pairing_code
    }
}
