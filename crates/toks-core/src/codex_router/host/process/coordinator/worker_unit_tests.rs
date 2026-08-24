use super::worker_unit::{output_with_timeout, parse_inventory, Liveness};
use std::time::Duration;
use tokio::process::Command;

#[tokio::test]
async fn a_stalled_command_is_timed_out_and_killed() {
    let directory = tempfile::tempdir().unwrap();
    let pid_path = directory.path().join("pid");
    let script = format!("echo $$ > '{}'; exec sleep 30", pid_path.display());
    let mut command = Command::new("sh");
    command.args(["-c", &script]);

    let error = output_with_timeout(command, Duration::from_millis(100))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("timed out"));

    let pid = std::fs::read_to_string(&pid_path)
        .unwrap()
        .trim()
        .parse::<u32>()
        .unwrap();
    for _ in 0..50 {
        if !std::path::Path::new(&format!("/proc/{pid}")).exists() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("timed-out worker command {pid} was not killed");
}

#[test]
fn inventory_distinguishes_running_stopped_and_unknown_units() {
    let inventory = parse_inventory(
        "toks-router-worker@1.service loaded active running worker\n\
         toks-router-worker@2.service loaded failed failed worker\n\
         toks-router-worker@3.service loaded mystery mystery worker\n",
    )
    .unwrap();

    assert_eq!(
        inventory[&crate::codex_router::host::GenerationId::from_raw(1)],
        Liveness::Running
    );
    assert_eq!(
        inventory[&crate::codex_router::host::GenerationId::from_raw(2)],
        Liveness::Stopped
    );
    assert_eq!(
        inventory[&crate::codex_router::host::GenerationId::from_raw(3)],
        Liveness::Unknown
    );
}
