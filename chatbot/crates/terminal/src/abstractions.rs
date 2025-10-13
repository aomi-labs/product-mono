use std::{
    fmt,
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use baml_client::BamlClient;

/// Abstraction for a stateless tool that can be executed by the agent.
pub trait AomiTool {
    type Output: Send + 'static;

    fn name(&self) -> String;
    fn description(&self) -> String;
    fn execute(&self, input: String) -> ToolResult<Self::Output>;
}


/// Future-aware wrapper for tool execution results.
pub struct ToolResult<T> {
    inner: Pin<Box<dyn Future<Output = T> + Send>>,
}

impl<T> ToolResult<T> {
    pub fn new<F>(future: F) -> Self
    where
        F: Future<Output = T> + Send + 'static,
    {
        Self { inner: Box::pin(future) }
    }
}

impl<T> Future for ToolResult<T> {
    type Output = T;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.inner.as_mut().poll(cx)
    }
}

impl<T> fmt::Debug for ToolResult<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ToolResult").finish_non_exhaustive()
    }
}



pub trait AomiApp: Clone + Default {
    type Input;
    type Output;
    type State: Clone;

    fn process(&self, input: Self::Input, state: Self::State, baml_client: BamlClient) -> Self::State;
    fn complete(&self, state: Self::State, baml_client: BamlClient) -> Self::Output;
}

pub trait AomiToolBox {}