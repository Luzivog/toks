use crate::rotation::ThreadId;
use crate::rotation::{UsageLimitTier, UsageLimitTierOrigin};

use crate::codex_router::proxy::lease::{StreamLease, ThreadAttachment};
use crate::codex_router::proxy::protocol::{
    requested_model, requested_service_tier, ResponseLifecycle,
};

pub(super) struct Turn {
    pub(super) active: bool,
    pub(super) delivered: bool,
    pub(super) header_thread: Option<ThreadId>,
    pub(super) thread: Option<ThreadId>,
    pub(super) attachment: Option<ThreadAttachment>,
    pub(super) lease: Option<StreamLease>,
    pub(super) forced_fast_request: Option<String>,
    pub(super) model: Option<String>,
    pub(super) request_tier: UsageLimitTier,
    pub(super) lifecycle: ResponseLifecycle,
    pub(super) resume_attempt: Option<String>,
}

impl Turn {
    pub(super) fn begin_request(&mut self, payload: &str, origin: UsageLimitTierOrigin) {
        self.model = requested_model(payload);
        let service_tier = requested_service_tier(payload);
        self.request_tier = UsageLimitTier::new(service_tier.as_deref(), origin);
    }
}
