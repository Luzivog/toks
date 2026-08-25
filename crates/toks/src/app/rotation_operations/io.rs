use std::collections::BTreeMap;

use anyhow::{Context as _, Result};
use toks_core::{
    accounts::AccountId,
    codex_router::{
        self, account_activation::SelectableModel, thread_lineage::ThreadLineage,
        RouterDeploymentStatus, RouterInstallStatus,
    },
    rotation::{
        InvalidThreadOverrideValue, RotationRuntime, RotationRuntimeStore, RotationSettings,
        RotationSettingsStore, ThreadId, ThreadOverrideChange,
    },
    StoreUpdate,
};

use super::{thread_metadata::ThreadMetadataStores, RotationServiceAction, SettingsAction};

pub(super) struct LoadedRotation {
    pub settings: RotationSettings,
    pub runtime: RotationRuntime,
    pub thread_titles: BTreeMap<ThreadId, String>,
    pub thread_lineage: BTreeMap<ThreadId, ThreadLineage>,
    pub selectable_models: Vec<SelectableModel>,
    pub install: RouterInstallStatus,
    pub deployment: RouterDeploymentStatus,
}

pub(super) fn change_settings(
    action: SettingsAction,
    accounts: Option<&[AccountId]>,
) -> Result<LoadedRotation> {
    let allowed_reasoning = allowed_reasoning_for_model_change(&action)?;
    let store = RotationSettingsStore::discover()?;
    store.update(|settings| {
        let reconciled = accounts.is_some_and(|accounts| settings.reconcile(accounts));
        let changed = match action {
            SettingsAction::Include(account, included) => {
                Ok(settings.set_included(&account, included))
            }
            SettingsAction::MoveAccount(account, index) => Ok(settings.move_to(&account, index)),
            SettingsAction::Cancel(thread) => Ok(settings.cancel_waiting(&thread)),
            SettingsAction::MoveWaiting(thread, index) => {
                Ok(settings.move_waiting_to(&thread, index))
            }
            SettingsAction::SetThreadOverride(thread, change) => {
                apply_thread_override(settings, &thread, change, allowed_reasoning.as_deref())
            }
        };
        match changed {
            Ok(changed) => StoreUpdate::from_changed(Ok(()), reconciled | changed),
            Err(error) => StoreUpdate::Unchanged(Err(error)),
        }
    })??;
    let metadata_stores = ThreadMetadataStores::discover();
    load_rotation(accounts, &metadata_stores)
}

fn allowed_reasoning_for_model_change(action: &SettingsAction) -> Result<Option<Vec<String>>> {
    let SettingsAction::SetThreadOverride(thread, ThreadOverrideChange::Model(model)) = action
    else {
        return Ok(None);
    };
    let effective_model = match model {
        Some(model) => Some(model.clone()),
        None => RotationRuntimeStore::discover()?
            .load()?
            .thread_rows()
            .into_iter()
            .find(|row| &row.thread_id == thread)
            .and_then(|row| row.request_settings.model),
    };
    let catalogue = codex_router::account_activation::selectable_models();
    Ok(Some(reasoning_efforts(
        &catalogue,
        effective_model.as_deref(),
    )))
}

fn reasoning_efforts(catalogue: &[SelectableModel], model: Option<&str>) -> Vec<String> {
    catalogue
        .iter()
        .find(|choice| Some(choice.slug.as_str()) == model)
        .map(|choice| choice.reasoning_efforts.clone())
        .filter(|efforts| !efforts.is_empty())
        .unwrap_or_else(|| {
            ["low", "medium", "high", "xhigh", "max"]
                .into_iter()
                .map(str::to_owned)
                .collect()
        })
}

pub(super) fn apply_thread_override(
    settings: &mut RotationSettings,
    thread: &ThreadId,
    change: ThreadOverrideChange,
    allowed_reasoning: Option<&[String]>,
) -> std::result::Result<bool, InvalidThreadOverrideValue> {
    let mut changed = settings.set_thread_override(thread, change)?;
    let reasoning_is_invalid = allowed_reasoning.is_some_and(|allowed| {
        settings
            .thread_override(thread)
            .and_then(|thread_override| thread_override.reasoning_effort())
            .is_some_and(|reasoning| !allowed.iter().any(|effort| effort == reasoning))
    });
    if reasoning_is_invalid {
        changed |=
            settings.set_thread_override(thread, ThreadOverrideChange::ReasoningEffort(None))?;
    }
    Ok(changed)
}

pub(super) fn load_rotation(
    accounts: Option<&[AccountId]>,
    metadata_stores: &ThreadMetadataStores,
) -> Result<LoadedRotation> {
    let settings_store = RotationSettingsStore::discover()?;
    let runtime = RotationRuntimeStore::discover()?.load()?;
    let deployment = codex_router::deployment_status(&runtime)?;
    let waiting = runtime.queued_or_resuming_threads();
    let settings = settings_store.update(|settings| {
        let accounts_changed = accounts.is_some_and(|accounts| settings.reconcile(accounts));
        let changed = accounts_changed | settings.reconcile_waiting(&waiting);
        StoreUpdate::from_changed(settings.clone(), changed)
    })?;
    let thread_metadata = metadata_stores.load(&runtime);
    Ok(LoadedRotation {
        settings,
        thread_titles: thread_metadata.titles,
        thread_lineage: thread_metadata.lineage,
        selectable_models: codex_router::account_activation::selectable_models(),
        runtime,
        install: codex_router::status(),
        deployment,
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
