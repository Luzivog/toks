use super::*;

use crate::codex_router::account_activation::{ManualTestOutcome, Store as ActivationStore};
use crate::rotation::{RotationRuntimeStore, ThreadId, UnixMillis};

const ATTEMPT: &str = "00000000-0000-4000-8000-000000000071";

#[tokio::test]
async fn activation_header_routes_selected_account_and_records_the_visible_task() {
    let capture = Arc::new(Mutex::new(None));
    let upstream_capture = capture.clone();
    let upstream = Router::new().fallback(any(move |headers: HeaderMap| {
        let capture = upstream_capture.clone();
        async move {
            *capture.lock().unwrap() = Some(headers);
            (
                [(axum::http::header::CONTENT_TYPE, "text/event-stream")],
                "data: {\"type\":\"response.completed\",\"response\":{}}\n\n",
            )
        }
    }));
    let origin = spawn(upstream).await;
    let harness = Harness::new(&[("a", "token-a"), ("b", "token-b")]);
    let runtime = RotationRuntimeStore::for_data_dir(harness._directory.path());
    let activation = ActivationStore::for_runtime(&runtime);
    let now = UnixMillis::now().get();
    activation
        .seed_running_manual_for_test(AccountId::new("b"), ATTEMPT, now)
        .unwrap();
    let proxy = spawn(app(harness.state(origin.clone(), origin))).await;
    let thread = ThreadId::new("01a051c0-5ad2-7060-8039-bfd1373e0c95");

    let response = reqwest::Client::new()
        .post(format!("{proxy}/backend-api/codex/responses"))
        .bearer_auth("token-a")
        .header("x-toks-activation-attempt", ATTEMPT)
        .json(&json!({
            "type":"response.create",
            "model":"gpt-5.6-sol",
            "service_tier":"default",
            "client_metadata":{"thread_id":thread.as_str()}
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    response.text().await.unwrap();
    let headers = capture.lock().unwrap().take().unwrap();
    assert_eq!(headers["authorization"], "Bearer token-b");
    assert_eq!(headers["chatgpt-account-id"], "chatgpt-b");
    assert!(!headers.contains_key("x-toks-activation-attempt"));

    activation
        .finish_success_for_test(ATTEMPT, UnixMillis::now().get())
        .unwrap();
    let receipt = activation
        .status_for_test(&AccountId::new("b"), UnixMillis::now().get())
        .unwrap()
        .manual_receipt
        .unwrap();
    assert_eq!(receipt.observed_account, Some(AccountId::new("b")));
    assert_eq!(receipt.thread_id, Some(thread));
    assert_eq!(receipt.outcome, ManualTestOutcome::Succeeded);
}
