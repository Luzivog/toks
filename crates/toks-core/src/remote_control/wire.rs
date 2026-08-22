use serde::Deserialize;

use super::{RemoteConnectionStatus, RemoteDevice};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct StatusResponse {
    pub status: Status,
    pub server_name: String,
    pub environment_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) enum Status {
    Disabled,
    Connecting,
    Connected,
    Errored,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct DevicesResponse {
    pub data: Vec<RemoteDeviceWire>,
    pub next_cursor: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RemoteDeviceWire {
    client_id: String,
    display_name: Option<String>,
    device_type: Option<String>,
    platform: Option<String>,
    os_version: Option<String>,
    device_model: Option<String>,
    app_version: Option<String>,
    last_seen_at: Option<i64>,
}

impl From<Status> for RemoteConnectionStatus {
    fn from(value: Status) -> Self {
        match value {
            Status::Disabled => Self::Off,
            Status::Connecting => Self::Connecting,
            Status::Connected => Self::Connected,
            Status::Errored => Self::Errored,
        }
    }
}

impl From<RemoteDeviceWire> for RemoteDevice {
    fn from(value: RemoteDeviceWire) -> Self {
        Self {
            client_id: value.client_id,
            display_name: value.display_name,
            device_type: value.device_type,
            platform: value.platform,
            os_version: value.os_version,
            device_model: value.device_model,
            app_version: value.app_version,
            last_seen_at: value.last_seen_at,
        }
    }
}
