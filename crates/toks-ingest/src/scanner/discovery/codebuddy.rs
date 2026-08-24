use std::path::PathBuf;

use crate::clients::ClientId;

use super::ScanPlan;

pub(super) fn add_tasks(plan: &mut ScanPlan<'_>) {
    if !plan.has(ClientId::CodeBuddy) {
        return;
    }
    let home_path = PathBuf::from(plan.home_dir);
    let mut roots = vec![(
        home_path
            .join("AppData")
            .join("Local")
            .join("CodeBuddyExtension")
            .join("Logs"),
        "*.log",
    )];
    roots.push((
        home_path
            .join("AppData")
            .join("Roaming")
            .join("CodeBuddy CN")
            .join("logs"),
        "codebuddy-extension-log",
    ));
    roots.push((
        home_path
            .join("AppData")
            .join("Roaming")
            .join("Code")
            .join("logs"),
        "codebuddy-extension-log",
    ));

    if plan.use_env_roots {
        if let Some(local) = dirs::data_local_dir() {
            roots.push((local.join("CodeBuddyExtension").join("Logs"), "*.log"));
        }
        if let Some(roaming) = dirs::config_dir() {
            roots.push((
                roaming.join("CodeBuddy CN").join("logs"),
                "codebuddy-extension-log",
            ));
            roots.push((roaming.join("Code").join("logs"), "codebuddy-extension-log"));
        }
    }

    for (root, pattern) in roots {
        if pattern == "*.log" {
            for child in ["CodeBuddyIDE", "VSCode"] {
                plan.push_with_pattern(ClientId::CodeBuddy, root.join(child), pattern);
            }
        } else {
            plan.push_with_pattern(ClientId::CodeBuddy, root, pattern);
        }
    }
}
