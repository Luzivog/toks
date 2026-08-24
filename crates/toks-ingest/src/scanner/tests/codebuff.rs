use super::*;

fn setup_mock_codebuff_chat(base: &Path, channel: &str, chat_id: &str) -> PathBuf {
    let chat_dir = base
        .join(".config")
        .join(channel)
        .join("projects")
        .join("sandbox")
        .join("chats")
        .join(chat_id);
    fs::create_dir_all(&chat_dir).unwrap();
    let file_path = chat_dir.join("chat-messages.json");
    let mut file = File::create(&file_path).unwrap();
    writeln!(file, "[]").unwrap();
    file_path
}

#[test]
#[serial]
fn test_scan_all_clients_codebuff_walks_all_three_channels_by_default() {
    let previous = std::env::var("CODEBUFF_DATA_DIR").ok();
    unsafe { std::env::remove_var("CODEBUFF_DATA_DIR") };

    let dir = TempDir::new().unwrap();
    let home = dir.path();
    setup_mock_codebuff_chat(home, "manicode", "2025-12-14T10-00-00.000Z");
    setup_mock_codebuff_chat(home, "manicode-dev", "2025-12-14T11-00-00.000Z");
    setup_mock_codebuff_chat(home, "manicode-staging", "2025-12-14T12-00-00.000Z");

    let result = scan_without_extra_dirs(home.to_str().unwrap(), &["codebuff".to_string()]);
    assert_eq!(result.get(ClientId::Codebuff).len(), 3);

    restore_env("CODEBUFF_DATA_DIR", previous);
}

#[test]
#[serial]
fn test_scan_all_clients_codebuff_empty_env_var_falls_back_to_default_channels() {
    let previous = std::env::var("CODEBUFF_DATA_DIR").ok();
    // Regression: a whitespace-only override used to produce zero scan
    // roots because the `Some(_)` branch was taken and then skipped.
    unsafe { std::env::set_var("CODEBUFF_DATA_DIR", "   ") };

    let dir = TempDir::new().unwrap();
    let home = dir.path();
    setup_mock_codebuff_chat(home, "manicode", "2025-12-14T10-00-00.000Z");
    setup_mock_codebuff_chat(home, "manicode-dev", "2025-12-14T11-00-00.000Z");

    let result = scan_without_extra_dirs(home.to_str().unwrap(), &["codebuff".to_string()]);
    assert_eq!(result.get(ClientId::Codebuff).len(), 2);

    restore_env("CODEBUFF_DATA_DIR", previous);
}

#[test]
fn join_native_preserves_an_absolute_root() {
    // `PathBuf::push` on an empty buffer emits no leading separator, so a
    // caller that strips the trailing `/` from a root of `/` turns an
    // absolute scan root into a cwd-relative one. Nothing about the join
    // needs that strip: a trailing separator is collapsed either way.
    #[cfg(unix)]
    {
        assert_eq!(join_native("/", "projects"), "/projects");
        assert_eq!(join_native("/foo/", "projects"), "/foo/projects");
        assert_eq!(join_native("/foo", "projects"), "/foo/projects");
    }
    // An empty root is the shape that produced the relative path.
    assert_eq!(join_native("", "projects"), "projects");
}

#[test]
#[serial]
fn test_scan_all_clients_codebuff_override_root_may_end_in_a_separator() {
    let previous = std::env::var("CODEBUFF_DATA_DIR").ok();

    let dir = TempDir::new().unwrap();
    let home = dir.path();
    let override_root = dir.path().join("custom-codebuff");
    let override_chat_dir = override_root
        .join("projects")
        .join("sandbox")
        .join("chats")
        .join("2025-12-14T11-00-00.000Z");
    fs::create_dir_all(&override_chat_dir).unwrap();
    File::create(override_chat_dir.join("chat-messages.json")).unwrap();

    // Trailing separator: the reason the call site used to trim.
    unsafe {
        std::env::set_var(
            "CODEBUFF_DATA_DIR",
            format!("{}/", override_root.to_string_lossy()),
        )
    };

    let result = scan_without_extra_dirs(home.to_str().unwrap(), &["codebuff".to_string()]);
    assert_eq!(result.get(ClientId::Codebuff).len(), 1);

    restore_env("CODEBUFF_DATA_DIR", previous);
}

#[test]
#[serial]
fn test_scan_all_clients_codebuff_honours_explicit_env_override() {
    let previous = std::env::var("CODEBUFF_DATA_DIR").ok();

    let dir = TempDir::new().unwrap();
    let home = dir.path();
    // Default-channel data that should NOT be picked up when the env is set.
    setup_mock_codebuff_chat(home, "manicode", "2025-12-14T10-00-00.000Z");
    // Override target (lives OUTSIDE ~/.config to prove the override wins).
    let override_root = dir.path().join("custom-codebuff");
    let override_chat_dir = override_root
        .join("projects")
        .join("sandbox")
        .join("chats")
        .join("2025-12-14T11-00-00.000Z");
    fs::create_dir_all(&override_chat_dir).unwrap();
    File::create(override_chat_dir.join("chat-messages.json")).unwrap();

    unsafe {
        std::env::set_var(
            "CODEBUFF_DATA_DIR",
            override_root.to_string_lossy().as_ref(),
        )
    };

    let result = scan_without_extra_dirs(home.to_str().unwrap(), &["codebuff".to_string()]);
    assert_eq!(result.get(ClientId::Codebuff).len(), 1);
    assert!(result.get(ClientId::Codebuff)[0]
        .to_string_lossy()
        .contains("custom-codebuff"));

    restore_env("CODEBUFF_DATA_DIR", previous);
}

#[test]
#[serial]
fn test_scan_all_clients_freebuff_enables_shared_manicode_scan() {
    let mut env = EnvGuard::capture(&["FREEBUFF_DATA_DIR", "CODEBUFF_DATA_DIR"]);
    env.remove("FREEBUFF_DATA_DIR");
    env.remove("CODEBUFF_DATA_DIR");

    let dir = TempDir::new().unwrap();
    let home = dir.path();
    setup_mock_codebuff_chat(home, "manicode", "2025-12-14T10-00-00.000Z");

    // Freebuff shares Codebuff's manicode layout, so enabling it alone must
    // still walk the shared root (populating the Codebuff scan vector) for
    // the estimated Freebuff parser in lib.rs to have files to read.
    let result = scan_without_extra_dirs(home.to_str().unwrap(), &["freebuff".to_string()]);
    assert_eq!(result.get(ClientId::Codebuff).len(), 1);
}

#[test]
#[serial]
fn test_freebuff_fallback_does_not_defeat_codebuff_data_dir_exclusivity() {
    // CODEBUFF_DATA_DIR is documented as exclusive: point it somewhere and
    // the default ~/.config/manicode channels are not scanned. Freebuff
    // writes into that same tree, so with only CODEBUFF_DATA_DIR set it has
    // to follow the redirect. Falling back to the default channel roots for
    // the Freebuff half of an all-clients run would re-scan the very
    // directory the user redirected away from.
    let mut env = EnvGuard::capture(&["FREEBUFF_DATA_DIR", "CODEBUFF_DATA_DIR"]);
    env.remove("FREEBUFF_DATA_DIR");

    let dir = TempDir::new().unwrap();
    let home = dir.path();
    // The default location the user redirected away from.
    setup_mock_codebuff_chat(home, "manicode", "2025-12-14T10-00-00.000Z");

    // The redirect target, with its own chat.
    let override_home = TempDir::new().unwrap();
    let override_root = override_home.path().join("elsewhere");
    let redirected_chat = override_root
        .join("projects")
        .join("sandbox")
        .join("chats")
        .join("2025-12-14T11-00-00.000Z");
    fs::create_dir_all(&redirected_chat).unwrap();
    writeln!(
        File::create(redirected_chat.join("chat-messages.json")).unwrap(),
        "[]"
    )
    .unwrap();
    env.set(
        "CODEBUFF_DATA_DIR",
        override_root.to_string_lossy().as_ref(),
    );

    let result = scan_without_extra_dirs(
        home.to_str().unwrap(),
        &["codebuff".to_string(), "freebuff".to_string()],
    );

    let found = result.get(ClientId::Codebuff);
    assert_eq!(
        found.len(),
        1,
        "only the redirected root should be scanned, got {found:?}"
    );
    assert!(
        !found[0].to_string_lossy().contains("manicode"),
        "the default manicode root must not be re-scanned via the Freebuff \
         fallback, got {found:?}"
    );
}

#[test]
#[serial]
fn test_scan_all_clients_codebuff_ignores_freebuff_override_when_not_enabled() {
    let mut env = EnvGuard::capture(&["FREEBUFF_DATA_DIR", "CODEBUFF_DATA_DIR"]);
    env.remove("CODEBUFF_DATA_DIR");
    let override_dir = TempDir::new().unwrap();
    // The Freebuff override must NOT redirect a codebuff-only scan: each
    // client resolves only its own override (see the shared-manicode scan).
    unsafe {
        std::env::set_var(
            "FREEBUFF_DATA_DIR",
            override_dir.path().to_string_lossy().as_ref(),
        )
    };

    let dir = TempDir::new().unwrap();
    let home = dir.path();
    setup_mock_codebuff_chat(home, "manicode", "2025-12-14T10-00-00.000Z");

    let result = scan_without_extra_dirs(home.to_str().unwrap(), &["codebuff".to_string()]);
    assert_eq!(result.get(ClientId::Codebuff).len(), 1);
    // Compare against a natively joined suffix: scan roots are built with
    // `join_native`, so this path is `manicode\projects` on Windows and a
    // hardcoded `manicode/projects` never matches there.
    let found = result.get(ClientId::Codebuff)[0]
        .to_string_lossy()
        .into_owned();
    assert!(
        found.contains(&join_native("manicode", "projects")),
        "expected the default manicode root, got {found}"
    );
}
