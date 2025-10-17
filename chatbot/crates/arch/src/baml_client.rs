use futures::stream::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio_stream::wrappers::IntervalStream;
use uuid::Uuid;

use crate::{ContractRequestParams, WeatherRequestParams};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamChunk {
    pub id: String,
    pub content: String,
    pub finished: bool,
}

pub struct StreamingResponse {
    stream: Pin<Box<dyn Stream<Item = StreamChunk> + Send>>,
}

impl StreamingResponse {
    pub async fn next_chunk(&mut self) -> Option<StreamChunk> {
        self.stream.next().await
    }
}

impl Stream for StreamingResponse {
    type Item = StreamChunk;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.stream.as_mut().poll_next(cx)
    }
}

pub struct BamlClient {
    request_id: String,
}

impl BamlClient {
    pub fn new() -> Self {
        Self {
            request_id: Uuid::new_v4().to_string(),
        }
    }

    pub async fn stream_completion(
        &self,
        current_prompt: String,
        chat_history: Vec<ChatMessage>,
    ) -> Result<StreamingResponse, anyhow::Error> {
        // Simulate network delay
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Create fake streaming response
        let stream = Self::create_fake_stream(current_prompt, chat_history);
        
        Ok(StreamingResponse {
            stream: Box::pin(stream),
        })
    }

    fn create_fake_stream(
        prompt: String,
        _history: Vec<ChatMessage>,
    ) -> impl Stream<Item = StreamChunk> + Send + 'static {
        // Simulate streaming response with chunks
        let response_text = format!("This is a fake LLM response to: {}", prompt);
        let chunks: Vec<String> = response_text
            .chars()
            .collect::<Vec<_>>()
            .chunks(3)
            .map(|chunk| chunk.iter().collect())
            .collect();

        let total_chunks = chunks.len();
        
        IntervalStream::new(tokio::time::interval(tokio::time::Duration::from_millis(50)))
            .enumerate()
            .take(total_chunks + 1)
            .map(move |(i, _)| {
                if i < total_chunks {
                    StreamChunk {
                        id: format!("chunk_{}", i),
                        content: chunks[i].clone(),
                        finished: false,
                    }
                } else {
                    StreamChunk {
                        id: "final".to_string(),
                        content: "".to_string(),
                        finished: true,
                    }
                }
            })
    }

    fn generate_fake_contract_api_parameters(&self) -> ContractRequestParams {
        ContractRequestParams {
            address: "0x1234567890".to_string(),
            block_number: 1234567890,
        }
    }

    fn generate_fake_weather_api_parameters(&self) -> WeatherRequestParams {
        WeatherRequestParams {
            city: "Tokyo".to_string(),
            country: "Japan".to_string(),
        }
    }
    
}

impl Default for BamlClient {
    fn default() -> Self {
        Self::new()
    }
}


#[tokio::test]
async fn test_stream_completion() {
    let client = BamlClient::new();
    let current_prompt = "What is the weather in Tokyo?";
    let chat_history = vec![];
    let stream = client.stream_completion(current_prompt.to_string(), chat_history);
    let mut stream = stream.await.unwrap();
    let chunk = stream.next_chunk().await.unwrap();

    assert_eq!(chunk.id, "chunk_0");
}