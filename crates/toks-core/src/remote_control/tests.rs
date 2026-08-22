use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::{future::Future, task::Poll, time::Duration};
use tokio::net::UnixListener;
use tokio_tungstenite::{accept_async, tungstenite::Message};

use super::{commands, devices, rpc};

#[test]
fn rpc_request_works_from_a_non_tokio_executor() {
    let directory = tempfile::tempdir().unwrap();
    let socket = directory.path().join("app-server.sock");
    let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
    listener.set_nonblocking(true).unwrap();
    let server = std::thread::spawn(move || {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let listener = UnixListener::from_std(listener).unwrap();
            serve_once(
                listener,
                "remoteControl/status/read",
                None,
                json!({
                    "status": "connected",
                    "serverName": "workstation",
                    "environmentId": "environment"
                }),
            )
            .await;
        });
    });

    let response: Value =
        block_on_without_tokio(rpc::request(&socket, "remoteControl/status/read", None)).unwrap();
    assert_eq!(response["status"], "connected");
    server.join().unwrap();
}

#[test]
fn lifecycle_command_works_from_a_non_tokio_executor() {
    let executable = std::env::current_exe().unwrap();
    let output = block_on_without_tokio(commands::run_at(&executable, &["--help"])).unwrap();
    assert!(!output.is_empty());
}

fn block_on_without_tokio<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    let waker = futures_util::task::noop_waker();
    let mut context = std::task::Context::from_waker(&waker);
    loop {
        if let Poll::Ready(output) = future.as_mut().poll(&mut context) {
            return output;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[tokio::test]
async fn rpc_initializes_experimental_api_and_sends_exact_request() {
    let directory = tempfile::tempdir().unwrap();
    let socket = directory.path().join("app-server.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let server = tokio::spawn(serve_once(
        listener,
        "remoteControl/status/read",
        None,
        json!({
            "status": "connected",
            "serverName": "workstation",
            "installationId": "installation",
            "environmentId": "environment"
        }),
    ));
    let response: Value = rpc::request(&socket, "remoteControl/status/read", None)
        .await
        .unwrap();
    assert_eq!(response["status"], "connected");
    server.await.unwrap();
}

#[tokio::test]
async fn device_pages_keep_metadata() {
    let directory = tempfile::tempdir().unwrap();
    let socket = directory.path().join("app-server.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let server = tokio::spawn(serve_once(
        listener,
        "remoteControl/client/list",
        Some(json!({
            "environmentId": "environment",
            "cursor": null,
            "limit": 100,
            "order": "desc"
        })),
        json!({
            "data": [{
                "clientId": "phone",
                "displayName": "Phone",
                "deviceType": "phone",
                "platform": "iOS",
                "osVersion": "19",
                "deviceModel": "iPhone",
                "appVersion": "1.0",
                "lastSeenAt": 1777000000
            }],
            "nextCursor": null
        }),
    ));
    let listed = devices::list(&socket, "environment").await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].client_id, "phone");
    assert_eq!(listed[0].device_type.as_deref(), Some("phone"));
    assert_eq!(listed[0].device_model.as_deref(), Some("iPhone"));
    assert_eq!(listed[0].last_seen_at, Some(1_777_000_000));
    server.await.unwrap();
}

#[tokio::test]
async fn device_pages_reject_repeated_cursors() {
    let directory = tempfile::tempdir().unwrap();
    let socket = directory.path().join("app-server.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let server = tokio::spawn(async move {
        serve_connection(
            &listener,
            "remoteControl/client/list",
            Some(json!({
                "environmentId": "environment",
                "cursor": null,
                "limit": 100,
                "order": "desc"
            })),
            json!({
                "data": [],
                "nextCursor": "again"
            }),
        )
        .await;
        serve_connection(
            &listener,
            "remoteControl/client/list",
            Some(json!({
                "environmentId": "environment",
                "cursor": "again",
                "limit": 100,
                "order": "desc"
            })),
            json!({ "data": [], "nextCursor": "again" }),
        )
        .await;
    });
    let error = devices::list(&socket, "environment").await.unwrap_err();
    assert!(error.to_string().contains("repeated a pagination cursor"));
    server.await.unwrap();
}

#[tokio::test]
async fn revoke_payload_uses_only_environment_and_opaque_client_id() {
    let directory = tempfile::tempdir().unwrap();
    let socket = directory.path().join("app-server.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let server = tokio::spawn(serve_once(
        listener,
        "remoteControl/client/revoke",
        Some(json!({ "environmentId": "environment", "clientId": "phone" })),
        json!({}),
    ));
    let _: Value = rpc::request(
        &socket,
        "remoteControl/client/revoke",
        Some(json!({ "environmentId": "environment", "clientId": "phone" })),
    )
    .await
    .unwrap();
    server.await.unwrap();
}

async fn serve_once(
    listener: UnixListener,
    method: &'static str,
    params: Option<Value>,
    result: Value,
) {
    serve_connection(&listener, method, params, result).await;
}

async fn serve_connection(
    listener: &UnixListener,
    method: &'static str,
    params: Option<Value>,
    result: Value,
) {
    let (stream, _) = listener.accept().await.unwrap();
    let mut websocket = accept_async(stream).await.unwrap();
    let initialize = next_json(&mut websocket).await;
    assert_eq!(initialize["method"], "initialize");
    assert_eq!(
        initialize.pointer("/params/capabilities/experimentalApi"),
        Some(&Value::Bool(true))
    );
    websocket
        .send(Message::Text(
            json!({ "id": 1, "result": {} }).to_string().into(),
        ))
        .await
        .unwrap();
    assert_eq!(next_json(&mut websocket).await["method"], "initialized");
    let request = next_json(&mut websocket).await;
    assert_eq!(request["method"], method);
    assert_eq!(request.get("params").cloned(), params);
    websocket
        .send(Message::Text(
            json!({ "id": 2, "result": result }).to_string().into(),
        ))
        .await
        .unwrap();
}

async fn next_json(
    websocket: &mut tokio_tungstenite::WebSocketStream<tokio::net::UnixStream>,
) -> Value {
    let Message::Text(payload) = websocket.next().await.unwrap().unwrap() else {
        panic!("expected text frame");
    };
    serde_json::from_str(&payload).unwrap()
}
