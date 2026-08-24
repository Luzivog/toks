use crate::{message_cache, sessions};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct DevinDesktopLookupSnapshot {
    db_paths: Vec<PathBuf>,
    related_files: Vec<message_cache::RelatedFileFingerprint>,
}

pub(crate) type DevinDesktopLookupCache = Mutex<
    HashMap<DevinDesktopLookupSnapshot, Arc<OnceLock<sessions::devin::DevinDesktopSessionLookup>>>,
>;

pub(crate) fn devin_desktop_lookup_cell_for_snapshot(
    lookup_cache: &DevinDesktopLookupCache,
    db_paths: &[PathBuf],
    fingerprint: &message_cache::SourceFingerprint,
) -> Arc<OnceLock<sessions::devin::DevinDesktopSessionLookup>> {
    let snapshot = DevinDesktopLookupSnapshot {
        db_paths: db_paths.to_vec(),
        related_files: fingerprint.related_files.clone(),
    };
    let mut lookups = lookup_cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    Arc::clone(
        lookups
            .entry(snapshot)
            .or_insert_with(|| Arc::new(OnceLock::new())),
    )
}
