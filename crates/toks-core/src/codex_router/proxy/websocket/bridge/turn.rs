use crate::rotation::ThreadId;

use super::super::super::lease::{StreamLease, ThreadAttachment};
use super::super::super::protocol::ResponseLifecycle;

pub(super) struct Turn {
    pub(super) active: bool,
    pub(super) delivered: bool,
    pub(super) thread: Option<ThreadId>,
    pub(super) attachment: Option<ThreadAttachment>,
    pub(super) lease: Option<StreamLease>,
    pub(super) forced_fast_request: Option<String>,
    pub(super) lifecycle: ResponseLifecycle,
}
