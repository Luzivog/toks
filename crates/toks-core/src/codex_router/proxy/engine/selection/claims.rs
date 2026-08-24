use std::collections::BTreeSet;

use anyhow::Result;

use crate::accounts::AccountId;
use crate::rotation::{RotationEventKind, ThreadId};

use super::super::{now, Engine, RouteTier};
use super::{policy::selected_account, RouteSelection};

impl Engine {
    pub fn attach_authorized(
        &self,
        account: &AccountId,
        thread: &ThreadId,
        resume_attempt: Option<&str>,
    ) -> Result<bool> {
        let discovered = self.credentials.account_ids();
        let at = now();
        self.settings.update(|settings| {
            settings.reconcile(&discovered);
            let attached = self.runtime.update(|runtime| {
                if !runtime.resume_route_authorized(thread, resume_attempt, account) {
                    let changed = runtime.release_reservation(account, thread);
                    return (false, changed);
                }
                let selected = selected_account(
                    settings,
                    runtime,
                    &discovered,
                    Some(thread),
                    super::super::super::headers::ResumeMarker::from_attempt(resume_attempt),
                    &BTreeSet::new(),
                    at,
                );
                if selected != RouteSelection::Selected(account.clone()) {
                    let changed = runtime.release_reservation(account, thread);
                    return (false, changed);
                }
                let attached = match self.connection_owner {
                    Some(owner) => runtime.thread_attached_by(owner, account, thread),
                    None => runtime.thread_attached(account, thread),
                };
                match attached {
                    Ok(changed) => (true, changed),
                    Err(_) => (false, false),
                }
            });
            (attached, false)
        })?
    }

    pub fn route_authorized(
        &self,
        account: &AccountId,
        thread: &ThreadId,
        resume_attempt: Option<&str>,
    ) -> Result<Option<RouteTier>> {
        let discovered = self.credentials.account_ids();
        let at = now();
        self.settings.update(|settings| {
            settings.reconcile(&discovered);
            let routed = self.runtime.update(|runtime| {
                if !runtime.resume_route_authorized(thread, resume_attempt, account) {
                    let changed = runtime.release_reservation(account, thread);
                    return (None, changed);
                }
                let selected = selected_account(
                    settings,
                    runtime,
                    &discovered,
                    Some(thread),
                    super::super::super::headers::ResumeMarker::from_attempt(resume_attempt),
                    &BTreeSet::new(),
                    at,
                );
                if selected != RouteSelection::Selected(account.clone()) {
                    let changed = runtime.release_reservation(account, thread);
                    return (None, changed);
                }
                let tier = if runtime.can_drain(account, thread, at) {
                    if runtime.requires_standard_tier(account, thread, at) {
                        RouteTier::Standard
                    } else {
                        RouteTier::Fast
                    }
                } else {
                    RouteTier::Original
                };
                let previous = runtime
                    .events()
                    .iter()
                    .find_map(|event| match &event.event {
                        RotationEventKind::Routed {
                            thread_id,
                            account_id,
                        } if thread_id == thread => Some(account_id.clone()),
                        _ => None,
                    });
                let opened = match self.connection_owner {
                    Some(owner) => runtime.connection_opened_by(owner, account, thread, at),
                    None => runtime.connection_opened(account, thread, at),
                };
                if opened.is_err() {
                    return (None, false);
                }
                if let Some(previous) = previous.filter(|previous| previous != account) {
                    runtime.rotated(thread, &previous, account, at);
                }
                runtime.resumed(thread, account, at);
                (Some(tier), true)
            });
            (routed, false)
        })?
    }

    #[cfg(test)]
    pub fn attach(&self, account: &AccountId, thread: &ThreadId) -> Result<bool> {
        self.attach_authorized(account, thread, None)
    }

    #[cfg(test)]
    pub fn route(&self, account: &AccountId, thread: &ThreadId) -> Result<Option<RouteTier>> {
        self.route_authorized(account, thread, None)
    }
}
