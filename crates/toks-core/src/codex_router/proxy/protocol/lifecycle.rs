use serde_json::Value;

const MAX_PENDING_EVENT_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ResponseLifecycleEnd {
    Continue,
    Finish,
}

#[derive(Debug, Default)]
pub(crate) struct ResponseLifecycle {
    pending_sse: Vec<u8>,
    needs_follow_up: bool,
    ended: bool,
}

impl ResponseLifecycle {
    pub fn reset(&mut self) {
        self.pending_sse.clear();
        self.needs_follow_up = false;
        self.ended = false;
    }

    pub fn observe_json(&mut self, payload: &[u8]) -> Option<ResponseLifecycleEnd> {
        if self.ended {
            return None;
        }
        let value: Value = serde_json::from_slice(payload).ok()?;
        match value.get("type").and_then(Value::as_str) {
            Some("response.output_item.done") if client_tool_call(&value) => {
                self.needs_follow_up = true;
                None
            }
            Some("response.completed") => {
                self.ended = true;
                let continues = self.needs_follow_up
                    || value.pointer("/response/end_turn").and_then(Value::as_bool) == Some(false);
                Some(if continues {
                    ResponseLifecycleEnd::Continue
                } else {
                    ResponseLifecycleEnd::Finish
                })
            }
            Some("response.failed" | "response.incomplete") => {
                self.ended = true;
                Some(ResponseLifecycleEnd::Finish)
            }
            _ => None,
        }
    }

    pub fn observe_sse(&mut self, chunk: &[u8]) -> Option<ResponseLifecycleEnd> {
        if self.ended {
            return None;
        }
        if self.pending_sse.len().saturating_add(chunk.len()) > MAX_PENDING_EVENT_BYTES {
            self.pending_sse.clear();
            return None;
        }
        self.pending_sse.extend_from_slice(chunk);
        let mut observed = None;
        while let Some((at, delimiter_len)) = event_boundary(&self.pending_sse) {
            let event = self.pending_sse.drain(..at).collect::<Vec<_>>();
            self.pending_sse.drain(..delimiter_len);
            if let Some(data) = event_data(&event) {
                observed = self.observe_json(&data).or(observed);
            }
        }
        observed
    }
}

fn client_tool_call(event: &Value) -> bool {
    match event.pointer("/item/type").and_then(Value::as_str) {
        Some("function_call" | "custom_tool_call") => true,
        Some("tool_search_call") => {
            event.pointer("/item/execution").and_then(Value::as_str) == Some("client")
        }
        _ => false,
    }
}

fn event_boundary(bytes: &[u8]) -> Option<(usize, usize)> {
    let newline = bytes.windows(2).position(|pair| pair == b"\n\n");
    let crlf = bytes.windows(4).position(|pair| pair == b"\r\n\r\n");
    match (newline, crlf) {
        (Some(left), Some(right)) if left < right => Some((left, 2)),
        (Some(_), Some(right)) => Some((right, 4)),
        (Some(at), None) => Some((at, 2)),
        (None, Some(at)) => Some((at, 4)),
        (None, None) => None,
    }
}

fn event_data(event: &[u8]) -> Option<Vec<u8>> {
    let mut data = Vec::new();
    for line in event.split(|byte| *byte == b'\n') {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        let Some(value) = line.strip_prefix(b"data:") else {
            continue;
        };
        let value = value.strip_prefix(b" ").unwrap_or(value);
        if !data.is_empty() {
            data.push(b'\n');
        }
        data.extend_from_slice(value);
    }
    (!data.is_empty()).then_some(data)
}

#[cfg(test)]
mod tests {
    use super::{ResponseLifecycle, ResponseLifecycleEnd};

    #[test]
    fn tool_calls_continue_the_thread_until_a_later_final_response() {
        let tool = br#"{"type":"response.output_item.done","item":{"type":"function_call"}}"#;
        let completed = br#"{"type":"response.completed","response":{}}"#;
        let mut lifecycle = ResponseLifecycle::default();
        assert_eq!(lifecycle.observe_json(tool), None);
        assert_eq!(
            lifecycle.observe_json(completed),
            Some(ResponseLifecycleEnd::Continue)
        );

        lifecycle.reset();
        assert_eq!(
            lifecycle.observe_json(completed),
            Some(ResponseLifecycleEnd::Finish)
        );
    }

    #[test]
    fn split_sse_events_preserve_follow_up_semantics() {
        let mut lifecycle = ResponseLifecycle::default();
        assert_eq!(
            lifecycle.observe_sse(
                b"event: response.output_item.done\r\ndata: {\"type\":\"response.output_item.done\",\"item\":{\"type\":\"custom_tool_call\"}}\r\n"
            ),
            None
        );
        assert_eq!(
            lifecycle.observe_sse(
                b"\r\nevent: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{}}\n\n"
            ),
            Some(ResponseLifecycleEnd::Continue)
        );
    }

    #[test]
    fn explicit_continuation_and_failures_are_terminal_signals() {
        let mut lifecycle = ResponseLifecycle::default();
        assert_eq!(
            lifecycle
                .observe_json(br#"{"type":"response.completed","response":{"end_turn":false}}"#),
            Some(ResponseLifecycleEnd::Continue)
        );
        lifecycle.reset();
        assert_eq!(
            lifecycle.observe_json(br#"{"type":"response.failed"}"#),
            Some(ResponseLifecycleEnd::Finish)
        );
    }
}
