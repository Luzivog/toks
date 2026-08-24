use std::ffi::OsString;
use std::path::PathBuf;

use super::{bounded_output_with_timeout, launch_arguments, parse_inventory, parse_state};
use crate::accounts::AccountId;
use crate::codex_router::resume::state::{ResumeAttempt, ResumePhase};
use crate::codex_router::resume::supervisor::TaskState;
use crate::rotation::{ThreadId, UnixMillis, WaitingThread};

#[test]
fn systemd_states_preserve_missing_running_and_completed_outcomes() {
    assert_eq!(parse_state("LoadState=not-found\n"), TaskState::Missing);
    assert_eq!(
        parse_state("LoadState=loaded\nActiveState=active\nSubState=running\nResult=success\n"),
        TaskState::Running
    );
    assert_eq!(
        parse_state("LoadState=loaded\nActiveState=active\nSubState=exited\nResult=success\nExecMainCode=1\nExecMainStatus=0\n"),
        TaskState::Succeeded
    );
    assert_eq!(
        parse_state("LoadState=loaded\nActiveState=inactive\nSubState=dead\nResult=success\nExecMainCode=2\nExecMainStatus=15\n"),
        TaskState::Failed
    );
    assert_eq!(
        parse_state("LoadState=loaded\nActiveState=inactive\nSubState=dead\nResult=success\nExecMainCode=1\nExecMainStatus=0\n"),
        TaskState::Succeeded
    );
    assert_eq!(
        parse_state("LoadState=loaded\nActiveState=inactive\nSubState=dead\nResult=success\n"),
        TaskState::Failed
    );
    assert_eq!(
        parse_state("LoadState=loaded\nActiveState=failed\nSubState=failed\nResult=exit-code\n"),
        TaskState::Failed
    );
}

#[test]
fn task_unit_preserves_dollar_arguments_and_the_exact_captured_environment() {
    let attempt = "00000000-0000-4000-8000-000000000001";
    let captured = crate::codex_router::systemd::UnitEnvironment::from_pairs(&[
        ("PATH", Some("/installer/$PATH/100%/bin")),
        ("CODEX_HOME", None),
        ("HOME", Some("/home/installer")),
        ("HTTPS_PROXY", Some("http://proxy.example:8080")),
        ("SSL_CERT_FILE", Some("/etc/ssl/installer.pem")),
        ("LD_LIBRARY_PATH", Some("/opt/installer/lib")),
    ]);
    let environment = super::TaskEnvironment::from_unit(
        PathBuf::from("/opt/codex$literal%value"),
        "build$literal%value".into(),
        captured,
    )
    .unwrap();
    let attempt = ResumeAttempt {
        id: attempt.into(),
        account: AccountId::new("account"),
        waiting: WaitingThread::new(
            ThreadId::new("thread-$ROOT-${CHANNEL}; untouched"),
            UnixMillis::new(7),
        ),
        cwd: PathBuf::from("/workspace/$ROOT/${CHANNEL} exact"),
        phase: ResumePhase::Launching,
        retry_waiting_id: crate::rotation::WaitingId::for_test("retry"),
        terminal: None,
    };
    let arguments = launch_arguments(
        std::path::Path::new("/opt/$ROOT/${CHANNEL}/toks router"),
        &environment,
        &attempt,
    )
    .unwrap();
    assert!(arguments.contains(&OsString::from("--property=CollectMode=inactive")));
    assert!(!arguments
        .iter()
        .any(|value| value == "--expand-environment=no"));
    assert!(!arguments.iter().any(|value| {
        let value = value.to_string_lossy();
        value.starts_with("--setenv=") || value.starts_with("--property=UnsetEnvironment=")
    }));
    assert!(!arguments.contains(&OsString::from("--property=CollectMode=inactive-or-failed")));
    let separator = arguments.iter().position(|value| value == "--").unwrap();
    assert!(arguments[separator + 1]
        .to_string_lossy()
        .starts_with("/proc/"));
    assert_eq!(arguments[separator + 2], "launch-resume-task");
    let encoded = arguments[separator + 3].to_str().unwrap();
    assert!(encoded
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-')));
    let command = super::launch::command_for_test(encoded).unwrap();
    assert_eq!(command.get_program(), "/opt/$ROOT/${CHANNEL}/toks router");
    let task_arguments = command.get_args().collect::<Vec<_>>();
    assert_eq!(task_arguments[0], "resume-task");
    assert_eq!(task_arguments[2], "thread-$ROOT-${CHANNEL}; untouched");
    assert_eq!(task_arguments[3], "/workspace/$ROOT/${CHANNEL} exact");
    let exact = command
        .get_envs()
        .map(|(name, value)| {
            (
                name.to_string_lossy().into_owned(),
                value.map(|value| value.to_string_lossy().into_owned()),
            )
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(
        exact["TOKS_CODEX_BIN"].as_deref(),
        Some("/opt/codex$literal%value")
    );
    assert_eq!(
        exact["TOKS_ROUTER_BUILD_ID"].as_deref(),
        Some("build$literal%value")
    );
    assert_eq!(exact["PATH"].as_deref(), Some("/installer/$PATH/100%/bin"));
    assert!(!exact.contains_key("CODEX_HOME"));
    assert!(!exact.contains_key("OPENAI_API_KEY"));
    assert!(!exact.contains_key("LD_PRELOAD"));
}

#[test]
fn one_inventory_maps_multiple_attempt_units_and_defaults_absent_units() {
    let requested = std::collections::BTreeMap::from([
        ("toks-router-resume-task-a.service".into(), "a".into()),
        ("toks-router-resume-task-b.service".into(), "b".into()),
        ("toks-router-resume-task-c.service".into(), "c".into()),
    ]);
    let properties = "Id=toks-router-resume-task-a.service\nLoadState=loaded\nActiveState=active\nSubState=running\nResult=success\n\nId=toks-router-resume-task-b.service\nLoadState=loaded\nActiveState=failed\nSubState=failed\nResult=exit-code\n";

    let (inventory, observed) = parse_inventory(properties, &requested);

    assert_eq!(observed, 2);
    assert_eq!(inventory["a"], TaskState::Running);
    assert_eq!(inventory["b"], TaskState::Failed);
    assert_eq!(inventory["c"], TaskState::Missing);
}

#[test]
fn task_control_command_is_killed_at_its_deadline() {
    let mut command = std::process::Command::new("sh");
    command.args(["-c", "sleep 5"]);
    let started = std::time::Instant::now();

    assert!(
        bounded_output_with_timeout(&mut command, std::time::Duration::from_millis(25)).is_err()
    );
    assert!(started.elapsed() < std::time::Duration::from_secs(1));
}

#[test]
fn task_control_drains_stdout_and_stderr_larger_than_pipe_capacity() {
    let mut command = std::process::Command::new("sh");
    command.args([
        "-c",
        "head -c 262144 /dev/zero; head -c 262144 /dev/zero >&2",
    ]);

    let output =
        bounded_output_with_timeout(&mut command, std::time::Duration::from_secs(3)).unwrap();

    assert!(output.status.success());
    assert_eq!(output.stdout.len(), 262_144);
    assert_eq!(output.stderr.len(), 262_144);
}

#[test]
fn cleanup_control_accepts_systemd_not_found_but_not_other_failures() {
    let mut missing = std::process::Command::new("sh");
    missing.args(["-c", "exit 5"]);
    super::control::checked_allow_not_found(missing, "synthetic cleanup").unwrap();

    let mut failed = std::process::Command::new("sh");
    failed.args(["-c", "echo denied >&2; exit 1"]);
    let error = super::control::checked_allow_not_found(failed, "synthetic cleanup").unwrap_err();
    assert!(error.to_string().contains("denied"));
}
