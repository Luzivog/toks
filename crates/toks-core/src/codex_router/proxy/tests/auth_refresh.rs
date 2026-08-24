use super::*;

use super::fixtures::one_percent_snapshot;

#[tokio::test]
async fn http_refresh_keeps_affinity_and_rechecks_fast_at_the_threshold() {
    let calls = Arc::new(Mutex::new(Vec::<(String, String)>::new()));
    let upstream_calls = calls.clone();
    let upstream =
        Router::new().fallback(any(move |headers: HeaderMap, body: axum::body::Bytes| {
            let calls = upstream_calls.clone();
            async move {
                let auth = headers["authorization"].to_str().unwrap().to_owned();
                let frame: serde_json::Value = serde_json::from_slice(&body).unwrap();
                let tier = frame["service_tier"]
                    .as_str()
                    .unwrap_or("default")
                    .to_owned();
                calls.lock().unwrap().push((auth.clone(), tier));
                if auth == "Bearer old-token" {
                    StatusCode::UNAUTHORIZED.into_response()
                } else {
                    (
                        [(axum::http::header::CONTENT_TYPE, "text/event-stream")],
                        "data: {\"type\":\"response.completed\",\"response\":{}}\n\n",
                    )
                        .into_response()
                }
            }
        }));
    let origin = spawn(upstream).await;
    let harness = Harness::new(&[("a", "old-token")]);
    let account = AccountId::new("a");
    harness.credentials.refreshes.lock().unwrap().insert(
        account.clone(),
        RouteCredential {
            account_id: account,
            access_token: "new-token".into(),
            chatgpt_account_id: "chatgpt-a".into(),
        },
    );
    let (started_tx, mut started_rx) = tokio::sync::mpsc::unbounded_channel();
    let proceed = Arc::new(tokio::sync::Notify::new());
    *harness.credentials.refresh_gate.lock().unwrap() = Some((started_tx, proceed.clone()));
    let proxy = spawn(app(harness.state(origin.clone(), origin))).await;
    let request = tokio::spawn(async move {
        reqwest::Client::new()
            .post(format!("{proxy}/backend-api/codex/responses"))
            .bearer_auth("old-token")
            .json(&json!({
                "type":"response.create",
                "model":"gpt-5.6-sol",
                "service_tier":"default",
                "client_metadata":{"thread_id":"refreshing"}
            }))
            .send()
            .await
            .unwrap()
    });

    started_rx.recv().await.unwrap();
    harness
        .runtime
        .engine
        .apply_authoritative_snapshots_for_test(&[one_percent_snapshot("a")], chrono::Utc::now())
        .unwrap();
    proceed.notify_one();
    let response = request.await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    response.text().await.unwrap();
    assert_eq!(
        *calls.lock().unwrap(),
        [
            ("Bearer old-token".into(), "default".into()),
            ("Bearer new-token".into(), "priority".into()),
        ]
    );
}
