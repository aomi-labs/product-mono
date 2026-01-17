// Allow manual_async_fn for trait methods using `impl Future` pattern
#![allow(clippy::manual_async_fn)]

pub mod app;
pub mod tools;

pub use app::{ForgeApp, ForgeCommand};
