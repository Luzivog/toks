use chrono::Utc;
use toks_core::{
    accounts::{AccountIdentityKind, AccountSource, CredentialProfileKind},
    rotation::{RotationEvent, RotationEventKind, ThreadId, UnixMillis},
    LimitSnapshot, Provider, ProviderAccount,
};

use super::{event_text, EventTone};
use crate::ToksApp;

#[test]
fn thread_and_account_identity_use_distinct_semantic_tones() {
    let account_id = "account-a";
    let mut app = ToksApp::from_snapshots(None, vec![snapshot(account_id)], Utc::now());
    let event = RotationEvent {
        at: UnixMillis::new(1),
        event: RotationEventKind::Routed {
            thread_id: ThreadId::new("thread-123"),
            account_id: account_id.into(),
        },
    };

    let visible = event_text(&app, &event);
    assert_eq!(
        visible.text,
        "Thread thread-123 routed to person@example.test"
    );
    assert_eq!(visible.tones.len(), 2);
    assert_eq!(visible.tones[0].1, EventTone::Thread);
    assert_eq!(&visible.text[visible.tones[0].0.clone()], "thread-123");
    assert_eq!(visible.tones[1].1, EventTone::Account);
    assert_eq!(
        &visible.text[visible.tones[1].0.clone()],
        "person@example.test"
    );

    app.emails_hidden = true;
    let hidden = event_text(&app, &event);
    assert_eq!(
        hidden.text,
        "Thread thread-123 routed to person@example.test"
    );
    assert_eq!(
        &hidden.text[hidden.tones[1].0.clone()],
        "person@example.test"
    );
}

fn snapshot(id: &str) -> LimitSnapshot {
    LimitSnapshot {
        provider: Provider::Codex,
        account: ProviderAccount {
            id: id.into(),
            identity_kind: AccountIdentityKind::ProviderPrincipal,
            email: Some("person@example.test".into()),
            sources: vec![AccountSource {
                profile_id: "profile-a".into(),
                kind: CredentialProfileKind::Managed,
                primary: true,
            }],
        },
        plan: None,
        plan_multiplier: None,
        banked_resets: 0,
        banked_reset_credits: None,
        windows: Vec::new(),
        extras: Vec::new(),
        fetched_at: None,
        source: String::new(),
        issue: None,
        status: Default::default(),
    }
}
