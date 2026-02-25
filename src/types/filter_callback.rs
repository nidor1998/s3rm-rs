//! Filter callback trait for user-defined object filtering.
//!
//! Reused from s3sync's `types/filter_callback.rs`.
//! Defines the interface for both Lua and Rust filter callbacks.

use anyhow::Result;
use async_trait::async_trait;

use crate::types::S3Object;

/// Trait for filter callbacks that determine whether an object should be deleted.
///
/// Implementations receive an `S3Object` and return:
/// - `Ok(true)` — the object should be deleted (passes the filter)
/// - `Ok(false)` — the object should be skipped (filtered out)
/// - `Err(_)` — an error occurred; the pipeline should be cancelled
///
/// # Notes
///
/// - Callbacks are called serially for each object in the pipeline
/// - Callbacks should return promptly to avoid blocking the pipeline
/// - Both Lua scripts and Rust closures can implement this trait
#[async_trait]
pub trait FilterCallback: Send {
    async fn filter(&mut self, object: &S3Object) -> Result<bool>;
}
