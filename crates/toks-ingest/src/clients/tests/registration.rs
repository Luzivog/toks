use super::super::{ClientDef, ClientId, PathRoot};
use super::{
    absolute_test_path, native_join, reasonix_default_root, reasonix_home, reasonix_stats_under,
};
use crate::paths::test_env::EnvGuard;
use serial_test::serial;

#[test]
fn every_registered_client_has_human_readable_display_metadata() {
    for client in ClientId::iter() {
        let display_name = client.display_name();
        assert!(
            !display_name.trim().is_empty(),
            "{} has no display name",
            client.as_str()
        );
        assert_ne!(
            display_name,
            client.as_str(),
            "{} falls back to its raw lowercase id",
            client.as_str()
        );
    }
}

#[test]
fn canonical_client_brand_labels_and_logos_are_registered() {
    assert_eq!(ClientId::Claude.display_name(), "Claude Code");
    assert_eq!(ClientId::Codex.display_name(), "Codex CLI");
    assert_eq!(ClientId::Cursor.display_name(), "Cursor IDE");
    assert_eq!(ClientId::KiloCode.display_name(), "Kilo Code");
    assert_eq!(ClientId::Kilo.display_name(), "Kilo CLI");
    assert_eq!(ClientId::Senpi.display_name(), "Senpi (OmO Native)");
    assert_eq!(
        ClientId::OpenCode.logo_url(),
        Some("https://tokscope.ai/assets/logos/opencode.png")
    );
}

#[test]
fn test_client_id_count() {
    assert_eq!(ClientId::COUNT, 45);
}

#[test]
fn test_senpi_client_registered_as_local_session_source() {
    let client = ClientId::from_str("senpi").expect("senpi client should be registered");
    assert_eq!(client.data().relative_path, "sessions");
    assert_eq!(client.data().pattern, "*.jsonl");
    assert!(client.data().parse_local);
    assert!(client.data().submit_default);
    assert!(!client.data().headless);
}

#[test]
fn test_prime_agent_client_registered_as_local_session_source() {
    let client =
        ClientId::from_str("prime-agent").expect("prime-agent client should be registered");
    assert_eq!(
        client
            .data()
            .resolve_path_with_env_strategy("/tmp/home", false),
        native_join(std::path::Path::new("/tmp/home"), ".prime/agent/sessions")
    );
    assert_eq!(client.data().pattern, "*.jsonl");
    assert!(client.data().parse_local);
    assert!(client.data().submit_default);
    assert!(!client.data().headless);
}

#[test]
fn test_resolve_path_joins_with_native_separators_not_hardcoded_slash() {
    // #1048: on Windows a `C:\Users\me` root joined with a `/`-separated
    // relative path used to produce `C:\Users\me/.codex/sessions` (mixed
    // separators) that reached user-facing `clients --json` output. The
    // joined result must use native separators throughout — component
    // pushes, not a hand-concatenated "{root}/{relative}" string and not
    // a single `Path::join` (which only normalizes the junction).
    let client = ClientDef {
        id: "codex",
        root: PathRoot::Home,
        relative_path: ".codex/sessions",
        pattern: "*.jsonl",
        headless: false,
        parse_local: true,
        submit_default: true,
    };
    let windows_style_home = r"C:\Users\me";
    let joined = client.resolve_path_with_env_strategy(windows_style_home, false);
    let expected = native_join(
        std::path::Path::new(windows_style_home),
        client.relative_path,
    );
    assert_eq!(joined, expected);
    // On Windows the resolved path must use native separators throughout:
    // no forward slash may remain from the relative half or the joiner.
    #[cfg(windows)]
    assert!(
        !joined.contains('/'),
        "mixed separators in resolved path: {joined:?}"
    );
}

#[test]
fn test_augment_client_registered_as_local_session_source() {
    let client = ClientId::from_str("augment").expect("augment client should be registered");
    assert_eq!(
        client.data().resolve_path("/tmp/home"),
        native_join(std::path::Path::new("/tmp/home"), ".augment/sessions")
    );
    assert_eq!(client.data().relative_path, ".augment/sessions");
    assert_eq!(client.data().pattern, "*.json");
    assert!(client.data().parse_local);
    assert!(client.data().submit_default);
    assert!(!client.data().headless);
}

#[test]
fn test_kimchi_client_registered_as_local_session_source() {
    let client = ClientId::from_str("kimchi").expect("kimchi client should be registered");
    assert_eq!(client.data().relative_path, "sessions");
    assert_eq!(client.data().pattern, "*.jsonl");
    assert!(client.data().parse_local);
    assert!(client.data().submit_default);
    assert!(!client.data().headless);
}

#[test]
fn test_reasonix_client_registered_as_local_session_source() {
    let client = ClientId::from_str("reasonix").expect("reasonix client should be registered");
    assert_eq!(
        client
            .data()
            .resolve_path_with_env_strategy(reasonix_home(), false),
        reasonix_stats_under(reasonix_default_root())
    );
    assert_eq!(client.data().pattern, "*.jsonl");
    assert!(client.data().parse_local);
    assert!(client.data().submit_default);
    assert!(!client.data().headless);
}

#[test]
#[serial]
fn test_reasonix_stats_prefers_state_home_then_reasonix_home() {
    let mut env = EnvGuard::capture(&["REASONIX_STATE_HOME", "REASONIX_HOME"]);
    let custom_home = absolute_test_path("custom/reasonix-home");
    let custom_state = absolute_test_path("custom/reasonix-state");
    env.set("REASONIX_HOME", &custom_home);
    env.set("REASONIX_STATE_HOME", &custom_state);
    let client = ClientId::Reasonix;
    assert_eq!(
        client.data().resolve_path(reasonix_home()),
        reasonix_stats_under(&custom_state)
    );
    env.remove("REASONIX_STATE_HOME");
    assert_eq!(
        client.data().resolve_path(reasonix_home()),
        reasonix_stats_under(&custom_home)
    );
}

#[test]
#[serial]
fn test_reasonix_stats_normalizes_env_roots_and_ignores_blank_values() {
    let mut env = EnvGuard::capture(&["REASONIX_STATE_HOME", "REASONIX_HOME"]);
    let client = ClientId::Reasonix;

    env.set("REASONIX_STATE_HOME", "  ~/reasonix-state  ");
    env.set("REASONIX_HOME", absolute_test_path("unused/reasonix-home"));
    assert_eq!(
        client.data().resolve_path(reasonix_home()),
        reasonix_stats_under(native_join(
            std::path::Path::new(reasonix_home()),
            "reasonix-state"
        ))
    );

    env.set("REASONIX_STATE_HOME", " \t ");
    env.set("REASONIX_HOME", " relative-reasonix ");
    let expected = reasonix_stats_under(
        std::env::current_dir()
            .expect("test process has a current directory")
            .join("relative-reasonix"),
    );
    assert_eq!(client.data().resolve_path(reasonix_home()), expected);
}

#[test]
#[serial]
fn test_reasonix_stats_expands_environment_references_before_normalizing_paths() {
    let mut env = EnvGuard::capture(&[
        "REASONIX_STATE_HOME",
        "TOKS_REASONIX_TEST_ROOT",
        "TOKS_REASONIX_TEST_UNSET",
    ]);
    let client = ClientId::Reasonix;

    env.set("TOKS_REASONIX_TEST_ROOT", "~/reasonix-state");
    env.remove("TOKS_REASONIX_TEST_UNSET");
    env.set("REASONIX_STATE_HOME", "${TOKS_REASONIX_TEST_ROOT}/nested");
    assert_eq!(
        client.data().resolve_path(reasonix_home()),
        reasonix_stats_under(native_join(
            std::path::Path::new(reasonix_home()),
            "reasonix-state/nested"
        ))
    );

    env.set(
        "REASONIX_STATE_HOME",
        "${TOKS_REASONIX_TEST_UNSET:-relative-reasonix}",
    );
    let expected = reasonix_stats_under(
        std::env::current_dir()
            .expect("test process has a current directory")
            .join("relative-reasonix"),
    );
    // The home argument cannot reach this expectation — the default the
    // reference falls back to is relative, so the resolver prepends the
    // working directory and never consults the home. It is still
    // `reasonix_home()` like every other call in this module: a literal
    // `/tmp/home` here would read as a claim that this arm is special,
    // when the only thing special about it is that any home would do.
    assert_eq!(client.data().resolve_path(reasonix_home()), expected);
}

#[test]
#[serial]
fn test_reasonix_stats_ignores_env_roots_when_requested() {
    let mut env = EnvGuard::capture(&["REASONIX_STATE_HOME", "REASONIX_HOME"]);
    env.set(
        "REASONIX_STATE_HOME",
        absolute_test_path("custom/reasonix-state"),
    );
    env.set("REASONIX_HOME", absolute_test_path("custom/reasonix-home"));

    assert_eq!(
        ClientId::Reasonix
            .data()
            .resolve_path_with_env_strategy(reasonix_home(), false),
        reasonix_stats_under(reasonix_default_root())
    );
}

#[test]
#[serial]
fn test_kimchi_defaults_to_home_agent_dir_without_env_override() {
    let mut env = EnvGuard::capture(&["KIMCHI_CODING_AGENT_DIR"]);
    env.remove("KIMCHI_CODING_AGENT_DIR");

    let client = ClientId::from_str("kimchi").expect("kimchi client should be registered");
    assert_eq!(
        client.data().resolve_path("/tmp/home"),
        native_join(
            std::path::Path::new("/tmp/home"),
            ".config/kimchi/harness/sessions"
        )
    );
}

#[test]
#[serial]
fn test_kimchi_honors_agent_dir_env_override() {
    let mut env = EnvGuard::capture(&["KIMCHI_CODING_AGENT_DIR"]);
    env.set("KIMCHI_CODING_AGENT_DIR", "/custom/kimchi-agent");

    let client = ClientId::from_str("kimchi").expect("kimchi client should be registered");
    assert_eq!(
        client.data().resolve_path("/tmp/home"),
        native_join(std::path::Path::new("/custom/kimchi-agent"), "sessions")
    );
}

#[test]
#[serial]
fn test_senpi_defaults_to_home_agent_dir_without_env_override() {
    let mut env = EnvGuard::capture(&["SENPI_CODING_AGENT_DIR"]);
    env.remove("SENPI_CODING_AGENT_DIR");

    let client = ClientId::from_str("senpi").expect("senpi client should be registered");
    assert_eq!(
        client.data().resolve_path("/tmp/home"),
        native_join(std::path::Path::new("/tmp/home"), ".senpi/agent/sessions")
    );
}

#[test]
#[serial]
fn test_senpi_honors_agent_dir_env_override() {
    let mut env = EnvGuard::capture(&["SENPI_CODING_AGENT_DIR"]);
    env.set("SENPI_CODING_AGENT_DIR", "/custom/senpi-agent");

    let client = ClientId::from_str("senpi").expect("senpi client should be registered");
    assert_eq!(
        client.data().resolve_path("/tmp/home"),
        native_join(std::path::Path::new("/custom/senpi-agent"), "sessions")
    );
}

#[test]
fn test_codebuddy_client_registered_as_local_session_source() {
    let client = ClientId::from_str("codebuddy").expect("codebuddy client should be registered");
    assert_eq!(
        client.data().resolve_path("/tmp/home"),
        native_join(std::path::Path::new("/tmp/home"), ".codebuddy/projects")
    );
    assert_eq!(client.data().pattern, "*.jsonl");
    assert!(client.data().parse_local);
    assert!(client.data().submit_default);
    assert!(!client.data().headless);
}

#[test]
fn test_workbuddy_client_registered_as_local_sqlite_source() {
    let client = ClientId::from_str("workbuddy").expect("workbuddy client should be registered");
    assert_eq!(
        client.data().resolve_path("/tmp/home"),
        native_join(std::path::Path::new("/tmp/home"), ".workbuddy")
    );
    assert_eq!(client.data().pattern, "workbuddy.db");
    assert!(client.data().parse_local);
    assert!(client.data().submit_default);
    assert!(!client.data().headless);
}

#[test]
fn test_devincli_client_registered_as_local_session_source() {
    let client = ClientId::from_str("devin-cli").expect("devin-cli client should be registered");
    assert_eq!(client.data().relative_path, "devin/cli/sessions.db");
    assert_eq!(client.data().pattern, "sessions.db");
    assert!(client.data().parse_local);
    assert!(client.data().submit_default);
    assert!(!client.data().headless);
}

#[test]
fn test_devindesktop_client_registered_as_local_session_source() {
    let client =
        ClientId::from_str("devin-desktop").expect("devin-desktop client should be registered");
    assert_eq!(
        client.data().relative_path,
        "Library/Application Support/Devin/User/acp-events"
    );
    assert_eq!(client.data().pattern, "*.ndjson");
    assert!(client.data().parse_local);
    assert!(client.data().submit_default);
    assert!(!client.data().headless);
}

#[test]
fn test_commandcode_client_registered_as_local_session_source() {
    let client =
        ClientId::from_str("commandcode").expect("commandcode client should be registered");
    assert_eq!(
        client.data().resolve_path("/tmp/home"),
        native_join(std::path::Path::new("/tmp/home"), ".commandcode/projects")
    );
    assert_eq!(client.data().pattern, "*.jsonl");
    assert!(client.data().parse_local);
    assert!(client.data().submit_default);
    assert!(!client.data().headless);
}

#[test]
fn test_junie_client_registered_as_local_session_source() {
    let client = ClientId::from_str("junie").expect("junie client should be registered");
    assert_eq!(
        client.data().resolve_path("/tmp/home"),
        native_join(std::path::Path::new("/tmp/home"), ".junie/sessions")
    );
    assert_eq!(client.data().pattern, "events.jsonl");
    assert!(client.data().parse_local);
    assert!(client.data().submit_default);
    assert!(!client.data().headless);
}

#[test]
fn test_client_id_all_len_matches_count() {
    assert_eq!(ClientId::ALL.len(), ClientId::COUNT);
}

#[test]
fn test_client_id_string_round_trip() {
    for client in ClientId::iter() {
        let id = client.as_str();
        assert_eq!(ClientId::from_str(id), Some(client));
    }
}

#[test]
fn test_warp_client_registered_as_aggregate_cache_source() {
    let client = ClientId::from_str("warp").expect("warp client should be registered");
    assert_eq!(client.data().relative_path, "warp-cache");
    assert_eq!(client.data().pattern, "usage*.json");
    assert!(client.data().parse_local);
    assert!(!client.data().submit_default);
}

#[test]
fn test_grok_client_registered_as_local_session_source() {
    let client = ClientId::from_str("grok").expect("grok client should be registered");
    assert_eq!(client.data().relative_path, "sessions");
    assert_eq!(client.data().pattern, "updates.jsonl");
    assert!(client.data().parse_local);
    assert!(client.data().submit_default);
}

#[test]
fn test_jcode_client_registered_as_local_session_source() {
    let client = ClientId::from_str("jcode").expect("jcode client should be registered");
    assert_eq!(client.data().relative_path, "sessions");
    assert_eq!(client.data().pattern, "session_*.json");
    assert!(client.data().parse_local);
    assert!(client.data().submit_default);
}
