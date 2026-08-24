use crate::accounts::AccountId;
use crate::rotation::{RotationRuntime, RotationSettings, UnixMillis, WaitingThread};

use super::{ResumeAdmission, ResumeAdmissionPhase, ResumeAuthorization};

impl RotationRuntime {
    pub(crate) fn authorize_resume(
        &mut self,
        settings: &RotationSettings,
        discovered: &[AccountId],
        waiting: &WaitingThread,
        attempt: &str,
        account: &AccountId,
        at: UnixMillis,
    ) -> ResumeAuthorization {
        if settings.cancelled_threads().contains(&waiting.thread_id) {
            return ResumeAuthorization::Cancelled;
        }
        if self
            .resume_admissions
            .get(&waiting.waiting_id)
            .is_some_and(|admission| {
                admission.attempt == attempt && admission.phase == ResumeAdmissionPhase::Active
            })
        {
            return ResumeAuthorization::Acquired;
        }
        if self
            .resume_admissions
            .values()
            .any(|admission| admission.attempt == attempt)
        {
            return ResumeAuthorization::Lost;
        }
        let Some(index) = self
            .waiting_threads
            .iter()
            .position(|current| current == waiting)
        else {
            return ResumeAuthorization::Lost;
        };
        let selected = settings.select_account_for_thread(self, discovered, &waiting.thread_id, at);
        if selected.as_ref() != Some(account) {
            return ResumeAuthorization::Stale;
        }
        self.waiting_threads.remove(index);
        self.resume_admissions.insert(
            waiting.waiting_id.clone(),
            ResumeAdmission {
                attempt: attempt.to_owned(),
                account: account.clone(),
                waiting: waiting.clone(),
                phase: ResumeAdmissionPhase::Active,
            },
        );
        ResumeAuthorization::Acquired
    }
}
