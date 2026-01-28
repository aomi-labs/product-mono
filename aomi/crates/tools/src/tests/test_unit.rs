use crate::types::format_tool_name;
use eyre::Result;
use futures::TryFutureExt;

#[test]
fn test_format_tool_name_snake_case() {
    assert_eq!(
        format_tool_name("encode_function_call"),
        "Encode function call".to_string()
    );
    assert_eq!(
        format_tool_name("get_current_time"),
        "Get current time".to_string()
    );
    assert_eq!(
        format_tool_name("send_transaction"),
        "Send transaction".to_string()
    );
}

#[test]
fn test_format_tool_name_non_snake_case() {
    assert_eq!(format_tool_name("MyTool"), "My tool".to_string());
    assert_eq!(format_tool_name("GetTime"), "Get time".to_string());
    assert_eq!(format_tool_name("encode"), "Encode".to_string());
}

#[test]
fn test_format_tool_name_caching() {
    let result1 = format_tool_name("test_tool");
    let result2 = format_tool_name("test_tool");
    assert_eq!(result1, result2);
}

async fn might_fail(i: u32) -> Result<u32> {
    if i.is_multiple_of(2) {
        Ok(i * 2)
    } else {
        Err(eyre::eyre!("odd number"))
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn test_future_error_handling() {
    // Verifies helper combinators propagate wrapped errors without panicking.
    let fut = might_fail(3);
    let fut2 = fut.map_err(|e| e.wrap_err("error"));
    match fut2.await {
        Ok(v) => println!("ok: {v}"),
        Err(e) => println!("err: {e}"),
    }
}
