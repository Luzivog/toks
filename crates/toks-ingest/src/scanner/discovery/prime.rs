use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::clients::ClientId;

use super::ScanPlan;

pub(in crate::scanner) fn expand_tilde_path_with_home(value: &str, home_dir: &str) -> PathBuf {
    if value == "~" {
        return PathBuf::from(home_dir);
    }
    if let Some(relative) = value.strip_prefix("~/") {
        return PathBuf::from(home_dir).join(relative);
    }
    PathBuf::from(value)
}

#[derive(Debug, PartialEq, Eq)]
pub(in crate::scanner) enum PrimeSessionDirSetting {
    Default,
    Path(PathBuf),
    CurrentDirectory(PathBuf),
}

pub(in crate::scanner) fn prime_agent_session_dir_from_settings_files(
    global_settings: &Path,
    project_settings: Option<&Path>,
    home_dir: &str,
    current_dir: Option<&Path>,
) -> Option<PrimeSessionDirSetting> {
    fn read_session_dir(path: &Path) -> Option<Option<String>> {
        let content = std::fs::read_to_string(path).ok()?;
        let settings: Value = serde_json::from_str(&content).ok()?;
        match settings.as_object()?.get("sessionDir")? {
            Value::Null => Some(None),
            Value::String(path) => Some(Some(path.clone())),
            _ => None,
        }
    }

    let global = read_session_dir(global_settings);
    let project = project_settings.and_then(read_session_dir);
    project.or(global).map(|setting| match setting {
        None => PrimeSessionDirSetting::Default,
        Some(path) if path.is_empty() => PrimeSessionDirSetting::CurrentDirectory(
            current_dir.unwrap_or_else(|| Path::new("")).to_path_buf(),
        ),
        Some(path) => PrimeSessionDirSetting::Path(expand_tilde_path_with_home(&path, home_dir)),
    })
}

fn session_dir_from_settings(agent_dir: &Path, home_dir: &str) -> Option<PrimeSessionDirSetting> {
    let current_dir = std::env::current_dir().ok();
    let project_settings = current_dir
        .as_ref()
        .map(|cwd| cwd.join(".prime/agent/settings.json"));
    prime_agent_session_dir_from_settings_files(
        &agent_dir.join("settings.json"),
        project_settings.as_deref(),
        home_dir,
        current_dir.as_deref(),
    )
}

pub fn prime_agent_session_roots_with_env_strategy(
    home_dir: &str,
    use_env_roots: bool,
) -> [PathBuf; 2] {
    fn with_artifacts(sessions: PathBuf) -> [PathBuf; 2] {
        let artifacts = sessions
            .parent()
            .unwrap_or_else(|| Path::new(""))
            .join("session-artifacts");
        [sessions, artifacts]
    }

    if !use_env_roots {
        let agent_dir = PathBuf::from(home_dir).join(".prime/agent");
        return [
            agent_dir.join("sessions"),
            agent_dir.join("session-artifacts"),
        ];
    }

    let session_override = std::env::var("PRIME_AGENT_SESSION_DIR")
        .ok()
        .or_else(|| std::env::var("PRIME_AGENT_CODING_AGENT_SESSION_DIR").ok());
    if let Some(path) = session_override.filter(|value| !value.is_empty()) {
        return with_artifacts(expand_tilde_path_with_home(&path, home_dir));
    }

    let agent_dir = std::env::var("PRIME_AGENT_CODING_AGENT_DIR")
        .ok()
        .filter(|value| !value.is_empty())
        .map(|path| expand_tilde_path_with_home(&path, home_dir))
        .unwrap_or_else(|| PathBuf::from(home_dir).join(".prime/agent"));
    match session_dir_from_settings(&agent_dir, home_dir) {
        Some(PrimeSessionDirSetting::Path(sessions)) => with_artifacts(sessions),
        Some(PrimeSessionDirSetting::CurrentDirectory(current_dir)) => {
            [current_dir.clone(), current_dir.join("session-artifacts")]
        }
        Some(PrimeSessionDirSetting::Default) | None => [
            agent_dir.join("sessions"),
            agent_dir.join("session-artifacts"),
        ],
    }
}

pub(super) fn add_tasks(plan: &mut ScanPlan<'_>) {
    if !plan.has(ClientId::PrimeAgent) {
        return;
    }
    let roots = prime_agent_session_roots_with_env_strategy(plan.home_dir, plan.use_env_roots);
    for root in roots {
        plan.push_with_pattern(ClientId::PrimeAgent, root, "prime-agent-session");
    }
}
