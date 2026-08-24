use std::collections::{HashMap, HashSet};

use crate::limits::{LimitSnapshot, Provider};

use super::{AccountId, AccountSource, CredentialProfileId};

mod identity;
#[cfg(test)]
pub(crate) use identity::codex_auth_account_id_for_test;
pub(crate) use identity::{codex_auth_account_id, provider_principal_id};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountBinding {
    pub provider: Provider,
    pub profile_id: CredentialProfileId,
    pub account_id: AccountId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountTransition {
    pub provider: Provider,
    pub profile_id: CredentialProfileId,
    pub previous_account_id: AccountId,
    pub account_id: AccountId,
}

impl AccountBinding {
    pub fn transition_to(&self, current: &Self) -> Option<AccountTransition> {
        (self.provider == current.provider
            && self.profile_id == current.profile_id
            && self.account_id != current.account_id)
            .then(|| AccountTransition {
                provider: current.provider,
                profile_id: current.profile_id.clone(),
                previous_account_id: self.account_id.clone(),
                account_id: current.account_id.clone(),
            })
    }
}

pub(super) fn coalesce_snapshots(snapshots: Vec<(usize, LimitSnapshot)>) -> Vec<LimitSnapshot> {
    let mut group_indexes: HashMap<(Provider, AccountId), usize> = HashMap::new();
    let mut groups: Vec<SnapshotGroup> = Vec::new();
    for (index, snapshot) in snapshots {
        let key = (snapshot.provider, snapshot.account.id.clone());
        if let Some(group_index) = group_indexes.get(&key).copied() {
            groups[group_index].add(snapshot);
        } else {
            group_indexes.insert(key, groups.len());
            groups.push(SnapshotGroup::new(index, snapshot));
        }
    }
    groups.sort_by_key(|group| group.first_index);
    groups.into_iter().map(SnapshotGroup::finish).collect()
}

struct SnapshotGroup {
    first_index: usize,
    selected: LimitSnapshot,
    selected_primary: Option<super::CredentialProfileId>,
    sources: Vec<AccountSource>,
    fallback_email: Option<String>,
}

impl SnapshotGroup {
    fn new(first_index: usize, selected: LimitSnapshot) -> Self {
        Self {
            first_index,
            selected_primary: primary_id(&selected),
            sources: selected.account.sources.clone(),
            fallback_email: selected.account.email.clone(),
            selected,
        }
    }

    fn add(&mut self, candidate: LimitSnapshot) {
        self.sources
            .extend(candidate.account.sources.iter().cloned());
        self.fallback_email = self
            .fallback_email
            .take()
            .or_else(|| candidate.account.email.clone());
        if is_fresher(&candidate, &self.selected) {
            self.selected_primary = primary_id(&candidate);
            self.selected = candidate;
        }
    }

    fn finish(mut self) -> LimitSnapshot {
        let mut unique = HashSet::new();
        self.sources
            .retain(|source| unique.insert(source.profile_id.clone()));
        for source in &mut self.sources {
            source.primary = Some(&source.profile_id) == self.selected_primary.as_ref();
        }
        self.selected.account.email = self.selected.account.email.take().or(self.fallback_email);
        self.selected.account.sources = self.sources;
        self.selected
    }
}

fn primary_id(snapshot: &LimitSnapshot) -> Option<super::CredentialProfileId> {
    snapshot
        .account
        .primary_source()
        .map(|source| source.profile_id.clone())
}

fn is_fresher(candidate: &LimitSnapshot, selected: &LimitSnapshot) -> bool {
    candidate
        .fetched_at
        .cmp(&selected.fetched_at)
        .then_with(|| (!candidate.windows.is_empty()).cmp(&!selected.windows.is_empty()))
        .then_with(|| candidate.issue.is_none().cmp(&selected.issue.is_none()))
        .is_gt()
}
