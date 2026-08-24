use crate::scanner::{ScanResult, ScannerSettings};

use super::{
    add_builtin_extras, add_cline_tasks, add_codebuddy_tasks, add_codebuff_tasks, add_codex_tasks,
    add_default_tasks, add_desktop_tasks, add_environment, add_gjc_tasks, add_grok_tasks,
    add_kimi_tasks, add_openclaw_tasks, add_pi_tasks, add_prime_tasks, add_settings,
    add_workbuddy_tasks, discover_crush, discover_devin_databases, discover_goose, discover_hermes,
    discover_kilo, discover_kiro, discover_micode, discover_opencode, discover_synthetic,
    discover_zcode, discover_zed, finish_copilot_discovery, ScanPlan,
};

/// Scan all requested clients with persistent scanner settings.
pub fn scan_all_clients_with_scanner_settings(
    home_dir: &str,
    clients: &[String],
    use_env_roots: bool,
    scanner_settings: &ScannerSettings,
) -> ScanResult {
    scan_all_clients_inner(home_dir, clients, use_env_roots, scanner_settings)
}

/// Scan all requested clients without persistent scanner overrides.
pub fn scan_all_clients_with_env_strategy(
    home_dir: &str,
    clients: &[String],
    use_env_roots: bool,
) -> ScanResult {
    scan_all_clients_with_scanner_settings(
        home_dir,
        clients,
        use_env_roots,
        &ScannerSettings::default(),
    )
}

fn scan_all_clients_inner(
    home_dir: &str,
    clients: &[String],
    use_env_roots: bool,
    scanner_settings: &ScannerSettings,
) -> ScanResult {
    let mut result = ScanResult::default();
    let mut plan = ScanPlan::new(home_dir, clients, use_env_roots);

    add_default_tasks(&mut plan);
    add_grok_tasks(&mut plan);
    add_settings(&mut plan, scanner_settings);
    add_builtin_extras(&mut plan);
    add_codebuddy_tasks(&mut plan);
    add_workbuddy_tasks(&mut plan);
    add_environment(&mut plan);

    discover_opencode(&mut plan, &mut result, scanner_settings);
    discover_micode(&plan, &mut result);
    add_kimi_tasks(&mut plan);
    add_codex_tasks(&mut plan);
    add_openclaw_tasks(&mut plan);
    add_pi_tasks(&mut plan);
    add_prime_tasks(&mut plan);
    discover_synthetic(&plan, &mut result);
    add_cline_tasks(&mut plan);
    add_desktop_tasks(&mut plan);
    discover_kilo(&plan, &mut result);
    discover_devin_databases(&mut plan, &mut result);
    discover_hermes(&mut plan, &mut result);
    discover_goose(&plan, &mut result);
    discover_zed(&plan, &mut result);
    discover_crush(&plan, &mut result);
    discover_zcode(&plan, &mut result);
    discover_kiro(&mut plan, &mut result);
    add_codebuff_tasks(&mut plan);
    add_gjc_tasks(&mut plan);

    let mut seen = plan.execute(&mut result);
    finish_copilot_discovery(&plan, &mut result, &mut seen);
    result
}

pub fn scan_all_clients(home_dir: &str, clients: &[String]) -> ScanResult {
    scan_all_clients_with_env_strategy(home_dir, clients, true)
}
