use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use super::Admission;

const VERSION: u8 = 1;

pub(super) struct AdmissionStore {
    path: Option<PathBuf>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct StoredAdmissions {
    version: u8,
    admissions: Vec<Admission>,
}

impl AdmissionStore {
    pub fn new(path: Option<PathBuf>) -> Self {
        Self { path }
    }

    pub fn load(&self) -> Result<BTreeMap<[u8; 32], Admission>> {
        let Some(path) = &self.path else {
            return Ok(BTreeMap::new());
        };
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(BTreeMap::new());
            }
            Err(error) => return Err(error).context("reading inbound token admissions"),
        };
        let stored: StoredAdmissions =
            serde_json::from_slice(&bytes).context("parsing inbound token admissions")?;
        if stored.version != VERSION {
            bail!("unsupported inbound token admission version");
        }
        Ok(stored
            .admissions
            .into_iter()
            .filter(|admission| admission.expires_at.is_some())
            .map(|admission| (admission.digest, admission))
            .collect())
    }

    pub fn save(&self, admissions: &BTreeMap<[u8; 32], Admission>) -> Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        let durable = admissions
            .values()
            .filter(|admission| admission.expires_at.is_some())
            .cloned()
            .collect();
        let bytes = serde_json::to_vec_pretty(&StoredAdmissions {
            version: VERSION,
            admissions: durable,
        })?;
        crate::rotation::write_private_atomic(path, &bytes, "inbound token admissions")
    }
}
