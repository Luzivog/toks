use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use super::model::{Document, DOCUMENT_VERSION};

#[derive(Clone, Debug)]
pub(super) struct Store {
    path: PathBuf,
}

impl Store {
    pub(super) fn discover() -> Result<Self> {
        let root = toks_ingest::paths::get_data_dir().context("no local data directory")?;
        Ok(Self::at(root.join("rotation/account-activation.json")))
    }

    pub(super) fn at(path: PathBuf) -> Self {
        Self { path }
    }

    pub(super) fn load(&self) -> Result<Document> {
        let bytes = match fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Document::new())
            }
            Err(error) => return Err(error).context("reading account activation state"),
        };
        let document: Document =
            serde_json::from_slice(&bytes).context("parsing account activation state")?;
        anyhow::ensure!(
            document.version == DOCUMENT_VERSION,
            "unsupported account activation state version {}",
            document.version
        );
        Ok(document)
    }

    pub(super) fn update<T>(&self, change: impl FnOnce(&mut Document) -> (T, bool)) -> Result<T> {
        let _lock = lock(&self.path)?;
        let mut document = self.load()?;
        let (value, changed) = change(&mut document);
        if changed {
            let bytes = serde_json::to_vec_pretty(&document)?;
            crate::rotation::write_private_atomic(&self.path, &bytes, "account activation state")?;
        }
        Ok(value)
    }
}

fn lock(path: &Path) -> Result<File> {
    let parent = path
        .parent()
        .context("account activation path has no parent")?;
    fs::create_dir_all(parent).context("creating account activation directory")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
    }
    let mut name = path
        .file_name()
        .context("account activation path has no file name")?
        .to_os_string();
    name.push(".lock");
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(parent.join(name))?;
    file.lock().context("locking account activation state")?;
    Ok(file)
}
