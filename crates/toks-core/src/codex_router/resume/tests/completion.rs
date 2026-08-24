use super::*;

#[test]
fn success_receipt_wins_even_before_unit_exit_and_clears_once() {
    let harness = Harness::new("success");
    harness.supervisor().tick(NOW).unwrap();
    let attempt = harness.attempt();
    harness.set_unit(&attempt, TaskState::Running);
    harness.store.record_outcome(&attempt, true).unwrap();

    harness.supervisor().tick(NOW).unwrap();
    harness.supervisor().tick(NOW).unwrap();

    assert!(harness.store.load().unwrap().attempts.is_empty());
    assert_eq!(harness.units.0.borrow().cleanups, [attempt]);
    assert!(harness.queue.0.borrow().waiting.is_empty());
}

#[test]
fn failed_attempt_requeues_durably_and_waits_before_new_identity() {
    let harness = Harness::new("failure");
    harness.supervisor().tick(NOW).unwrap();
    let failed = harness.attempt();
    harness.queue.0.borrow_mut().waiting.clear();
    harness.store.record_outcome(&failed, false).unwrap();

    harness.supervisor().tick(NOW).unwrap();
    let state = harness.store.load().unwrap();
    assert!(state.attempts.is_empty());
    let waiting_id = harness.queue.0.borrow().waiting[0].waiting_id.clone();
    assert!(state.retry_after.contains_key(&waiting_id));
    assert_eq!(harness.units.0.borrow().launches.len(), 1);

    harness
        .supervisor()
        .tick(UnixMillis::new(NOW.get() + 5 * 60 * 1_000))
        .unwrap();
    assert_ne!(harness.attempt(), failed);
    assert_eq!(harness.units.0.borrow().launches.len(), 2);
}

#[test]
fn systemd_outcome_recovers_when_receipt_is_missing() {
    for (state, clears) in [(TaskState::Succeeded, true), (TaskState::Failed, false)] {
        let harness = Harness::new("unit-outcome");
        harness.supervisor().tick(NOW).unwrap();
        let attempt = harness.attempt();
        harness.queue.0.borrow_mut().waiting.clear();
        harness.set_unit(&attempt, state);
        harness.supervisor().tick(NOW).unwrap();
        assert!(harness.store.load().unwrap().attempts.is_empty());
        assert_eq!(harness.queue.0.borrow().waiting.is_empty(), clears);
    }
}

#[test]
fn continuation_uses_exact_thread_argument_without_a_shell() {
    let command = super::super::task_command::command_for_test(
        std::path::Path::new("/opt/codex"),
        "00000000-0000-0000-0000-000000000001",
        &ThreadId::new("thread; literal"),
        std::path::Path::new("/workspace exact"),
    );
    assert_eq!(command.get_program(), "/opt/codex");
    let arguments = command.get_args().collect::<Vec<_>>();
    assert!(arguments.windows(2).any(|pair| {
        pair == [
            "-c",
            "model_providers.toks_resume.env_http_headers={\"x-toks-resume-attempt\"=\"TOKS_RESUME_ATTEMPT\"}",
        ]
    }));
    assert_eq!(
        &arguments[arguments.len() - 8..],
        [
            "exec",
            "--skip-git-repo-check",
            "-C",
            "/workspace exact",
            "resume",
            "--all",
            "thread; literal",
            super::super::task_command::PROMPT_FOR_TEST,
        ]
    );
    assert_eq!(
        command
            .get_envs()
            .find(|(name, _)| *name == "TOKS_RESUME_ATTEMPT")
            .and_then(|(_, value)| value),
        Some(std::ffi::OsStr::new("00000000-0000-0000-0000-000000000001"))
    );
    let environment = command
        .get_envs()
        .map(|(name, _)| name.to_string_lossy().into_owned())
        .collect::<std::collections::BTreeSet<_>>();
    assert!(!environment.contains("OPENAI_API_KEY"));
    assert!(!environment.contains("LD_PRELOAD"));
    assert!(environment.contains("TOKS_RESUME_ATTEMPT"));
}

#[test]
fn outcome_paths_reject_non_uuid_attempts() {
    let directory = tempfile::tempdir().unwrap();
    let store = ResumeStore::for_data_dir(directory.path());
    assert!(store.record_outcome("../escape", true).is_err());
    assert!(!directory.path().join("escape.json").exists());
}

#[test]
fn supervisor_tick_errors_include_the_full_cause_chain() {
    let error = anyhow::anyhow!("disk unavailable").context("saving resume state");
    assert_eq!(
        super::super::tick_error_message(&error),
        "toks resume supervisor tick failed: saving resume state: disk unavailable"
    );
}

#[cfg(unix)]
#[test]
fn nonzero_resume_exit_reports_the_exact_status_without_output() {
    let status = std::process::Command::new("/bin/sh")
        .args(["-c", "exit 23"])
        .status()
        .unwrap();
    assert_eq!(
        super::super::task_failure_message(status),
        "resumed Codex task exited unsuccessfully (exit status: 23)"
    );
}

#[tokio::test]
async fn resume_spawn_failure_preserves_the_operating_system_cause() {
    let command = std::process::Command::new("/definitely/missing/toks-codex");
    let error = super::super::task_command::execute(command)
        .await
        .unwrap_err();
    let message = format!("{error:#}");
    assert!(message.contains("spawning Codex resume process"));
    assert!(message.contains("No such file") || message.contains("not found"));
}

#[test]
fn persisted_attempt_ids_must_be_canonical_and_unique() {
    let harness = Harness::new("state-validation");
    harness.supervisor().tick(NOW).unwrap();
    let mut state = harness.store.load().unwrap();
    let thread = ThreadId::new("state-validation");
    state.attempts.get_mut(&thread).unwrap().id = "00000000-0000-4000-8000-0000000000AA".into();
    harness.store.save(&state).unwrap();
    assert!(harness
        .store
        .load()
        .unwrap_err()
        .to_string()
        .contains("non-canonical resume attempt id"));

    let harness = Harness::new("duplicate-state");
    harness.supervisor().tick(NOW).unwrap();
    let mut state = harness.store.load().unwrap();
    let duplicate_thread = ThreadId::new("duplicate-state-2");
    let mut duplicate = state.attempts.values().next().unwrap().clone();
    duplicate.waiting = crate::rotation::WaitingThread::new(duplicate_thread.clone(), NOW);
    state.attempts.insert(duplicate_thread, duplicate);
    harness.store.save(&state).unwrap();
    assert!(harness
        .store
        .load()
        .unwrap_err()
        .to_string()
        .contains("duplicate resume attempt id"));
}

#[test]
fn persisted_attempt_phase_terminal_and_thread_invariants_are_validated() {
    let harness = Harness::new("phase-state");
    harness.supervisor().tick(NOW).unwrap();
    let mut state = harness.store.load().unwrap();
    let attempt = state.attempts.values_mut().next().unwrap();
    attempt.terminal = Some(super::super::state::ResumeTerminalState::Success);
    harness.store.save(&state).unwrap();
    assert!(harness
        .store
        .load()
        .unwrap_err()
        .to_string()
        .contains("phase and terminal state are inconsistent"));

    let harness = Harness::new("thread-state");
    harness.supervisor().tick(NOW).unwrap();
    let mut state = harness.store.load().unwrap();
    state
        .attempts
        .values_mut()
        .next()
        .unwrap()
        .waiting
        .thread_id = ThreadId::new("different-thread");
    harness.store.save(&state).unwrap();
    assert!(harness
        .store
        .load()
        .unwrap_err()
        .to_string()
        .contains("thread key does not match"));
}

#[cfg(unix)]
#[test]
fn task_launch_rejects_a_persisted_workspace_retargeted_by_symlink() {
    use std::os::unix::fs::symlink;

    let harness = Harness::new("retargeted-workspace");
    harness.supervisor().tick(NOW).unwrap();
    let state = harness.store.load().unwrap();
    let attempt = state.attempts.values().next().unwrap();
    let target = harness._directory.path().join("redirect-target");
    std::fs::create_dir(&target).unwrap();
    std::fs::remove_dir(&harness.workspace).unwrap();
    symlink(&target, &harness.workspace).unwrap();

    let error = super::super::task_workspace(
        &state,
        &attempt.id,
        &attempt.waiting.thread_id,
        attempt.cwd.clone(),
    )
    .unwrap_err();

    assert_eq!(error.to_string(), "resume workspace does not match attempt");
}
