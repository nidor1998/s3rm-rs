//! E2E tests for tracing and logging options (Tests 29.38 - 29.44).
//!
//! Tests verbosity levels, JSON tracing, quiet mode, progress display,
//! color options, deletion summary logging, and AWS SDK tracing.

#![cfg(e2e_test)]

mod common;

use common::TestHelper;

// ---------------------------------------------------------------------------
// Helper: upload 5 objects and return bucket name
// ---------------------------------------------------------------------------

async fn setup_small_bucket(helper: &TestHelper) -> String {
    let bucket = helper.generate_bucket_name();
    helper.create_bucket(&bucket).await;

    for i in 0..5 {
        helper
            .put_object(&bucket, &format!("trace/file{i}.dat"), vec![b't'; 100])
            .await;
    }

    bucket
}

// ---------------------------------------------------------------------------
// 29.38 Verbosity Levels
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_verbosity_levels() {
    e2e_timeout!(async {
        // Purpose: Verify -v, -vv, -vvv verbosity flags work without affecting
        //          deletion functionality. Each level increases logging detail
        //          but should not change behavior.
        // Setup:   Upload 5 objects per bucket, 3 separate buckets.
        // Expected: All 5 objects deleted in each run; no errors; pipeline
        //           completes regardless of verbosity level.
        //
        // Note: Three separate buckets are used (one per verbosity level) because
        //       each pipeline run deletes the objects. Reusing a single bucket
        //       would require re-uploading objects between runs, which adds
        //       latency without improving isolation. Separate buckets ensure
        //       each run starts with a known, independent state.
        //
        // Validates: Requirements 4.1, 4.2, 4.3, 4.4, 4.5

        let helper = TestHelper::new().await;

        // Test with -v (standard logging)
        let bucket_v = setup_small_bucket(&helper).await;
        let guard_v = helper.bucket_guard(&bucket_v);

        let config =
            TestHelper::build_config(vec![&format!("s3://{bucket_v}/trace/"), "-v", "--force"]);
        let result = TestHelper::run_pipeline(config).await;
        assert!(
            !result.has_error,
            "-v: Pipeline should complete without errors"
        );
        assert_eq!(
            result.stats.stats_deleted_objects, 5,
            "-v: Should delete 5 objects"
        );

        // Test with -vv (detailed logging)
        let bucket_vv = setup_small_bucket(&helper).await;
        let guard_vv = helper.bucket_guard(&bucket_vv);

        let config =
            TestHelper::build_config(vec![&format!("s3://{bucket_vv}/trace/"), "-vv", "--force"]);
        let result = TestHelper::run_pipeline(config).await;
        assert!(
            !result.has_error,
            "-vv: Pipeline should complete without errors"
        );
        assert_eq!(
            result.stats.stats_deleted_objects, 5,
            "-vv: Should delete 5 objects"
        );

        // Test with -vvv (debug logging)
        let bucket_vvv = setup_small_bucket(&helper).await;
        let guard_vvv = helper.bucket_guard(&bucket_vvv);

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket_vvv}/trace/"),
            "-vvv",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;
        assert!(
            !result.has_error,
            "-vvv: Pipeline should complete without errors"
        );
        assert_eq!(
            result.stats.stats_deleted_objects, 5,
            "-vvv: Should delete 5 objects"
        );
        guard_v.cleanup().await;
        guard_vv.cleanup().await;
        guard_vvv.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// 29.39 JSON Tracing
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_json_tracing() {
    e2e_timeout!(async {
        // Purpose: Verify --json-tracing outputs structured JSON format for log
        //          entries. JSON tracing requires --force since it is incompatible
        //          with interactive confirmation prompts.
        // Setup:   Upload 5 objects.
        // Expected: Pipeline completes; all 5 objects deleted.
        //
        // Validates: Requirements 4.7, 13.3

        let helper = TestHelper::new().await;
        let bucket = setup_small_bucket(&helper).await;

        let guard = helper.bucket_guard(&bucket);

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/trace/"),
            "--json-tracing",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(result.stats.stats_deleted_objects, 5);
        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// 29.40 Quiet Mode
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_quiet_mode() {
    e2e_timeout!(async {
        // Purpose: Verify -q (quiet mode) suppresses progress output. The pipeline
        //          should still function correctly.
        // Setup:   Upload 5 objects.
        // Expected: Pipeline completes; all 5 objects deleted.
        //
        // Validates: Requirement 7.4

        let helper = TestHelper::new().await;
        let bucket = setup_small_bucket(&helper).await;

        let guard = helper.bucket_guard(&bucket);

        let config =
            TestHelper::build_config(vec![&format!("s3://{bucket}/trace/"), "-q", "--force"]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(result.stats.stats_deleted_objects, 5);
        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// 29.41 Show No Progress
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_show_no_progress() {
    e2e_timeout!(async {
        // Purpose: Verify --show-no-progress hides the progress bar without
        //          affecting functionality.
        // Setup:   Upload 5 objects.
        // Expected: Pipeline completes; all 5 objects deleted.
        //
        // Validates: Requirement 7.4

        let helper = TestHelper::new().await;
        let bucket = setup_small_bucket(&helper).await;

        let guard = helper.bucket_guard(&bucket);

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/trace/"),
            "--show-no-progress",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(result.stats.stats_deleted_objects, 5);
        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// 29.42 Disable Color Tracing
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_disable_color_tracing() {
    e2e_timeout!(async {
        // Purpose: Verify --disable-color-tracing disables colored output without
        //          affecting pipeline functionality.
        // Setup:   Upload 5 objects.
        // Expected: Pipeline completes; all 5 objects deleted.
        //
        // Validates: Requirements 4.8, 4.9

        let helper = TestHelper::new().await;
        let bucket = setup_small_bucket(&helper).await;

        let guard = helper.bucket_guard(&bucket);

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/trace/"),
            "--disable-color-tracing",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(result.stats.stats_deleted_objects, 5);
        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// 29.43 Log Deletion Summary
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_log_deletion_summary() {
    e2e_timeout!(async {
        // Purpose: Verify --log-deletion-summary outputs a summary of deletion
        //          statistics at the end of the pipeline run.
        // Setup:   Upload 10 objects.
        // Expected: Pipeline completes; all 10 objects deleted; stats are logged.
        //
        // Validates: Requirement 7.3

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        for i in 0..10 {
            helper
                .put_object(&bucket, &format!("summary/file{i}.dat"), vec![b's'; 200])
                .await;
        }

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/summary/"),
            "--log-deletion-summary",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(
            result.stats.stats_deleted_objects, 10,
            "Should delete all 10 objects"
        );
        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// 29.44 AWS SDK Tracing and Span Events
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_aws_sdk_tracing_and_span_events() {
    e2e_timeout!(async {
        // Purpose: Verify --aws-sdk-tracing and --span-events-tracing options
        //          enable additional tracing output without affecting functionality.
        // Setup:   Upload 5 objects.
        // Expected: Pipeline completes; all 5 objects deleted; AWS SDK trace
        //           events are produced (verified by no errors, not by parsing output).
        //
        // Validates: Requirement 4.5

        let helper = TestHelper::new().await;
        let bucket = setup_small_bucket(&helper).await;

        let guard = helper.bucket_guard(&bucket);

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/trace/"),
            "--aws-sdk-tracing",
            "--span-events-tracing",
            "-vvv",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(result.stats.stats_deleted_objects, 5);
        guard.cleanup().await;
    });
}
