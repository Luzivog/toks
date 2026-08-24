use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::rc::Rc;

use anyhow::Result;
use tempfile::TempDir;

use super::state::{ResumePhase, ResumeStore};
use super::supervisor::{ResumeQueue, Supervisor, TaskState, TaskUnits};
use crate::accounts::AccountId;
use crate::rotation::{
    ResumeAuthorization, ResumeTerminal, RotationSettings, RotationSettingsStore, ThreadId,
    UnixMillis, WaitingId, WaitingThread,
};

const NOW: UnixMillis = UnixMillis::new(1_000);

#[derive(Default)]
struct QueueState {
    eligible: Option<AccountId>,
    eligible_by_thread: BTreeMap<ThreadId, Option<AccountId>>,
    waiting: Vec<WaitingThread>,
    claims: Vec<ThreadId>,
    requeues: Vec<ThreadId>,
    discarded: Vec<ThreadId>,
    authorization_calls: Vec<ThreadId>,
    admissions: BTreeMap<WaitingId, FakeAdmission>,
    fail_authorizations: usize,
    cancel_on_authorization_call: Option<usize>,
    crash_after_authorize: usize,
    fail_forgets: usize,
    stale_once_to: Option<AccountId>,
    stale_threads: std::collections::BTreeSet<ThreadId>,
}

#[derive(Clone)]
struct FakeAdmission {
    attempt: String,
    finished: Option<Option<WaitingId>>,
}

#[derive(Clone)]
struct FakeQueue(Rc<RefCell<QueueState>>);

impl ResumeQueue for FakeQueue {
    fn eligible_account(&mut self, thread: &ThreadId) -> Result<Option<AccountId>> {
        let state = self.0.borrow();
        Ok(state
            .eligible_by_thread
            .get(thread)
            .cloned()
            .unwrap_or_else(|| state.eligible.clone()))
    }

    fn waiting_threads(&mut self) -> Vec<WaitingThread> {
        self.0.borrow().waiting.clone()
    }

    fn discard_waiting_entries(&mut self, discarded: &[WaitingThread]) -> Result<()> {
        let mut state = self.0.borrow_mut();
        let removed = state
            .waiting
            .iter()
            .filter(|current| discarded.iter().any(|candidate| candidate == *current))
            .map(|current| current.thread_id.clone())
            .collect::<Vec<_>>();
        state
            .waiting
            .retain(|current| !discarded.iter().any(|candidate| candidate == current));
        state.discarded.extend(removed);
        Ok(())
    }

    fn authorize(
        &mut self,
        waiting: &WaitingThread,
        attempt: &str,
        account: &AccountId,
    ) -> Result<ResumeAuthorization> {
        let mut state = self.0.borrow_mut();
        state.authorization_calls.push(waiting.thread_id.clone());
        if state.cancel_on_authorization_call == Some(state.authorization_calls.len()) {
            return Ok(ResumeAuthorization::Cancelled);
        }
        if state.fail_authorizations > 0 {
            state.fail_authorizations -= 1;
            anyhow::bail!("synthetic authorization failure");
        }
        if let Some(next) = state.stale_once_to.take() {
            assert_eq!(state.eligible.as_ref(), Some(account));
            state.eligible = Some(next);
            return Ok(ResumeAuthorization::Stale);
        }
        if state.stale_threads.contains(&waiting.thread_id) {
            return Ok(ResumeAuthorization::Stale);
        }
        let authorization = if state
            .admissions
            .get(&waiting.waiting_id)
            .is_some_and(|admission| admission.attempt == attempt)
        {
            ResumeAuthorization::Acquired
        } else if let Some(index) = state.waiting.iter().position(|current| current == waiting) {
            state.waiting.remove(index);
            state.admissions.insert(
                waiting.waiting_id.clone(),
                FakeAdmission {
                    attempt: attempt.to_owned(),
                    finished: None,
                },
            );
            state.claims.push(waiting.thread_id.clone());
            ResumeAuthorization::Acquired
        } else {
            ResumeAuthorization::Lost
        };
        if state.crash_after_authorize > 0 {
            state.crash_after_authorize -= 1;
            anyhow::bail!("synthetic post-authorization crash");
        }
        Ok(authorization)
    }

    fn finish(
        &mut self,
        waiting: &WaitingThread,
        attempt: &str,
        terminal: ResumeTerminal,
        replacement: WaitingId,
    ) -> Result<Option<WaitingThread>> {
        let mut state = self.0.borrow_mut();
        let Some(admission) = state.admissions.get(&waiting.waiting_id) else {
            return Ok(None);
        };
        if admission.attempt != attempt {
            return Ok(None);
        }
        if let Some(finished) = &admission.finished {
            let id = finished.clone();
            return Ok(id.and_then(|id| {
                state
                    .waiting
                    .iter()
                    .find(|current| current.waiting_id == id)
                    .cloned()
            }));
        }
        let queued = match terminal {
            ResumeTerminal::Success | ResumeTerminal::Discarded => None,
            ResumeTerminal::Failure => state
                .waiting
                .iter()
                .find(|current| current.thread_id == waiting.thread_id)
                .cloned()
                .or_else(|| {
                    Some(WaitingThread::with_id(
                        replacement,
                        waiting.thread_id.clone(),
                        NOW,
                    ))
                }),
            ResumeTerminal::Cancelled => state
                .waiting
                .iter()
                .find(|current| current.thread_id == waiting.thread_id)
                .cloned()
                .or_else(|| Some(waiting.clone())),
        };
        if let Some(queued) = &queued {
            if !state.waiting.iter().any(|current| current == queued) {
                state.waiting.push(queued.clone());
                state.requeues.push(waiting.thread_id.clone());
            }
        }
        state
            .admissions
            .get_mut(&waiting.waiting_id)
            .unwrap()
            .finished = Some(queued.as_ref().map(|entry| entry.waiting_id.clone()));
        Ok(queued)
    }

    fn forget(&mut self, waiting: &WaitingThread, attempt: &str) -> Result<()> {
        let mut state = self.0.borrow_mut();
        if state.fail_forgets > 0 {
            state.fail_forgets -= 1;
            anyhow::bail!("synthetic forget failure");
        }
        if state
            .admissions
            .get(&waiting.waiting_id)
            .is_some_and(|admission| admission.attempt == attempt && admission.finished.is_some())
        {
            state.admissions.remove(&waiting.waiting_id);
        }
        Ok(())
    }
}

#[derive(Default)]
struct UnitState {
    states: BTreeMap<String, TaskState>,
    launches: Vec<(String, ThreadId, PathBuf)>,
    cleanups: Vec<String>,
    cancels: Vec<String>,
    fail_launches: usize,
    fail_cancels: usize,
    fail_cleanups: usize,
    permanent_cleanup_failures: std::collections::BTreeSet<String>,
    fail_inventory: bool,
    inventory_calls: usize,
}

#[derive(Clone)]
struct FakeUnits(Rc<RefCell<UnitState>>);

impl TaskUnits for FakeUnits {
    fn launch(&mut self, attempt: &super::state::ResumeAttempt) -> Result<()> {
        let mut state = self.0.borrow_mut();
        state.launches.push((
            attempt.id.clone(),
            attempt.waiting.thread_id.clone(),
            attempt.cwd.clone(),
        ));
        if state.fail_launches > 0 {
            state.fail_launches -= 1;
            anyhow::bail!("synthetic launch failure");
        }
        state
            .states
            .entry(attempt.id.clone())
            .or_insert(TaskState::Starting);
        Ok(())
    }

    fn inventory(&mut self, attempts: &[String]) -> Result<BTreeMap<String, TaskState>> {
        let mut state = self.0.borrow_mut();
        state.inventory_calls += 1;
        if state.fail_inventory {
            anyhow::bail!("synthetic inventory failure");
        }
        Ok(attempts
            .iter()
            .map(|attempt| {
                (
                    attempt.clone(),
                    state
                        .states
                        .get(attempt)
                        .copied()
                        .unwrap_or(TaskState::Missing),
                )
            })
            .collect())
    }

    fn cleanup(&mut self, attempt: &str) -> Result<()> {
        let mut state = self.0.borrow_mut();
        if state.permanent_cleanup_failures.contains(attempt) {
            anyhow::bail!("synthetic permanent cleanup failure for {attempt}");
        }
        if state.fail_cleanups > 0 {
            state.fail_cleanups -= 1;
            anyhow::bail!("synthetic cleanup failure");
        }
        state.states.remove(attempt);
        state.cleanups.push(attempt.to_owned());
        Ok(())
    }

    fn cancel(&mut self, attempt: &str, _: TaskState) -> Result<()> {
        if self.0.borrow().fail_cancels > 0 {
            self.0.borrow_mut().fail_cancels -= 1;
            anyhow::bail!("synthetic cancel failure");
        }
        let mut state = self.0.borrow_mut();
        state.states.remove(attempt);
        state.cancels.push(attempt.to_owned());
        Ok(())
    }
}

struct Harness {
    _directory: TempDir,
    store: ResumeStore,
    settings: RotationSettingsStore,
    queue: FakeQueue,
    units: FakeUnits,
    workspace: PathBuf,
}

impl Harness {
    fn new(thread: &str) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let store = ResumeStore::for_data_dir(directory.path());
        let settings = RotationSettingsStore::for_data_dir(directory.path());
        settings.save(&RotationSettings::default()).unwrap();
        let workspace = directory.path().join("workspace");
        std::fs::create_dir(&workspace).unwrap();
        let queue = FakeQueue(Rc::new(RefCell::new(QueueState {
            eligible: Some(AccountId::new("account")),
            waiting: vec![WaitingThread::new(ThreadId::new(thread), NOW)],
            ..QueueState::default()
        })));
        Self {
            _directory: directory,
            store,
            settings,
            queue,
            units: FakeUnits(Rc::new(RefCell::new(UnitState::default()))),
            workspace,
        }
    }

    fn supervisor(&self) -> Supervisor<FakeQueue, FakeUnits> {
        let workspace = self.workspace.clone();
        self.supervisor_with_workspace(move |_| Ok(workspace.clone()))
    }

    fn supervisor_with_workspace(
        &self,
        workspace: impl Fn(&ThreadId) -> Result<PathBuf> + 'static,
    ) -> Supervisor<FakeQueue, FakeUnits> {
        Supervisor::for_test(
            self.store.clone(),
            self.settings.clone(),
            self.queue.clone(),
            self.units.clone(),
            workspace,
        )
    }

    fn supervisor_with_thread_sources(
        &self,
        thread_sources: crate::codex_router::thread_source::ThreadSourceStore,
    ) -> Supervisor<FakeQueue, FakeUnits> {
        let workspace = self.workspace.clone();
        Supervisor::for_test_with_thread_sources(
            self.store.clone(),
            self.settings.clone(),
            self.queue.clone(),
            self.units.clone(),
            move |_| Ok(workspace.clone()),
            thread_sources,
        )
    }

    fn attempt(&self) -> String {
        self.store
            .load()
            .unwrap()
            .attempts
            .values()
            .next()
            .unwrap()
            .id
            .clone()
    }

    fn set_unit(&self, attempt: &str, state: TaskState) {
        self.units
            .0
            .borrow_mut()
            .states
            .insert(attempt.to_owned(), state);
    }
}

#[test]
fn crash_before_spawn_relaunches_the_same_attempt_identity() {
    let harness = Harness::new("pre-spawn");
    harness.units.0.borrow_mut().fail_launches = 1;
    assert!(harness.supervisor().tick(NOW).is_err());
    let attempt = harness.attempt();

    harness.supervisor().tick(NOW).unwrap();

    let launches = &harness.units.0.borrow().launches;
    assert_eq!(launches.len(), 2);
    assert!(launches.iter().all(|launch| launch.0 == attempt));
    assert_eq!(
        harness.queue.0.borrow().claims,
        [ThreadId::new("pre-spawn")]
    );
}

#[test]
fn cancellation_retires_launching_missing_and_starting_units() {
    for unit_state in [TaskState::Missing, TaskState::Starting] {
        let harness = Harness::new("cancelled");
        harness.supervisor().tick(NOW).unwrap();
        let attempt = harness.attempt();
        if unit_state == TaskState::Missing {
            harness.units.0.borrow_mut().states.remove(&attempt);
        }
        let mut settings = harness.settings.load().unwrap();
        settings.cancel_waiting(&ThreadId::new("cancelled"));
        harness.settings.save(&settings).unwrap();

        harness.supervisor().tick(NOW).unwrap();

        assert!(harness.store.load().unwrap().attempts.is_empty());
        assert_eq!(harness.units.0.borrow().cleanups, [attempt]);
        assert_eq!(harness.units.0.borrow().launches.len(), 1);
        assert_eq!(harness.queue.0.borrow().waiting.len(), 1);
    }
}

#[test]
fn failed_cancellation_keeps_the_attempt_for_safe_retry() {
    let harness = Harness::new("cancel-retry");
    harness.supervisor().tick(NOW).unwrap();
    let attempt = harness.attempt();
    harness.units.0.borrow_mut().fail_cancels = 1;
    let mut settings = harness.settings.load().unwrap();
    settings.cancel_waiting(&ThreadId::new("cancel-retry"));
    harness.settings.save(&settings).unwrap();

    assert!(harness.supervisor().tick(NOW).is_err());
    assert_eq!(harness.attempt(), attempt);
    harness.supervisor().tick(NOW).unwrap();

    assert!(harness.store.load().unwrap().attempts.is_empty());
    assert_eq!(harness.units.0.borrow().cleanups, [attempt]);
}

#[test]
fn delayed_success_cannot_claim_a_newer_waiting_entry() {
    let harness = Harness::new("newer-wait");
    harness.supervisor().tick(NOW).unwrap();
    let attempt = harness.attempt();
    let newer = WaitingThread::new(ThreadId::new("newer-wait"), UnixMillis::new(NOW.get() + 1));
    harness.queue.0.borrow_mut().waiting = vec![newer.clone()];
    harness.store.record_outcome(&attempt, true).unwrap();

    harness.supervisor().tick(NOW).unwrap();

    let state = harness.store.load().unwrap();
    assert_eq!(
        state.attempts[&newer.thread_id].waiting.waiting_id,
        newer.waiting_id
    );
    assert_eq!(harness.units.0.borrow().launches.len(), 2);
}

#[test]
fn delayed_failure_preserves_and_delays_the_exact_newer_waiting_entry() {
    let harness = Harness::new("newer-failure");
    harness.supervisor().tick(NOW).unwrap();
    let attempt = harness.attempt();
    let newer = WaitingThread::new(
        ThreadId::new("newer-failure"),
        UnixMillis::new(NOW.get() + 1),
    );
    harness.queue.0.borrow_mut().waiting = vec![newer.clone()];
    harness.store.record_outcome(&attempt, false).unwrap();

    harness.supervisor().tick(NOW).unwrap();

    assert_eq!(
        harness.queue.0.borrow().waiting.as_slice(),
        std::slice::from_ref(&newer)
    );
    let state = harness.store.load().unwrap();
    assert!(state.retry_after.contains_key(&newer.waiting_id));
    assert!(state.attempts.is_empty());
    assert_eq!(harness.units.0.borrow().launches.len(), 1);
}

mod adversarial;
mod cancellation;
mod completion;
mod waiting_lifecycle;
