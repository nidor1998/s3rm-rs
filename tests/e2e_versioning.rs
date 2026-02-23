//! E2E tests for S3 versioning support (Tests 29.21 - 29.23).
//!
//! Tests delete marker creation, all-versions deletion, and error handling
//! for --delete-all-versions on non-versioned buckets.

#![cfg(e2e_test)]

mod common;

use common::TestHelper;

// ---------------------------------------------------------------------------
// 29.21 Versioned Bucket Creates Delete Markers
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_versioned_bucket_creates_delete_markers() {
    e2e_timeout!(async {
        // Purpose: Verify that deleting from a versioned bucket WITHOUT
        //          --delete-all-versions creates delete markers instead of
        //          permanently removing objects. The original versions must
        //          still exist.
        // Setup:   Create versioned bucket; upload 10 objects.
        // Expected: Delete markers created; original versions still exist in
        //           list_object_versions output; both markers and originals visible.
        //
        // Validates: Requirement 5.1

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_versioned_bucket(&bucket).await;

        let _guard = helper.bucket_guard(&bucket);

        for i in 0..10 {
            helper
                .put_object(&bucket, &format!("versioned/file{i}.dat"), vec![b'v'; 100])
                .await;
        }

        // Run without --delete-all-versions (creates delete markers)
        let config =
            TestHelper::build_config(vec![&format!("s3://{bucket}/versioned/"), "--force"]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(
            result.stats.stats_deleted_objects, 10,
            "Should report 10 deletions (delete marker creations)"
        );

        // Verify: objects appear deleted via normal listing
        let normal_listing = helper.list_objects(&bucket, "versioned/").await;
        assert_eq!(
            normal_listing.len(),
            0,
            "Normal listing should show no objects (hidden by delete markers)"
        );

        // Verify: version listing shows both original versions and delete markers
        let versions = helper.list_object_versions(&bucket).await;
        let delete_markers: Vec<_> = versions
            .iter()
            .filter(|(k, _)| k.starts_with("[delete-marker]"))
            .collect();
        let original_versions: Vec<_> = versions
            .iter()
            .filter(|(k, _)| !k.starts_with("[delete-marker]"))
            .collect();

        assert_eq!(delete_markers.len(), 10, "Should have 10 delete markers");
        assert_eq!(
            original_versions.len(),
            10,
            "Original versions should still exist"
        );
    });
}

// ---------------------------------------------------------------------------
// 29.22 Delete All Versions
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_delete_all_versions() {
    e2e_timeout!(async {
        // Purpose: Verify --delete-all-versions permanently removes all versions
        //          of matching objects including delete markers.
        // Setup:   Create versioned bucket; upload 10 objects; overwrite each once
        //          (creates 2 versions each = 20 versions); delete 3 via normal API
        //          (creates 3 delete markers). Total: 20 versions + 3 markers = 23.
        // Expected: No object versions or delete markers remain; bucket is clean;
        //           stats count each version/marker as a separate deletion.
        //
        // Validates: Requirements 5.2, 5.4

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_versioned_bucket(&bucket).await;

        let _guard = helper.bucket_guard(&bucket);

        // Upload initial versions
        for i in 0..10 {
            helper
                .put_object(&bucket, &format!("versioned/file{i}.dat"), vec![b'v'; 100])
                .await;
        }

        // Overwrite each object to create a second version
        for i in 0..10 {
            helper
                .put_object(&bucket, &format!("versioned/file{i}.dat"), vec![b'w'; 200])
                .await;
        }

        // Delete 3 objects via normal API to create delete markers
        for i in 0..3 {
            helper
                .client()
                .delete_object()
                .bucket(&bucket)
                .key(format!("versioned/file{i}.dat"))
                .send()
                .await
                .unwrap_or_else(|e| panic!("Failed to create delete marker: {e}"));
        }

        // Run with --delete-all-versions (should delete all versions + delete markers)
        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/versioned/"),
            "--delete-all-versions",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        // 10 objects x 2 versions = 20 versions + 3 delete markers = 23 total
        assert_eq!(
            result.stats.stats_deleted_objects, 23,
            "Should delete all 23 items (20 versions + 3 delete markers)"
        );

        // Verify nothing remains
        let versions = helper.list_object_versions(&bucket).await;
        assert!(
            versions.is_empty(),
            "No versions or delete markers should remain; found {}",
            versions.len()
        );
    });
}

// ---------------------------------------------------------------------------
// 29.23 Delete All Versions on Unversioned Bucket Error
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_delete_all_versions_unversioned_bucket_error() {
    e2e_timeout!(async {
        // Purpose: Verify that --delete-all-versions on a non-versioned bucket
        //          produces an error. The pipeline should detect the mismatch
        //          and report it.
        // Setup:   Create standard (non-versioned) bucket; upload 5 objects.
        // Expected: Pipeline reports error about versioning requirement;
        //           objects should remain (no deletion attempted).
        //
        // Validates: Requirement 5.2

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await; // non-versioned

        let _guard = helper.bucket_guard(&bucket);

        for i in 0..5 {
            helper
                .put_object(&bucket, &format!("data/file{i}.dat"), vec![b'd'; 100])
                .await;
        }

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/data/"),
            "--delete-all-versions",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(
            result.has_error,
            "Pipeline should report error for --delete-all-versions on non-versioned bucket"
        );

        // Objects should still exist
        let remaining = helper.count_objects(&bucket, "data/").await;
        assert_eq!(remaining, 5, "All objects should remain after the error");
    });
}
