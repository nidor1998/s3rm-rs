//! E2E stress tests for concurrent pipeline behavior.
//!
//! These tests exercise the pipeline under higher-than-normal load to verify
//! correctness of statistics accounting, channel backpressure handling, and
//! batch processing across multiple concurrent workers.

#![cfg(e2e_test)]

mod common;

use common::{CollectingEventCallback, TestHelper};
use s3rm_rs::EventType;
use std::sync::{Arc, Mutex};

// ---------------------------------------------------------------------------
// Stress: Concurrent Stats Accuracy
// ---------------------------------------------------------------------------

#[tokio::test]
async fn stress_concurrent_stats_accuracy() {
    e2e_timeout!(async {
        // Purpose: Verify that deletion statistics remain accurate under high
        //          concurrency with many workers racing to update shared atomic
        //          counters. This exercises the real DeletionStatsReport with
        //          multiple ObjectDeleter workers performing actual S3 deletions.
        //
        // Why this matters: DeletionStatsReport uses AtomicU64 with Relaxed
        //          ordering for performance. Under high concurrency, we need to
        //          verify that no increments are lost and byte totals are exact.
        //
        // Setup:   Upload 500 objects across 10 prefixes with known sizes:
        //          - 250 x 1KB (256,000 bytes)
        //          - 250 x 2KB (512,000 bytes)
        //          Total: 768,000 bytes
        //          Run with 32 workers and batch-size 50 to maximize contention.
        //          Register an event callback to cross-check DELETE_COMPLETE count.
        //
        // Expected: stats.stats_deleted_objects == 500
        //           stats.stats_deleted_bytes == 768,000
        //           stats.stats_failed_objects == 0
        //           DELETE_COMPLETE events == 500
        //           All objects removed from S3.
        //
        // Validates: Requirements 1.3, 1.4, 6.5, 7.1

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        // Upload 500 objects: 250 x 1KB + 250 x 2KB across 10 prefixes
        let mut objects: Vec<(String, Vec<u8>)> = Vec::with_capacity(500);
        for prefix_idx in 0..10 {
            for i in 0..25 {
                // 1KB objects
                objects.push((
                    format!("stress/p{prefix_idx:02}/small{i:02}.dat"),
                    vec![b's'; 1024],
                ));
                // 2KB objects
                objects.push((
                    format!("stress/p{prefix_idx:02}/large{i:02}.dat"),
                    vec![b'L'; 2048],
                ));
            }
        }
        helper.put_objects_parallel(&bucket, objects).await;

        // Verify pre-state
        let pre_count = helper.count_objects(&bucket, "stress/").await;
        assert_eq!(pre_count, 500, "Should start with 500 objects");

        // Register event callback to cross-check stats
        let collected_events = Arc::new(Mutex::new(Vec::new()));
        let callback = CollectingEventCallback {
            events: Arc::clone(&collected_events),
        };

        let mut config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/stress/"),
            "--worker-size",
            "32",
            "--batch-size",
            "50",
            "--force",
        ]);
        config
            .event_manager
            .register_callback(EventType::ALL_EVENTS, callback, false);

        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(
            result.stats.stats_deleted_objects, 500,
            "All 500 objects must be accounted for in stats (no lost increments)"
        );

        // Expected: 250 * 1024 + 250 * 2048 = 256,000 + 512,000 = 768,000
        assert_eq!(
            result.stats.stats_deleted_bytes, 768_000,
            "Byte total must be exact: 250*1KB + 250*2KB = 768,000 bytes"
        );
        assert_eq!(
            result.stats.stats_failed_objects, 0,
            "No objects should fail"
        );

        // Cross-check: event callback should have received 500 DELETE_COMPLETE events
        {
            let events = collected_events.lock().unwrap();
            let delete_completes: Vec<_> = events
                .iter()
                .filter(|e| e.event_type == EventType::DELETE_COMPLETE)
                .collect();
            assert_eq!(
                delete_completes.len(),
                500,
                "Event callback should receive exactly 500 DELETE_COMPLETE events, \
                 matching the stats counter"
            );
        }

        // Verify all objects are actually removed from S3
        let remaining = helper.count_objects(&bucket, "stress/").await;
        assert_eq!(remaining, 0, "All objects should be removed from S3");
        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// Stress: Channel Backpressure
// ---------------------------------------------------------------------------

#[tokio::test]
async fn stress_channel_backpressure() {
    e2e_timeout!(async {
        // Purpose: Verify the pipeline handles backpressure correctly when the
        //          object listing queue is very small relative to the number of
        //          objects. A tiny queue forces frequent blocking between the
        //          lister and filter/deleter stages, exercising the bounded
        //          channel flow control.
        //
        // Why this matters: The pipeline uses bounded async_channel between
        //          stages. If backpressure handling has bugs (e.g., dropped
        //          messages, deadlocks), a small queue will expose them because
        //          the lister must pause frequently while downstream catches up.
        //
        // Setup:   Upload 300 objects.
        //          Run with --object-listing-queue-size 2 (extremely small),
        //          --worker-size 4, --batch-size 10.
        //          This forces the lister to block after every 2 objects.
        //
        // Expected: All 300 objects deleted; no deadlocks; no lost objects.
        //           Stats accurately reflect all 300 deletions.
        //
        // Validates: Requirements 1.3, 1.8

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        let objects: Vec<(String, Vec<u8>)> = (0..300)
            .map(|i| (format!("backpressure/file{i:04}.dat"), vec![b'B'; 512]))
            .collect();
        helper.put_objects_parallel(&bucket, objects).await;

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/backpressure/"),
            "--object-listing-queue-size",
            "2",
            "--worker-size",
            "4",
            "--batch-size",
            "10",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(
            !result.has_error,
            "Pipeline should complete without errors even with tiny queue"
        );
        assert_eq!(
            result.stats.stats_deleted_objects, 300,
            "All 300 objects must be deleted despite extreme backpressure"
        );
        assert_eq!(
            result.stats.stats_failed_objects, 0,
            "No objects should fail"
        );

        let remaining = helper.count_objects(&bucket, "backpressure/").await;
        assert_eq!(remaining, 0, "All objects should be removed from S3");
        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// Stress: ObjectDeleter Batch Processing
// ---------------------------------------------------------------------------

#[tokio::test]
async fn stress_object_deleter_batch_processing() {
    e2e_timeout!(async {
        // Purpose: Verify that ObjectDeleter correctly handles batch boundaries
        //          and remainder batches under realistic load. When the total
        //          object count is not evenly divisible by batch size, the last
        //          batch will be a partial batch. Multiple workers must not
        //          double-count or skip objects at batch boundaries.
        //
        // Why this matters: With 997 objects and batch-size 100, there will be
        //          9 full batches (900 objects) and 1 partial batch (97 objects)
        //          distributed across 16 workers. This exercises:
        //          - Batch boundary correctness (no off-by-one)
        //          - Partial batch handling (last batch < batch_size)
        //          - Multi-worker contention on the shared MPMC channel
        //          - Accurate stats aggregation across all workers
        //
        // Setup:   Upload 997 objects (prime-ish to avoid even distribution).
        //          Each object is 768 bytes. Run with --worker-size 16,
        //          --batch-size 100, --max-keys 50 (forces many listing pages).
        //
        // Expected: stats.stats_deleted_objects == 997
        //           stats.stats_deleted_bytes == 997 * 768 = 765,696
        //           stats.stats_failed_objects == 0
        //           All objects removed from S3.
        //
        // Validates: Requirements 1.1, 1.3, 1.4, 1.9

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        const TOTAL_OBJECTS: u64 = 997;
        const OBJECT_SIZE: usize = 768;

        let objects: Vec<(String, Vec<u8>)> = (0..TOTAL_OBJECTS)
            .map(|i| {
                (
                    format!("batchstress/obj{i:04}.dat"),
                    vec![b'X'; OBJECT_SIZE],
                )
            })
            .collect();
        helper.put_objects_parallel(&bucket, objects).await;

        // Verify pre-state
        let pre_count = helper.count_objects(&bucket, "batchstress/").await;
        assert_eq!(
            pre_count, TOTAL_OBJECTS as usize,
            "Should start with {TOTAL_OBJECTS} objects"
        );

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/batchstress/"),
            "--worker-size",
            "16",
            "--batch-size",
            "100",
            "--max-keys",
            "50",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(
            !result.has_error,
            "Pipeline should complete without errors; errors: {:?}",
            result.errors
        );
        assert_eq!(
            result.stats.stats_deleted_objects, TOTAL_OBJECTS,
            "Exactly {TOTAL_OBJECTS} objects must be deleted (batch boundary correctness)"
        );

        let expected_bytes = TOTAL_OBJECTS * OBJECT_SIZE as u64;
        assert_eq!(
            result.stats.stats_deleted_bytes, expected_bytes,
            "Byte total must be exact: {TOTAL_OBJECTS} * {OBJECT_SIZE} = {expected_bytes}"
        );
        assert_eq!(
            result.stats.stats_failed_objects, 0,
            "No objects should fail"
        );

        // Verify all objects are actually removed from S3
        let remaining = helper.count_objects(&bucket, "batchstress/").await;
        assert_eq!(remaining, 0, "All objects should be removed from S3");
        guard.cleanup().await;
    });
}
