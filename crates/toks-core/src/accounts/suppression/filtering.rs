use crate::limits::LimitSnapshot;

use super::super::{AccountIdentityKind, CredentialProfileKind};
use super::model::{SuppressedAccount, SuppressionDocument};

pub(super) fn update_for_observed_accounts(
    document: &mut SuppressionDocument,
    snapshots: &[LimitSnapshot],
) -> bool {
    let before = document.accounts.clone();
    for hidden in &mut document.accounts {
        release_transitioned_current_profiles(hidden, snapshots);
    }
    document.normalize();
    document.accounts != before
}

pub(super) fn retain_visible(
    snapshots: Vec<LimitSnapshot>,
    document: &SuppressionDocument,
) -> Vec<LimitSnapshot> {
    snapshots
        .into_iter()
        .filter(|snapshot| {
            !document
                .accounts
                .iter()
                .any(|hidden| hides_snapshot(hidden, snapshot))
        })
        .collect()
}

fn release_transitioned_current_profiles(
    hidden: &mut SuppressedAccount,
    snapshots: &[LimitSnapshot],
) {
    hidden.current_profile_ids.retain(|profile_id| {
        !snapshots.iter().any(|snapshot| {
            snapshot.provider == hidden.provider
                && snapshot.account.identity_kind == AccountIdentityKind::ProviderPrincipal
                && snapshot.account.id != hidden.account_id
                && snapshot.account.sources.iter().any(|source| {
                    source.kind == CredentialProfileKind::Current
                        && &source.profile_id == profile_id
                })
        })
    });
}

fn hides_snapshot(hidden: &SuppressedAccount, snapshot: &LimitSnapshot) -> bool {
    if snapshot.provider != hidden.provider {
        return false;
    }
    if snapshot.account.id == hidden.account_id {
        return true;
    }
    snapshot.account.identity_kind == AccountIdentityKind::ProfileFallback
        && snapshot.account.sources.iter().any(|source| {
            source.kind == CredentialProfileKind::Current
                && hidden.current_profile_ids.contains(&source.profile_id)
        })
}
