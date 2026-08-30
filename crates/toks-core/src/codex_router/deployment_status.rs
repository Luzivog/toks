use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

use super::host::{DeploymentState, GenerationStatus};
use crate::rotation::{TaskActivity, UnixMillis};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouterGenerationRole {
    Active,
    Pending,
    Draining,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouterGenerationSummary {
    pub generation: u64,
    pub build: String,
    pub role: RouterGenerationRole,
    pub task_count: Option<u32>,
    pub oldest_task_at: Option<crate::rotation::UnixMillis>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RouterDeploymentStatus {
    pub generations: Vec<RouterGenerationSummary>,
    pub update_waiting: bool,
}

pub(super) fn load(
    activity: &TaskActivity,
    observed_at: UnixMillis,
) -> Result<RouterDeploymentStatus> {
    load_at(
        &super::systemd::deployment_state_path()?,
        activity,
        observed_at,
    )
}

fn load_at(
    path: &Path,
    activity: &TaskActivity,
    observed_at: UnixMillis,
) -> Result<RouterDeploymentStatus> {
    let state = match fs::read(path) {
        Ok(bytes) => serde_json::from_slice::<DeploymentState>(&bytes)
            .context("parsing router deployment state")?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(RouterDeploymentStatus::default());
        }
        Err(error) => return Err(error).context("reading router deployment state"),
    };
    state
        .validate()
        .context("validating router deployment state")?;
    Ok(project(&state, activity, observed_at))
}

fn project(
    state: &DeploymentState,
    activity: &TaskActivity,
    observed_at: UnixMillis,
) -> RouterDeploymentStatus {
    let workloads = activity.generation_activity_at(observed_at).ok();
    let snapshot = state.snapshot();
    let update_waiting = snapshot.generations.iter().any(|generation| {
        matches!(
            generation.status,
            GenerationStatus::Staged | GenerationStatus::Draining
        )
    });
    let mut generations = snapshot
        .generations
        .into_iter()
        .filter_map(|generation| {
            let role = match generation.status {
                GenerationStatus::Active => RouterGenerationRole::Active,
                GenerationStatus::Staged => RouterGenerationRole::Pending,
                GenerationStatus::Draining => RouterGenerationRole::Draining,
                GenerationStatus::Retired | GenerationStatus::Failed => return None,
            };
            let workload = workloads
                .as_ref()
                .and_then(|workloads| workloads.get(&generation.id.get()))
                .copied();
            Some(RouterGenerationSummary {
                generation: generation.id.get(),
                build: generation.build.as_str().to_owned(),
                role,
                task_count: workload.map(|workload| workload.task_count),
                oldest_task_at: workload.and_then(|workload| workload.oldest_task_at),
            })
        })
        .collect::<Vec<_>>();
    generations.sort_by_key(|generation| match generation.role {
        RouterGenerationRole::Active => (0, generation.generation),
        RouterGenerationRole::Pending => (1, generation.generation),
        RouterGenerationRole::Draining => (2, generation.generation),
    });
    RouterDeploymentStatus {
        generations,
        update_waiting,
    }
}

#[cfg(test)]
#[path = "deployment_status_tests.rs"]
mod tests;
