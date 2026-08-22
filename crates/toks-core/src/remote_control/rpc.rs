use std::{path::Path, time::Duration};

use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use tokio::{net::UnixStream, time::timeout};
use tokio_tungstenite::{client_async, tungstenite::Message, WebSocketStream};

const RESPONSE_TIMEOUT: Duration = Duration::from_secs(12);

pub(super) async fn request<T: DeserializeOwned>(
    socket: &Path,
    method: &str,
    params: Option<Value>,
) -> Result<T> {
    timeout(RESPONSE_TIMEOUT, request_inner(socket, method, params))
        .await
        .with_context(|| format!("timed out waiting for {method}"))?
}

async fn request_inner<T: DeserializeOwned>(
    socket: &Path,
    method: &str,
    params: Option<Value>,
) -> Result<T> {
    let stream = UnixStream::connect(socket)
        .await
        .with_context(|| format!("connecting to {}", socket.display()))?;
    let (mut websocket, _) = client_async("ws://localhost/", stream)
        .await
        .context("opening the Codex app-server control socket")?;
    send(
        &mut websocket,
        json!({
            "id": 1,
            "method": "initialize",
            "params": {
                "clientInfo": {
                    "name": "toks",
                    "title": "Toks",
                    "version": env!("CARGO_PKG_VERSION")
                },
                "capabilities": { "experimentalApi": true }
            }
        }),
    )
    .await?;
    read_response::<Value>(&mut websocket, 1, "initialize").await?;
    send(&mut websocket, json!({ "method": "initialized" })).await?;
    let mut request = json!({ "id": 2, "method": method });
    if let Some(params) = params {
        request["params"] = params;
    }
    send(&mut websocket, request).await?;
    let response = read_response(&mut websocket, 2, method).await;
    websocket.close(None).await.ok();
    response
}

async fn send(websocket: &mut WebSocketStream<UnixStream>, message: Value) -> Result<()> {
    websocket
        .send(Message::Text(message.to_string().into()))
        .await
        .context("sending an app-server request")
}

async fn read_response<T: DeserializeOwned>(
    websocket: &mut WebSocketStream<UnixStream>,
    id: i64,
    method: &str,
) -> Result<T> {
    loop {
        let frame = websocket
            .next()
            .await
            .context("app-server closed the control socket")??;
        let Message::Text(payload) = frame else {
            continue;
        };
        let message: Value = serde_json::from_str(&payload)
            .with_context(|| format!("reading the {method} response"))?;
        if message.get("id").and_then(Value::as_i64) != Some(id) {
            continue;
        }
        if let Some(error) = message.get("error") {
            let code = error
                .get("code")
                .and_then(Value::as_i64)
                .unwrap_or_default();
            let detail = error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("unknown app-server error");
            anyhow::bail!("{method} failed ({code}): {detail}");
        }
        return serde_json::from_value(message.get("result").cloned().unwrap_or(Value::Null))
            .with_context(|| format!("decoding the {method} response"));
    }
}
