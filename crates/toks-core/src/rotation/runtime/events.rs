use super::RotationRuntime;
use crate::rotation::{RotationEvent, RotationEventKind, UnixMillis};

const EVENT_LIMIT: usize = 100;
const INCIDENT_EVENT_RESERVE: usize = 20;

impl RotationRuntime {
    pub(super) fn push_event(&mut self, at: UnixMillis, event: RotationEventKind) {
        self.events.push_front(RotationEvent { at, event });
        self.trim_events();
    }

    pub(super) fn trim_events(&mut self) {
        while self.events.len() > EVENT_LIMIT {
            let incident_count = self
                .events
                .iter()
                .filter(|event| event.event.is_incident())
                .count();
            let oldest = self.events.len() - 1;
            let remove = if incident_count <= INCIDENT_EVENT_RESERVE {
                self.events
                    .iter()
                    .rposition(|event| !event.event.is_incident())
                    .unwrap_or(oldest)
            } else {
                oldest
            };
            self.events.remove(remove);
        }
    }
}
