use futures_util::FutureExt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use super::channel::AsyncListener;
use super::coordinator_identity::matches_snapshot;
use super::worker::{run_with_authorizer, PeerAuthorizer, Service};
use crate::codex_router::handoff::{
    Control, GenerationId as WireGenerationId, HandoffListener, PeerIdentity, Received,
};
use crate::codex_router::host::GenerationId;

#[test]
fn same_uid_peer_requires_stable_main_pid_and_exact_coordinator_contract() {
    let directory = tempfile::tempdir().unwrap();
    let artifact_root = directory.path().join("router-artifacts");
    let executable = directory.path().join("candidate-router");
    std::fs::write(&executable, b"router").unwrap();
    let environment = crate::codex_router::systemd::UnitEnvironment::from_pairs(&[
        ("PATH", Some("/bin")),
        ("HOME", Some("/home/router")),
    ]);
    let build = crate::codex_router::systemd::persist_test_launch_contract(
        &artifact_root,
        &executable,
        std::path::Path::new("/opt/codex"),
        &environment,
    )
    .unwrap();
    let stable = std::fs::read_dir(artifact_root.join("executables"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path()
        .join("toks-router");
    let proc_root = directory.path().join("proc");
    let process = proc_root.join("701");
    std::fs::create_dir_all(&process).unwrap();
    std::os::unix::fs::symlink(&stable, process.join("exe")).unwrap();
    std::fs::write(process.join("cmdline"), b"router\0host\0").unwrap();
    let exact_environment = format!(
        "HOME=/home/router\0PATH=/bin\0TOKS_CODEX_BIN=/opt/codex\0TOKS_ROUTER_BUILD_ID={}\0LISTEN_PID=701\0LISTEN_FDS=1\0LISTEN_FDNAMES=router\0",
        build.as_str()
    );
    std::fs::write(process.join("environ"), &exact_environment).unwrap();
    std::fs::write(
        process.join("cgroup"),
        b"0::/user.slice/app.slice/toks-router.service\n",
    )
    .unwrap();
    let uid = nix::unistd::Uid::current().as_raw();
    let legitimate = PeerIdentity { pid: 701, uid };

    assert!(matches_snapshot(
        legitimate,
        uid,
        Some(701),
        Some(701),
        &artifact_root,
        &proc_root,
    ));
    assert!(!matches_snapshot(
        PeerIdentity { pid: 700, uid },
        uid,
        Some(701),
        Some(701),
        &artifact_root,
        &proc_root,
    ));
    assert!(!matches_snapshot(
        PeerIdentity {
            pid: 701,
            uid: uid.saturating_add(1),
        },
        uid,
        Some(701),
        Some(701),
        &artifact_root,
        &proc_root,
    ));
    assert!(!matches_snapshot(
        legitimate,
        uid,
        Some(701),
        Some(702),
        &artifact_root,
        &proc_root,
    ));

    std::fs::write(
        process.join("cgroup"),
        b"0::/user.slice/app.slice/session.scope\n",
    )
    .unwrap();
    assert!(!matches_snapshot(
        legitimate,
        uid,
        Some(701),
        Some(701),
        &artifact_root,
        &proc_root,
    ));

    std::fs::write(
        process.join("cgroup"),
        b"0::/user.slice/app.slice/toks-router.service\n",
    )
    .unwrap();
    std::fs::write(process.join("cmdline"), b"router\0host\0unexpected\0").unwrap();
    assert!(!matches_snapshot(
        legitimate,
        uid,
        Some(701),
        Some(701),
        &artifact_root,
        &proc_root,
    ));
    std::fs::write(process.join("cmdline"), b"router\0host\0").unwrap();
    std::fs::write(
        process.join("environ"),
        format!("{exact_environment}LD_PRELOAD=/tmp/injected.so\0"),
    )
    .unwrap();
    assert!(!matches_snapshot(
        legitimate,
        uid,
        Some(701),
        Some(701),
        &artifact_root,
        &proc_root,
    ));
}

#[tokio::test]
async fn rejected_coordinator_cannot_command_worker_before_restarted_coordinator_is_adopted() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("handoff.sock");
    let listener = AsyncListener::new(HandoffListener::bind(&path).unwrap()).unwrap();
    let permitted = Arc::new(AtomicBool::new(false));
    let authorizer: PeerAuthorizer = {
        let permitted = permitted.clone();
        Arc::new(move |_| futures_util::future::ready(permitted.load(Ordering::Acquire)).boxed())
    };
    let worker = tokio::spawn(run_with_authorizer(
        GenerationId::from_raw(31),
        path,
        idle_service(),
        authorizer,
    ));

    let rejected = listener.accept().await.unwrap();
    rejected
        .send_control(&Control::CoordinatorHello { epoch: 40 })
        .await
        .unwrap();
    rejected
        .send_control(&Control::Activate {
            generation: WireGenerationId::new(31),
        })
        .await
        .unwrap();
    assert!(
        tokio::time::timeout(Duration::from_secs(1), rejected.receive())
            .await
            .unwrap()
            .is_err()
    );

    permitted.store(true, Ordering::Release);
    let adopted = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let channel = listener.accept().await.unwrap();
            if matches!(
                channel.receive().await,
                Ok(Received::Control(Control::WorkerHello { generation, .. }))
                    if generation.raw() == 31
            ) {
                break channel;
            }
        }
    })
    .await
    .unwrap();
    adopted
        .send_control(&Control::CoordinatorHello { epoch: 41 })
        .await
        .unwrap();
    assert!(matches!(
        adopted.receive().await.unwrap(),
        Received::Control(Control::Ready { generation }) if generation.raw() == 31
    ));
    assert!(matches!(
        adopted.receive().await.unwrap(),
        Received::Control(Control::ConnectionsObserved { active: 0, .. })
    ));
    adopted
        .send_control(&Control::Drain {
            generation: WireGenerationId::new(31),
        })
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(2), worker)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}

fn idle_service() -> Service {
    Arc::new(|_, lifetime| async move { drop(lifetime) }.boxed())
}
