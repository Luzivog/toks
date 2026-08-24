use std::path::{Path, PathBuf};

use crate::clients::ClientId;
use crate::scanner::ScanResult;

use super::common::join_native;
use super::ScanPlan;

pub(super) fn discover_synthetic(plan: &ScanPlan<'_>, result: &mut ScanResult) {
    if !plan.include_synthetic {
        return;
    }
    let xdg_data = if plan.use_env_roots {
        std::env::var("XDG_DATA_HOME")
            .unwrap_or_else(|_| join_native(plan.home_dir, ".local/share"))
    } else {
        join_native(plan.home_dir, ".local/share")
    };
    let database = PathBuf::from(join_native(&xdg_data, "octofriend/sqlite.db"));
    if database.exists() {
        result.synthetic_db = Some(database);
    }
}

pub(super) fn discover_kilo(plan: &ScanPlan<'_>, result: &mut ScanResult) {
    if !plan.has(ClientId::Kilo) {
        return;
    }
    let path = ClientId::Kilo
        .data()
        .resolve_path_with_env_strategy(plan.home_dir, plan.use_env_roots);
    if Path::new(&path).exists() {
        result.kilo_db = Some(PathBuf::from(path));
    }
}

pub(super) fn discover_goose(plan: &ScanPlan<'_>, result: &mut ScanResult) {
    if !plan.has(ClientId::Goose) {
        return;
    }
    if plan.use_env_roots {
        if let Ok(custom_root) = std::env::var("GOOSE_PATH_ROOT") {
            let trimmed = custom_root.trim();
            if !trimmed.is_empty() {
                set_if_file(
                    &mut result.goose_db,
                    PathBuf::from(trimmed).join("data/sessions/sessions.db"),
                );
            }
        }
    }
    let primary = PathBuf::from(
        ClientId::Goose
            .data()
            .resolve_path_with_env_strategy(plan.home_dir, plan.use_env_roots),
    );
    set_if_file(&mut result.goose_db, primary);
    set_if_file(
        &mut result.goose_db,
        PathBuf::from(format!(
            "{}/Library/Application Support/goose/sessions/sessions.db",
            plan.home_dir
        )),
    );
    set_if_file(
        &mut result.goose_db,
        PathBuf::from(format!(
            "{}/Library/Application Support/Block/goose/sessions/sessions.db",
            plan.home_dir
        )),
    );
    set_if_file(
        &mut result.goose_db,
        PathBuf::from(format!(
            "{}/.local/share/Block/goose/sessions/sessions.db",
            plan.home_dir
        )),
    );
}

pub(super) fn discover_zed(plan: &ScanPlan<'_>, result: &mut ScanResult) {
    if !plan.has(ClientId::Zed) {
        return;
    }
    let primary = PathBuf::from(
        ClientId::Zed
            .data()
            .resolve_path_with_env_strategy(plan.home_dir, plan.use_env_roots),
    );
    set_if_file(&mut result.zed_db, primary);
    #[cfg(target_os = "macos")]
    set_if_file(
        &mut result.zed_db,
        PathBuf::from(format!(
            "{}/Library/Application Support/Zed/threads/threads.db",
            plan.home_dir
        )),
    );
    if !plan.use_env_roots {
        set_if_file(
            &mut result.zed_db,
            PathBuf::from(plan.home_dir).join("AppData/Local/Zed/threads/threads.db"),
        );
    }
    #[cfg(target_os = "windows")]
    if plan.use_env_roots && result.zed_db.is_none() {
        if let Some(local) = dirs::data_local_dir() {
            set_if_file(&mut result.zed_db, local.join("Zed/threads/threads.db"));
        }
    }
}

pub(super) fn discover_zcode(plan: &ScanPlan<'_>, result: &mut ScanResult) {
    if plan.has(ClientId::Zcode) {
        let path = PathBuf::from(join_native(plan.home_dir, ".zcode/cli/db/db.sqlite"));
        if path.is_file() {
            result.zcode_db = Some(path);
        }
    }
}

fn set_if_file(slot: &mut Option<PathBuf>, candidate: PathBuf) {
    if slot.is_none() && candidate.is_file() {
        *slot = Some(candidate);
    }
}
