use super::*;

fn jwt(marker: &str, expires_at: i64) -> String {
    let payload = base64::Engine::encode(
        &base64::engine::general_purpose::URL_SAFE_NO_PAD,
        serde_json::json!({"exp": expires_at, "marker": marker}).to_string(),
    );
    format!("header.{payload}.signature-{marker}")
}

#[test]
fn admitted_startup_token_survives_router_recreation() {
    let token = jwt("restart", 4_102_444_800);
    let harness = Harness::new(&[("a", &token)]);
    let store = harness._directory.path().join("inbound-tokens.json");

    let tokens = InboundTokens::at(harness.runtime.credentials.clone(), store.clone());
    assert!(tokens.accepts(&token));
    harness.credentials.incoming.lock().unwrap().remove(&token);

    let recreated = InboundTokens::at(harness.runtime.credentials.clone(), store.clone());
    assert!(recreated.accepts(&token));
    assert!(!std::fs::read_to_string(&store).unwrap().contains(&token));

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(store).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

#[test]
fn expired_startup_token_is_rejected() {
    let token = jwt("expired", 1);
    let harness = Harness::new(&[("a", &token)]);
    let store = harness._directory.path().join("inbound-tokens.json");

    assert!(!InboundTokens::at(harness.runtime.credentials.clone(), store).accepts(&token));
}

#[test]
fn removed_accounts_lose_their_persisted_admissions() {
    let token = jwt("removed", 4_102_444_800);
    let enrolled = Harness::new(&[("a", &token)]);
    let store = enrolled._directory.path().join("inbound-tokens.json");
    assert!(InboundTokens::at(enrolled.runtime.credentials.clone(), store.clone()).accepts(&token));

    let removed = Harness::new(&[]);
    assert!(!InboundTokens::at(removed.runtime.credentials.clone(), store).accepts(&token));
}

#[test]
fn persisted_admissions_are_bounded() {
    let accounts = (0..65)
        .map(|index| {
            (
                format!("account-{index}"),
                jwt(&index.to_string(), 4_102_444_800),
            )
        })
        .collect::<Vec<_>>();
    let borrowed = accounts
        .iter()
        .map(|(account, token)| (account.as_str(), token.as_str()))
        .collect::<Vec<_>>();
    let harness = Harness::new(&borrowed);
    let store = harness._directory.path().join("inbound-tokens.json");
    let admissions = InboundTokens::at(harness.runtime.credentials.clone(), store.clone());
    for (_, token) in &accounts {
        assert!(admissions.accepts(token));
    }

    let stored: serde_json::Value = serde_json::from_slice(&std::fs::read(store).unwrap()).unwrap();
    assert_eq!(stored["admissions"].as_array().unwrap().len(), 64);
}

#[test]
fn overlapping_generations_preserve_each_others_admissions() {
    let first_token = jwt("generation-one", 4_102_444_800);
    let second_token = jwt("generation-two", 4_102_444_800);
    let harness = Harness::new(&[("a", &first_token), ("b", &second_token)]);
    let store = harness._directory.path().join("inbound-tokens.json");
    let first = InboundTokens::at(harness.runtime.credentials.clone(), store.clone());
    let second = InboundTokens::at(harness.runtime.credentials.clone(), store.clone());
    let ready = std::sync::Arc::new(std::sync::Barrier::new(3));

    let first_ready = ready.clone();
    let first_writer = std::thread::spawn(move || {
        first_ready.wait();
        first.accepts(&first_token)
    });
    let second_ready = ready.clone();
    let second_writer = std::thread::spawn(move || {
        second_ready.wait();
        second.accepts(&second_token)
    });
    ready.wait();
    assert!(first_writer.join().unwrap());
    assert!(second_writer.join().unwrap());

    harness.credentials.incoming.lock().unwrap().clear();
    let recovered = InboundTokens::at(harness.runtime.credentials.clone(), store);
    assert!(recovered.accepts(&jwt("generation-one", 4_102_444_800)));
    assert!(recovered.accepts(&jwt("generation-two", 4_102_444_800)));
}
