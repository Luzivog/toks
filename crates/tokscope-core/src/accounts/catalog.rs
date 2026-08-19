use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::collections::{HashMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::PathBuf;

use crate::limits::{self, LimitSnapshot, Provider};

use super::{AccountId, AccountProfile, AccountSource, CredentialProfileId};

const KEY_BYTES: usize = 32;
const PRINCIPAL_KEY_FILE: &str = "account-principal.key";

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

pub(crate) fn provider_principal_id(profile: &AccountProfile) -> Option<AccountId> {
    let material = match profile.provider {
        Provider::Claude => {
            limits::claude::read_principal_material(&profile.home_dir, &profile.config_dir)
        }
        Provider::Codex => limits::codex::read_principal_material(&profile.config_dir),
    }?;
    let key = load_or_create_key()?;
    let mut mac = Hmac::<Sha256>::new_from_slice(&key).ok()?;
    mac.update(b"tokscope.account.v1\0");
    mac.update(profile.provider.slug().as_bytes());
    mac.update(&[0]);
    mac.update(&material);
    let digest = mac.finalize().into_bytes();
    Some(AccountId::new(format!(
        "{}-{}",
        profile.provider.slug(),
        encode_hex(&digest)
    )))
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

fn load_or_create_key() -> Option<Vec<u8>> {
    let path = principal_key_path()?;
    if let Some(key) = read_key(&path) {
        return Some(key);
    }
    let parent = path.parent()?;
    fs::create_dir_all(parent).ok()?;
    super::restrict_directory(parent).ok()?;
    let mut key = vec![0_u8; KEY_BYTES];
    getrandom::fill(&mut key).ok()?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    match options.open(&path) {
        Ok(mut file) => {
            file.write_all(&key).ok()?;
            file.sync_all().ok()?;
            fs::File::open(parent).ok()?.sync_all().ok()?;
            Some(key)
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => read_key(&path),
        Err(_) => None,
    }
}

fn principal_key_path() -> Option<PathBuf> {
    dirs::data_local_dir()
        .or_else(dirs::data_dir)
        .map(|root| root.join("tokscope").join(PRINCIPAL_KEY_FILE))
}

fn read_key(path: &std::path::Path) -> Option<Vec<u8>> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).ok()?;
    }
    let mut key = Vec::new();
    fs::File::open(path).ok()?.read_to_end(&mut key).ok()?;
    (key.len() == KEY_BYTES).then_some(key)
}

fn encode_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
