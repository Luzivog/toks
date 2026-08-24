use anyhow::{bail, Context, Result};
use std::fs;
use std::os::unix::fs::FileTypeExt;
use std::path::{Path, PathBuf};

use crate::codex_router::host::{BuildId, DeploymentState, GenerationId};

mod worker_identity;

#[cfg(not(test))]
const INSTALLED_BUILD_ID: &str = "TOKS_ROUTER_BUILD_ID";

#[derive(Clone)]
pub(super) struct HostPaths {
    #[cfg(test)]
    pub(super) executable: PathBuf,
    pub(super) generations: PathBuf,
    pub(super) control: PathBuf,
    pub(super) state: PathBuf,
}

impl HostPaths {
    pub(super) fn discover() -> Result<Self> {
        #[cfg(test)]
        let executable = std::env::current_exe()?.canonicalize()?;
        let runtime = std::env::var_os("RUNTIME_DIRECTORY")
            .map(PathBuf::from)
            .or_else(|| dirs::runtime_dir().map(|root| root.join("toks-router")))
            .context("no router runtime directory")?;
        let data = crate::paths::data_dir()?;
        let artifact_root = crate::paths::router_artifacts_dir_at(&data);
        Ok(Self {
            #[cfg(test)]
            executable,
            generations: artifact_root.join("generations"),
            control: runtime.join("handoff.sock"),
            state: crate::paths::router_deployment_state_at(&data),
        })
    }

    pub(super) fn build_id(&self) -> Result<BuildId> {
        #[cfg(test)]
        {
            let codex = crate::codex_router::codex_binary::discover()?;
            let artifact_root = self
                .generations
                .parent()
                .context("generation directory has no artifact root")?;
            crate::codex_router::systemd::deployment_build_id(
                artifact_root,
                &self.executable,
                &codex,
            )
        }
        #[cfg(not(test))]
        {
            installed_build_id(std::env::var(INSTALLED_BUILD_ID))
        }
    }

    pub(super) fn highest_generation(&self) -> Result<u64> {
        let entries = match fs::read_dir(&self.generations) {
            Ok(entries) => entries,
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
                ) =>
            {
                return Ok(0);
            }
            Err(error) => return Err(error.into()),
        };
        Ok(entries
            .filter_map(Result::ok)
            .filter_map(|entry| entry.file_name().to_string_lossy().parse::<u64>().ok())
            .max()
            .unwrap_or(0))
    }

    pub(super) fn stage(&self, generation: GenerationId, build: &BuildId) -> Result<PathBuf> {
        let directory = self.generations.join(generation.get().to_string());
        let root = self
            .generations
            .parent()
            .context("generation directory has no artifact root")?;
        crate::codex_router::systemd::stage_generation(root, &directory, build)
    }

    pub(super) fn worker_matches(&self, generation: GenerationId, pid: i32) -> bool {
        worker_identity::matches(&self.generations, generation, pid, Path::new("/proc"))
    }

    #[cfg(test)]
    pub(super) fn worker_matches_in(
        &self,
        generation: GenerationId,
        pid: i32,
        proc_root: &Path,
    ) -> bool {
        worker_identity::matches(&self.generations, generation, pid, proc_root)
    }

    pub(super) fn prepare_control_socket(&self) -> Result<()> {
        let parent = self
            .control
            .parent()
            .context("control socket has no parent")?;
        fs::create_dir_all(parent)?;
        if !self.control.exists() {
            return Ok(());
        }
        let metadata = fs::symlink_metadata(&self.control)?;
        if !metadata.file_type().is_socket() {
            bail!("refusing to replace non-socket control path");
        }
        match crate::codex_router::handoff::HandoffChannel::connect(&self.control) {
            Ok(_) => bail!("another router coordinator owns the control socket"),
            Err(crate::codex_router::handoff::HandoffError::System(
                nix::errno::Errno::ECONNREFUSED,
            )) => fs::remove_file(&self.control)?,
            Err(crate::codex_router::handoff::HandoffError::System(nix::errno::Errno::ENOENT)) => {}
            Err(error) => return Err(error.into()),
        }
        Ok(())
    }
}

pub(super) fn installed_build_id(value: Result<String, std::env::VarError>) -> Result<BuildId> {
    let value = value.context("coordinator is missing its installed deployment build identity")?;
    BuildId::new(value).context("coordinator has an invalid installed deployment build identity")
}

pub(super) fn load_state(path: &Path) -> Result<DeploymentState> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).context("parsing router deployment state"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok(DeploymentState::default())
        }
        Err(error) => Err(error).context("reading router deployment state"),
    }
}

pub(super) fn save_state(path: &Path, state: &DeploymentState) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(state)?;
    crate::storage::write_private_atomic(path, &bytes, "router deployment state")
}
