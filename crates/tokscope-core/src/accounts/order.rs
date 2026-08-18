use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::{LimitSnapshot, Provider};

const ORDER_VERSION: u8 = 1;
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountOrderKey {
    pub provider: Provider,
    pub account_id: String,
}

impl AccountOrderKey {
    pub fn new(provider: Provider, account_id: impl Into<String>) -> Self {
        Self {
            provider,
            account_id: account_id.into(),
        }
    }

    pub fn from_snapshot(snapshot: &LimitSnapshot) -> Self {
        Self::new(snapshot.provider, snapshot.account.id.clone())
    }
}

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredOrder {
    version: u8,
    accounts: Vec<AccountOrderKey>,
}

/// Apply the user's saved global account order. Accounts that have never been
/// ordered are appended deterministically without exposing email identities.
pub fn apply_saved_order(snapshots: &mut [LimitSnapshot]) {
    let order = order_path()
        .and_then(|path| load_order(&path))
        .unwrap_or_default();
    apply_order(snapshots, &order);
}

pub fn move_account_to(
    snapshots: &mut [LimitSnapshot],
    moving: &AccountOrderKey,
    target: &AccountOrderKey,
) -> Result<bool> {
    if moving == target {
        return Ok(false);
    }
    let mut keys = snapshot_keys(snapshots);
    if !reorder_to(&mut keys, moving, target) {
        return Ok(false);
    }
    save_order(&order_path()?, &keys)?;
    apply_order(snapshots, &keys);
    Ok(true)
}

fn snapshot_keys(snapshots: &[LimitSnapshot]) -> Vec<AccountOrderKey> {
    snapshots
        .iter()
        .map(AccountOrderKey::from_snapshot)
        .collect()
}

pub(super) fn reorder_to(
    keys: &mut Vec<AccountOrderKey>,
    moving: &AccountOrderKey,
    target: &AccountOrderKey,
) -> bool {
    if moving == target {
        return false;
    }
    let Some(from) = keys.iter().position(|key| key == moving) else {
        return false;
    };
    let Some(target_index) = keys.iter().position(|key| key == target) else {
        return false;
    };
    let moved = keys.remove(from);
    let target_after_removal = keys
        .iter()
        .position(|key| key == target)
        .expect("target remains after removing a different account");
    let insertion = if from < target_index {
        target_after_removal + 1
    } else {
        target_after_removal
    };
    keys.insert(insertion, moved);
    true
}

pub(super) fn apply_order(snapshots: &mut [LimitSnapshot], order: &[AccountOrderKey]) {
    let ranks: HashMap<_, _> = order.iter().enumerate().map(|(i, key)| (key, i)).collect();
    snapshots.sort_by(|left, right| {
        let left_key = AccountOrderKey::from_snapshot(left);
        let right_key = AccountOrderKey::from_snapshot(right);
        ranks
            .get(&left_key)
            .copied()
            .unwrap_or(usize::MAX)
            .cmp(&ranks.get(&right_key).copied().unwrap_or(usize::MAX))
            .then_with(|| left.provider.cmp(&right.provider))
            .then_with(|| left.account.id.cmp(&right.account.id))
    });
}

fn order_path() -> Result<PathBuf> {
    dirs::data_local_dir()
        .or_else(dirs::data_dir)
        .map(|root| root.join("tokscope").join("account-order.json"))
        .context("no local data directory")
}

pub(super) fn load_order(path: &Path) -> Result<Vec<AccountOrderKey>> {
    let raw = fs::read(path).context("reading saved account order")?;
    let stored: StoredOrder =
        serde_json::from_slice(&raw).context("parsing saved account order")?;
    if stored.version != ORDER_VERSION {
        return Ok(Vec::new());
    }
    let mut unique = HashSet::new();
    Ok(stored
        .accounts
        .into_iter()
        .filter(|key| !key.account_id.is_empty() && unique.insert(key.clone()))
        .collect())
}

pub(super) fn save_order(path: &Path, accounts: &[AccountOrderKey]) -> Result<()> {
    let parent = path.parent().context("account order path has no parent")?;
    fs::create_dir_all(parent).context("creating Tokscope data directory")?;
    super::restrict_directory(parent)?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before Unix epoch")?
        .as_nanos();
    let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let temporary = parent.join(format!(
        ".account-order-{}-{nonce}-{sequence}.tmp",
        std::process::id()
    ));
    let bytes = serde_json::to_vec_pretty(&StoredOrder {
        version: ORDER_VERSION,
        accounts: accounts.to_vec(),
    })?;
    let result = write_atomic_file(&temporary, path, &bytes);
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn write_atomic_file(temporary: &Path, destination: &Path, bytes: &[u8]) -> Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(temporary).context("creating account order")?;
    file.write_all(bytes).context("writing account order")?;
    file.sync_all().context("syncing account order")?;
    fs::rename(temporary, destination).context("publishing account order")?;
    fs::File::open(
        destination
            .parent()
            .context("account order has no parent")?,
    )
    .and_then(|directory| directory.sync_all())
    .context("syncing Tokscope data directory")
}
