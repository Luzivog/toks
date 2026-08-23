use serde_json::json;

use crate::accounts::{AccountId, ProviderAccount};
use crate::limits::{LimitSnapshot, LimitWindow, Provider};

pub(super) fn one_percent_snapshot(id: &str) -> LimitSnapshot {
    LimitSnapshot {
        windows: vec![LimitWindow {
            id: "weekly".into(),
            label: "Weekly".into(),
            percent_used: 99.0,
            resets_at: Some(chrono::Utc::now() + chrono::Duration::hours(3)),
            severity: None,
            scope: None,
            is_active: true,
            raw: json!({}),
        }],
        ..LimitSnapshot::loading_account(
            Provider::Codex,
            ProviderAccount {
                id: AccountId::new(id),
                ..ProviderAccount::unidentified_for(Provider::Codex)
            },
        )
    }
}
