use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;

use super::KEY_BYTES;

pub(super) fn load_or_create(path: &Path) -> Result<[u8; KEY_BYTES], String> {
    if let Some(key) = read(path)? {
        return Ok(key);
    }
    let mut key = [0_u8; KEY_BYTES];
    getrandom::fill(&mut key).map_err(|error| error.to_string())?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    match options.open(path) {
        Ok(mut file) => {
            file.write_all(&key).map_err(|error| error.to_string())?;
            file.sync_all().map_err(|error| error.to_string())?;
            sync_parent(path)?;
            Ok(key)
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            read(path)?.ok_or_else(|| "accounting source key has an invalid length".to_string())
        }
        Err(error) => Err(error.to_string()),
    }
}

fn read(path: &Path) -> Result<Option<[u8; KEY_BYTES]>, String> {
    let mut bytes = Vec::new();
    match fs::File::open(path) {
        Ok(mut file) => file
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    make_private_file(path)?;
    bytes
        .try_into()
        .map(Some)
        .map_err(|_| "accounting source key has an invalid length".to_string())
}

pub(super) fn ensure_private_directory(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path).map_err(|error| error.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

pub(super) fn make_private_file(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

pub(super) fn sync_parent(path: &Path) -> Result<(), String> {
    let parent = path.parent().ok_or_else(|| "missing parent".to_string())?;
    fs::File::open(parent)
        .and_then(|file| file.sync_all())
        .map_err(|error| error.to_string())
}
