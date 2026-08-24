use super::*;

#[tokio::test]
async fn incident_observability_records_sse_and_forced_fast_origin() {
    let calls = Arc::new(Mutex::new(Vec::<String>::new()));
    let upstream_calls = calls.clone();
    let upstream = Router::new().fallback(any(move |body: axum::body::Bytes| {
        let calls = upstream_calls.clone();
        async move {
            let frame: serde_json::Value = serde_json::from_slice(&body).unwrap();
            let tier = frame["service_tier"]
                .as_str()
                .unwrap_or("default")
                .to_owned();
            calls.lock().unwrap().push(tier.clone());
            if tier == "priority" {
                split_sse_failure()
            } else {
                continuing_response()
            }
        }
    }));
    let origin = spawn(upstream).await;
    let harness = Harness::new(&[("a", "token-a")]);
    let proxy = spawn(app(harness.state(origin.clone(), origin))).await;
    let client = reqwest::Client::new();
    let body = request_body("victim");

    post(&client, &proxy, &body).await.text().await.unwrap();
    harness
        .runtime
        .engine
        .apply_authoritative_snapshots_for_test(&[one_percent_snapshot("a")], chrono::Utc::now())
        .unwrap();
    let response = post(&client, &proxy, &body).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert!(response
        .text()
        .await
        .unwrap()
        .contains("response.completed"));
    assert_eq!(*calls.lock().unwrap(), ["default", "priority", "default"]);

    let runtime = RotationRuntimeStore::for_data_dir(harness._directory.path())
        .load()
        .unwrap();
    let incident = runtime
        .events()
        .iter()
        .find_map(|event| match &event.event {
            RotationEventKind::UsageLimited { incident, .. } => Some(incident),
            _ => None,
        })
        .unwrap();
    assert_eq!(incident.phase(), UsageLimitPhase::HttpStream);
    assert_eq!(incident.tier().effective(), Some("priority"));
    assert_eq!(
        incident.tier().origin(),
        UsageLimitTierOrigin::ToksForcedFast
    );
    assert_eq!(incident.evidence().frame_type(), Some("turn.failed"));
}

#[tokio::test]
async fn incident_observability_records_the_standard_fallback_origin() {
    let calls = Arc::new(Mutex::new(0_u8));
    let upstream_calls = calls.clone();
    let upstream = Router::new().fallback(any(move || {
        let calls = upstream_calls.clone();
        async move {
            let mut calls = calls.lock().unwrap();
            *calls += 1;
            if *calls == 1 {
                continuing_response()
            } else {
                usage_limit()
            }
        }
    }));
    let origin = spawn(upstream).await;
    let harness = Harness::new(&[("a", "token-a")]);
    let proxy = spawn(app(harness.state(origin.clone(), origin))).await;
    let client = reqwest::Client::new();
    let body = request_body("fallback-context");

    post(&client, &proxy, &body).await.text().await.unwrap();
    harness
        .runtime
        .engine
        .apply_authoritative_snapshots_for_test(&[one_percent_snapshot("a")], chrono::Utc::now())
        .unwrap();
    assert_eq!(
        post(&client, &proxy, &body).await.status(),
        StatusCode::TOO_MANY_REQUESTS
    );

    let runtime = RotationRuntimeStore::for_data_dir(harness._directory.path())
        .load()
        .unwrap();
    let incidents = runtime
        .events()
        .iter()
        .filter_map(|event| match &event.event {
            RotationEventKind::UsageLimited { incident, .. } => Some(incident),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(incidents.len(), 2);
    assert_eq!(incidents[0].tier().effective(), Some("default"));
    assert_eq!(
        incidents[0].tier().origin(),
        UsageLimitTierOrigin::ToksStandardFallback
    );
    assert_eq!(incidents[1].tier().effective(), Some("priority"));
    assert_eq!(
        incidents[1].tier().origin(),
        UsageLimitTierOrigin::ToksForcedFast
    );
}
