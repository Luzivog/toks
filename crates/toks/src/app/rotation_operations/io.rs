use anyhow::{Context as _, Result};
use toks_core::{
    accounts::AccountId,
    codex_router::{self, RouterInstallStatus},
    rotation::{RotationRuntime, RotationRuntimeStore, RotationSettings, RotationSettingsStore},
};

use super::{RotationServiceAction, SettingsAction};

pub(super) struct LoadedRotation {
    pub settings: RotationSettings,
    pub runtime: RotationRuntime,
    pub install: RouterInstallStatus,
}

pub(super) fn change_settings(
    action: SettingsAction,
    accounts: Option<&[AccountId]>,
) -> Result<LoadedRotation> {
    let store = RotationSettingsStore::discover()?;
    let mut settings = store.load()?;
    if let Some(accounts) = accounts {
        settings.reconcile(accounts);
    }
    match action {
        SettingsAction::Include(account, included) => {
            settings.set_included(&account, included);
        }
        SettingsAction::MoveAccount(account, index) => {
            settings.move_to(&account, index);
        }
        SettingsAction::Cancel(thread) => {
            settings.cancel_waiting(&thread);
        }
        SettingsAction::MoveWaiting(thread, index) => {
            settings.move_waiting_to(&thread, index);
        }
    }
    store.save(&settings)?;
    load_rotation(accounts)
}

pub(super) fn load_rotation(accounts: Option<&[AccountId]>) -> Result<LoadedRotation> {
    let settings_store = RotationSettingsStore::discover()?;
    let runtime = RotationRuntimeStore::discover()?.load()?;
    let mut settings = settings_store.load()?;
    let waiting: Vec<_> = runtime
        .waiting_threads()
        .iter()
        .map(|entry| entry.thread_id.clone())
        .collect();
    let accounts_changed = accounts.is_some_and(|accounts| settings.reconcile(accounts));
    if accounts_changed | settings.reconcile_waiting(&waiting) {
        settings_store.save(&settings)?;
    }
    Ok(LoadedRotation {
        settings,
        runtime,
        install: codex_router::status(),
    })
}

pub(super) fn run_service_action(action: RotationServiceAction) -> Result<()> {
    match action {
        RotationServiceAction::Enable => {
            let app = std::env::current_exe().context("finding the Toks executable")?;
            let router = codex_router::router_executable_for(&app)?;
            codex_router::enable(&router)
        }
        RotationServiceAction::Disable => codex_router::disable(),
    }
}
