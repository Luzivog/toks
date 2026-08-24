use hmac::{Hmac, Mac};
use serde_json::Value;
use sha2::Sha256;

use crate::limits::{self, Provider};

use super::super::{AccountId, AccountProfile, CredentialProfileId};

mod key;

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
    let key = key::load_or_create()?;
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
    let key = key::load_or_create()?;
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
    codex_auth_account_id_with_key(profile, auth, &[7_u8; key::KEY_BYTES])
}

fn encode_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
