mod utils;

use aomi_backend::session::{BackendwithTool, DefaultSessionState};
use aomi_chat::SystemEvent;
use std::sync::Arc;
use utils::{MultiStepToolBackend, SystemEventBackend, flush_state};

#[tokio::test]
async fn system_tool_display_moves_into_active_events() {
    let backend: Arc<BackendwithTool> =
        Arc::new(SystemEventBackend::with_tool_display(
            "manual_tool",
            "manual-call",
            serde_json::json!({"hello": "world"}),
        ));
    let mut state = DefaultSessionState::new(backend, Vec::new())
        .await
        .expect("session init");

    state
        .process_user_message("trigger".into())
        .await
        .expect("send user message");

    flush_state(&mut state).await;
    state.update_state().await;

    let has_manual = state.active_system_events.iter().any(|event| {
        matches!(
            event,
            SystemEvent::SystemToolDisplay { tool_name, call_id, result }
            if tool_name == "manual_tool"
                && call_id == "manual-call"
                && result.get("hello") == Some(&serde_json::json!("world"))
        )
    });

    assert!(
        has_manual,
        "SystemToolDisplay should be surfaced in active_system_events"
    );
}

#[tokio::test]
async fn async_tool_results_populate_system_events() {
    let backend: Arc<BackendwithTool> = Arc::new(MultiStepToolBackend::new());
    let mut state = DefaultSessionState::new(backend, Vec::new())
        .await
        .expect("session init");

    state
        .process_user_message("run async tool".into())
        .await
        .expect("send user message");

    flush_state(&mut state).await;
    state.update_state().await;

    let tool_events: Vec<_> = state
        .active_system_events
        .iter()
        .filter_map(|event| match event {
            SystemEvent::SystemToolDisplay {
                tool_name,
                call_id,
                result,
            } => Some((tool_name.clone(), call_id.clone(), result.clone())),
            _ => None,
        })
        .collect();

    assert_eq!(
        tool_events.len(),
        1,
        "expected async tool chunk(s) to be surfaced"
    );

    let (tool, call_id, result) = &tool_events[0];
    assert_eq!(tool, "multi_step_tool");
    assert_eq!(call_id, "multi_step_call_1");
    assert_eq!(
        result.get("status"),
        Some(&serde_json::json!("completed")),
        "status field should reflect completion"
    );
    assert!(
        result
            .get("data")
            .and_then(|v| v.as_array())
            .is_some(),
        "expected data array in result, got: {result}"
    );
}
