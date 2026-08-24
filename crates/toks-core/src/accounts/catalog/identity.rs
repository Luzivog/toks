use std::{fs, io::Read, path::Path};

use hmac::{Hmac, Mac};
use serde_json::Value;
use sha2::Sha256;

use crate::limits::{self, Provider};
use crate::storage::LockMode;

use crate::accounts::{AccountId, AccountProfile, CredentialProfileId};

pub(crate) fn provider_principal_id(profile: &AccountProfile) -> Option<AccountId> {
    let material = match profile.provider {
        Provider::Claude => {
            limits::claude::read_principal_material(&profile.home_dir, &profile.config_dir)
        }
        Provider::Codex => limits::codex::read_principal_material(&profile.config_dir),
    }?;
    principal_id(profile.provider, &material)
}

pub(crate) fn codex_auth_account_id(
    profile: &CredentialProfileId,
    auth: &Value,
) -> Option<AccountId> {
    let key = load_or_create()?;
    codex_auth_account_id_with_key(profile, auth, &key)
}

fn codex_auth_account_id_with_key(
    _profile: &CredentialProfileId,
    auth: &Value,
    key: &[u8],
) -> Option<AccountId> {
    limits::codex::principal_material_from_auth(auth)
        .and_then(|material| principal_id_with_key(Provider::Codex, &material, key))
}

fn principal_id(provider: Provider, material: &[u8]) -> Option<AccountId> {
    let key = load_or_create()?;
    principal_id_with_key(provider, material, &key)
}

fn principal_id_with_key(provider: Provider, material: &[u8], key: &[u8]) -> Option<AccountId> {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).ok()?;
    mac.update(b"tokscope.account.v1\0");
    mac.update(provider.slug().as_bytes());
    mac.update(&[0]);
    mac.update(material);
    let digest = mac.finalize().into_bytes();
    Some(AccountId::new(format!(
        "{}-{}",
        provider.slug(),
        encode_hex(&digest)
    )))
}

#[cfg(test)]
pub(crate) fn codex_auth_account_id_for_test(
    profile: &CredentialProfileId,
    auth: &Value,
) -> Option<AccountId> {
    codex_auth_account_id_with_key(profile, auth, &[7_u8; KEY_BYTES])
}

fn encode_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub(super) const KEY_BYTES: usize = 32;
const LOCK_FILE: &str = ".account-principal.lock";

fn load_or_create() -> Option<Vec<u8>> {
    let path = crate::paths::account_identity_key().ok()?;
    load_or_create_at(&path)
}

pub(super) fn load_or_create_at(path: &Path) -> Option<Vec<u8>> {
    if let Some(key) = read(path) {
        return Some(key);
    }
    let parent = path.parent()?;
    fs::create_dir_all(parent).ok()?;
    crate::storage::restrict_directory(parent).ok()?;
    let _lock = crate::storage::lock_private(
        &parent.join(LOCK_FILE),
        "account principal",
        LockMode::Blocking,
    )
    .ok()?;
    if let Some(key) = read(path) {
        return Some(key);
    }
    let mut key = vec![0_u8; KEY_BYTES];
    getrandom::fill(&mut key).ok()?;
    crate::storage::write_private_atomic(path, &key, "account principal key").ok()?;
    Some(key)
}

fn read(path: &Path) -> Option<Vec<u8>> {
    let metadata = fs::symlink_metadata(path).ok()?;
    if !metadata.file_type().is_file() {
        return None;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).ok()?;
    }
    let mut key = Vec::new();
    fs::File::open(path).ok()?.read_to_end(&mut key).ok()?;
    (key.len() == KEY_BYTES).then_some(key)
}
