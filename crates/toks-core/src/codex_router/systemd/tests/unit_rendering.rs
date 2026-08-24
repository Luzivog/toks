use std::path::Path;

use crate::codex_router::host::BuildId;

use super::super::resume_unit;
use super::super::units::{
    render_service_unit, render_socket_unit, render_worker_unit, UnitEnvironment,
};

#[test]
fn socket_unit_declares_the_loaded_contract_semantics() {
    let unit = render_socket_unit();
    for directive in [
        "ListenStream=127.0.0.1:47837",
        "FileDescriptorName=router",
        "SocketMode=0600",
        "Accept=no",
        "NoDelay=true",
        "Backlog=256",
        "ReusePort=no",
        "FreeBind=no",
        "FlushPending=no",
        "KeepAlive=no",
        "PassCredentials=no",
        "PassSecurity=no",
        "PassPacketInfo=no",
        "Timestamping=off",
        "RemoveOnStop=no",
    ] {
        assert!(unit.lines().any(|line| line == directive), "{directive}");
    }
}

#[test]
fn unit_values_reject_control_characters() {
    let environment =
        UnitEnvironment::from_values([Some("/bin\nEnvironment=INJECTED=1"), None, None, None]);
    let build = BuildId::new("build").unwrap();
    let error = render_service_unit(
        Path::new("/opt/toks-router"),
        Path::new("/opt/codex"),
        &build,
        &environment,
    )
    .unwrap_err();

    assert!(error.to_string().contains("control character"));

    let clean = UnitEnvironment::from_values([None, None, None, None]);
    assert!(render_service_unit(
        Path::new("/opt/toks-router\rExecStart=/bin/false"),
        Path::new("/opt/codex"),
        &build,
        &clean,
    )
    .is_err());
}

#[test]
fn literal_percent_in_paths_is_escaped_but_worker_specifiers_remain_live() {
    let environment = UnitEnvironment::from_values([
        Some("/opt/100%/bin"),
        Some("/opt/$CODEX/%HOME"),
        None,
        None,
    ]);
    let build = BuildId::new("build").unwrap();
    let service = render_service_unit(
        Path::new("/opt/100%/toks-router"),
        Path::new("/opt/100%/codex"),
        &build,
        &environment,
    )
    .unwrap();
    assert!(service.contains("ExecStart=\"/opt/100%%/toks-router\" launch-host"));
    assert!(service.contains("TOKS_CODEX_BIN=/opt/100%%/codex"));
    assert!(service.contains("PATH=/opt/100%%/bin"));
    assert!(service.contains("CODEX_HOME=/opt/$CODEX/%%HOME"));

    let worker = render_worker_unit(Path::new("/opt/100%/router")).unwrap();
    assert!(worker.contains("/opt/100%%/router/generations/%i/toks-router"));
    assert!(worker.contains("/opt/100%%/router/generations/%i/launch.json"));
    assert!(!worker.contains("generations/%%i"));

    let resume = resume_unit::render(
        Path::new("/opt/100%/toks-router"),
        Path::new("/opt/100%/codex"),
        &build,
        &environment,
    )
    .unwrap();
    assert!(resume.contains("ExecStart=\"/opt/100%%/toks-router\" launch-resume-supervisor"));
    assert!(resume.contains("TOKS_CODEX_BIN=/opt/100%%/codex"));
    assert!(resume.contains("CODEX_HOME=/opt/$CODEX/%%HOME"));
}

#[test]
fn literal_dollars_in_exec_paths_cannot_expand_as_environment_variables() {
    let environment = UnitEnvironment::from_values([None, None, None, None]);
    let build = BuildId::new("build").unwrap();
    let service = render_service_unit(
        Path::new("/opt/$ROOT/${CHANNEL}/toks-router"),
        Path::new("/opt/codex"),
        &build,
        &environment,
    )
    .unwrap();
    assert!(service.contains("ExecStart=\"/opt/$$ROOT/$${CHANNEL}/toks-router\" launch-host"));

    let worker = render_worker_unit(Path::new("/opt/$ROOT/${CHANNEL}/router")).unwrap();
    assert!(worker.contains("/opt/$$ROOT/$${CHANNEL}/router/generations/%i/toks-router"));
    assert!(worker.contains("/opt/$$ROOT/$${CHANNEL}/router/generations/%i/launch.json"));
    assert!(!worker.contains("generations/%%i"));

    let resume = resume_unit::render(
        Path::new("/opt/$ROOT/${CHANNEL}/toks-router"),
        Path::new("/opt/codex"),
        &build,
        &environment,
    )
    .unwrap();
    assert!(resume
        .contains("ExecStart=\"/opt/$$ROOT/$${CHANNEL}/toks-router\" launch-resume-supervisor"));
}

#[test]
fn resume_unit_rejects_control_characters() {
    let environment = UnitEnvironment::from_values([None, None, None, None]);
    let build = BuildId::new("build").unwrap();
    assert!(resume_unit::render(
        Path::new("/opt/toks-router\nExecStart=/bin/false"),
        Path::new("/opt/codex"),
        &build,
        &environment,
    )
    .is_err());
}
