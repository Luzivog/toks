use axum::{http::HeaderMap, routing::any, Json, Router};
use serde_json::{json, Value};

use crate::{accounts::AccountId, rotation::RotationSettingsStore};

use super::{app, spawn, Harness};

#[tokio::test]
async fn excluded_control_identity_routes_model_work_through_an_eligible_account() {
    let upstream = Router::new().fallback(any(|headers: HeaderMap| async move {
        Json(json!({
            "selectedAccount": headers["chatgpt-account-id"].to_str().unwrap()
        }))
    }));
    let origin = spawn(upstream).await;
    let harness = Harness::new(&[("control", "control-token"), ("worker", "worker-token")]);
    let settings_store = RotationSettingsStore::for_data_dir(harness._directory.path());
    let mut settings = settings_store.load().unwrap();
    assert!(settings.set_included(&AccountId::new("control"), false));
    settings_store.save(&settings).unwrap();
    let proxy = spawn(app(harness.state(origin.clone(), origin))).await;

    let response = reqwest::Client::new()
        .post(format!("{proxy}/backend-api/codex/responses"))
        .bearer_auth("control-token")
        .json(&json!({"client_metadata":{"thread_id":"remote-thread"}}))
        .send()
        .await
        .unwrap();
    assert!(response.status().is_success());
    let payload: Value = response.json().await.unwrap();
    assert_eq!(payload["selectedAccount"], "chatgpt-worker");
}
