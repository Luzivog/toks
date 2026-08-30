use std::collections::BTreeMap;
use std::convert::Infallible;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use axum::body::{Body, Bytes};
use axum::routing::any;
use axum::Router;
use serde_json::json;

use super::*;
use crate::rotation::TaskActivityStore;

#[tokio::test]
async fn http_tool_continuation_clears_when_the_response_body_ends() {
    let gate = Arc::new(tokio::sync::Semaphore::new(0));
    let upstream = Router::new().fallback(any({
        let gate = gate.clone();
        move || held_tool_response(gate.clone())
    }));
    let origin = spawn(upstream).await;
    let harness = Harness::new_worker(&[("a", "token-a")]);
    let activity = activity_store(&harness);
    let proxy = spawn(app(harness.state(origin.clone(), origin))).await;

    let response = post(&proxy, "http-abandoned").await;
    assert_eq!(active_count(&activity), 1);

    gate.add_permits(1);
    response.bytes().await.unwrap();

    assert_eq!(active_count(&activity), 0);
}

#[tokio::test]
async fn http_tool_continuation_clears_when_the_client_drops_the_body() {
    let gate = Arc::new(tokio::sync::Semaphore::new(0));
    let upstream = Router::new().fallback(any({
        let gate = gate.clone();
        move || held_tool_response(gate.clone())
    }));
    let origin = spawn(upstream).await;
    let harness = Harness::new_worker(&[("a", "token-a")]);
    let activity = activity_store(&harness);
    let proxy = spawn(app(harness.state(origin.clone(), origin))).await;

    let response = post(&proxy, "http-disconnected").await;
    assert_eq!(active_count(&activity), 1);

    drop(response);
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        while active_count(&activity) != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn overlapping_http_follow_up_stays_active_until_its_own_response_ends() {
    let calls = Arc::new(AtomicUsize::new(0));
    let gates = Arc::new([
        Arc::new(tokio::sync::Semaphore::new(0)),
        Arc::new(tokio::sync::Semaphore::new(0)),
    ]);
    let upstream = Router::new().fallback(any({
        let calls = calls.clone();
        let gates = gates.clone();
        move || {
            let turn = calls.fetch_add(1, Ordering::SeqCst);
            held_tool_response(gates[turn].clone())
        }
    }));
    let origin = spawn(upstream).await;
    let harness = Harness::new_worker(&[("a", "token-a")]);
    let activity = activity_store(&harness);
    let proxy = spawn(app(harness.state(origin.clone(), origin))).await;

    let mut first = post(&proxy, "http-follow-up").await;
    first.chunk().await.unwrap().unwrap();
    let second = post(&proxy, "http-follow-up").await;
    assert_eq!(active_count(&activity), 1);

    gates[0].add_permits(1);
    first.bytes().await.unwrap();
    assert_eq!(active_count(&activity), 1);

    gates[1].add_permits(1);
    second.bytes().await.unwrap();
    assert_eq!(active_count(&activity), 0);
}

fn activity_store(harness: &Harness) -> TaskActivityStore {
    let store = TaskActivityStore::for_data_dir(harness._directory.path());
    store
        .reconcile_expected_workers(&BTreeMap::from([(1, 1)]))
        .unwrap();
    store
}

fn active_count(store: &TaskActivityStore) -> usize {
    store.load().unwrap().active_task_rows().unwrap().len()
}

async fn post(proxy: &str, thread: &str) -> reqwest::Response {
    reqwest::Client::new()
        .post(format!("{proxy}/backend-api/codex/responses"))
        .bearer_auth("token-a")
        .json(&json!({
            "type":"response.create",
            "model":"gpt-5.6-sol",
            "service_tier":"default",
            "client_metadata":{"thread_id":thread}
        }))
        .send()
        .await
        .unwrap()
}

async fn held_tool_response(gate: Arc<tokio::sync::Semaphore>) -> axum::response::Response {
    let chunk = Bytes::from_static(
        concat!(
            "data: {\"type\":\"response.output_item.done\",",
            "\"item\":{\"type\":\"function_call\"}}\n\n",
            "data: {\"type\":\"response.completed\",\"response\":{}}\n\n"
        )
        .as_bytes(),
    );
    let stream = futures_util::stream::unfold(Some(chunk), move |chunk| {
        let gate = gate.clone();
        async move {
            match chunk {
                Some(chunk) => Some((Ok::<_, Infallible>(chunk), None)),
                None => {
                    gate.acquire().await.unwrap().forget();
                    None
                }
            }
        }
    });
    axum::response::Response::builder()
        .header(axum::http::header::CONTENT_TYPE, "text/event-stream")
        .body(Body::from_stream(stream))
        .unwrap()
}
