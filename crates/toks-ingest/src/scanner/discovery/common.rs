use std::collections::HashSet;
use std::path::{Path, PathBuf};

use rayon::iter::{IntoParallelIterator, ParallelIterator};

use crate::clients::ClientId;

use super::super::{scan_directory, ScanResult};
use super::headless::headless_roots_with_env_strategy;

pub(super) type ScanTask = (ClientId, String, &'static str);

pub(super) struct ScanPlan<'a> {
    pub(super) home_dir: &'a str,
    pub(super) use_env_roots: bool,
    pub(super) include_synthetic: bool,
    pub(super) enabled: HashSet<ClientId>,
    pub(super) enabled_with_devin_lookup: HashSet<ClientId>,
    pub(super) headless_roots: Vec<PathBuf>,
    pub(super) tasks: Vec<ScanTask>,
    pub(super) seen_scan_roots: HashSet<(ClientId, PathBuf)>,
    pub(super) devin_cli_roots: Vec<PathBuf>,
}

impl<'a> ScanPlan<'a> {
    pub(super) fn new(home_dir: &'a str, clients: &[String], use_env_roots: bool) -> Self {
        let include_all = clients.is_empty();
        let include_synthetic = include_all || clients.iter().any(|client| client == "synthetic");
        let enabled = enabled_clients(clients, include_all, include_synthetic);
        let mut enabled_with_devin_lookup = enabled.clone();
        if enabled.contains(&ClientId::DevinDesktop) {
            enabled_with_devin_lookup.insert(ClientId::DevinCli);
        }
        let headless_roots = headless_roots_with_env_strategy(home_dir, use_env_roots);

        Self {
            home_dir,
            use_env_roots,
            include_synthetic,
            enabled,
            enabled_with_devin_lookup,
            headless_roots,
            tasks: Vec::new(),
            seen_scan_roots: HashSet::new(),
            devin_cli_roots: Vec::new(),
        }
    }

    pub(super) fn has(&self, client_id: ClientId) -> bool {
        self.enabled.contains(&client_id)
    }

    pub(super) fn push(&mut self, client_id: ClientId, path: impl Into<PathBuf>) {
        self.push_with_pattern(client_id, path, client_id.data().pattern);
    }

    pub(super) fn push_with_pattern(
        &mut self,
        client_id: ClientId,
        path: impl Into<PathBuf>,
        pattern: &'static str,
    ) {
        push_unique_scan_task(
            &mut self.tasks,
            &mut self.seen_scan_roots,
            client_id,
            path,
            pattern,
        );
    }

    pub(super) fn execute(&mut self, result: &mut ScanResult) -> HashSet<PathBuf> {
        let scan_results: Vec<(ClientId, Vec<PathBuf>)> = std::mem::take(&mut self.tasks)
            .into_par_iter()
            .map(|(client_id, path, pattern)| (client_id, scan_directory(&path, pattern)))
            .collect();

        let mut seen = HashSet::new();
        for (client_id, files) in scan_results {
            for file in files {
                if seen.insert(file.clone()) {
                    result.get_mut(client_id).push(file);
                }
            }
        }
        seen
    }
}

pub(super) fn push_unique_scan_task(
    tasks: &mut Vec<ScanTask>,
    seen: &mut HashSet<(ClientId, PathBuf)>,
    client_id: ClientId,
    path: impl Into<PathBuf>,
    pattern: &'static str,
) {
    let path = path.into();
    if path.as_os_str().is_empty() {
        return;
    }
    let key = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
    if seen.insert((client_id, key)) {
        tasks.push((client_id, path.to_string_lossy().to_string(), pattern));
    }
}

fn enabled_clients(
    clients: &[String],
    include_all: bool,
    include_synthetic: bool,
) -> HashSet<ClientId> {
    if include_all || include_synthetic {
        return ClientId::iter().collect();
    }
    clients
        .iter()
        .filter_map(|client| {
            ClientId::from_str(client).or_else(|| {
                client
                    .eq_ignore_ascii_case("9router")
                    .then_some(ClientId::Gjc)
            })
        })
        .collect()
}

pub(in crate::scanner) fn join_native(root: &str, relative: &str) -> String {
    let mut path = PathBuf::from(root);
    for component in Path::new(relative).components() {
        path.push(component.as_os_str());
    }
    path.to_string_lossy().into_owned()
}

pub(super) fn warn_if_escapes_home(home: &Path, client_id: ClientId, path: &Path) {
    if !path.starts_with(home) {
        tracing::warn!(
            client = client_id.as_str(),
            path = %path.display(),
            home = %home.display(),
            "extra scan path is outside $HOME — verify this is intentional"
        );
    }
}

pub(super) fn dedup_dbs_by_canonical_path(
    paths: impl IntoIterator<Item = PathBuf>,
) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    let mut databases = Vec::new();
    for path in paths {
        let key = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
        if seen.insert(key) {
            databases.push(path);
        }
    }
    databases.sort_unstable();
    databases
}
