//! Event callback trait and event data types for deletion pipeline events.
//!
//! Adapted from s3sync's `types/event_callback.rs`.
//! Provides structured event data for monitoring deletion operations.

use async_trait::async_trait;
use bitflags::bitflags;

bitflags! {
    /// Event type flags for filtering which events a callback receives.
    ///
    /// Adapted from s3sync's EventType bitflags, with sync-specific events
    /// replaced by deletion-specific events.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
    pub struct EventType: u64 {
        const UNDEFINED = 0u64;
        const PIPELINE_START = 1u64 << 1;
        const PIPELINE_END = 1u64 << 2;
        const DELETE_COMPLETE = 1u64 << 3;
        const DELETE_FAILED = 1u64 << 4;
        const DELETE_FILTERED = 1u64 << 5;
        const PIPELINE_ERROR = 1u64 << 6;
        const DELETE_CANCEL = 1u64 << 7;
        const STATS_REPORT = 1u64 << 8;
        const ALL_EVENTS = !0;
    }
}

/// Structured event data passed to event callbacks.
///
/// Adapted from s3sync's EventData. Simplified for deletion operations
/// by removing sync-specific fields (upload_id, checksums, etc.) and
/// adding deletion-specific fields.
#[derive(Default, Debug, Clone)]
pub struct EventData {
    pub event_type: EventType,
    pub dry_run: bool,
    pub key: Option<String>,
    pub version_id: Option<String>,
    pub size: Option<u64>,
    pub last_modified: Option<String>,
    pub e_tag: Option<String>,
    pub error_message: Option<String>,
    pub message: Option<String>,

    // Statistics fields (populated in STATS_REPORT events)
    pub stats_deleted_objects: Option<u64>,
    pub stats_deleted_bytes: Option<u64>,
    pub stats_failed_objects: Option<u64>,
    pub stats_skipped_objects: Option<u64>,
    pub stats_error_count: Option<u64>,
    pub stats_duration_sec: Option<f64>,
    pub stats_objects_per_sec: Option<f64>,
}

impl EventData {
    pub fn new(event_type: EventType) -> Self {
        Self {
            event_type,
            ..Default::default()
        }
    }
}

/// Trait for event callbacks that receive pipeline events.
///
/// Implementations receive `EventData` for monitoring, logging, or
/// custom processing of deletion events.
///
/// # Notes
///
/// - Callbacks are called serially for each event
/// - Callbacks should return promptly to avoid blocking the pipeline
/// - Both Lua scripts and Rust closures can implement this trait
#[async_trait]
pub trait EventCallback: Send {
    async fn on_event(&mut self, event_data: EventData);
}
