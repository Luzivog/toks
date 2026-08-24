use std::collections::BTreeMap;
use std::io::Write;
use std::net::TcpListener;
use std::process::Command;
use std::time::{Duration, Instant};

use super::{active_status, bounded_output_until, health_check_until, process_matches};

#[test]
fn stalled_child_is_killed_and_reaped_at_the_deadline() {
    let directory = tempfile::tempdir().unwrap();
    let pid_path = directory.path().join("pid");
    let script = format!("echo $$ > '{}'; exec sleep 30", pid_path.display());
    let mut command = Command::new("sh");
    command.args(["-c", &script]);
    let started = Instant::now();

    let error =
        bounded_output_until(&mut command, started + Duration::from_millis(100)).unwrap_err();

    assert!(error.to_string().contains("timed out"));
    assert!(started.elapsed() < Duration::from_secs(1));
    let pid = std::fs::read_to_string(pid_path).unwrap();
    assert!(!std::path::Path::new(&format!("/proc/{}", pid.trim())).exists());
}

#[test]
fn expired_deadline_does_not_launch_the_child() {
    let directory = tempfile::tempdir().unwrap();
    let marker = directory.path().join("launched");
    let mut command = Command::new("touch");
    command.arg(&marker);

    let error =
        bounded_output_until(&mut command, Instant::now() - Duration::from_millis(1)).unwrap_err();

    assert!(error.to_string().contains("timed out"));
    assert!(!marker.exists());
}

#[test]
fn completed_child_output_is_preserved() {
    let mut command = Command::new("sh");
    command.args(["-c", "printf output; printf error >&2"]);

    let output =
        bounded_output_until(&mut command, Instant::now() + Duration::from_secs(1)).unwrap();

    assert!(output.status.success());
    assert_eq!(output.stdout, b"output");
    assert_eq!(output.stderr, b"error");
}

#[test]
fn active_state_distinguishes_inactive_units_from_manager_errors() {
    fn output(status: i32) -> std::process::Output {
        Command::new("sh")
            .args(["-c", &format!("printf manager-error >&2; exit {status}")])
            .output()
            .unwrap()
    }

    assert!(active_status(output(0)).unwrap());
    assert!(!active_status(output(3)).unwrap());
    assert!(!active_status(output(4)).unwrap());
    assert!(active_status(output(1)).is_err());
}

#[test]
fn slow_drip_health_response_cannot_outlive_the_absolute_deadline() {
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let response = b"HTTP/1.1 200 OK\r\ncontent-length: 12\r\n\r\ntoks-router\n";
        for byte in response {
            if stream.write_all(std::slice::from_ref(byte)).is_err() {
                break;
            }
            std::thread::sleep(Duration::from_millis(30));
        }
    });
    let started = Instant::now();

    let error = health_check_until(address, started + Duration::from_millis(120)).unwrap_err();

    assert!(started.elapsed() < Duration::from_millis(400));
    assert!(error.to_string().contains("deadline") || error.to_string().contains("timed out"));
    server.join().unwrap();
}

#[test]
fn process_identity_rejects_environment_and_argument_overrides() {
    let directory = tempfile::tempdir().unwrap();
    let executable = directory.path().join("toks-router");
    let process = directory.path().join("proc");
    std::fs::write(&executable, b"router").unwrap();
    std::fs::create_dir(&process).unwrap();
    std::os::unix::fs::symlink(&executable, process.join("exe")).unwrap();
    std::fs::write(
        process.join("cmdline"),
        format!("{}\0host\0", executable.display()),
    )
    .unwrap();
    let expected = BTreeMap::from([
        ("TOKS_ROUTER_BUILD_ID".into(), Some("build".into())),
        ("TOKS_CODEX_BIN".into(), Some("/opt/codex".into())),
        ("PATH".into(), Some("/bin".into())),
        ("CODEX_HOME".into(), None),
        ("XDG_DATA_HOME".into(), Some("/data".into())),
        ("XDG_CONFIG_HOME".into(), None),
    ]);
    let exact =
        b"TOKS_ROUTER_BUILD_ID=build\0TOKS_CODEX_BIN=/opt/codex\0PATH=/bin\0XDG_DATA_HOME=/data\0";
    std::fs::write(process.join("environ"), exact).unwrap();
    assert!(process_matches(&process, &executable, b"host", &expected));

    for extra in [
        b"OPENAI_API_KEY=manager-secret\0".as_slice(),
        b"LD_PRELOAD=/tmp/injected.so\0",
    ] {
        let mut contaminated = exact.to_vec();
        contaminated.extend_from_slice(extra);
        std::fs::write(process.join("environ"), contaminated).unwrap();
        assert!(!process_matches(&process, &executable, b"host", &expected));
    }

    for overridden in [
        b"TOKS_ROUTER_BUILD_ID=other\0TOKS_CODEX_BIN=/opt/codex\0PATH=/bin\0XDG_DATA_HOME=/data\0".as_slice(),
        b"TOKS_ROUTER_BUILD_ID=build\0TOKS_CODEX_BIN=/other\0PATH=/bin\0XDG_DATA_HOME=/data\0",
        b"TOKS_ROUTER_BUILD_ID=build\0TOKS_CODEX_BIN=/opt/codex\0PATH=/drop-in\0XDG_DATA_HOME=/data\0",
        b"TOKS_ROUTER_BUILD_ID=build\0TOKS_CODEX_BIN=/opt/codex\0PATH=/bin\0CODEX_HOME=/override\0XDG_DATA_HOME=/data\0",
    ] {
        std::fs::write(process.join("environ"), overridden).unwrap();
        assert!(!process_matches(&process, &executable, b"host", &expected));
    }
    std::fs::write(process.join("environ"), exact).unwrap();
    std::fs::write(
        process.join("cmdline"),
        format!("{}\0host\0unexpected\0", executable.display()),
    )
    .unwrap();
    assert!(!process_matches(&process, &executable, b"host", &expected));
    std::fs::write(
        process.join("cmdline"),
        format!("{}\0host\0\0", executable.display()),
    )
    .unwrap();
    assert!(!process_matches(&process, &executable, b"host", &expected));
}

#[test]
fn process_identity_checks_home_proxy_certificate_and_loader_environment() {
    let directory = tempfile::tempdir().unwrap();
    let executable = directory.path().join("toks-router");
    let process = directory.path().join("proc");
    std::fs::write(&executable, b"router").unwrap();
    std::fs::create_dir(&process).unwrap();
    std::os::unix::fs::symlink(&executable, process.join("exe")).unwrap();
    std::fs::write(
        process.join("cmdline"),
        format!("{}\0host\0", executable.display()),
    )
    .unwrap();
    let expected = BTreeMap::from([
        ("HOME".into(), Some("/home/router".into())),
        ("HTTPS_PROXY".into(), Some("http://proxy:8080".into())),
        ("HTTP_PROXY".into(), None),
        ("SSL_CERT_FILE".into(), Some("/etc/ssl/router.pem".into())),
        ("LD_LIBRARY_PATH".into(), Some("/opt/router/lib".into())),
    ]);
    let exact = b"HOME=/home/router\0HTTPS_PROXY=http://proxy:8080\0SSL_CERT_FILE=/etc/ssl/router.pem\0LD_LIBRARY_PATH=/opt/router/lib\0";
    std::fs::write(process.join("environ"), exact).unwrap();
    assert!(process_matches(&process, &executable, b"host", &expected));

    for changed in [
        b"HOME=/other\0HTTPS_PROXY=http://proxy:8080\0SSL_CERT_FILE=/etc/ssl/router.pem\0LD_LIBRARY_PATH=/opt/router/lib\0".as_slice(),
        b"HOME=/home/router\0HTTPS_PROXY=http://other:8080\0SSL_CERT_FILE=/etc/ssl/router.pem\0LD_LIBRARY_PATH=/opt/router/lib\0",
        b"HOME=/home/router\0HTTPS_PROXY=http://proxy:8080\0HTTP_PROXY=http://unexpected\0SSL_CERT_FILE=/etc/ssl/router.pem\0LD_LIBRARY_PATH=/opt/router/lib\0",
        b"HOME=/home/router\0HTTPS_PROXY=http://proxy:8080\0SSL_CERT_FILE=/other.pem\0LD_LIBRARY_PATH=/opt/router/lib\0",
        b"HOME=/home/router\0HTTPS_PROXY=http://proxy:8080\0SSL_CERT_FILE=/etc/ssl/router.pem\0LD_LIBRARY_PATH=/other\0",
    ] {
        std::fs::write(process.join("environ"), changed).unwrap();
        assert!(!process_matches(&process, &executable, b"host", &expected));
    }
}
