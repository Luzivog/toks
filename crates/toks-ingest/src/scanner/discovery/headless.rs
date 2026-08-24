use std::path::PathBuf;

use super::common::join_native;

pub fn headless_roots_with_env_strategy(home_dir: &str, use_env_roots: bool) -> Vec<PathBuf> {
    if use_env_roots {
        if let Some(path) =
            crate::paths::renamed_env_var("TOKS_HEADLESS_DIR", "TOKSCOPE_HEADLESS_DIR")
        {
            return vec![PathBuf::from(path)];
        }
    }

    let config_parent = PathBuf::from(join_native(home_dir, ".config"));
    let application_support = PathBuf::from(join_native(home_dir, "Library/Application Support"));
    vec![
        crate::paths::resolved_named_root(&config_parent).join("headless"),
        crate::paths::resolved_named_root(&application_support).join("headless"),
    ]
}

pub fn headless_roots(home_dir: &str) -> Vec<PathBuf> {
    headless_roots_with_env_strategy(home_dir, true)
}
