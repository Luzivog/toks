use super::super::{ClientCounts, ClientDef, ClientId, PathRoot};
use super::native_join;
use serial_test::serial;

// Retained for the env tests below that predate this module's move to one
// serialization domain. New tests should capture an `EnvGuard` instead;
// this pairing is only panic-safe when nothing between it and the restore
// can fail.
fn restore_env(var: &str, previous: Option<String>) {
    match previous {
        Some(value) => unsafe { std::env::set_var(var, value) },
        None => unsafe { std::env::remove_var(var) },
    }
}

#[test]
fn test_path_root_home_resolves_to_home_dir() {
    let home = "/tmp/home";
    assert_eq!(PathRoot::Home.resolve(home), home);
}

#[test]
#[serial]
fn test_path_root_xdg_data_uses_env_var_when_set() {
    let previous = std::env::var("XDG_DATA_HOME").ok();
    unsafe { std::env::set_var("XDG_DATA_HOME", "/tmp/xdg-data-home") };

    let resolved = PathRoot::XdgData.resolve("/tmp/home");
    assert_eq!(resolved, "/tmp/xdg-data-home");

    restore_env("XDG_DATA_HOME", previous);
}

#[test]
#[serial]
fn test_path_root_xdg_data_falls_back_when_unset() {
    let previous = std::env::var("XDG_DATA_HOME").ok();
    unsafe { std::env::remove_var("XDG_DATA_HOME") };

    let resolved = PathRoot::XdgData.resolve("/tmp/home");
    assert_eq!(
        resolved,
        native_join(std::path::Path::new("/tmp/home"), ".local/share")
    );

    restore_env("XDG_DATA_HOME", previous);
}

#[test]
#[serial]
fn test_path_root_xdg_data_ignores_env_when_disabled() {
    let previous = std::env::var("XDG_DATA_HOME").ok();
    unsafe { std::env::set_var("XDG_DATA_HOME", "/tmp/xdg-data-home") };

    let resolved = PathRoot::XdgData.resolve_with_env_strategy("/tmp/home", false);
    assert_eq!(
        resolved,
        native_join(std::path::Path::new("/tmp/home"), ".local/share")
    );

    restore_env("XDG_DATA_HOME", previous);
}

#[test]
#[serial]
fn test_path_root_env_var_uses_env_when_set() {
    let var = "TOKS_TEST_PATH_ROOT";
    let previous = std::env::var(var).ok();
    unsafe { std::env::set_var(var, "/tmp/custom-root") };

    let root = PathRoot::EnvVar {
        var,
        fallback_relative: ".fallback",
    };
    let resolved = root.resolve("/tmp/home");
    assert_eq!(resolved, "/tmp/custom-root");

    restore_env(var, previous);
}

#[test]
#[serial]
fn test_path_root_env_var_falls_back_when_unset() {
    let var = "TOKS_TEST_PATH_ROOT";
    let previous = std::env::var(var).ok();
    unsafe { std::env::remove_var(var) };

    let root = PathRoot::EnvVar {
        var,
        fallback_relative: ".fallback",
    };
    let resolved = root.resolve("/tmp/home");
    assert_eq!(
        resolved,
        native_join(std::path::Path::new("/tmp/home"), ".fallback")
    );

    restore_env(var, previous);
}

#[test]
#[serial]
fn test_path_root_env_var_ignores_env_when_disabled() {
    let var = "TOKS_TEST_PATH_ROOT";
    let previous = std::env::var(var).ok();
    unsafe { std::env::set_var(var, "/tmp/custom-root") };

    let root = PathRoot::EnvVar {
        var,
        fallback_relative: ".fallback",
    };
    let resolved = root.resolve_with_env_strategy("/tmp/home", false);
    assert_eq!(
        resolved,
        native_join(std::path::Path::new("/tmp/home"), ".fallback")
    );

    restore_env(var, previous);
}

#[test]
fn test_client_def_resolve_path_combines_root_and_relative() {
    let client = ClientDef {
        id: "test",
        root: PathRoot::Home,
        relative_path: ".test/sessions",
        pattern: "*.jsonl",
        headless: false,
        parse_local: true,
        submit_default: true,
    };

    assert_eq!(
        client.resolve_path("/tmp/home"),
        native_join(std::path::Path::new("/tmp/home"), ".test/sessions")
    );
}

#[test]
fn test_client_id_iter_yields_all_in_order() {
    let all: Vec<ClientId> = ClientId::iter().collect();
    assert_eq!(all, ClientId::ALL);
}

#[test]
fn test_client_counts_get_set_add_work() {
    let mut counts = ClientCounts::new();

    assert_eq!(counts.get(ClientId::Claude), 0);
    counts.set(ClientId::Claude, 3);
    assert_eq!(counts.get(ClientId::Claude), 3);
    counts.add(ClientId::Claude, 2);
    assert_eq!(counts.get(ClientId::Claude), 5);
}

#[test]
fn test_codex_root_uses_codex_home_env_var() {
    assert_eq!(
        ClientId::Codex.data().root,
        PathRoot::EnvVar {
            var: "CODEX_HOME",
            fallback_relative: ".codex",
        }
    );
}

#[test]
#[serial]
fn test_gjc_data_dir_path() {
    let var = "GJC_CODING_AGENT_DIR";
    let previous = std::env::var(var).ok();
    // Env unset (cleared): resolves under home/.gjc/agent/sessions.
    unsafe { std::env::remove_var(var) };
    assert_eq!(
        ClientId::Gjc.data().resolve_path("/tmp/home"),
        native_join(std::path::Path::new("/tmp/home"), ".gjc/agent/sessions")
    );
    assert_eq!(ClientId::Gjc.data().pattern, "*.jsonl");
    assert!(ClientId::Gjc.data().parse_local);
    assert!(ClientId::Gjc.data().submit_default);
    assert_eq!(ClientId::from_str("gjc"), Some(ClientId::Gjc));

    // Env set but env roots disabled: falls back to home, ignoring env.
    unsafe { std::env::set_var(var, "/tmp/custom-gjc") };
    assert_eq!(
        ClientId::Gjc
            .data()
            .resolve_path_with_env_strategy("/tmp/home", false),
        native_join(std::path::Path::new("/tmp/home"), ".gjc/agent/sessions")
    );

    restore_env(var, previous);
}

#[test]
fn test_cursor_parse_local_is_false() {
    assert!(!ClientId::Cursor.data().parse_local);
}

#[test]
fn test_crush_submit_default_is_false() {
    assert!(!ClientId::Crush.submit_default());
}

#[test]
fn test_hermes_root_uses_hermes_home_env_var() {
    assert_eq!(
        ClientId::Hermes.data().root,
        PathRoot::EnvVar {
            var: "HERMES_HOME",
            fallback_relative: ".hermes",
        }
    );
    assert_eq!(ClientId::Hermes.data().relative_path, "state.db");
}

#[test]
fn test_codebuff_root_uses_codebuff_data_dir_env_var() {
    assert_eq!(
        ClientId::Codebuff.data().root,
        PathRoot::EnvVar {
            var: "CODEBUFF_DATA_DIR",
            fallback_relative: ".config/manicode",
        }
    );
    assert_eq!(ClientId::Codebuff.data().pattern, "chat-messages.json");
}

#[test]
fn test_freebuff_root_uses_freebuff_data_dir_env_var() {
    // Freebuff shares Codebuff's ~/.config/manicode layout (built on the
    // same runtime), keyed via its own FREEBUFF_DATA_DIR override.
    assert_eq!(
        ClientId::Freebuff.data().root,
        PathRoot::EnvVar {
            var: "FREEBUFF_DATA_DIR",
            fallback_relative: ".config/manicode",
        }
    );
    assert_eq!(ClientId::Freebuff.data().pattern, "chat-messages.json");
}

#[test]
fn test_antigravity_parse_local_is_true() {
    assert!(ClientId::Antigravity.data().parse_local);
}

#[test]
fn test_antigravity_submit_default_is_true() {
    assert!(ClientId::Antigravity.submit_default());
}

#[test]
#[serial]
fn test_zed_data_dir_path() {
    let previous = std::env::var("XDG_DATA_HOME").ok();
    unsafe { std::env::remove_var("XDG_DATA_HOME") };

    assert_eq!(
        ClientId::Zed.data().resolve_path("/tmp/home"),
        native_join(
            std::path::Path::new("/tmp/home"),
            ".local/share/zed/threads/threads.db"
        )
    );

    restore_env("XDG_DATA_HOME", previous);
}

#[test]
fn test_zed_submit_default_is_true() {
    assert!(ClientId::Zed.submit_default());
}

#[test]
fn test_kiro_data_dir_path() {
    assert_eq!(
        ClientId::Kiro.data().resolve_path("/tmp/home"),
        native_join(std::path::Path::new("/tmp/home"), ".kiro/sessions/cli")
    );
    assert_eq!(ClientId::Kiro.data().pattern, "*.json");
    assert!(ClientId::Kiro.parse_local());
    assert!(ClientId::Kiro.submit_default());
    assert!(!ClientId::Kiro.supports_headless());
}
