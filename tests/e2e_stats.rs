//! E2E tests for statistics and result verification (Tests 29.60 - 29.61).
//!
//! Tests accuracy of deletion statistics and event callback completeness.

#![cfg(e2e_test)]

mod common;

use common::TestHelper;
use s3rm_rs::{EventCallback, EventData, EventType};
use std::sync::{Arc, Mutex};

// ---------------------------------------------------------------------------
// Helper: Collecting event callback
// ---------------------------------------------------------------------------

struct CollectingEventCallback {
    events: Arc<Mutex<Vec<EventData>>>,
}

#[async_trait::async_trait]
impl EventCallback for CollectingEventCallback {
    async fn on_event(&mut self, event_data: EventData) {
        self.events.lock().unwrap().push(event_data);
    }
}

// ---------------------------------------------------------------------------
// 29.60 Deletion Stats Accuracy
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_deletion_stats_accuracy() {
    e2e_timeout!(async {
        // Purpose: Verify get_deletion_stats() returns accurate counts and byte
        //          totals after pipeline completion. The stats should reflect
        //          exact object counts and cumulative byte sizes.
        // Setup:   Upload 15 objects of known sizes:
        //          - 5 x 1KB (5120 bytes total)
        //          - 5 x 2KB (10240 bytes total)
        //          - 5 x 5KB (25600 bytes total)
        //          Total: 40960 bytes
        // Expected: stats.stats_deleted_objects == 15;
        //           stats.stats_deleted_bytes == 40960;
        //           stats.stats_failed_objects == 0;
        //           stats.duration > 0.
        //
        // Validates: Requirements 6.5, 7.1, 7.3

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let _guard = helper.bucket_guard(&bucket);

        // 5 x 1KB objects
        for i in 0..5 {
            helper
                .put_object(&bucket, &format!("stats/small{i}.dat"), vec![b'a'; 1024])
                .await;
        }
        // 5 x 2KB objects
        for i in 0..5 {
            helper
                .put_object(&bucket, &format!("stats/medium{i}.dat"), vec![b'b'; 2048])
                .await;
        }
        // 5 x 5KB objects
        for i in 0..5 {
            helper
                .put_object(&bucket, &format!("stats/large{i}.dat"), vec![b'c'; 5120])
                .await;
        }

        let config = TestHelper::build_config(vec![&format!("s3://{bucket}/stats/"), "--force"]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(
            result.stats.stats_deleted_objects, 15,
            "Should delete exactly 15 objects"
        );

        // Expected total: 5*1024 + 5*2048 + 5*5120 = 5120 + 10240 + 25600 = 40960
        assert_eq!(
            result.stats.stats_deleted_bytes, 40960,
            "Deleted bytes should equal 40960 (5*1KB + 5*2KB + 5*5KB)"
        );
        assert_eq!(
            result.stats.stats_failed_objects, 0,
            "No objects should fail"
        );
        assert!(
            !result.stats.duration.is_zero(),
            "Duration should be positive"
        );
    });
}

// ---------------------------------------------------------------------------
// 29.61 Event Callback Receives All Event Types
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_event_callback_receives_all_event_types() {
    e2e_timeout!(async {
        // Purpose: Verify a Rust event callback receives all expected event types
        //          during a normal pipeline run: PIPELINE_START, DELETE_COMPLETE
        //          (one per object), and PIPELINE_END.
        // Setup:   Upload 5 objects. Register Rust event callback collecting all events.
        // Expected: Received exactly 1 PIPELINE_START, 5 DELETE_COMPLETE events,
        //           at least 1 PIPELINE_END. DELETE_COMPLETE events should contain
        //           key and size fields.
        //
        // Validates: Requirements 7.6, 7.7

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let _guard = helper.bucket_guard(&bucket);

        for i in 0..5 {
            helper
                .put_object(&bucket, &format!("events/file{i}.dat"), vec![b'e'; 512])
                .await;
        }

        let collected_events = Arc::new(Mutex::new(Vec::new()));
        let callback = CollectingEventCallback {
            events: Arc::clone(&collected_events),
        };

        let mut config =
            TestHelper::build_config(vec![&format!("s3://{bucket}/events/"), "--force"]);
        config
            .event_manager
            .register_callback(EventType::ALL_EVENTS, callback, false);

        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(result.stats.stats_deleted_objects, 5);

        let events = collected_events.lock().unwrap();

        // PIPELINE_START
        let starts: Vec<_> = events
            .iter()
            .filter(|e| e.event_type == EventType::PIPELINE_START)
            .collect();
        assert_eq!(starts.len(), 1, "Should have exactly 1 PIPELINE_START");

        // DELETE_COMPLETE
        let completes: Vec<_> = events
            .iter()
            .filter(|e| e.event_type == EventType::DELETE_COMPLETE)
            .collect();
        assert_eq!(completes.len(), 5, "Should have 5 DELETE_COMPLETE events");

        // Verify DELETE_COMPLETE events have key and size
        for event in &completes {
            assert!(
                event.key.is_some(),
                "DELETE_COMPLETE event should have a key"
            );
            assert!(
                event.size.is_some(),
                "DELETE_COMPLETE event should have a size"
            );
        }

        // PIPELINE_END
        let ends: Vec<_> = events
            .iter()
            .filter(|e| e.event_type == EventType::PIPELINE_END)
            .collect();
        assert!(
            !ends.is_empty(),
            "Should have at least 1 PIPELINE_END event"
        );
    });
}
