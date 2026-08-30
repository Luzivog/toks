use super::lifecycle::{ResponseLifecycle, ResponseLifecycleEnd};

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
    assert_eq!(lifecycle.observe_json(completed), None);

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
        ).end,
        None,
    );
    assert_eq!(
        lifecycle.observe_sse(
            b"\r\nevent: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{}}\n\n"
        ).end,
        Some(ResponseLifecycleEnd::Continue)
    );
}

#[test]
fn explicit_continuation_and_failures_are_terminal_signals() {
    let mut lifecycle = ResponseLifecycle::default();
    assert_eq!(
        lifecycle.observe_json(br#"{"type":"response.completed","response":{"end_turn":false}}"#),
        Some(ResponseLifecycleEnd::Continue)
    );
    lifecycle.reset();
    assert_eq!(
        lifecycle.observe_json(br#"{"type":"response.failed"}"#),
        Some(ResponseLifecycleEnd::Finish)
    );
}

#[test]
fn known_error_events_finish_even_after_a_client_tool_call() {
    let tool = br#"{"type":"response.output_item.done","item":{"type":"function_call"}}"#;
    let failures: [&[u8]; 4] = [
        br#"{"type":"error","error":{"message":"failed"}}"#,
        br#"{"type":"turn.failed","error":{"message":"failed"}}"#,
        br#"{"type":"stream.error","error":{"message":"failed"}}"#,
        br#"{"type":"stream_error","error":{"message":"failed"}}"#,
    ];

    for failure in failures {
        let mut lifecycle = ResponseLifecycle::default();
        assert_eq!(lifecycle.observe_json(tool), None);
        assert_eq!(
            lifecycle.observe_json(failure),
            Some(ResponseLifecycleEnd::Finish)
        );
        assert_eq!(lifecycle.observe_json(failure), None);
    }
}

#[test]
fn split_sse_usage_failures_are_classified_only_after_the_event_is_complete() {
    let mut lifecycle = ResponseLifecycle::default();
    let first = lifecycle.observe_sse(
        b"data: {\"type\":\"turn.failed\",\"error\":{\"message\":\"You've hit your usage",
    );
    assert!(first.usage.is_none());

    let second = lifecycle.observe_sse(b" limit.\"}}\n\n");
    assert!(second.usage.is_some());
}
