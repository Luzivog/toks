use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};

use tempfile::tempdir;

#[test]
fn discovery_validation_pins_the_canonical_executable() {
    let directory = tempdir().unwrap();
    let bin = directory.path().join("bin");
    let real = directory.path().join("real/codex");
    fs::create_dir_all(&bin).unwrap();
    fs::create_dir_all(real.parent().unwrap()).unwrap();
    fs::write(&real, b"#!/bin/sh\n").unwrap();
    fs::set_permissions(&real, fs::Permissions::from_mode(0o755)).unwrap();
    let alias = bin.join("codex");
    symlink(&real, &alias).unwrap();

    let found = super::codex_binary::validate(bin.join("../bin/codex")).unwrap();

    assert_eq!(found, real.canonicalize().unwrap());
    fs::remove_file(&alias).unwrap();
    symlink("/bin/false", &alias).unwrap();
    assert_eq!(found, real.canonicalize().unwrap());
}
