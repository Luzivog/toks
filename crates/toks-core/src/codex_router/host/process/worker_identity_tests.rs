use super::paths::HostPaths;
use crate::codex_router::host::GenerationId;

#[test]
fn registration_requires_exact_executable_arguments_environment_and_unit() {
    let directory = tempfile::tempdir().unwrap();
    let executable = directory.path().join("candidate-router");
    std::fs::write(&executable, b"router").unwrap();
    let artifact_root = directory.path().join("router-artifacts");
    let environment = crate::codex_router::systemd::UnitEnvironment::from_pairs(&[
        ("PATH", Some("/bin")),
        ("HOME", Some("/home/router")),
        ("SSL_CERT_FILE", Some("/etc/ssl/cert.pem")),
    ]);
    let build = crate::codex_router::systemd::persist_test_launch_contract(
        &artifact_root,
        &executable,
        std::path::Path::new("/opt/codex"),
        &environment,
    )
    .unwrap();
    let paths = HostPaths {
        executable: executable.clone(),
        generations: artifact_root.join("generations"),
        control: directory.path().join("control.sock"),
        state: directory.path().join("state.json"),
    };
    let expected = GenerationId::from_raw(19);
    let generation = paths.generations.join(expected.get().to_string());
    crate::codex_router::systemd::stage_generation(&artifact_root, &generation, &build).unwrap();
    let stable = generation.join("toks-router").canonicalize().unwrap();
    let proc_root = directory.path().join("proc");
    let process = proc_root.join("701");
    std::fs::create_dir_all(&process).unwrap();
    std::os::unix::fs::symlink(&stable, process.join("exe")).unwrap();
    std::fs::write(process.join("cmdline"), b"router\0worker\x0019\0").unwrap();
    let exact_environment = format!(
        "HOME=/home/router\0PATH=/bin\0SSL_CERT_FILE=/etc/ssl/cert.pem\0TOKS_CODEX_BIN=/opt/codex\0TOKS_ROUTER_BUILD_ID={}\0",
        build.as_str()
    );
    std::fs::write(process.join("environ"), exact_environment.as_bytes()).unwrap();
    std::fs::write(
        process.join("cgroup"),
        b"0::/user.slice/app.slice/toks-router-worker@19.service\n",
    )
    .unwrap();

    assert!(paths.worker_matches_in(expected, 701, &proc_root));

    std::fs::write(process.join("cmdline"), b"router\0worker\x0020\0").unwrap();
    assert!(!paths.worker_matches_in(expected, 701, &proc_root));
    std::fs::write(process.join("cmdline"), b"router\0worker\x0019\0").unwrap();

    std::fs::write(
        process.join("environ"),
        format!("{exact_environment}LD_PRELOAD=/tmp/injected.so\0"),
    )
    .unwrap();
    assert!(!paths.worker_matches_in(expected, 701, &proc_root));
    std::fs::write(process.join("environ"), exact_environment.as_bytes()).unwrap();

    std::fs::write(
        process.join("cgroup"),
        b"0::/user.slice/app.slice/toks-router-worker@20.service\n",
    )
    .unwrap();
    assert!(!paths.worker_matches_in(expected, 701, &proc_root));
    std::fs::write(
        process.join("cgroup"),
        b"0::/user.slice/app.slice/toks-router-worker@19.service\n",
    )
    .unwrap();

    std::fs::remove_file(process.join("exe")).unwrap();
    std::os::unix::fs::symlink(&executable, process.join("exe")).unwrap();
    assert!(!paths.worker_matches_in(expected, 701, &proc_root));
    assert!(!paths.worker_matches_in(expected, -1, &proc_root));
}
