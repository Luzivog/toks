use crate::rotation::ThreadOverride;

use super::RouteTier;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::codex_router::proxy) struct AuthorizedRoute {
    tier: RouteTier,
    request_override: Option<ThreadOverride>,
}

impl AuthorizedRoute {
    pub(in crate::codex_router::proxy) fn new(
        tier: RouteTier,
        request_override: Option<ThreadOverride>,
    ) -> Self {
        Self {
            tier,
            request_override,
        }
    }

    pub(in crate::codex_router::proxy) fn tier(&self) -> RouteTier {
        self.tier
    }

    pub(in crate::codex_router::proxy) fn request_override(&self) -> Option<&ThreadOverride> {
        self.request_override.as_ref()
    }
}
