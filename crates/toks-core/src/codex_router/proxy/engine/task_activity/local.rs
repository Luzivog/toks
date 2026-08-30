use std::collections::BTreeMap;

use crate::accounts::AccountId;
use crate::rotation::{ActiveTask, ThreadId, ThreadRequestSettings, UnixMillis};

use super::Publication;

#[derive(Clone, Debug, PartialEq, Eq)]
struct TrackedTask {
    active: ActiveTask,
    generating: u32,
    pending_follow_ups: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct LocalTaskActivity {
    pub(super) revision: u64,
    tasks: BTreeMap<ThreadId, TrackedTask>,
    attachments: BTreeMap<ThreadId, u32>,
}

impl LocalTaskActivity {
    pub(super) fn started(
        &mut self,
        account: &AccountId,
        thread: &ThreadId,
        settings: ThreadRequestSettings,
        now: UnixMillis,
    ) -> bool {
        match self.tasks.get_mut(thread) {
            Some(task) => {
                task.active.account_id = account.clone();
                task.active.request_settings = settings;
                task.pending_follow_ups = task.pending_follow_ups.saturating_sub(1);
                task.generating = task.generating.saturating_add(1);
            }
            None => {
                self.tasks.insert(
                    thread.clone(),
                    TrackedTask {
                        active: ActiveTask {
                            account_id: account.clone(),
                            request_settings: settings,
                            started_at: now,
                        },
                        generating: 1,
                        pending_follow_ups: 0,
                    },
                );
            }
        }
        true
    }

    pub(super) fn continues(&mut self, thread: &ThreadId) -> bool {
        let Some(task) = self.tasks.get_mut(thread) else {
            return false;
        };
        if task.generating == 0 {
            return false;
        }
        task.generating -= 1;
        task.pending_follow_ups = task.pending_follow_ups.saturating_add(1);
        true
    }

    pub(super) fn finished(&mut self, thread: &ThreadId) -> bool {
        let remove = {
            let Some(task) = self.tasks.get_mut(thread) else {
                return false;
            };
            if task.generating == 0 {
                return false;
            }
            task.generating -= 1;
            task.generating == 0 && task.pending_follow_ups == 0
        };
        if remove {
            self.tasks.remove(thread);
        }
        true
    }

    pub(super) fn cancelled(&mut self, thread: &ThreadId) -> bool {
        self.tasks.remove(thread).is_some()
    }

    pub(super) fn attachment_opened(&mut self, thread: &ThreadId) {
        let count = self.attachments.entry(thread.clone()).or_default();
        *count = count.saturating_add(1);
    }

    pub(super) fn attachment_closed(&mut self, thread: &ThreadId) -> bool {
        let last = match self.attachments.get_mut(thread) {
            Some(count) if *count > 1 => {
                *count -= 1;
                false
            }
            Some(_) => {
                self.attachments.remove(thread);
                true
            }
            None => return false,
        };
        let remove = last
            && self.tasks.get_mut(thread).is_some_and(|task| {
                task.pending_follow_ups = 0;
                task.generating == 0
            });
        if remove {
            self.tasks.remove(thread);
        }
        true
    }

    pub(super) fn bump_if(&mut self, changed: bool) {
        if changed {
            self.revision = self
                .revision
                .checked_add(1)
                .expect("task activity revision exhausted");
        }
    }

    pub(super) fn publication(&self) -> Publication {
        Publication {
            revision: self.revision,
            tasks: self.visible_tasks(),
        }
    }

    pub(super) fn visible_tasks(&self) -> BTreeMap<ThreadId, ActiveTask> {
        self.tasks
            .iter()
            .map(|(thread, task)| (thread.clone(), task.active.clone()))
            .collect()
    }
}
