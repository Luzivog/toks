use std::collections::BTreeMap;
use std::sync::Mutex;

use crate::accounts::AccountId;
use crate::rotation::{
    ActiveTask, TaskActivityStore, ThreadId, ThreadRequestSettings, UnixMillis,
    WorkerConnectionOwner,
};

use super::Engine;

mod local;
use local::LocalTaskActivity;

pub(super) struct TaskActivityPublisher {
    store: Option<TaskActivityStore>,
    owner: Option<WorkerConnectionOwner>,
    local: Mutex<LocalTaskActivity>,
}

#[derive(Clone)]
struct Publication {
    revision: u64,
    tasks: BTreeMap<ThreadId, ActiveTask>,
}

impl TaskActivityPublisher {
    pub(super) fn new(
        owner: Option<WorkerConnectionOwner>,
        store: Option<TaskActivityStore>,
    ) -> Self {
        let publisher = Self {
            store,
            owner,
            local: Mutex::new(LocalTaskActivity::default()),
        };
        publisher.publish_current();
        publisher
    }

    pub(super) fn started(
        &self,
        account: &AccountId,
        thread: &ThreadId,
        settings: ThreadRequestSettings,
        now: UnixMillis,
    ) {
        let publication = self.change(|local| local.started(account, thread, settings, now));
        self.publish(publication);
    }

    pub(super) fn continues(&self, thread: &ThreadId) {
        let publication = self.change(|local| local.continues(thread));
        self.publish(publication);
    }

    pub(super) fn finished(&self, thread: &ThreadId) {
        let publication = self.change(|local| local.finished(thread));
        self.publish(publication);
    }

    pub(super) fn cancelled(&self, thread: &ThreadId) {
        let publication = self.change(|local| local.cancelled(thread));
        self.publish(publication);
    }

    pub(super) fn attachment_opened(&self, thread: &ThreadId) {
        self.local
            .lock()
            .expect("router task activity poisoned")
            .attachment_opened(thread);
    }

    pub(super) fn attachment_closed(&self, thread: &ThreadId) {
        let publication = self.change(|local| local.attachment_closed(thread));
        self.publish(publication);
    }

    pub(super) fn publish_current(&self) {
        let publication = self
            .local
            .lock()
            .expect("router task activity poisoned")
            .publication();
        self.publish(Some(publication));
        if let Some(owner) = self.owner {
            if owner.generation() == super::super::DIRECT_ROUTER_GENERATION {
                self.reconcile_expected_workers(&BTreeMap::from([(
                    owner.generation(),
                    owner.instance_id(),
                )]));
            }
        }
    }

    pub(super) fn reconcile_expected_workers(&self, expected: &BTreeMap<u64, u64>) -> bool {
        let Some(store) = &self.store else {
            return true;
        };
        match store.reconcile_expected_workers(expected) {
            Ok(_) => true,
            Err(error) => {
                eprintln!("router failed to reconcile task activity owners: {error:#}");
                false
            }
        }
    }

    fn change(&self, change: impl FnOnce(&mut LocalTaskActivity) -> bool) -> Option<Publication> {
        let mut local = self.local.lock().expect("router task activity poisoned");
        let before = local.visible_tasks();
        let changed = change(&mut local);
        if !changed {
            return None;
        }
        let after = local.visible_tasks();
        local.bump_if(before != after);
        Some(Publication {
            revision: local.revision,
            tasks: after,
        })
    }

    fn publish(&self, publication: Option<Publication>) {
        let (Some(store), Some(owner), Some(publication)) = (&self.store, self.owner, publication)
        else {
            return;
        };
        if let Err(error) = store.replace_worker(owner, publication.revision, publication.tasks) {
            eprintln!("router failed to publish task activity: {error:#}");
        }
    }
}

impl Engine {
    pub(in crate::codex_router::proxy) fn open_task_activity_scope(&self, thread: &ThreadId) {
        self.task_activity.attachment_opened(thread);
    }

    pub(in crate::codex_router::proxy) fn close_task_activity_scope(&self, thread: &ThreadId) {
        self.task_activity.attachment_closed(thread);
    }

    pub(in crate::codex_router::proxy) fn cancel_task(&self, thread: &ThreadId) {
        self.task_activity.cancelled(thread);
    }

    pub(crate) fn reconcile_task_activity_owners(&self, expected: &BTreeMap<u64, u64>) -> bool {
        self.task_activity.reconcile_expected_workers(expected)
    }
}
