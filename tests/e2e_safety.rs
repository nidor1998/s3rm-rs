//! E2E tests for safety features (Tests 29.18 - 29.20).
//!
//! Tests dry-run mode, max-delete threshold, and force flag behavior.

#![cfg(e2e_test)]

mod common;

use common::TestHelper;

// Note on Req 13.1 (Non-TTY detection without --force):
// The tool returns exit code 2 with an InvalidConfig error in non-interactive
// environments when neither --force nor --dry-run is specified. This is covered
// by unit tests (safety_properties, cicd_properties) rather than E2E because
// cargo test inherits the terminal, making is_terminal() return true.

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

        let guard = helper.bucket_guard(&bucket);

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
        // Note: Verification of the [dry-run] log prefix (Req 3.1) requires tracing
        // output capture, which is covered by unit tests for the dry-run implementation.
        let remaining = helper.count_objects(&bucket, "data/").await;
        assert_eq!(
            remaining, 20,
            "All 20 objects must still exist after dry-run"
        );
        guard.cleanup().await;
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
        // Expected: Exactly 10 objects deleted (with batch-size=1 ensuring the
        //           max-delete counter is checked after every single object);
        //           at least 40 remain.
        //
        // Note:    --batch-size 1 is essential for deterministic behavior. Without
        //          it, a single large batch could delete all objects before the
        //          max-delete check fires, making assertions unreliable.
        //
        // Validates: Requirement 3.6

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        let max_delete_value: u64 = 10;

        for i in 0..50 {
            helper
                .put_object(&bucket, &format!("data/file{i:02}.dat"), vec![b'd'; 100])
                .await;
        }

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/data/"),
            "--max-delete",
            "10",
            "--batch-size",
            "1",
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
        assert_eq!(
            result.stats.stats_deleted_objects, max_delete_value,
            "Exactly {max_delete_value} objects should be deleted (batch-size=1 ensures precise max-delete enforcement)"
        );
        guard.cleanup().await;
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

        let guard = helper.bucket_guard(&bucket);

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

        let remaining = helper.count_objects(&bucket, "data/").await;
        assert_eq!(remaining, 0, "All data/ objects should be removed from S3");
        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// Max Delete + Delete All Versions: counts each version separately
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_max_delete_counts_versions() {
    e2e_timeout!(async {
        // Purpose: Verify that --max-delete combined with --delete-all-versions
        //          counts each object version as a separate deletion toward the
        //          max-delete threshold, not each key.
        // Setup:   Create versioned bucket; upload 5 objects, overwrite each once
        //          (10 versions total). Run with --delete-all-versions --max-delete 5
        //          --batch-size 1 --force.
        // Expected: Exactly 5 versions deleted (max-delete counts versions);
        //           at least 5 versions remain.
        //
        // Validates: Requirements 3.6, 5.4

        const NUM_KEYS: usize = 5;
        const MAX_DELETE: u64 = 5;

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_versioned_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        // Upload initial versions
        for i in 0..NUM_KEYS {
            helper
                .put_object(&bucket, &format!("data/file{i}.dat"), vec![b'v'; 100])
                .await;
        }
        // Overwrite each object to create a second version (10 versions total)
        for i in 0..NUM_KEYS {
            helper
                .put_object(&bucket, &format!("data/file{i}.dat"), vec![b'w'; 200])
                .await;
        }

        // Verify pre-state: 10 versions
        let pre_versions = helper.list_object_versions(&bucket).await;
        assert_eq!(
            pre_versions.len(),
            NUM_KEYS * 2,
            "Should start with {} versions",
            NUM_KEYS * 2
        );

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/data/"),
            "--delete-all-versions",
            "--max-delete",
            &MAX_DELETE.to_string(),
            "--batch-size",
            "1",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        // max-delete should have capped the deletions
        assert_eq!(
            result.stats.stats_deleted_objects, MAX_DELETE,
            "Exactly {MAX_DELETE} versions should be deleted (max-delete enforced per version)"
        );

        // At least 5 versions should remain
        let post_versions = helper.list_object_versions(&bucket).await;
        assert!(
            post_versions.len() >= NUM_KEYS,
            "At least {} versions should remain; got {}",
            NUM_KEYS,
            post_versions.len()
        );

        guard.cleanup().await;
    });
}
