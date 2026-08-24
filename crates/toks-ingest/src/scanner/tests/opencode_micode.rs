use super::*;

#[test]
#[serial]
fn test_scan_all_clients_opencode() {
    let previous_xdg = std::env::var("XDG_DATA_HOME").ok();

    let dir = TempDir::new().unwrap();
    let home = dir.path();
    setup_mock_opencode_dir(home);

    // Set XDG_DATA_HOME for the test
    unsafe { std::env::set_var("XDG_DATA_HOME", home.join(".local/share")) };

    let result = scan_without_extra_dirs(home.to_str().unwrap(), &["opencode".to_string()]);
    assert_eq!(result.get(ClientId::OpenCode).len(), 1);
    assert!(result.get(ClientId::Claude).is_empty());
    assert!(result.get(ClientId::Codex).is_empty());
    assert!(result.get(ClientId::Gemini).is_empty());

    restore_env("XDG_DATA_HOME", previous_xdg);
}

#[test]
#[serial]
fn test_scan_all_clients_opencode_home_override_ignores_xdg_env() {
    let previous_xdg = std::env::var("XDG_DATA_HOME").ok();

    let dir = TempDir::new().unwrap();
    let home = dir.path().join("target-home");
    let conflicting_xdg = dir.path().join("conflicting-xdg");
    setup_mock_opencode_dir(&home);
    fs::create_dir_all(&conflicting_xdg).unwrap();

    unsafe { std::env::set_var("XDG_DATA_HOME", &conflicting_xdg) };

    let result = scan_all_clients_with_env_strategy(
        home.to_str().unwrap(),
        &["opencode".to_string()],
        false,
    );
    assert_eq!(result.get(ClientId::OpenCode).len(), 1);
    assert_eq!(
        result.opencode_json_dir,
        Some(home.join(".local/share/opencode/storage/message"))
    );

    restore_env("XDG_DATA_HOME", previous_xdg);
}

#[test]
fn test_is_opencode_db_filename_accepts_default_and_channel_variants() {
    // Default channel (`latest`/`beta`) and explicit-disable use this name.
    assert!(is_opencode_db_filename("opencode.db"));
    // Channel-suffixed dbs, drawn from opencode's `[a-zA-Z0-9._-]`
    // character class in getChannelPath.
    assert!(is_opencode_db_filename("opencode-stable.db"));
    assert!(is_opencode_db_filename("opencode-nightly.db"));
    assert!(is_opencode_db_filename("opencode-canary.db"));
    assert!(is_opencode_db_filename("opencode-local.db"));
    assert!(is_opencode_db_filename("opencode-1.2.3.db"));
    assert!(is_opencode_db_filename("opencode-pr_42.db"));
}

#[test]
fn test_is_opencode_db_filename_rejects_sidecars_and_unrelated_files() {
    // WAL/SHM/journal sidecar files share the prefix — must be ignored
    // so we don't try to "parse" them.
    assert!(!is_opencode_db_filename("opencode.db-wal"));
    assert!(!is_opencode_db_filename("opencode.db-shm"));
    assert!(!is_opencode_db_filename("opencode.db-journal"));
    assert!(!is_opencode_db_filename("opencode-stable.db-wal"));
    // Unrelated / malformed names.
    assert!(!is_opencode_db_filename("opencode"));
    assert!(!is_opencode_db_filename("opencode-.db"));
    assert!(!is_opencode_db_filename("opencode_stable.db"));
    assert!(!is_opencode_db_filename("opencode-stable/beta.db"));
    assert!(!is_opencode_db_filename("auth.json"));
    assert!(!is_opencode_db_filename("other.db"));
}

#[test]
fn test_is_micode_db_filename_accepts_default_and_channel_rejects_sidecars() {
    // Default and channel-suffixed db names are accepted.
    assert!(is_micode_db_filename("mimocode.db"));
    assert!(is_micode_db_filename("mimocode-stable.db"));
    assert!(is_micode_db_filename("mimocode-nightly.db"));
    // WAL/SHM sidecar files share the prefix — must be ignored.
    assert!(!is_micode_db_filename("mimocode.db-wal"));
    assert!(!is_micode_db_filename("mimocode.db-shm"));
}

#[test]
fn test_discover_micode_dbs_in_dirs_unions_xdg_and_orca_roots() {
    let dir = TempDir::new().unwrap();
    // Primary XDG location.
    let xdg_dir = dir.path().join(".local/share/mimocode");
    fs::create_dir_all(&xdg_dir).unwrap();
    let xdg_db = xdg_dir.join("mimocode.db");
    fs::write(&xdg_db, b"").unwrap();

    // orca hook-sandbox location, holding both the default db and a
    // channel-suffixed one that the XDG root is missing.
    let orca_dir = dir
        .path()
        .join("Library/Application Support/orca/mimocode-hooks/shared/data");
    fs::create_dir_all(&orca_dir).unwrap();
    let orca_db = orca_dir.join("mimocode.db");
    let orca_channel_db = orca_dir.join("mimocode-nightly.db");
    fs::write(&orca_db, b"").unwrap();
    fs::write(&orca_channel_db, b"").unwrap();
    // Sidecar files must be ignored across both roots.
    fs::write(orca_dir.join("mimocode.db-wal"), b"").unwrap();

    let dbs = discover_micode_dbs_in_dirs([xdg_dir, orca_dir]);

    assert!(dbs.contains(&xdg_db), "XDG db should be discovered");
    assert!(dbs.contains(&orca_db), "orca db should be discovered");
    assert!(
        dbs.contains(&orca_channel_db),
        "orca channel db should be discovered"
    );
    assert_eq!(dbs.len(), 3, "no sidecar files, no missed dbs");
}

#[test]
fn test_discover_micode_dbs_in_dirs_collapses_same_file_via_symlink() {
    // A symlink making two roots resolve to the same file must not yield the
    // db twice, or its non-embedded-id messages would be double-counted.
    let dir = TempDir::new().unwrap();
    let real_dir = dir.path().join("real/mimocode");
    fs::create_dir_all(&real_dir).unwrap();
    let real_db = real_dir.join("mimocode.db");
    fs::write(&real_db, b"").unwrap();

    let link_dir = dir.path().join("linked");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&real_dir, &link_dir).unwrap();
    #[cfg(not(unix))]
    std::os::windows::fs::symlink_dir(&real_dir, &link_dir).unwrap();

    let dbs = discover_micode_dbs_in_dirs([real_dir, link_dir]);
    assert_eq!(dbs.len(), 1, "symlinked duplicate must collapse to one db");
}

#[test]
fn test_discover_opencode_dbs_finds_multiple_channels_and_skips_sidecars() {
    let dir = TempDir::new().unwrap();
    let data_dir = dir.path().join("opencode");
    fs::create_dir_all(&data_dir).unwrap();

    // Real dbs for two channels running side by side — the case from
    // junhoyeo/tokscope#387.
    File::create(data_dir.join("opencode.db")).unwrap();
    File::create(data_dir.join("opencode-stable.db")).unwrap();
    // SQLite WAL/SHM sidecars that must not be treated as dbs.
    File::create(data_dir.join("opencode.db-wal")).unwrap();
    File::create(data_dir.join("opencode.db-shm")).unwrap();
    File::create(data_dir.join("opencode-stable.db-wal")).unwrap();
    // Unrelated files that live in the same dir.
    File::create(data_dir.join("auth.json")).unwrap();

    let found = discover_opencode_dbs(&data_dir);
    let names: Vec<String> = found
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, vec!["opencode-stable.db", "opencode.db"]);
}

#[test]
fn test_discover_opencode_dbs_returns_empty_for_missing_dir() {
    let dir = TempDir::new().unwrap();
    let missing = dir.path().join("does-not-exist");
    assert!(discover_opencode_dbs(&missing).is_empty());
}

#[test]
fn test_merge_user_opencode_db_paths_picks_up_path_outside_xdg() {
    // Simulate `OPENCODE_DB=/arbitrary/abs/path/custom.db` upstream:
    // the file is a real opencode db but lives outside
    // `~/.local/share/opencode`, so auto-discovery never sees it.
    let dir = TempDir::new().unwrap();
    let outside = dir.path().join("somewhere-else");
    fs::create_dir_all(&outside).unwrap();
    let user_db = outside.join("opencode.db");
    File::create(&user_db).unwrap();

    let mut discovered: Vec<PathBuf> = Vec::new();
    merge_user_opencode_db_paths(&mut discovered, std::slice::from_ref(&user_db));

    assert_eq!(discovered, vec![user_db]);
}

#[test]
fn test_merge_user_opencode_db_paths_skips_nonexistent_and_sidecars() {
    let dir = TempDir::new().unwrap();
    let real = dir.path().join("opencode-stable.db");
    File::create(&real).unwrap();
    let wal = dir.path().join("opencode-stable.db-wal");
    File::create(&wal).unwrap();
    let missing = dir.path().join("opencode-missing.db"); // never created

    let mut discovered: Vec<PathBuf> = Vec::new();
    merge_user_opencode_db_paths(
        &mut discovered,
        &[real.clone(), wal.clone(), missing.clone()],
    );

    // Nonexistent path: silently skipped so stale config can't break a scan.
    // Sidecar path: rejected by is_opencode_db_filename.
    assert_eq!(discovered, vec![real]);
}

#[test]
fn test_merge_user_opencode_db_paths_dedups_against_auto_discovered() {
    let dir = TempDir::new().unwrap();
    let shared = dir.path().join("opencode.db");
    File::create(&shared).unwrap();

    // User explicitly lists a path that auto-discovery also found —
    // must not double-parse the same sqlite file.
    let mut discovered: Vec<PathBuf> = vec![shared.clone()];
    merge_user_opencode_db_paths(&mut discovered, std::slice::from_ref(&shared));

    assert_eq!(discovered, vec![shared]);
}

#[test]
#[serial]
fn test_scan_all_clients_opencode_picks_up_channel_suffixed_dbs() {
    let previous_xdg = std::env::var("XDG_DATA_HOME").ok();

    let dir = TempDir::new().unwrap();
    let home = dir.path();
    let data_dir = home.join(".local/share/opencode");
    fs::create_dir_all(&data_dir).unwrap();

    File::create(data_dir.join("opencode.db")).unwrap();
    File::create(data_dir.join("opencode-stable.db")).unwrap();
    File::create(data_dir.join("opencode-nightly.db")).unwrap();
    // Sidecars that must be ignored.
    File::create(data_dir.join("opencode.db-wal")).unwrap();
    File::create(data_dir.join("opencode-stable.db-shm")).unwrap();

    unsafe { std::env::set_var("XDG_DATA_HOME", home.join(".local/share")) };

    let result = scan_without_extra_dirs(home.to_str().unwrap(), &["opencode".to_string()]);

    let names: Vec<String> = result
        .opencode_dbs
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        names,
        vec![
            "opencode-nightly.db".to_string(),
            "opencode-stable.db".to_string(),
            "opencode.db".to_string(),
        ],
        "expected all channel dbs, got {names:?}"
    );

    restore_env("XDG_DATA_HOME", previous_xdg);
}
