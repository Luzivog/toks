use super::*;

use super::fixtures::one_percent_snapshot;

#[tokio::test]
async fn ping_before_a_fast_limit_does_not_prevent_the_standard_retry() {
    let calls = Arc::new(Mutex::new(Vec::<String>::new()));
    let upstream_calls = calls.clone();
    let upstream = Router::new().fallback(any(move |ws| {
        ping_then_fast_limit(ws, upstream_calls.clone())
    }));
    let origin = spawn(upstream).await;
    let harness = Harness::new(&[("a", "token-a")]);
    let proxy = spawn(app(
        harness.state(origin.clone(), origin.replacen("http://", "ws://", 1))
    ))
    .await;
    let ws = proxy.replacen("http://", "ws://", 1);
    let mut socket = connect(&ws, "token-a", "victim").await;
    harness
        .runtime
        .engine
        .apply_snapshots(&[one_percent_snapshot("a")], chrono::Utc::now())
        .unwrap();

    socket
        .send(response_frame("victim", "gpt-5.6-sol").into())
        .await
        .unwrap();
    assert_eq!(next_json(&mut socket).await["type"], "session.updated");
    assert!(matches!(
        socket.next().await.unwrap().unwrap(),
        tokio_tungstenite::tungstenite::Message::Ping(_)
    ));
    let response = next_json(&mut socket).await;
    assert_eq!(response["tier"], "default");
    assert_eq!(*calls.lock().unwrap(), ["priority", "default"]);
}

async fn ping_then_fast_limit(
    ws: WebSocketUpgrade,
    calls: Arc<Mutex<Vec<String>>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |mut socket| async move {
        while let Some(Ok(message)) = socket.next().await {
            let Message::Text(text) = message else {
                continue;
            };
            let frame: serde_json::Value = serde_json::from_str(&text).unwrap();
            let tier = frame["service_tier"]
                .as_str()
                .unwrap_or("default")
                .to_owned();
            calls.lock().unwrap().push(tier.clone());
            if tier == "priority" {
                socket
                    .send(Message::Text(
                        json!({"type":"session.updated"}).to_string().into(),
                    ))
                    .await
                    .unwrap();
                socket.send(Message::Ping("control".into())).await.unwrap();
                socket
                    .send(Message::Text(usage_error().into()))
                    .await
                    .unwrap();
                continue;
            }
            socket
                .send(Message::Text(json!({"tier":tier}).to_string().into()))
                .await
                .unwrap();
        }
    })
}

async fn connect(
    origin: &str,
    token: &str,
    thread: &str,
) -> tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>> {
    let mut request = format!("{origin}/backend-api/codex/responses")
        .into_client_request()
        .unwrap();
    request
        .headers_mut()
        .insert("authorization", format!("Bearer {token}").parse().unwrap());
    request
        .headers_mut()
        .insert("thread-id", thread.parse().unwrap());
    tokio_tungstenite::connect_async(request).await.unwrap().0
}

async fn next_json(
    socket: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
) -> serde_json::Value {
    let text = socket.next().await.unwrap().unwrap().into_text().unwrap();
    serde_json::from_str(&text).unwrap()
}

fn response_frame(thread: &str, model: &str) -> String {
    json!({
        "type":"response.create",
        "model":model,
        "service_tier":"auto",
        "client_metadata":{"thread_id":thread}
    })
    .to_string()
}
