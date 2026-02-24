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

        let guard = helper.bucket_guard(&bucket);

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
        guard.cleanup().await;
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
        // Version count breakdown:
        //   NUM_OBJECTS (10) keys, each uploaded twice = VERSIONS_PER_OBJECT (2)
        //   => NUM_OBJECTS * VERSIONS_PER_OBJECT = 20 object versions
        //   NUM_DELETE_MARKERS (3) keys deleted via normal API
        //   => 3 delete markers created
        //   EXPECTED_TOTAL_ITEMS = 20 + 3 = 23
        //
        // Validates: Requirements 5.2, 5.4

        const NUM_OBJECTS: usize = 10;
        const VERSIONS_PER_OBJECT: usize = 2;
        const NUM_DELETE_MARKERS: usize = 3;
        const EXPECTED_TOTAL_ITEMS: u64 =
            (NUM_OBJECTS * VERSIONS_PER_OBJECT + NUM_DELETE_MARKERS) as u64;

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_versioned_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        // Upload initial versions (10 keys x 1 version each = 10 versions)
        for i in 0..NUM_OBJECTS {
            helper
                .put_object(&bucket, &format!("versioned/file{i}.dat"), vec![b'v'; 100])
                .await;
        }

        // Overwrite each object to create a second version (10 keys x 2 versions = 20 versions)
        for i in 0..NUM_OBJECTS {
            helper
                .put_object(&bucket, &format!("versioned/file{i}.dat"), vec![b'w'; 200])
                .await;
        }

        // Delete 3 objects via normal API to create delete markers (20 versions + 3 markers)
        for i in 0..NUM_DELETE_MARKERS {
            helper
                .client()
                .delete_object()
                .bucket(&bucket)
                .key(format!("versioned/file{i}.dat"))
                .send()
                .await
                .unwrap_or_else(|e| panic!("Failed to create delete marker: {e}"));
        }

        // Verify the expected version count before running the pipeline
        let pre_versions = helper.list_object_versions(&bucket).await;
        let pre_real_versions = pre_versions
            .iter()
            .filter(|(k, _)| !k.starts_with("[delete-marker]"))
            .count();
        let pre_delete_markers = pre_versions
            .iter()
            .filter(|(k, _)| k.starts_with("[delete-marker]"))
            .count();
        assert_eq!(
            pre_real_versions,
            NUM_OBJECTS * VERSIONS_PER_OBJECT,
            "Should have {expected} object versions before pipeline; got {pre_real_versions}",
            expected = NUM_OBJECTS * VERSIONS_PER_OBJECT
        );
        assert_eq!(
            pre_delete_markers, NUM_DELETE_MARKERS,
            "Should have {NUM_DELETE_MARKERS} delete markers before pipeline; got {pre_delete_markers}"
        );

        // Run with --delete-all-versions (should delete all versions + delete markers)
        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/versioned/"),
            "--delete-all-versions",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(
            result.stats.stats_deleted_objects,
            EXPECTED_TOTAL_ITEMS,
            "Should delete all {EXPECTED_TOTAL_ITEMS} items ({versions} versions + {markers} delete markers)",
            versions = NUM_OBJECTS * VERSIONS_PER_OBJECT,
            markers = NUM_DELETE_MARKERS
        );

        // Verify nothing remains
        let versions = helper.list_object_versions(&bucket).await;
        assert!(
            versions.is_empty(),
            "No versions or delete markers should remain; found {}",
            versions.len()
        );
        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// 29.23 Delete All Versions on Unversioned Bucket Error
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_delete_all_versions_unversioned_bucket_succeeds() {
    e2e_timeout!(async {
        // Purpose: Verify that --delete-all-versions on a non-versioned bucket
        //          silently ignores the flag and deletes objects normally.
        // Setup:   Create standard (non-versioned) bucket; upload 5 objects.
        // Expected: Pipeline succeeds; all objects are deleted.
        //
        // Validates: Requirement 5.6

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await; // non-versioned

        let guard = helper.bucket_guard(&bucket);

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
            !result.has_error,
            "Pipeline should succeed with --delete-all-versions on non-versioned bucket"
        );

        // All objects should be deleted
        let remaining = helper.count_objects(&bucket, "data/").await;
        assert_eq!(remaining, 0, "All objects should be deleted");
        guard.cleanup().await;
    });
}
