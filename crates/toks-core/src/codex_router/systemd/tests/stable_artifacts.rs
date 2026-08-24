use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};

use tempfile::tempdir;

use super::executable;
use crate::codex_router::systemd::launch_contract::{
    command_for_test, inspect, persist, LaunchContract, CONTRACT_NAME,
};
use crate::codex_router::systemd::persist_test_launch_contract;
use crate::codex_router::systemd::receipt::render_units_test;
use crate::codex_router::systemd::stage_generation;
use crate::codex_router::systemd::units::UnitEnvironment;

#[test]
fn a_staged_build_from_one_prefix_is_recoverable_by_another_prefix() {
    const ROUTER_B: &[u8] = b"#!/bin/sh\nprintf '%s|%s|%s' \"$1\" \"$2\" \"$TOKS_CODEX_BIN\"\n";
    let directory = tempdir().unwrap();
    let stable = directory.path().join("data/rotation/router-artifacts");
    let prefix_one = directory.path().join("prefix-one");
    let prefix_two = directory.path().join("prefix-two");
    let executable_b = executable(&prefix_one, "build-b", ROUTER_B);
    let executable_c = executable(&prefix_two, "build-c", b"router-c");
    let environment = UnitEnvironment::from_values([Some("/bin"), None, None, None]);
    let build_b = persist_test_launch_contract(
        &stable,
        &executable_b,
        std::path::Path::new("/prefix-one/codex"),
        &environment,
    )
    .unwrap();
    persist_test_launch_contract(
        &stable,
        &executable_c,
        std::path::Path::new("/prefix-two/codex"),
        &environment,
    )
    .unwrap();
    fs::remove_dir_all(&prefix_one).unwrap();

    let recovered_generation = stable.join("generations/9");
    stage_generation(&stable, &recovered_generation, &build_b).unwrap();
    let staged = recovered_generation
        .join("toks-router")
        .canonicalize()
        .unwrap();
    assert!(staged.starts_with(&stable));
    assert_eq!(fs::read(&staged).unwrap(), ROUTER_B);
    let contract = recovered_generation.join(CONTRACT_NAME);
    let (found, executable, _) = inspect(&contract).unwrap();
    assert_eq!((found, executable), (build_b, staged.clone()));
    let output = command_for_test(&contract, 9).unwrap().output().unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"worker|9|/prefix-one/codex");
}

#[test]
fn deployment_identity_does_not_depend_on_the_install_prefix() {
    let directory = tempdir().unwrap();
    let stable = directory.path().join("router-artifacts");
    let first = executable(directory.path(), "prefix-one", b"same-router");
    let second = executable(directory.path(), "prefix-two", b"same-router");
    let environment = UnitEnvironment::from_values([Some("/bin"), None, None, None]);
    let codex = std::path::Path::new("/opt/codex");

    let from_first = persist_test_launch_contract(&stable, &first, codex, &environment).unwrap();
    let from_second = persist_test_launch_contract(&stable, &second, codex, &environment).unwrap();

    assert_eq!(from_first, from_second);
}

#[test]
fn mutable_install_symlink_cannot_change_the_candidate_behind_its_build_identity() {
    let directory = tempdir().unwrap();
    let stable = directory.path().join("data/rotation/router-artifacts");
    let prefix = directory.path().join("prefix");
    let binary = prefix.join("lib/build-b/toks-router");
    let mutable = prefix.join("bin/toks-router");
    fs::create_dir_all(binary.parent().unwrap()).unwrap();
    fs::create_dir_all(mutable.parent().unwrap()).unwrap();
    fs::write(&binary, b"router-b").unwrap();
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o755)).unwrap();
    symlink(&binary, &mutable).unwrap();

    let environment = UnitEnvironment::from_values([Some("/bin"), None, None, None]);
    let rendered = render_units_test(
        &stable,
        &mutable,
        std::path::Path::new("/opt/codex"),
        &environment,
    )
    .unwrap();
    fs::remove_file(&mutable).unwrap();
    let replacement = prefix.join("lib/build-c/toks-router");
    fs::create_dir_all(replacement.parent().unwrap()).unwrap();
    fs::write(&replacement, b"router-c").unwrap();
    symlink(&replacement, &mutable).unwrap();

    let immutable = rendered.executable.display().to_string();
    assert!(rendered.executable.starts_with(&stable));
    assert_eq!(fs::read(&rendered.executable).unwrap(), b"router-b");
    assert_eq!(
        fs::metadata(&rendered.executable)
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o755
    );
    assert!(rendered
        .coordinator
        .contains(&format!("ExecStart=\"{immutable}\" launch-host")));
    assert!(rendered.resume.contains(&format!(
        "ExecStart=\"{immutable}\" launch-resume-supervisor"
    )));
    assert!(!rendered
        .coordinator
        .contains(&mutable.display().to_string()));
    assert!(!rendered.resume.contains(&mutable.display().to_string()));
}

#[test]
fn occupied_content_address_cannot_silently_replace_a_router_artifact() {
    use sha2::{Digest, Sha256};

    let directory = tempdir().unwrap();
    let stable = directory.path().join("router-artifacts");
    let source = directory.path().join("candidate");
    fs::write(&source, b"expected-router").unwrap();
    let hash = format!("{:x}", Sha256::digest(b"expected-router"));
    let occupied = stable.join("executables").join(hash).join("toks-router");
    fs::create_dir_all(occupied.parent().unwrap()).unwrap();
    fs::write(&occupied, b"different-router").unwrap();
    fs::set_permissions(&occupied, fs::Permissions::from_mode(0o755)).unwrap();

    let environment = UnitEnvironment::from_values([None, None, None, None]);
    let error = persist_test_launch_contract(
        &stable,
        &source,
        std::path::Path::new("/opt/codex"),
        &environment,
    )
    .unwrap_err();

    assert!(error.to_string().contains("hash collision"));
    assert_eq!(fs::read(&occupied).unwrap(), b"different-router");
    assert_eq!(fs::read_dir(occupied.parent().unwrap()).unwrap().count(), 1);
}

#[test]
fn artifact_and_contract_ancestor_symlinks_cannot_escape_the_stable_root() {
    let directory = tempdir().unwrap();
    let stable = directory.path().join("router-artifacts");
    let outside = directory.path().join("outside");
    let source = executable(directory.path(), "candidate", b"router");
    fs::create_dir_all(&stable).unwrap();
    fs::create_dir_all(&outside).unwrap();
    symlink(&outside, stable.join("executables")).unwrap();
    let environment = UnitEnvironment::from_values([None, None, None, None]);

    let error = persist_test_launch_contract(
        &stable,
        &source,
        std::path::Path::new("/opt/codex"),
        &environment,
    )
    .unwrap_err();

    assert!(error.to_string().contains("symlink"));
    assert_eq!(fs::read_dir(&outside).unwrap().count(), 0);

    fs::remove_file(stable.join("executables")).unwrap();
    fs::create_dir_all(stable.join("executables")).unwrap();
    symlink(&outside, stable.join("contracts")).unwrap();
    let error = persist_test_launch_contract(
        &stable,
        &source,
        std::path::Path::new("/opt/codex"),
        &environment,
    )
    .unwrap_err();
    assert!(error.to_string().contains("symlink"));
    assert_eq!(fs::read_dir(&outside).unwrap().count(), 0);
}

#[test]
fn artifact_root_parent_symlink_cannot_redirect_materialization() {
    let directory = tempdir().unwrap();
    let data = directory.path().join("data");
    let outside = directory.path().join("outside");
    let source = executable(directory.path(), "candidate", b"router");
    fs::create_dir_all(&data).unwrap();
    fs::create_dir_all(&outside).unwrap();
    symlink(&outside, data.join("rotation")).unwrap();
    let stable = data.join("rotation/router-artifacts");
    let environment = UnitEnvironment::from_values([None, None, None, None]);

    let error = persist_test_launch_contract(
        &stable,
        &source,
        std::path::Path::new("/opt/codex"),
        &environment,
    )
    .unwrap_err();

    assert!(error.to_string().contains("symlink"));
    assert_eq!(fs::read_dir(&outside).unwrap().count(), 0);
}

#[test]
fn generation_staging_rejects_outside_and_symlinked_destinations() {
    let directory = tempdir().unwrap();
    let stable = directory.path().join("router-artifacts");
    let outside = directory.path().join("outside");
    let source = executable(directory.path(), "candidate", b"router");
    let environment = UnitEnvironment::from_values([None, None, None, None]);
    let build = persist_test_launch_contract(
        &stable,
        &source,
        std::path::Path::new("/opt/codex"),
        &environment,
    )
    .unwrap();
    fs::create_dir_all(&outside).unwrap();

    let error = stage_generation(&stable, &outside.join("1"), &build).unwrap_err();
    assert!(error.to_string().contains("outside router artifact root"));
    assert_eq!(fs::read_dir(&outside).unwrap().count(), 0);

    symlink(&outside, stable.join("generations")).unwrap();
    let error = stage_generation(&stable, &stable.join("generations/1"), &build).unwrap_err();
    assert!(error.to_string().contains("symlink"));
    assert_eq!(fs::read_dir(&outside).unwrap().count(), 0);
}

#[test]
fn a_self_consistent_contract_cannot_reference_an_external_executable() {
    let directory = tempdir().unwrap();
    let stable = directory.path().join("router-artifacts");
    let external = executable(directory.path(), "outside", b"router");
    fs::set_permissions(&external, fs::Permissions::from_mode(0o755)).unwrap();
    let environment = UnitEnvironment::from_values([None, None, None, None]);
    let contract =
        LaunchContract::capture(&external, std::path::Path::new("/opt/codex"), &environment)
            .unwrap();
    let build = contract.build_id().unwrap();

    let error = persist(&stable, &contract, &build).unwrap_err();

    assert!(error
        .to_string()
        .contains("outside router executable artifact root"));
    assert!(!stable.join("contracts").exists());

    let generation = stable.join("generations/1");
    fs::create_dir_all(&generation).unwrap();
    fs::write(
        generation.join(CONTRACT_NAME),
        serde_json::to_vec(&serde_json::json!({
            "build": build,
            "contract": contract,
        }))
        .unwrap(),
    )
    .unwrap();
    let error = command_for_test(&generation.join(CONTRACT_NAME), 1).unwrap_err();
    assert!(error
        .to_string()
        .contains("outside router executable artifact root"));
}

#[test]
fn worker_launch_uses_only_the_captured_non_secret_runtime_environment() {
    const REENTRY: &str = "TOKS_TEST_ENV_CLEAR_REENTRY";
    const AMBIENT: &str = "TOKS_UNRELATED_AMBIENT";
    if std::env::var_os(REENTRY).is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "codex_router::systemd::tests::stable_artifacts::worker_launch_uses_only_the_captured_non_secret_runtime_environment",
                "--nocapture",
            ])
            .env(REENTRY, "1")
            .env(AMBIENT, "must-be-cleared")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    let directory = tempdir().unwrap();
    let stable = directory.path().join("router-artifacts");
    let source = executable(
        directory.path(),
        "candidate",
        b"#!/bin/sh\nprintf '%s' \"${TOKS_UNRELATED_AMBIENT-unset}\"\n",
    );
    let environment = UnitEnvironment::from_pairs(&[
        ("HOME", Some("/home/router")),
        ("HTTPS_PROXY", Some("http://proxy.example:8080")),
        ("HTTP_PROXY", Some("http://user:password@proxy.example")),
        ("SSL_CERT_FILE", Some("/etc/ssl/custom.pem")),
        ("LD_LIBRARY_PATH", Some("/opt/router/lib")),
        ("OPENAI_API_KEY", Some("must-not-persist")),
    ]);
    let build = persist_test_launch_contract(
        &stable,
        &source,
        std::path::Path::new("/opt/codex"),
        &environment,
    )
    .unwrap();
    let generation = stable.join("generations/4");
    stage_generation(&stable, &generation, &build).unwrap();

    let mut command = command_for_test(&generation.join(CONTRACT_NAME), 4).unwrap();
    let environment = command
        .get_envs()
        .map(|(name, value)| {
            (
                name.to_string_lossy().into_owned(),
                value.map(|value| value.to_string_lossy().into_owned()),
            )
        })
        .collect::<std::collections::BTreeMap<_, _>>();

    assert_eq!(environment["HOME"].as_deref(), Some("/home/router"));
    assert_eq!(
        environment["HTTPS_PROXY"].as_deref(),
        Some("http://proxy.example:8080")
    );
    assert_eq!(
        environment["SSL_CERT_FILE"].as_deref(),
        Some("/etc/ssl/custom.pem")
    );
    assert_eq!(
        environment["LD_LIBRARY_PATH"].as_deref(),
        Some("/opt/router/lib")
    );
    assert_eq!(
        environment["TOKS_ROUTER_BUILD_ID"].as_deref(),
        Some(build.as_str())
    );
    assert!(!environment.contains_key("HTTP_PROXY"));
    assert!(!environment.contains_key("OPENAI_API_KEY"));
    let output = command.output().unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"unset");
}
