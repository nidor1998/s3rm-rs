//! E2E tests for retry and timeout options (Tests 29.45 - 29.47).
//!
//! Tests AWS retry configuration, timeout settings, and stalled stream
//! protection options.

#![cfg(e2e_test)]

mod common;

use common::TestHelper;

// ---------------------------------------------------------------------------
// 29.45 Retry Options
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_retry_options() {
    e2e_timeout!(async {
        // Purpose: Verify retry-related options (--aws-max-attempts,
        //          --initial-backoff-milliseconds, --force-retry-count,
        //          --force-retry-interval-milliseconds) can be configured
        //          without breaking normal operation.
        // Setup:   Upload 10 objects.
        // Expected: Pipeline completes; all 10 objects deleted (retry options
        //           do not prevent normal operation).
        //
        // Validates: Requirements 6.1, 6.2

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let _guard = helper.bucket_guard(&bucket);

        for i in 0..10 {
            helper
                .put_object(&bucket, &format!("retry/file{i}.dat"), vec![b'r'; 100])
                .await;
        }

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/retry/"),
            "--aws-max-attempts",
            "2",
            "--initial-backoff-milliseconds",
            "100",
            "--force-retry-count",
            "1",
            "--force-retry-interval-milliseconds",
            "100",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(
            result.stats.stats_deleted_objects, 10,
            "Should delete all 10 objects"
        );
    });
}

// ---------------------------------------------------------------------------
// 29.46 Timeout Options
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_timeout_options() {
    e2e_timeout!(async {
        // Purpose: Verify timeout-related options (--operation-timeout-milliseconds,
        //          --operation-attempt-timeout-milliseconds,
        //          --connect-timeout-milliseconds, --read-timeout-milliseconds)
        //          can be configured. The timeouts should be generous enough for
        //          normal operations to complete.
        // Setup:   Upload 10 objects.
        // Expected: Pipeline completes within timeout; all 10 objects deleted.
        //
        // Validates: Requirement 8.3

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let _guard = helper.bucket_guard(&bucket);

        for i in 0..10 {
            helper
                .put_object(&bucket, &format!("timeout/file{i}.dat"), vec![b't'; 100])
                .await;
        }

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/timeout/"),
            "--operation-timeout-milliseconds",
            "30000",
            "--operation-attempt-timeout-milliseconds",
            "10000",
            "--connect-timeout-milliseconds",
            "5000",
            "--read-timeout-milliseconds",
            "5000",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(
            result.stats.stats_deleted_objects, 10,
            "Should delete all 10 objects"
        );
    });
}

// ---------------------------------------------------------------------------
// 29.47 Disable Stalled Stream Protection
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_disable_stalled_stream_protection() {
    e2e_timeout!(async {
        // Purpose: Verify --disable-stalled-stream-protection disables the SDK's
        //          stalled stream detection without breaking normal operation.
        // Setup:   Upload 10 objects.
        // Expected: Pipeline completes; all 10 objects deleted.
        //
        // Validates: Requirement 8.3

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let _guard = helper.bucket_guard(&bucket);

        for i in 0..10 {
            helper
                .put_object(&bucket, &format!("stalled/file{i}.dat"), vec![b'x'; 100])
                .await;
        }

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/stalled/"),
            "--disable-stalled-stream-protection",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(
            result.stats.stats_deleted_objects, 10,
            "Should delete all 10 objects"
        );
    });
}
