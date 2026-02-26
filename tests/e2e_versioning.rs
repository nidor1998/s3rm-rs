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
// Versioned Bucket: Prefix Boundary Respect
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_batch_deletion_respects_prefix_boundary_versioned() {
    e2e_timeout!(async {
        // Purpose: Verify that batch deletion (default mode) with a prefix only
        //          deletes objects under that prefix and leaves objects outside
        //          completely untouched — including objects with overlapping key
        //          prefixes (e.g., "data/" vs "data-archive/") — on a versioned bucket.
        // Setup:   Create versioned bucket; upload objects under three prefixes:
        //          data/, data-archive/, other/.
        // Expected: Only data/ objects are deleted (delete markers created);
        //           data-archive/ and other/ remain untouched.
        //           Actual S3 state is verified, not just stats.
        //
        // Validates: Requirements 1.1, 2.1, 5.1 (batch mode, versioned bucket)

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_versioned_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        // Target prefix: 5 objects under data/
        for i in 0..5 {
            helper
                .put_object(&bucket, &format!("data/item{i}.txt"), vec![b'd'; 256])
                .await;
        }
        // Overlapping prefix name: 3 objects under data-archive/
        for i in 0..3 {
            helper
                .put_object(
                    &bucket,
                    &format!("data-archive/item{i}.txt"),
                    vec![b'a'; 256],
                )
                .await;
        }
        // Unrelated prefix: 4 objects under other/
        for i in 0..4 {
            helper
                .put_object(&bucket, &format!("other/item{i}.txt"), vec![b'o'; 256])
                .await;
        }

        let total_before = helper.count_objects(&bucket, "").await;
        assert_eq!(total_before, 12, "Should start with 12 objects");

        let config = TestHelper::build_config(vec![&format!("s3://{bucket}/data/"), "--force"]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(
            result.stats.stats_deleted_objects, 5,
            "Should delete all 5 data/ objects"
        );
        assert_eq!(
            result.stats.stats_failed_objects, 0,
            "No objects should fail"
        );

        // Verify data/ objects are hidden by delete markers
        let data_remaining = helper.count_objects(&bucket, "data/").await;
        assert_eq!(
            data_remaining, 0,
            "All data/ objects should be hidden by delete markers"
        );

        // Verify data-archive/ objects are untouched
        let archive_remaining = helper.count_objects(&bucket, "data-archive/").await;
        assert_eq!(
            archive_remaining, 3,
            "data-archive/ objects must not be affected by data/ deletion"
        );

        // Verify other/ objects are untouched
        let other_remaining = helper.count_objects(&bucket, "other/").await;
        assert_eq!(other_remaining, 4, "other/ objects must remain");

        // Total remaining = 3 + 4 = 7
        let total_after = helper.count_objects(&bucket, "").await;
        assert_eq!(
            total_after, 7,
            "Only 5 data/ objects should have been removed"
        );

        // Verify original versions still exist (versioned bucket creates delete markers)
        let versions = helper.list_object_versions(&bucket).await;
        let data_delete_markers: Vec<_> = versions
            .iter()
            .filter(|(k, _)| k.starts_with("[delete-marker]") && k.contains("data/item"))
            .collect();
        let data_original_versions: Vec<_> = versions
            .iter()
            .filter(|(k, _)| !k.starts_with("[delete-marker]") && k.contains("data/item"))
            .collect();
        assert_eq!(
            data_delete_markers.len(),
            5,
            "Should have 5 delete markers for data/ objects"
        );
        assert_eq!(
            data_original_versions.len(),
            5,
            "Original data/ versions should still exist"
        );

        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// Versioned Bucket: Parallel list_object_versions with Multi-Level Prefixes
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_delete_all_versions_with_parallel_listing() {
    e2e_timeout!(async {
        // Purpose: Verify that --delete-all-versions combined with
        //          --max-parallel-listings actually exercises the
        //          list_object_versions_with_parallel code path. The bucket
        //          contains objects spread across multiple nested prefix levels
        //          so that the parallel lister discovers common prefixes via
        //          the "/" delimiter and fans out into sub-prefix tasks.
        // Setup:   Create a versioned bucket; upload objects under 4 top-level
        //          prefixes, each with 2 sub-prefixes (3 levels deep), plus
        //          overwrite some objects to create multiple versions.
        //          Total: 4 prefixes × 2 sub-prefixes × 5 objects = 40 keys,
        //          10 of which are overwritten once = 50 versions total.
        // Expected: All 50 versions are deleted; nothing remains in the bucket;
        //           parallel listing does not cause errors or missed objects.
        //
        // Validates: Requirements 1.5, 1.6, 1.7, 5.2 (parallel versioned listing)

        const TOP_PREFIXES: usize = 4;
        const SUB_PREFIXES: usize = 2;
        const OBJECTS_PER_SUB: usize = 5;
        const OVERWRITES: usize = 10; // first 10 keys get a second version

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_versioned_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        // Upload objects across nested prefixes: level1/level2/fileN.dat
        let mut all_keys: Vec<String> = Vec::new();
        let objects: Vec<(String, Vec<u8>)> = (0..TOP_PREFIXES)
            .flat_map(|t| {
                (0..SUB_PREFIXES).flat_map(move |s| {
                    (0..OBJECTS_PER_SUB).map(move |i| {
                        (
                            format!("area{t}/sub{s}/file{i:02}.dat"),
                            vec![b'v'; 100],
                        )
                    })
                })
            })
            .collect();
        for (key, _) in &objects {
            all_keys.push(key.clone());
        }
        helper.put_objects_parallel(&bucket, objects).await;

        // Overwrite the first OVERWRITES keys to create a second version
        let overwrites: Vec<(String, Vec<u8>)> = all_keys
            .iter()
            .take(OVERWRITES)
            .map(|k| (k.clone(), vec![b'w'; 200]))
            .collect();
        helper.put_objects_parallel(&bucket, overwrites).await;

        let total_keys = TOP_PREFIXES * SUB_PREFIXES * OBJECTS_PER_SUB;
        let expected_versions = (total_keys + OVERWRITES) as u64; // 40 + 10 = 50

        // Verify version count before deletion
        let pre_versions = helper.list_object_versions(&bucket).await;
        assert_eq!(
            pre_versions.len() as u64, expected_versions,
            "Should have {expected_versions} total versions before deletion"
        );

        // Run with --delete-all-versions AND parallel listing enabled
        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/"),
            "--delete-all-versions",
            "--max-parallel-listings",
            "4",
            "--max-parallel-listing-max-depth",
            "2",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(
            !result.has_error,
            "Pipeline should complete without errors"
        );
        assert_eq!(
            result.stats.stats_deleted_objects, expected_versions,
            "Should delete all {expected_versions} versions"
        );
        assert_eq!(
            result.stats.stats_failed_objects, 0,
            "No objects should fail"
        );

        // Verify nothing remains
        let post_versions = helper.list_object_versions(&bucket).await;
        assert!(
            post_versions.is_empty(),
            "No versions or delete markers should remain; found {}",
            post_versions.len()
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
