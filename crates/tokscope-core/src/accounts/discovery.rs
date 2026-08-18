use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::limits::{self, Provider};

use super::{profiles_root, AccountProfile, ProfileMetadata, ProviderAccount, PROFILE_VERSION};

pub(crate) fn discover_profiles() -> Vec<AccountProfile> {
    let mut profiles = Vec::new();
    if let Some(home) = dirs::home_dir() {
        for provider in Provider::ALL {
            let config_dir = match provider {
                Provider::Claude => std::env::var("CLAUDE_CONFIG_DIR")
                    .map(PathBuf::from)
                    .unwrap_or_else(|_| home.join(".claude")),
                Provider::Codex => {
                    limits::codex::codex_home().unwrap_or_else(|| home.join(".codex"))
                }
            };
            profiles.push(AccountProfile {
                provider,
                account: ProviderAccount {
                    id: format!("{}-current", provider.slug()),
                    email: account_email(provider, &home, &config_dir),
                },
                home_dir: home.clone(),
                config_dir,
                managed: false,
                created_at_ms: None,
            });
        }
    }

    if let Ok(root) = profiles_root() {
        for provider in Provider::ALL {
            profiles.extend(discover_managed_profiles(
                &root.join(provider.slug()),
                provider,
            ));
        }
    }
    profiles.sort_by(|a, b| {
        a.provider
            .cmp(&b.provider)
            .then_with(|| a.managed.cmp(&b.managed))
            .then_with(|| a.account.email.cmp(&b.account.email))
    });
    retain_unique_profiles(&mut profiles);
    profiles
}

pub(super) fn retain_unique_profiles(profiles: &mut Vec<AccountProfile>) {
    let mut identities = HashSet::new();
    profiles.retain(|profile| identities.insert((profile.provider, profile.account.id.clone())));
}

pub(super) fn discover_managed_profiles(root: &Path, provider: Provider) -> Vec<AccountProfile> {
    let mut profiles = Vec::new();
    for entry in fs::read_dir(root).into_iter().flatten().flatten() {
        let profile_root = entry.path();
        if !profile_root.is_dir() {
            continue;
        }
        let Ok(raw) = fs::read_to_string(profile_root.join("profile.json")) else {
            continue;
        };
        let Ok(metadata) = serde_json::from_str::<ProfileMetadata>(&raw) else {
            continue;
        };
        let directory_id = entry.file_name().to_string_lossy().into_owned();
        if metadata.version != PROFILE_VERSION
            || metadata.provider != provider
            || metadata.id != directory_id
        {
            continue;
        }
        let home = profile_root.join("home");
        let config_dir = match provider {
            Provider::Claude => home.join(".claude"),
            Provider::Codex => home.join(".codex"),
        };
        profiles.push((
            metadata.created_at_ms,
            AccountProfile {
                provider,
                account: ProviderAccount {
                    id: metadata.id,
                    email: account_email(provider, &home, &config_dir),
                },
                home_dir: home,
                config_dir,
                managed: true,
                created_at_ms: Some(metadata.created_at_ms),
            },
        ));
    }
    profiles.sort_by_key(|(created_at_ms, _)| *created_at_ms);
    profiles.into_iter().map(|(_, profile)| profile).collect()
}

pub(super) fn account_email(provider: Provider, home: &Path, config_dir: &Path) -> Option<String> {
    match provider {
        Provider::Claude => limits::claude::read_email_from_profile(home, config_dir),
        Provider::Codex => limits::codex::read_email_from_home(config_dir),
    }
}
