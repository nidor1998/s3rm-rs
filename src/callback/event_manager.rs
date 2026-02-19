//! Event callback manager.
//!
//! Adapted from s3sync's `callback/event_manager.rs`.
//! Manages event callback registration and dispatching for the deletion pipeline.

use std::fmt;
use std::sync::Arc;

use tokio::sync::Mutex;
use tokio::time::Instant;

use crate::types::event_callback::{EventCallback, EventData, EventType};

/// Accumulated pipeline statistics for the STATS_REPORT event.
#[derive(Default, Debug, Clone)]
pub struct PipelineStats {
    pub pipeline_start_time: Option<Instant>,
    pub stats_deleted_objects: u64,
    pub stats_deleted_bytes: u64,
    pub stats_failed_objects: u64,
    pub stats_skipped_objects: u64,
    pub stats_warning_count: u64,
    pub stats_error_count: u64,
    pub stats_duration_sec: f64,
    pub stats_objects_per_sec: f64,
}

impl From<PipelineStats> for EventData {
    fn from(stats: PipelineStats) -> Self {
        let mut event_data = EventData::new(EventType::STATS_REPORT);
        event_data.stats_deleted_objects = Some(stats.stats_deleted_objects);
        event_data.stats_deleted_bytes = Some(stats.stats_deleted_bytes);
        event_data.stats_failed_objects = Some(stats.stats_failed_objects);
        event_data.stats_skipped_objects = Some(stats.stats_skipped_objects);
        event_data.stats_warning_count = Some(stats.stats_warning_count);
        event_data.stats_error_count = Some(stats.stats_error_count);
        event_data.stats_duration_sec = Some(stats.stats_duration_sec);
        event_data.stats_objects_per_sec = Some(stats.stats_objects_per_sec);
        event_data
    }
}

/// Manages event callback registration and dispatching.
///
/// Holds an optional `EventCallback` trait object and accumulated pipeline statistics.
/// On `PIPELINE_END`, sends a `STATS_REPORT` event with accumulated stats if the caller
/// subscribed to `STATS_REPORT` via `event_flags`.
#[derive(Clone)]
pub struct EventManager {
    pub event_callback: Option<Arc<Mutex<Box<dyn EventCallback + Send + Sync>>>>,
    pub event_flags: EventType,
    pub dry_run: bool,
    pub pipeline_stats: Arc<Mutex<PipelineStats>>,
}

impl Default for EventManager {
    fn default() -> Self {
        Self::new()
    }
}

impl EventManager {
    pub fn new() -> Self {
        Self {
            event_callback: None,
            event_flags: EventType::ALL_EVENTS,
            dry_run: false,
            pipeline_stats: Arc::new(Mutex::new(PipelineStats::default())),
        }
    }

    /// Register an event callback with event type filter and dry-run flag.
    pub fn register_callback<T: EventCallback + Send + Sync + 'static>(
        &mut self,
        events_flag: EventType,
        callback: T,
        dry_run: bool,
    ) {
        self.event_callback = Some(Arc::new(Mutex::new(Box::new(callback))));
        self.event_flags = events_flag;
        self.dry_run = dry_run;
    }

    /// Returns true if an event callback has been registered.
    pub fn is_callback_registered(&self) -> bool {
        self.event_callback.is_some()
    }

    /// Trigger an event, updating internal stats and dispatching to the callback.
    pub async fn trigger_event(&self, mut event_data: EventData) {
        self.update_pipeline_stats(&event_data).await;

        if let Some(callback) = &self.event_callback {
            let event_type = event_data.event_type;
            if self.event_flags.contains(event_type) {
                event_data.dry_run = self.dry_run;
                callback.lock().await.on_event(event_data).await;
            }
            // On PIPELINE_END, send accumulated stats report if caller subscribed to it
            if event_type == EventType::PIPELINE_END
                && self.event_flags.contains(EventType::STATS_REPORT)
            {
                let stats = self.pipeline_stats.lock().await.clone();
                let mut stats_event: EventData = stats.into();
                stats_event.dry_run = self.dry_run;
                callback.lock().await.on_event(stats_event).await;
            }
        }
    }

    async fn update_pipeline_stats(&self, event_data: &EventData) {
        let mut stats = self.pipeline_stats.lock().await;

        match event_data.event_type {
            EventType::PIPELINE_START => {
                stats.pipeline_start_time = Some(Instant::now());
            }
            EventType::PIPELINE_END => {
                if let Some(start) = stats.pipeline_start_time {
                    stats.stats_duration_sec = start.elapsed().as_secs_f64();
                    if stats.stats_duration_sec > 1.0 {
                        stats.stats_objects_per_sec =
                            stats.stats_deleted_objects as f64 / stats.stats_duration_sec;
                    } else {
                        stats.stats_objects_per_sec = stats.stats_deleted_objects as f64;
                    }
                }
            }
            EventType::DELETE_COMPLETE => {
                stats.stats_deleted_objects += 1;
                if let Some(size) = event_data.size {
                    stats.stats_deleted_bytes += size;
                }
            }
            EventType::DELETE_FAILED => {
                stats.stats_failed_objects += 1;
            }
            EventType::DELETE_FILTERED => {
                stats.stats_skipped_objects += 1;
            }
            EventType::DELETE_WARNING => {
                stats.stats_warning_count += 1;
            }
            EventType::PIPELINE_ERROR => {
                stats.stats_error_count += 1;
            }
            _ => {}
        }
    }
}

impl fmt::Debug for EventManager {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EventManager")
            .field("event_flags", &self.event_flags)
            .field("callback_registered", &self.event_callback.is_some())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    struct CollectingCallback {
        events: Arc<Mutex<Vec<EventData>>>,
    }

    impl CollectingCallback {
        fn new() -> (Self, Arc<Mutex<Vec<EventData>>>) {
            let events = Arc::new(Mutex::new(Vec::new()));
            (
                Self {
                    events: events.clone(),
                },
                events,
            )
        }
    }

    #[async_trait]
    impl EventCallback for CollectingCallback {
        async fn on_event(&mut self, event_data: EventData) {
            self.events.lock().await.push(event_data);
        }
    }

    #[tokio::test]
    async fn new_manager_has_no_callback() {
        let manager = EventManager::new();
        assert!(!manager.is_callback_registered());
    }

    #[tokio::test]
    async fn register_callback_and_trigger_event() {
        let mut manager = EventManager::new();
        let (callback, events) = CollectingCallback::new();
        manager.register_callback(EventType::ALL_EVENTS, callback, false);
        assert!(manager.is_callback_registered());

        let event_data = EventData::new(EventType::PIPELINE_START);
        manager.trigger_event(event_data).await;

        let collected = events.lock().await;
        assert_eq!(collected.len(), 1);
        assert_eq!(collected[0].event_type, EventType::PIPELINE_START);
    }

    #[tokio::test]
    async fn pipeline_end_sends_stats_report() {
        let mut manager = EventManager::new();
        let (callback, events) = CollectingCallback::new();
        manager.register_callback(EventType::ALL_EVENTS, callback, false);

        // Start pipeline
        manager
            .trigger_event(EventData::new(EventType::PIPELINE_START))
            .await;

        // Simulate some deletions
        let mut delete_event = EventData::new(EventType::DELETE_COMPLETE);
        delete_event.size = Some(1024);
        manager.trigger_event(delete_event).await;

        // End pipeline
        manager
            .trigger_event(EventData::new(EventType::PIPELINE_END))
            .await;

        let collected = events.lock().await;
        // PIPELINE_START + DELETE_COMPLETE + PIPELINE_END + STATS_REPORT
        assert_eq!(collected.len(), 4);
        assert_eq!(collected[3].event_type, EventType::STATS_REPORT);
        assert_eq!(collected[3].stats_deleted_objects, Some(1));
        assert_eq!(collected[3].stats_deleted_bytes, Some(1024));
    }

    #[tokio::test]
    async fn event_flag_filtering() {
        let mut manager = EventManager::new();
        let (callback, events) = CollectingCallback::new();
        // Only register for DELETE_COMPLETE events
        manager.register_callback(EventType::DELETE_COMPLETE, callback, false);

        manager
            .trigger_event(EventData::new(EventType::PIPELINE_START))
            .await;
        manager
            .trigger_event(EventData::new(EventType::DELETE_COMPLETE))
            .await;

        let collected = events.lock().await;
        // Only DELETE_COMPLETE should be dispatched
        assert_eq!(collected.len(), 1);
        assert_eq!(collected[0].event_type, EventType::DELETE_COMPLETE);
    }

    #[tokio::test]
    async fn pipeline_end_without_stats_flag_skips_stats_report() {
        let mut manager = EventManager::new();
        let (callback, events) = CollectingCallback::new();
        // Register for PIPELINE_START and PIPELINE_END, but NOT STATS_REPORT
        manager.register_callback(
            EventType::PIPELINE_START | EventType::PIPELINE_END,
            callback,
            false,
        );

        manager
            .trigger_event(EventData::new(EventType::PIPELINE_START))
            .await;
        manager
            .trigger_event(EventData::new(EventType::PIPELINE_END))
            .await;

        let collected = events.lock().await;
        // Should receive PIPELINE_START + PIPELINE_END only, no STATS_REPORT
        assert_eq!(collected.len(), 2);
        assert_eq!(collected[0].event_type, EventType::PIPELINE_START);
        assert_eq!(collected[1].event_type, EventType::PIPELINE_END);
    }

    #[tokio::test]
    async fn dry_run_flag_propagated() {
        let mut manager = EventManager::new();
        let (callback, events) = CollectingCallback::new();
        manager.register_callback(EventType::ALL_EVENTS, callback, true);

        manager
            .trigger_event(EventData::new(EventType::PIPELINE_START))
            .await;

        let collected = events.lock().await;
        assert!(collected[0].dry_run);
    }

    #[test]
    fn debug_format() {
        let manager = EventManager::new();
        let debug = format!("{manager:?}");
        assert!(debug.contains("callback_registered: false"));
    }

    #[test]
    fn pipeline_stats_to_event_data() {
        let stats = PipelineStats {
            pipeline_start_time: None,
            stats_deleted_objects: 100,
            stats_deleted_bytes: 50_000,
            stats_failed_objects: 5,
            stats_skipped_objects: 10,
            stats_warning_count: 2,
            stats_error_count: 1,
            stats_duration_sec: 10.0,
            stats_objects_per_sec: 10.0,
        };

        let event_data: EventData = stats.into();
        assert_eq!(event_data.event_type, EventType::STATS_REPORT);
        assert_eq!(event_data.stats_deleted_objects, Some(100));
        assert_eq!(event_data.stats_deleted_bytes, Some(50_000));
        assert_eq!(event_data.stats_failed_objects, Some(5));
        assert_eq!(event_data.stats_skipped_objects, Some(10));
    }
}
