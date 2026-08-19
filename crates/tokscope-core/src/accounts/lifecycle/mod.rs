mod managed;
mod paths;
mod types;

use anyhow::{bail, Result};
use std::collections::HashSet;

use super::order::{order_path, remove_accounts_at, AccountOrderKey};
use super::{discover_profiles, profiles_root};
use paths::{validate_local_id, LifecyclePaths};

pub use types::{
    AccountRemovalPlan, AccountRemovalResult, ManagedProfileRemoval, ManagedRemovalState,
};

/// Remove every exact credential source backing one logical account.
///
/// Managed profiles are atomically quarantined and deleted. Provider-owned
/// current profiles are never touched; the result asks the account catalog to
/// suppress them. History is deliberately outside this lifecycle operation.
pub fn remove_account(plan: &AccountRemovalPlan) -> Result<AccountRemovalResult> {
    let paths = LifecyclePaths {
        profiles_root: profiles_root()?,
        order_path: order_path()?,
    };
    let profiles = discover_profiles();
    execute(plan, &profiles, &paths)
}

fn execute(
    plan: &AccountRemovalPlan,
    profiles: &[super::AccountProfile],
    paths: &LifecyclePaths,
) -> Result<AccountRemovalResult> {
    let mut ids = Vec::new();
    let mut seen = HashSet::new();
    for id in &plan.local_profile_ids {
        validate_local_id(id.as_str())?;
        if seen.insert(id.clone()) {
            ids.push(id.clone());
        }
    }
    if ids.is_empty() {
        bail!("account removal requires at least one local credential profile")
    }

    let mut result = AccountRemovalResult {
        provider: plan.provider,
        logical_account_id: plan.logical_account_id.clone(),
        managed_profiles: Vec::new(),
        hide_current_profile_ids: Vec::new(),
        invalidate_local_profile_ids: ids.clone(),
        history_retained: true,
    };
    let mut removed_order_keys = Vec::new();
    for id in ids {
        // Generation-guard the login watcher before touching its profile. A
        // late CLI completion must not resurrect a removed credential source.
        super::login::cancel_login(plan.provider, &id);
        match profiles
            .iter()
            .find(|profile| profile.provider == plan.provider && profile.profile_id == id)
        {
            Some(profile) if !profile.managed => result.hide_current_profile_ids.push(id),
            Some(_) => remove_one_managed(
                &mut result,
                &mut removed_order_keys,
                paths,
                plan.provider,
                id,
            )?,
            None if paths.tombstone(plan.provider, id.as_str()).is_file()
                || paths.quarantine(plan.provider, id.as_str()).exists() =>
            {
                remove_one_managed(
                    &mut result,
                    &mut removed_order_keys,
                    paths,
                    plan.provider,
                    id,
                )?
            }
            None => bail!("local account profile was not discovered"),
        }
    }
    removed_order_keys.push(AccountOrderKey::new(
        plan.provider,
        plan.logical_account_id.as_str(),
    ));
    remove_accounts_at(&paths.order_path, &removed_order_keys)?;
    Ok(result)
}

fn remove_one_managed(
    result: &mut AccountRemovalResult,
    order_keys: &mut Vec<AccountOrderKey>,
    paths: &LifecyclePaths,
    provider: crate::Provider,
    id: super::CredentialProfileId,
) -> Result<()> {
    let state = managed::remove_managed(paths, provider, id.as_str())?;
    order_keys.push(AccountOrderKey::new(provider, id.as_str()));
    result.managed_profiles.push(ManagedProfileRemoval {
        local_profile_id: id,
        state,
    });
    Ok(())
}

#[cfg(test)]
mod tests;
