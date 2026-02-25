//! E2E tests for Express One Zone directory bucket support (Requirement 1.11).
//!
//! Tests that s3rm-rs auto-detects Express One Zone directory buckets (via the
//! `--x-s3` bucket name suffix), sets batch_size=1, disables parallel listing,
//! and that the `--allow-parallel-listings-in-express-one-zone` flag restores
//! user-specified settings.

#![cfg(e2e_test)]

mod common;

use common::TestHelper;

/// Returns the availability zone ID for Express One Zone directory buckets.
///
/// Reads `S3RM_E2E_AZ_ID` from the environment; falls back to `apne1-az4`
/// (ap-northeast-1 AZ 4) which is a commonly available Express One Zone AZ.
fn express_az_id() -> String {
    std::env::var("S3RM_E2E_AZ_ID").unwrap_or_else(|_| "apne1-az4".to_string())
}

// ---------------------------------------------------------------------------
// Express One Zone: auto batch_size=1
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_express_one_zone_auto_batch_size_one() {
    e2e_timeout!(async {
        // Purpose: Verify that when the target is an Express One Zone directory
        //          bucket (detected by the `--x-s3` suffix), the pipeline
        //          automatically sets batch_size to 1 and successfully deletes
        //          all objects.
        // Setup:   Create a directory bucket, upload 10 objects.
        // Expected: All 10 objects deleted; stats show 10 deleted, 0 failed.
        //           No explicit --batch-size argument is provided — the auto-
        //           detection should set it to 1.
        //
        // Validates: Requirement 1.11

        let az_id = express_az_id();
        let helper = TestHelper::new().await;
        let bucket = helper.generate_directory_bucket_name(&az_id);
        helper.create_directory_bucket(&bucket, &az_id).await;

        let guard = helper.bucket_guard(&bucket);

        for i in 0..10 {
            helper
                .put_object(&bucket, &format!("data/file{i:02}.dat"), vec![b'd'; 256])
                .await;
        }

        // No --batch-size: auto-detection should set batch_size=1 for --x-s3 buckets
        let config = TestHelper::build_config(vec![&format!("s3://{bucket}/data/"), "--force"]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(
            !result.has_error,
            "Pipeline should complete without errors: {:?}",
            result.errors
        );
        assert_eq!(
            result.stats.stats_deleted_objects, 10,
            "Should delete all 10 objects"
        );
        assert_eq!(
            result.stats.stats_failed_objects, 0,
            "No objects should fail"
        );

        // Verify objects are actually removed from S3
        let remaining = helper.count_objects(&bucket, "data/").await;
        assert_eq!(remaining, 0, "No data/ objects should remain");

        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// Express One Zone: filtering works with directory buckets
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_express_one_zone_with_filter() {
    e2e_timeout!(async {
        // Purpose: Verify that regex filtering works correctly with Express One
        //          Zone directory buckets. The auto-detected batch_size=1 should
        //          not interfere with filter evaluation.
        // Setup:   Create a directory bucket, upload 10 objects: 5 with keys
        //          matching `^keep/` and 5 with keys matching `^delete/`.
        // Expected: Only the 5 `delete/` objects are deleted (via include regex);
        //           5 `keep/` objects remain untouched.
        //
        // Validates: Requirement 1.11, 2.2

        let az_id = express_az_id();
        let helper = TestHelper::new().await;
        let bucket = helper.generate_directory_bucket_name(&az_id);
        helper.create_directory_bucket(&bucket, &az_id).await;

        let guard = helper.bucket_guard(&bucket);

        for i in 0..5 {
            helper
                .put_object(&bucket, &format!("delete/file{i}.dat"), vec![b'x'; 128])
                .await;
        }
        for i in 0..5 {
            helper
                .put_object(&bucket, &format!("keep/file{i}.dat"), vec![b'y'; 128])
                .await;
        }

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/"),
            "--filter-include-regex",
            r"^delete/",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(
            !result.has_error,
            "Pipeline should complete without errors: {:?}",
            result.errors
        );
        assert_eq!(
            result.stats.stats_deleted_objects, 5,
            "Should delete exactly 5 delete/ objects"
        );
        assert_eq!(
            result.stats.stats_failed_objects, 0,
            "No objects should fail"
        );

        // Verify keep/ objects remain
        let keep_remaining = helper.count_objects(&bucket, "keep/").await;
        assert_eq!(keep_remaining, 5, "All 5 keep/ objects should remain");

        // Verify delete/ objects are gone
        let delete_remaining = helper.count_objects(&bucket, "delete/").await;
        assert_eq!(delete_remaining, 0, "No delete/ objects should remain");

        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// Express One Zone: --allow-parallel-listings-in-express-one-zone override
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_express_one_zone_allow_parallel_listings_override() {
    e2e_timeout!(async {
        // Purpose: Verify that `--allow-parallel-listings-in-express-one-zone`
        //          combined with an explicit `--batch-size` restores user-specified
        //          settings, overriding the auto-detection defaults.
        // Setup:   Create a directory bucket, upload 20 objects.
        // Expected: All 20 objects deleted with the user-specified batch-size 10
        //           (not auto-set to 1). Stats show 20 deleted, 0 failed.
        //
        // Validates: Requirement 1.11 (override flag)

        let az_id = express_az_id();
        let helper = TestHelper::new().await;
        let bucket = helper.generate_directory_bucket_name(&az_id);
        helper.create_directory_bucket(&bucket, &az_id).await;

        let guard = helper.bucket_guard(&bucket);

        let objects: Vec<(String, Vec<u8>)> = (0..20)
            .map(|i| (format!("bulk/file{i:03}.dat"), vec![b'z'; 200]))
            .collect();
        helper.put_objects_parallel(&bucket, objects).await;

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/bulk/"),
            "--allow-parallel-listings-in-express-one-zone",
            "--batch-size",
            "10",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(
            !result.has_error,
            "Pipeline should complete without errors: {:?}",
            result.errors
        );
        assert_eq!(
            result.stats.stats_deleted_objects, 20,
            "Should delete all 20 objects"
        );
        assert_eq!(
            result.stats.stats_failed_objects, 0,
            "No objects should fail"
        );

        // Verify objects are actually removed from S3
        let remaining = helper.count_objects(&bucket, "bulk/").await;
        assert_eq!(remaining, 0, "No bulk/ objects should remain");

        guard.cleanup().await;
    });
}
