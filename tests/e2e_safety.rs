//! E2E tests for safety features (Tests 29.18 - 29.20).
//!
//! Tests dry-run mode, max-delete threshold, and force flag behavior.

#![cfg(e2e_test)]

mod common;

use common::TestHelper;

// ---------------------------------------------------------------------------
// 29.18 Dry Run No Deletion
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_dry_run_no_deletion() {
    e2e_timeout!(async {
        // Purpose: Verify --dry-run runs the full pipeline (listing, filtering)
        //          but does NOT actually delete any objects from S3. The pipeline
        //          simulates deletions and reports stats as if deletions occurred.
        // Setup:   Upload 20 objects.
        // Expected: All 20 objects still exist after pipeline; stats show 20
        //           "deleted" (simulated); no actual S3 deletions occurred.
        //
        // Validates: Requirement 3.1

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let _guard = helper.bucket_guard(&bucket);

        for i in 0..20 {
            helper
                .put_object(&bucket, &format!("data/file{i:02}.dat"), vec![b'd'; 200])
                .await;
        }

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/data/"),
            "--dry-run",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(
            !result.has_error,
            "Dry-run pipeline should complete without errors"
        );
        assert_eq!(
            result.stats.stats_deleted_objects, 20,
            "Dry-run stats should report 20 simulated deletions"
        );

        // Verify NO objects were actually deleted
        let remaining = helper.count_objects(&bucket, "data/").await;
        assert_eq!(
            remaining, 20,
            "All 20 objects must still exist after dry-run"
        );
    });
}

// ---------------------------------------------------------------------------
// 29.19 Max Delete Threshold
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_max_delete_threshold() {
    e2e_timeout!(async {
        // Purpose: Verify --max-delete stops the deletion pipeline after the
        //          specified number of objects have been deleted. The pipeline
        //          should cancel and leave remaining objects intact.
        // Setup:   Upload 50 objects.
        // Expected: At most 10 objects deleted (may be slightly more due to
        //           in-flight batch operations); at least 40 remain.
        //
        // Validates: Requirement 3.6

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let _guard = helper.bucket_guard(&bucket);

        for i in 0..50 {
            helper
                .put_object(&bucket, &format!("data/file{i:02}.dat"), vec![b'd'; 100])
                .await;
        }

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/data/"),
            "--max-delete",
            "10",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        // Pipeline may report cancellation as an error (max-delete triggers cancel)
        // The key assertion is that not all objects were deleted
        let remaining = helper.count_objects(&bucket, "data/").await;
        assert!(
            remaining >= 40,
            "At least 40 objects should remain (max-delete 10); got {remaining} remaining"
        );
        assert!(
            result.stats.stats_deleted_objects <= 10,
            "At most 10 objects should be deleted; got {}",
            result.stats.stats_deleted_objects
        );
    });
}

// ---------------------------------------------------------------------------
// 29.20 Force Flag Skips Confirmation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_force_flag_skips_confirmation() {
    e2e_timeout!(async {
        // Purpose: Verify that --force flag allows the pipeline to run without
        //          requiring interactive confirmation. In the library API context,
        //          this means the pipeline proceeds directly to deletion.
        // Setup:   Upload 10 objects.
        // Expected: All 10 objects deleted; no prompt interaction required.
        //
        // Validates: Requirements 3.4, 13.2

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let _guard = helper.bucket_guard(&bucket);

        for i in 0..10 {
            helper
                .put_object(&bucket, &format!("data/file{i}.dat"), vec![b'd'; 100])
                .await;
        }

        let config = TestHelper::build_config(vec![&format!("s3://{bucket}/data/"), "--force"]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(
            !result.has_error,
            "Pipeline with --force should complete without errors"
        );
        assert_eq!(
            result.stats.stats_deleted_objects, 10,
            "All 10 objects should be deleted"
        );
    });
}
