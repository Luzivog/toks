use std::fs;

use crate::provider_visibility::{load_at, save_at};
use crate::{ClientId, ProviderVisibility, USAGE_PROVIDERS};

#[test]
fn default_visibility_keeps_every_usage_provider_visible() {
    let visibility = ProviderVisibility::default();

    assert_eq!(
        USAGE_PROVIDERS,
        [ClientId::Codex, ClientId::Claude, ClientId::OpenCode]
    );
    assert_eq!(visibility.visible_count(), USAGE_PROVIDERS.len());
    assert!(USAGE_PROVIDERS
        .iter()
        .all(|provider| visibility.is_visible(*provider)));
}

#[test]
fn last_visible_provider_cannot_be_hidden() {
    let mut visibility = ProviderVisibility::default();

    assert!(visibility.set_visible(ClientId::Codex, false));
    assert!(visibility.set_visible(ClientId::Claude, false));
    assert_eq!(visibility.visible_count(), 1);
    assert!(!visibility.can_hide(ClientId::OpenCode));
    assert!(!visibility.set_visible(ClientId::OpenCode, false));
    assert!(visibility.is_visible(ClientId::OpenCode));
    assert!(!visibility.set_visible(ClientId::Cursor, false));
}

#[test]
fn visibility_round_trip_replaces_the_private_file() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("state/provider-visibility.json");
    let mut visibility = ProviderVisibility::default();
    visibility.set_visible(ClientId::Claude, false);
    save_at(&path, &visibility).unwrap();

    visibility.set_visible(ClientId::Claude, true);
    visibility.set_visible(ClientId::OpenCode, false);
    save_at(&path, &visibility).unwrap();

    assert_eq!(load_at(&path), visibility);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&fs::read(&path).unwrap()).unwrap(),
        serde_json::json!({
            "version": 1,
            "hiddenProviders": ["opencode"]
        })
    );
    assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 1);

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        assert_eq!(
            fs::metadata(path.parent().unwrap())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

#[test]
fn missing_or_invalid_documents_fail_open_to_every_provider() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("provider-visibility.json");
    let expected = ProviderVisibility::default();

    assert_eq!(load_at(&path), expected);
    for contents in [
        "not json",
        r#"{"version":2,"hiddenProviders":[]}"#,
        r#"{"version":1,"hiddenProviders":["unknown"]}"#,
        r#"{"version":1,"hiddenProviders":["cursor"]}"#,
        r#"{"version":1,"hiddenProviders":["codex","claude","opencode"]}"#,
    ] {
        fs::write(&path, contents).unwrap();
        assert_eq!(load_at(&path), expected, "document was {contents}");
    }
}
