use super::*;

/// The `workspace_key` a Crush fixture path should produce, spelled out
/// here rather than obtained from the production normalizer.
///
/// `scan_crush_registry` keys a workspace with `normalize_workspace_key`,
/// which folds `\` to `/` on purpose so one workspace reached under two
/// separator spellings is one key. That fold is the claim these tests
/// carry on Windows, and a raw `display()` expectation could not state it —
/// it only agreed with the normalizer on Unix, where there is nothing to
/// fold.
///
/// Calling `normalize_workspace_key` in the expectation states it, but
/// states it self-referentially: production applies the same function to
/// the same input, so the assertion would hold no matter what that function
/// did, including nothing. The fold is one line to write out, so write it
/// out — this expectation is wrong whenever the normalizer stops folding,
/// which is the entire point of having it.
///
/// The single `replace` is the whole rule for these inputs. The normalizer
/// also collapses repeated separators and trims a trailing one; a
/// `TempDir`-rooted `join` produces neither, so nothing else applies. (It
/// agrees on a UNC root too: `\\srv\share\p` folds to `//srv/share/p` with
/// no doubled separator left inside to collapse.)
fn expected_workspace_key(path: &Path) -> Option<String> {
    Some(path.to_string_lossy().replace('\\', "/"))
}

#[test]
fn test_scan_crush_registry_resolves_relative_and_absolute_data_dirs() {
    let dir = TempDir::new().unwrap();
    let project_a = dir.path().join("project-a");
    let project_b_data = dir.path().join("project-b-data");
    fs::create_dir_all(project_a.join(".crush")).unwrap();
    fs::create_dir_all(&project_b_data).unwrap();
    File::create(project_a.join(".crush").join("crush.db")).unwrap();
    File::create(project_b_data.join("crush.db")).unwrap();

    let registry_path = dir.path().join("projects.json");
    let projects_json = format!(
        r#"{{
  "projects": [
{{ "path": {}, "data_dir": ".crush" }},
{{ "path": {}, "data_dir": {} }},
{{ "path": {}, "data_dir": ".crush" }}
  ]
}}"#,
        json_path_literal(&project_a),
        json_path_literal(&dir.path().join("project-b")),
        json_path_literal(&project_b_data),
        json_path_literal(&dir.path().join("missing-project")),
    );
    setup_mock_crush_registry(&registry_path, &projects_json);

    let result = scan_crush_registry(&registry_path);
    assert_eq!(
        result,
        vec![
            CrushDbSource {
                db_path: project_a.join(".crush").join("crush.db"),
                workspace_key: expected_workspace_key(&project_a),
                workspace_label: Some("project-a".to_string()),
            },
            CrushDbSource {
                db_path: project_b_data.join("crush.db"),
                workspace_key: expected_workspace_key(&dir.path().join("project-b")),
                workspace_label: Some("project-b".to_string()),
            },
        ]
    );
}

#[test]
fn test_scan_crush_registry_skips_malformed_project_entries() {
    let dir = TempDir::new().unwrap();
    let valid_project = dir.path().join("valid-project");
    fs::create_dir_all(valid_project.join(".crush")).unwrap();
    File::create(valid_project.join(".crush").join("crush.db")).unwrap();

    let registry_path = dir.path().join("projects.json");
    let projects_json = format!(
        r#"{{
  "projects": [
{{ "path": {}, "data_dir": ".crush" }},
{{ "path": 123, "data_dir": ".crush" }},
{{ "data_dir": ".crush" }},
"not-an-object"
  ]
}}"#,
        json_path_literal(&valid_project)
    );
    setup_mock_crush_registry(&registry_path, &projects_json);

    let result = scan_crush_registry(&registry_path);
    assert_eq!(
        result,
        vec![CrushDbSource {
            db_path: valid_project.join(".crush").join("crush.db"),
            workspace_key: expected_workspace_key(&valid_project),
            workspace_label: Some("valid-project".to_string()),
        }]
    );
}

#[test]
#[serial]
fn test_discover_crush_dbs_ignores_cwd_without_override() {
    let previous_xdg = std::env::var("XDG_DATA_HOME").ok();
    let previous_dir = std::env::current_dir().unwrap();

    let dir = TempDir::new().unwrap();
    let home = dir.path().join("home");
    let project = dir.path().join("workspace");
    let nested = project.join("src/subdir");
    let xdg = dir.path().join("xdg");

    fs::create_dir_all(&nested).unwrap();
    fs::create_dir_all(xdg.join("crush")).unwrap();
    fs::create_dir_all(project.join(".crush")).unwrap();
    File::create(project.join(".crush").join("crush.db")).unwrap();
    fs::write(
        xdg.join("crush").join("projects.json"),
        r#"{"projects":[]}"#,
    )
    .unwrap();

    unsafe { std::env::set_var("XDG_DATA_HOME", &xdg) };
    std::env::set_current_dir(&nested).unwrap();

    let result = discover_crush_dbs(home.to_str().unwrap(), false);
    assert!(result.is_empty());

    restore_current_dir(&previous_dir);
    restore_env("XDG_DATA_HOME", previous_xdg);
}

#[test]
#[serial]
fn test_discover_crush_dbs_honors_crush_global_data_env() {
    let previous_global = std::env::var("CRUSH_GLOBAL_DATA").ok();
    let previous_xdg = std::env::var("XDG_DATA_HOME").ok();
    let previous_local_app_data = std::env::var("LOCALAPPDATA").ok();
    unsafe { std::env::remove_var("LOCALAPPDATA") };

    let dir = TempDir::new().unwrap();
    let home = dir.path().join("home");
    let global_data = dir.path().join("crush-global");
    let project = dir.path().join("project");
    fs::create_dir_all(project.join(".crush")).unwrap();
    File::create(project.join(".crush").join("crush.db")).unwrap();

    let projects_json = format!(
        r#"{{ "projects": [ {{ "path": {}, "data_dir": ".crush" }} ] }}"#,
        json_path_literal(&project)
    );
    setup_mock_crush_registry(&global_data.join("projects.json"), &projects_json);

    unsafe { std::env::set_var("CRUSH_GLOBAL_DATA", &global_data) };
    unsafe { std::env::remove_var("XDG_DATA_HOME") };

    let result = discover_crush_dbs(home.to_str().unwrap(), true);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].db_path, project.join(".crush").join("crush.db"));

    let without_env_roots = discover_crush_dbs(home.to_str().unwrap(), false);
    assert!(
        without_env_roots.is_empty(),
        "CRUSH_GLOBAL_DATA must be ignored when env roots are disabled"
    );

    restore_env("CRUSH_GLOBAL_DATA", previous_global);
    restore_env("XDG_DATA_HOME", previous_xdg);
    restore_env("LOCALAPPDATA", previous_local_app_data);
}

#[test]
#[serial]
fn test_discover_crush_dbs_scans_windows_local_appdata_under_home() {
    let previous_global = std::env::var("CRUSH_GLOBAL_DATA").ok();
    let previous_xdg = std::env::var("XDG_DATA_HOME").ok();
    unsafe { std::env::remove_var("CRUSH_GLOBAL_DATA") };
    unsafe { std::env::remove_var("XDG_DATA_HOME") };

    let dir = TempDir::new().unwrap();
    let home = dir.path().join("home");
    let project = dir.path().join("project");
    fs::create_dir_all(project.join(".crush")).unwrap();
    File::create(project.join(".crush").join("crush.db")).unwrap();

    let projects_json = format!(
        r#"{{ "projects": [ {{ "path": {}, "data_dir": ".crush" }} ] }}"#,
        json_path_literal(&project)
    );
    setup_mock_crush_registry(
        &home.join("AppData/Local/crush/projects.json"),
        &projects_json,
    );

    let result = discover_crush_dbs(home.to_str().unwrap(), false);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].db_path, project.join(".crush").join("crush.db"));

    restore_env("CRUSH_GLOBAL_DATA", previous_global);
    restore_env("XDG_DATA_HOME", previous_xdg);
}

#[test]
#[serial]
fn test_discover_crush_dbs_dedups_across_registry_candidates() {
    let previous_global = std::env::var("CRUSH_GLOBAL_DATA").ok();
    let previous_xdg = std::env::var("XDG_DATA_HOME").ok();
    let previous_local_app_data = std::env::var("LOCALAPPDATA").ok();
    unsafe { std::env::remove_var("LOCALAPPDATA") };

    let dir = TempDir::new().unwrap();
    let home = dir.path().join("home");
    let xdg = dir.path().join("xdg");
    let project = dir.path().join("project");
    fs::create_dir_all(project.join(".crush")).unwrap();
    File::create(project.join(".crush").join("crush.db")).unwrap();

    let projects_json = format!(
        r#"{{ "projects": [ {{ "path": {}, "data_dir": ".crush" }} ] }}"#,
        json_path_literal(&project)
    );
    setup_mock_crush_registry(&xdg.join("crush/projects.json"), &projects_json);
    setup_mock_crush_registry(
        &home.join("AppData/Local/crush/projects.json"),
        &projects_json,
    );

    unsafe { std::env::remove_var("CRUSH_GLOBAL_DATA") };
    unsafe { std::env::set_var("XDG_DATA_HOME", &xdg) };

    let result = discover_crush_dbs(home.to_str().unwrap(), true);
    assert_eq!(
        result.len(),
        1,
        "same crush.db reachable via multiple registries must be deduplicated"
    );

    restore_env("CRUSH_GLOBAL_DATA", previous_global);
    restore_env("XDG_DATA_HOME", previous_xdg);
    restore_env("LOCALAPPDATA", previous_local_app_data);
}

#[test]
#[serial]
fn test_scan_all_clients_crush_populates_crush_db_paths() {
    let mut env = EnvGuard::capture(&["XDG_DATA_HOME", "CRUSH_GLOBAL_DATA", "LOCALAPPDATA"]);
    env.remove("CRUSH_GLOBAL_DATA");
    env.remove("LOCALAPPDATA");

    let dir = TempDir::new().unwrap();
    let home = dir.path().join("home");
    let xdg = dir.path().join("xdg");
    let project = dir.path().join("project");
    let data_dir = project.join(".crush");

    fs::create_dir_all(xdg.join("crush")).unwrap();
    fs::create_dir_all(&data_dir).unwrap();
    File::create(data_dir.join("crush.db")).unwrap();

    let registry_path = xdg.join("crush").join("projects.json");
    let projects_json = format!(
        r#"{{
  "projects": [
{{ "path": {}, "data_dir": ".crush" }}
  ]
}}"#,
        json_path_literal(&project)
    );
    setup_mock_crush_registry(&registry_path, &projects_json);

    env.set("XDG_DATA_HOME", &xdg);

    let result = scan_without_extra_dirs(home.to_str().unwrap(), &["crush".to_string()]);
    assert_eq!(
        result.crush_dbs,
        vec![CrushDbSource {
            db_path: data_dir.join("crush.db"),
            workspace_key: expected_workspace_key(&project),
            workspace_label: Some("project".to_string()),
        }]
    );
    assert!(result.get(ClientId::Crush).is_empty());
}
