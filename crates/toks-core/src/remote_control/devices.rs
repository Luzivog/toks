use std::{collections::HashSet, path::Path};

use anyhow::Result;
use serde_json::json;

use super::{rpc, wire::DevicesResponse, RemoteDevice};

pub(super) async fn list(socket: &Path, environment: &str) -> Result<Vec<RemoteDevice>> {
    let mut devices = Vec::new();
    let mut cursor = None;
    let mut seen_cursors = HashSet::new();
    loop {
        let response: DevicesResponse = rpc::request(
            socket,
            "remoteControl/client/list",
            Some(json!({
                "environmentId": environment,
                "cursor": cursor,
                "limit": 100,
                "order": "desc"
            })),
        )
        .await?;
        devices.extend(response.data.into_iter().map(Into::into));
        cursor = response.next_cursor;
        let Some(next) = cursor.as_deref() else {
            return Ok(devices);
        };
        if !seen_cursors.insert(next.to_string()) {
            anyhow::bail!("remoteControl/client/list repeated a pagination cursor");
        }
    }
}
