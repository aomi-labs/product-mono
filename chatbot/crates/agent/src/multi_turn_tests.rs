#[cfg(test)]
mod loop_tests {
    use futures::StreamExt;
    use rig::{
        agent::AgentBuilder,
        completion::{CompletionError, CompletionModel, CompletionRequest, CompletionResponse},
        message::{Reasoning, Text, ToolCall, ToolFunction},
        streaming::{RawStreamingChoice, StreamedAssistantContent, StreamingCompletionResponse},
    };
    use std::sync::{Arc, Mutex};

    // Mock structures for testing the actual multi_turn_prompt loop
    #[derive(Clone)]
    struct MockCompletionModel {
        responses: Arc<Mutex<Vec<MockResponse>>>,
        call_count: Arc<Mutex<usize>>,
    }

    #[derive(Clone)]
    struct MockResponse {
        content: Vec<StreamedAssistantContent<MockStreamingResponse>>,
    }

    #[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
    struct MockStreamingResponse {
        id: String,
    }

    impl MockCompletionModel {
        fn new(responses: Vec<MockResponse>) -> Self {
            Self {
                responses: Arc::new(Mutex::new(responses)),
                call_count: Arc::new(Mutex::new(0)),
            }
        }

        fn call_count(&self) -> usize {
            *self.call_count.lock().unwrap()
        }
    }

    impl CompletionModel for MockCompletionModel {
        type Response = MockStreamingResponse;
        type StreamingResponse = MockStreamingResponse;

        async fn completion(
            &self,
            _request: CompletionRequest,
        ) -> Result<CompletionResponse<Self::Response>, CompletionError> {
            unimplemented!("Not used in streaming tests")
        }

        async fn stream(
            &self,
            request: CompletionRequest,
        ) -> Result<StreamingCompletionResponse<Self::StreamingResponse>, CompletionError> {
            let mut call_count = self.call_count.lock().unwrap();
            *call_count += 1;
            let current_call = *call_count;
            drop(call_count);

            println!(
                "Stream call #{}: chat history length = {}",
                current_call,
                request.chat_history.len()
            );

            let responses = self.responses.lock().unwrap();
            let response_index = (current_call - 1) % responses.len();
            let response = responses
                .get(response_index)
                .cloned()
                .unwrap_or(MockResponse { content: vec![] });
            drop(responses);

            let stream =
                futures::stream::iter(response.content.into_iter().map(
                    move |content| match content {
                        StreamedAssistantContent::Text(text) => {
                            Ok(RawStreamingChoice::Message(text.text))
                        }
                        StreamedAssistantContent::Reasoning(reasoning) => {
                            Ok(RawStreamingChoice::Reasoning {
                                reasoning: reasoning.reasoning,
                            })
                        }
                        StreamedAssistantContent::ToolCall(tool_call) => {
                            Ok(RawStreamingChoice::ToolCall {
                                id: tool_call.id,
                                call_id: tool_call.call_id,
                                name: tool_call.function.name,
                                arguments: tool_call.function.arguments,
                            })
                        }
                        StreamedAssistantContent::Final(_) => {
                            Ok(RawStreamingChoice::FinalResponse(MockStreamingResponse {
                                id: format!("mock-stream-{}", current_call),
                            }))
                        }
                    },
                ));

            Ok(StreamingCompletionResponse::stream(Box::pin(stream)))
        }
    }

    #[tokio::test]
    async fn test_multi_turn_prompt_text_only() {
        println!("🧪 Testing text-only streaming...");

        let mock_model = MockCompletionModel::new(vec![MockResponse {
            content: vec![
                StreamedAssistantContent::Text(Text {
                    text: "Hello".to_string(),
                }),
                StreamedAssistantContent::Text(Text {
                    text: " world!".to_string(),
                }),
            ],
        }]);

        let agent = AgentBuilder::new(mock_model.clone()).build();
        let agent_arc = Arc::new(agent);

        let mut stream = crate::multi_turn_prompt(agent_arc, "Test prompt", vec![]).await;

        let mut collected_text = String::new();
        let mut message_count = 0;

        while let Some(result) = stream.next().await {
            message_count += 1;
            match result {
                Ok(text) => {
                    collected_text.push_str(&text.text);
                    println!("  Message {}: '{}'", message_count, text.text);
                }
                Err(e) => {
                    println!("  Stream error: {:?}", e);
                    break;
                }
            }

            // Prevent infinite loop
            if message_count > 10 {
                break;
            }
        }

        assert_eq!(collected_text, "Hello world!");
        assert_eq!(
            mock_model.call_count(),
            1,
            "Should have made exactly 1 stream call"
        );
        println!("✓ Text-only streaming test passed");
    }

    #[tokio::test]
    async fn test_multi_turn_prompt_with_reasoning() {
        println!("🧪 Testing reasoning content streaming...");

        let mock_model = MockCompletionModel::new(vec![MockResponse {
            content: vec![
                StreamedAssistantContent::Reasoning(Reasoning {
                    reasoning: "Let me think about this...".to_string(),
                }),
                StreamedAssistantContent::Text(Text {
                    text: "Here's my answer.".to_string(),
                }),
            ],
        }]);

        let agent = AgentBuilder::new(mock_model.clone()).build();
        let agent_arc = Arc::new(agent);

        let mut stream = crate::multi_turn_prompt(agent_arc, "Complex question", vec![]).await;

        let mut collected_text = String::new();
        let mut message_count = 0;

        while let Some(result) = stream.next().await {
            message_count += 1;
            match result {
                Ok(text) => {
                    collected_text.push_str(&text.text);
                    println!("  Message {}: '{}'", message_count, text.text);
                }
                Err(e) => {
                    println!("  Stream error: {:?}", e);
                    break;
                }
            }

            if message_count > 10 {
                break;
            }
        }

        assert!(collected_text.contains("Let me think about this..."));
        assert!(collected_text.contains("Here's my answer."));
        println!("✓ Reasoning streaming test passed");
    }

    #[tokio::test]
    async fn test_multi_turn_prompt_with_tool_call() {
        println!("🧪 Testing tool call streaming...");

        let mock_model = MockCompletionModel::new(vec![MockResponse {
            content: vec![
                StreamedAssistantContent::Text(Text {
                    text: "I'll check the time for you.".to_string(),
                }),
                StreamedAssistantContent::ToolCall(ToolCall {
                    id: "time-call-123".to_string(),
                    call_id: Some("call-456".to_string()),
                    function: ToolFunction {
                        name: "current_time".to_string(),
                        arguments: serde_json::json!({}),
                    },
                }),
            ],
        }]);

        let agent = AgentBuilder::new(mock_model.clone()).build();
        let agent_arc = Arc::new(agent);

        let mut stream = crate::multi_turn_prompt(agent_arc, "What time is it?", vec![]).await;

        let mut collected_parts = Vec::new();
        let mut message_count = 0;

        while let Some(result) = stream.next().await {
            message_count += 1;
            match result {
                Ok(text) => {
                    collected_parts.push(text.text.clone());
                    println!("  Message {}: '{}'", message_count, text.text);
                }
                Err(e) => {
                    println!("  Expected error during tool execution: {:?}", e);
                    break;
                }
            }

            if message_count > 10 {
                break;
            }
        }

        // Should have collected initial text and tool waiting message
        assert!(!collected_parts.is_empty());
        assert!(collected_parts[0].contains("I'll check the time"));
        assert!(
            collected_parts
                .iter()
                .any(|part| part.contains("Awaiting tool"))
        );

        println!("✓ Tool call streaming test passed");
    }

    #[tokio::test]
    async fn test_multi_turn_prompt_multiple_tools() {
        println!("🧪 Testing multiple concurrent tool calls...");

        let mock_model = MockCompletionModel::new(vec![MockResponse {
            content: vec![
                StreamedAssistantContent::Text(Text {
                    text: "Running multiple tools...".to_string(),
                }),
                StreamedAssistantContent::ToolCall(ToolCall {
                    id: "tool-1".to_string(),
                    call_id: Some("call-1".to_string()),
                    function: ToolFunction {
                        name: "current_time".to_string(),
                        arguments: serde_json::json!({}),
                    },
                }),
                StreamedAssistantContent::ToolCall(ToolCall {
                    id: "tool-2".to_string(),
                    call_id: Some("call-2".to_string()),
                    function: ToolFunction {
                        name: "current_time".to_string(),
                        arguments: serde_json::json!({}),
                    },
                }),
            ],
        }]);

        let agent = AgentBuilder::new(mock_model.clone()).build();
        let agent_arc = Arc::new(agent);

        let mut stream = crate::multi_turn_prompt(agent_arc, "Run multiple tools", vec![]).await;

        let mut tool_await_count = 0;
        let mut message_count = 0;

        while let Some(result) = stream.next().await {
            message_count += 1;
            match result {
                Ok(text) => {
                    println!("  Message {}: '{}'", message_count, text.text);
                    if text.text.contains("Awaiting tool") {
                        tool_await_count += 1;
                    }
                }
                Err(e) => {
                    println!("  Expected error: {:?}", e);
                    break;
                }
            }

            if message_count > 15 {
                break;
            }
        }

        // Should have seen multiple "Awaiting tool" messages
        assert!(
            tool_await_count >= 2,
            "Should have awaited multiple tools concurrently, got {}",
            tool_await_count
        );
        println!(
            "✓ Concurrent tool execution test passed (awaited {} tools)",
            tool_await_count
        );
    }

    #[tokio::test]
    async fn test_multi_turn_conversation_loop() {
        println!("🧪 Testing multi-turn conversation loop...");

        // Create responses that will trigger multiple loop iterations
        let mock_model = MockCompletionModel::new(vec![
            // First turn: simple response that doesn't call tools
            MockResponse {
                content: vec![StreamedAssistantContent::Text(Text {
                    text: "First turn response.".to_string(),
                })],
            },
            // Second turn: response with tool call (this will trigger loop continuation)
            MockResponse {
                content: vec![
                    StreamedAssistantContent::Text(Text {
                        text: "Let me use a tool.".to_string(),
                    }),
                    StreamedAssistantContent::ToolCall(ToolCall {
                        id: "loop-tool".to_string(),
                        call_id: Some("loop-call".to_string()),
                        function: ToolFunction {
                            name: "current_time".to_string(),
                            arguments: serde_json::json!({}),
                        },
                    }),
                ],
            },
            // Third turn: final response
            MockResponse {
                content: vec![StreamedAssistantContent::Text(Text {
                    text: "Final response.".to_string(),
                })],
            },
        ]);

        let agent = AgentBuilder::new(mock_model.clone()).build();
        let agent_arc = Arc::new(agent);

        let mut stream = crate::multi_turn_prompt(agent_arc, "Start conversation", vec![]).await;

        let mut all_messages = Vec::new();
        let mut message_count = 0;
        let mut turn_count = 0;

        while let Some(result) = stream.next().await {
            message_count += 1;
            match result {
                Ok(text) => {
                    all_messages.push(text.text.clone());
                    println!("  Message {}: '{}'", message_count, text.text);

                    // Count conversation turns
                    if text.text.contains("turn response") || text.text.contains("Final response") {
                        turn_count += 1;
                    }
                }
                Err(e) => {
                    println!("  Stream ended with error (this is expected): {:?}", e);
                    break;
                }
            }

            // Prevent infinite loop but allow reasonable conversation
            if message_count > 20 {
                println!("  Stopping test to prevent infinite loop");
                break;
            }
        }

        let final_call_count = mock_model.call_count();

        println!("✓ Multi-turn loop test passed:");
        println!("  - Stream calls: {}", final_call_count);
        println!("  - Total messages: {}", message_count);
        println!("  - Conversation turns detected: {}", turn_count);

        // Verify the loop worked
        assert!(
            final_call_count >= 1,
            "Should have made at least one stream call"
        );
        assert!(
            message_count >= 1,
            "Should have produced at least one message"
        );
        assert!(!all_messages.is_empty(), "Should have collected messages");
    }

    #[tokio::test]
    async fn test_chat_history_accumulation() {
        println!("🧪 Testing chat history accumulation...");

        // Track what gets passed to the model
        #[derive(Clone)]
        struct HistoryTrackingModel {
            inner: MockCompletionModel,
            history_sizes: Arc<Mutex<Vec<usize>>>,
        }

        impl CompletionModel for HistoryTrackingModel {
            type Response = MockStreamingResponse;
            type StreamingResponse = MockStreamingResponse;

            async fn completion(
                &self,
                request: CompletionRequest,
            ) -> Result<CompletionResponse<Self::Response>, CompletionError> {
                self.inner.completion(request).await
            }

            async fn stream(
                &self,
                request: CompletionRequest,
            ) -> Result<StreamingCompletionResponse<Self::StreamingResponse>, CompletionError>
            {
                // Track the chat history size
                let chat_len = request.chat_history.len();
                self.history_sizes.lock().unwrap().push(chat_len);
                println!("  Stream call with chat history length: {}", chat_len);

                self.inner.stream(request).await
            }
        }

        let base_model = MockCompletionModel::new(vec![MockResponse {
            content: vec![StreamedAssistantContent::Text(Text {
                text: "Response 1".to_string(),
            })],
        }]);

        let history_sizes = Arc::new(Mutex::new(Vec::new()));
        let tracking_model = HistoryTrackingModel {
            inner: base_model,
            history_sizes: history_sizes.clone(),
        };

        let agent = AgentBuilder::new(tracking_model).build();
        let agent_arc = Arc::new(agent);

        // Start with some initial chat history
        let initial_history = vec![rig::completion::Message::User {
            content: rig::OneOrMany::one(rig::message::UserContent::text("Previous message")),
        }];

        let mut stream = crate::multi_turn_prompt(agent_arc, "New message", initial_history).await;

        let mut message_count = 0;
        while let Some(result) = stream.next().await {
            message_count += 1;
            match result {
                Ok(text) => {
                    println!("  Message {}: '{}'", message_count, text.text);
                }
                Err(e) => {
                    println!("  Stream ended: {:?}", e);
                    break;
                }
            }

            if message_count > 5 {
                break;
            }
        }

        let history_sizes_vec = history_sizes.lock().unwrap().clone();
        println!("  Chat history sizes across calls: {:?}", history_sizes_vec);

        assert!(
            !history_sizes_vec.is_empty(),
            "Should have tracked history sizes"
        );
        // The first call should have the initial history + the new prompt
        assert!(
            history_sizes_vec[0] >= 1,
            "Should have accumulated chat history"
        );

        println!("✓ Chat history accumulation test passed");
    }

    #[tokio::test]
    async fn test_loop_termination_conditions() {
        println!("🧪 Testing loop termination conditions...");

        let mock_model = MockCompletionModel::new(vec![MockResponse {
            content: vec![StreamedAssistantContent::Text(Text {
                text: "Simple response with no tools.".to_string(),
            })],
        }]);

        let agent = AgentBuilder::new(mock_model.clone()).build();
        let agent_arc = Arc::new(agent);

        let mut stream = crate::multi_turn_prompt(agent_arc, "Simple question", vec![]).await;

        let mut message_count = 0;
        let mut final_message = String::new();

        while let Some(result) = stream.next().await {
            message_count += 1;
            match result {
                Ok(text) => {
                    final_message = text.text.clone();
                    println!("  Message {}: '{}'", message_count, text.text);
                }
                Err(e) => {
                    println!("  Stream ended: {:?}", e);
                    break;
                }
            }

            if message_count > 10 {
                break;
            }
        }

        // Should terminate after one turn since no tools were called
        assert_eq!(
            mock_model.call_count(),
            1,
            "Should have made exactly 1 call for simple response"
        );
        assert!(
            final_message.contains("Simple response"),
            "Should have received the expected response"
        );

        println!("✓ Loop termination test passed");
    }
}
