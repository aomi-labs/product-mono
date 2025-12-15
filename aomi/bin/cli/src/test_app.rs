#![cfg(test)]

use aomi_chat::{ChatAppBuilder, SystemEvent, SystemEventQueue};
use aomi_tools::test_utils::{
    MockMultiStepTool, MockSingleTool, register_mock_multi_step_tool,
};
use eyre::Result;
use futures::StreamExt;
use rig::tool::Tool;
use serde_json::{Value, json};

/// Build a ChatAppBuilder and exercise tool scheduling paths (single + multi-step),
/// plus inline/async system event fan-out.
#[tokio::test(flavor = "multi_thread")]
async fn test_app_builder_covers_tool_and_system_paths() -> Result<()> {
    let system_events = SystemEventQueue::new();
    let mut builder = ChatAppBuilder::new_for_tests(Some(&system_events)).await?;

    // Register tools across both single and multi-step paths using shared test mocks.
    builder.add_tool(MockSingleTool)?;
    let scheduler = builder.scheduler_for_tests();
    register_mock_multi_step_tool(
        &scheduler,
        Some(MockMultiStepTool::default().with_error_at(2)),
    );

    // Single tool should round-trip via oneshot channel.
    let mut handler = scheduler.get_handler();
    let call_id = "single_1".to_string();
    let payload = json!({ "input": "hello" });
    handler
        .request(MockSingleTool::NAME.to_string(), payload, call_id.clone())
        .await;
    let (_internal, mut ui_stream) = handler
        .take_last_call_as_streams()
        .expect("stream for single tool");
    let (_id, value) = ui_stream
        .next()
        .await
        .expect("single tool yields");
    let value = value.map_err(|e| eyre::eyre!(e))?;
    let parsed: Value = serde_json::from_str(value.as_str().unwrap())?;
    assert_eq!(parsed.get("result").and_then(Value::as_str), Some("single"));

    // Multi-step tool: first chunk surfaces via UI stream, remaining via handler poll.
    let mut handler = scheduler.get_handler();
    handler
        .request(
            "mock_multi_step".to_string(),
            json!({ "input": "world" }),
            "multi_1".to_string(),
        )
        .await;
    let (internal_stream, mut ui_stream) = handler
        .take_last_call_as_streams()
        .expect("stream for multi tool");
    handler.add_ongoing_stream(internal_stream);

    let (chunk_call_id, first_result) = ui_stream
        .next()
        .await
        .expect("first chunk");
    assert_eq!(chunk_call_id, "multi_1");
    let first_chunk = first_result.map_err(|e| eyre::eyre!(e))?;
    assert_eq!(
        first_chunk.get("step").and_then(Value::as_i64),
        Some(1)
    );

    // Collect remaining chunks via poll_streams_to_next_result
    let mut results = Vec::new();
    while let Some(completion) = handler.poll_streams_to_next_result().await {
        results.push(completion.result);
    }
    assert_eq!(results.len(), 3, "fanout should include first chunk plus remaining");
    assert_eq!(
        results[0]
            .as_ref()
            .ok()
            .and_then(|v| v.get("step"))
            .and_then(Value::as_i64),
        Some(1)
    );
    assert_eq!(
        results[1]
            .as_ref()
            .ok()
            .and_then(|v| v.get("step"))
            .and_then(Value::as_i64),
        Some(2)
    );
    assert!(
        results[2].is_err(),
        "final chunk should surface the stream error"
    );

    // Exercise system event fan-out (inline + async).
    system_events.push(SystemEvent::InlineDisplay(json!({"type": "test_inline"})));
    system_events.push(SystemEvent::AsyncUpdate(json!({"type": "async_update"})));
    let inline = system_events.slice_from(0);
    assert!(
        inline.iter().any(|e| matches!(e, SystemEvent::InlineDisplay(_))),
        "inline event surfaced"
    );
    assert!(
        inline.iter().any(|e| matches!(e, SystemEvent::AsyncUpdate(_))),
        "async update surfaced"
    );

    Ok(())
}
