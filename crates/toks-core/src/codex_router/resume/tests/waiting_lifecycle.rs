use super::{Harness, ResumePhase, TaskState, ThreadId, UnixMillis, WaitingThread, NOW};

#[test]
fn launch_is_authorized_durably_before_spawn() {
    let harness = Harness::new("waiting");
    harness.supervisor().tick(NOW).unwrap();

    let state = harness.store.load().unwrap();
    let attempt = &state.attempts[&ThreadId::new("waiting")];
    assert_eq!(attempt.phase, ResumePhase::Launching);
    assert_eq!(harness.units.0.borrow().launches[0].0, attempt.id);
    assert_eq!(attempt.cwd, harness.workspace);
    assert_eq!(harness.queue.0.borrow().claims, [ThreadId::new("waiting")]);
    assert!(harness.queue.0.borrow().waiting.is_empty());
}

#[test]
fn supervisor_prunes_only_exact_known_subagent_entries_before_launch() {
    let harness = Harness::new("child");
    let root = WaitingThread::new(ThreadId::new("root"), UnixMillis::new(NOW.get() + 1));
    let unknown = WaitingThread::new(ThreadId::new("unknown"), UnixMillis::new(NOW.get() + 2));
    {
        let mut queue = harness.queue.0.borrow_mut();
        queue.waiting.extend([root.clone(), unknown.clone()]);
        queue.eligible = None;
    }
    let database = harness._directory.path().join("state.sqlite");
    let connection = rusqlite::Connection::open(&database).unwrap();
    connection
        .execute(
            "CREATE TABLE threads (id TEXT PRIMARY KEY, thread_source TEXT)",
            [],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO threads (id, thread_source) VALUES \
                ('child', 'subagent'), ('root', 'cli')",
            [],
        )
        .unwrap();
    drop(connection);

    harness
        .supervisor_with_thread_sources(
            crate::codex_router::thread_source::ThreadSourceStore::for_database(database),
        )
        .tick(NOW)
        .unwrap();

    let queue = harness.queue.0.borrow();
    assert_eq!(queue.waiting, [root, unknown]);
    assert_eq!(queue.discarded, [ThreadId::new("child")]);
    assert!(harness.units.0.borrow().launches.is_empty());
}

#[test]
fn supervisor_keeps_waiting_entries_when_codex_thread_state_is_unavailable() {
    let harness = Harness::new("unknown");
    harness.queue.0.borrow_mut().eligible = None;

    harness.supervisor().tick(NOW).unwrap();

    let queue = harness.queue.0.borrow();
    assert_eq!(queue.waiting.len(), 1);
    assert!(queue.discarded.is_empty());
}

#[test]
fn supervisor_stops_an_active_resume_when_thread_becomes_a_known_subagent() {
    let harness = Harness::new("child");
    let database = harness._directory.path().join("state.sqlite");
    let mut supervisor = harness.supervisor_with_thread_sources(
        crate::codex_router::thread_source::ThreadSourceStore::for_database(&database),
    );

    supervisor.tick(NOW).unwrap();
    let attempt = harness.attempt();
    harness.set_unit(&attempt, TaskState::Running);

    let connection = rusqlite::Connection::open(database).unwrap();
    connection
        .execute(
            "CREATE TABLE threads (id TEXT PRIMARY KEY, thread_source TEXT)",
            [],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO threads (id, thread_source) VALUES ('child', 'subagent')",
            [],
        )
        .unwrap();
    drop(connection);

    supervisor.tick(UnixMillis::new(NOW.get() + 1)).unwrap();

    assert!(harness.store.load().unwrap().attempts.is_empty());
    let queue = harness.queue.0.borrow();
    assert!(queue.waiting.is_empty());
    assert!(queue.requeues.is_empty());
    assert!(queue.admissions.is_empty());
    assert_eq!(
        harness.units.0.borrow().cancels.as_slice(),
        std::slice::from_ref(&attempt)
    );
    assert_eq!(harness.units.0.borrow().cleanups, [attempt]);
}

#[test]
fn replacement_adopts_running_unit_without_duplicate_launch() {
    let harness = Harness::new("running");
    harness.supervisor().tick(NOW).unwrap();
    let attempt = harness.attempt();
    harness.set_unit(&attempt, TaskState::Running);

    harness.supervisor().tick(NOW).unwrap();

    assert_eq!(harness.units.0.borrow().launches.len(), 1);
    assert_eq!(
        harness.store.load().unwrap().attempts[&ThreadId::new("running")].phase,
        ResumePhase::Running
    );
    assert_eq!(harness.queue.0.borrow().claims, [ThreadId::new("running")]);
}

#[test]
fn missing_previously_running_unit_is_requeued_instead_of_duplicated() {
    let harness = Harness::new("vanished-running");
    harness.supervisor().tick(NOW).unwrap();
    let attempt = harness.attempt();
    harness.set_unit(&attempt, TaskState::Running);
    harness.supervisor().tick(NOW).unwrap();
    harness.units.0.borrow_mut().states.remove(&attempt);

    harness.supervisor().tick(NOW).unwrap();

    assert!(harness.store.load().unwrap().attempts.is_empty());
    assert_eq!(harness.units.0.borrow().launches.len(), 1);
    assert_eq!(
        harness.queue.0.borrow().requeues,
        [ThreadId::new("vanished-running")]
    );
}
