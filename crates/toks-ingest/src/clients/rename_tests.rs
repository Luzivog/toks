use super::PathRoot;
use crate::paths::test_env::EnvGuard;
use serial_test::serial;

fn native_join(root: &std::path::Path, relative: &str) -> String {
    let mut path = root.to_path_buf();
    for component in std::path::Path::new(relative).components() {
        path.push(component.as_os_str());
    }
    path.to_string_lossy().into_owned()
}

#[test]
#[cfg(target_os = "linux")]
#[serial]
fn config_root_uses_toks_xdg_default_without_override() {
    let mut env = EnvGuard::capture(&["TOKS_CONFIG_DIR", "TOKSCOPE_CONFIG_DIR", "XDG_CONFIG_HOME"]);
    env.remove("TOKS_CONFIG_DIR");
    env.remove("TOKSCOPE_CONFIG_DIR");
    env.set("XDG_CONFIG_HOME", "/tmp/xdg-config-home");

    assert_eq!(
        PathRoot::Config.resolve("/tmp/home"),
        "/tmp/xdg-config-home/toks"
    );
}

#[test]
#[cfg(target_os = "windows")]
#[serial]
fn config_root_uses_toks_windows_default_without_override() {
    let mut env = EnvGuard::capture(&["TOKS_CONFIG_DIR", "TOKSCOPE_CONFIG_DIR"]);
    env.remove("TOKS_CONFIG_DIR");
    env.remove("TOKSCOPE_CONFIG_DIR");

    let expected = dirs::config_dir()
        .expect("Windows always exposes dirs::config_dir")
        .join("toks")
        .to_string_lossy()
        .into_owned();
    assert_eq!(PathRoot::Config.resolve("C:\\fake-home"), expected);
}

#[test]
#[serial]
fn config_root_ignores_renamed_overrides_when_disabled() {
    let mut env = EnvGuard::capture(&["TOKS_CONFIG_DIR", "TOKSCOPE_CONFIG_DIR", "XDG_CONFIG_HOME"]);
    env.set("TOKS_CONFIG_DIR", "/tmp/current-config-root");
    env.set("TOKSCOPE_CONFIG_DIR", "/tmp/legacy-config-root");
    env.set("XDG_CONFIG_HOME", "/tmp/xdg-config-home");

    let expected = if cfg!(target_os = "windows") {
        std::path::Path::new("/tmp/home")
            .join("AppData/Roaming/toks")
            .to_string_lossy()
            .into_owned()
    } else {
        native_join(std::path::Path::new("/tmp/home"), ".config/toks")
    };
    assert_eq!(
        PathRoot::Config.resolve_with_env_strategy("/tmp/home", false),
        expected
    );
}
