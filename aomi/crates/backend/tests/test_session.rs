mod utils;

use aomi_backend::session::{AomiBackend, DefaultSessionState};
use aomi_chat::SystemEvent;
use std::sync::Arc;
use utils::{flush_state, InterruptingBackend, AsyncToolBackend, SystemEventBackend};

#[tokio::test]
async fn system_tool_display_moves_into_active_events() {
    let backend: Arc<AomiBackend> = Arc::new(SystemEventBackend::with_tool_display(
        "manual_tool",
        "manual-call",
        serde_json::json!({"hello": "world"}),
    ));
    let mut state = DefaultSessionState::new(backend, Vec::new())
        .await
        .expect("session init");

    state
        .send_user_input("trigger".into())
        .await
        .expect("send user message");

    flush_state(&mut state).await;
    state.sync_state().await;

    let has_manual = state.advance_frontend_events().into_iter().any(|event| {
        if let SystemEvent::InlineDisplay(payload) = event {
            return payload.get("type").and_then(|v| v.as_str()) == Some("tool_display")
                && payload.get("tool_name") == Some(&serde_json::json!("manual_tool"))
                && payload.get("call_id") == Some(&serde_json::json!("manual-call"))
                && payload.get("result").and_then(|v| v.get("hello"))
                    == Some(&serde_json::json!("world"));
        }
        false
    });

    assert!(
        has_manual,
        "SystemToolDisplay should be surfaced in system events"
    );
}

#[tokio::test]
async fn async_tool_results_populate_system_events() {
    let backend: Arc<AomiBackend> = Arc::new(AsyncToolBackend::new());
    let mut state = DefaultSessionState::new(backend, Vec::new())
        .await
        .expect("session init");

    state
        .send_user_input("run async tool".into())
        .await
        .expect("send user message");

    flush_state(&mut state).await;
    state.sync_state().await;

    let tool_events: Vec<_> = state
        .advance_frontend_events()
        .into_iter()
        .filter_map(|event| match event {
            SystemEvent::AsyncUpdate(payload)
                if payload.get("type").and_then(|v| v.as_str()) == Some("tool_completion") =>
            {
                Some((
                    payload.get("tool_name").cloned(),
                    payload.get("id").cloned(),
                    payload.get("call_id").cloned(),
                    payload.get("sync").cloned(),
                    payload.get("result").cloned(),
                ))
            }
            _ => None,
        })
        .collect();

    assert_eq!(
        tool_events.len(),
        1,
        "expected async tool chunk(s) to be surfaced"
    );

    let (tool, id, call_id, sync, result) = &tool_events[0];
    assert_eq!(tool, &Some(serde_json::json!("multi_step_tool")));
    assert_eq!(id, &Some(serde_json::json!("multi_step_call_1")));
    assert_eq!(call_id, &Some(serde_json::Value::Null));
    assert_eq!(sync, &Some(serde_json::json!(false)));
    assert_eq!(
        result.as_ref().and_then(|v| v.get("status")),
        Some(&serde_json::json!("completed")),
        "status field should reflect completion"
    );
    assert!(
        result
            .as_ref()
            .and_then(|v| v.get("data"))
            .and_then(|v| v.as_array())
            .is_some(),
        "expected data array in result, got: {:?}",
        result
    );
}

#[tokio::test]
async fn async_tool_error_is_reported() {
    let backend: Arc<AomiBackend> = Arc::new(AsyncToolBackend::new().with_error());
    let mut state = DefaultSessionState::new(backend, Vec::new())
        .await
        .expect("session init");

    state
        .send_user_input("run async tool error".into())
        .await
        .expect("send user message");

    flush_state(&mut state).await;
    state.sync_state().await;

    let error_event = state
        .advance_frontend_events()
        .into_iter()
        .find_map(|event| match event {
            SystemEvent::AsyncUpdate(payload)
                if payload.get("type").and_then(|v| v.as_str()) == Some("tool_completion") =>
            {
                payload.get("result").and_then(|v| v.get("error")).cloned()
            }
            _ => None,
        });

    assert_eq!(
        error_event,
        Some(serde_json::json!("multi-step failed")),
        "expected error payload to surface in async update"
    );
}

#[tokio::test]
async fn interrupted_clears_streaming_and_processing_flag() {
    let backend: Arc<AomiBackend> = Arc::new(InterruptingBackend);
    let mut state = DefaultSessionState::new(backend, Vec::new())
        .await
        .expect("session init");

    state
        .send_user_input("interrupt me".into())
        .await
        .expect("send user message");

    flush_state(&mut state).await;
    state.sync_state().await;

    let any_streaming = state.messages.iter().any(|m| m.is_streaming);
    assert!(
        !any_streaming,
        "no messages should remain streaming after interrupt"
    );
    assert!(
        !state.is_processing,
        "session should not be processing after interrupt"
    );
}
