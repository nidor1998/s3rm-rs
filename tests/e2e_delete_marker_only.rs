//! E2E tests for the --filter-delete-marker-only feature.
//!
//! Tests that --filter-delete-marker-only deletes only delete markers while
//! retaining all object versions (both latest and non-latest) in versioned buckets.

#![cfg(e2e_test)]

mod common;

use common::TestHelper;

/// Helper: put an object and return its version ID.
async fn put_object_versioned(
    helper: &TestHelper,
    bucket: &str,
    key: &str,
    body: Vec<u8>,
) -> String {
    let resp = helper
        .client()
        .put_object()
        .bucket(bucket)
        .key(key)
        .body(body.into())
        .send()
        .await
        .unwrap_or_else(|e| panic!("Failed to put object {key} in {bucket}: {e}"));
    resp.version_id()
        .unwrap_or_else(|| panic!("No version ID returned for {key}"))
        .to_string()
}

/// Helper: delete an object via normal API and return the delete marker's version ID.
async fn create_delete_marker(helper: &TestHelper, bucket: &str, key: &str) -> String {
    let resp = helper
        .client()
        .delete_object()
        .bucket(bucket)
        .key(key)
        .send()
        .await
        .unwrap_or_else(|e| panic!("Failed to create delete marker for {key}: {e}"));
    resp.version_id()
        .unwrap_or_else(|| panic!("No version ID for delete marker {key}"))
        .to_string()
}

// ---------------------------------------------------------------------------
// Basic: Only delete markers are deleted, object versions are retained
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_filter_delete_marker_only_basic() {
    e2e_timeout!(async {
        // Purpose: Verify that --filter-delete-marker-only deletes only delete markers
        //          and retains all object versions.
        // Setup:   Create versioned bucket; upload 5 objects; delete 3 via normal API
        //          (creates 3 delete markers). Total: 5 versions + 3 markers = 8 items.
        // Expected: 3 delete markers deleted; 5 object versions remain; each original
        //           version ID is still present.

        const NUM_OBJECTS: usize = 5;
        const NUM_DELETED: usize = 3;

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_versioned_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        // Upload objects and capture version IDs
        let mut version_ids = Vec::new();
        for i in 0..NUM_OBJECTS {
            let vid = put_object_versioned(
                &helper,
                &bucket,
                &format!("data/file{i}.dat"),
                vec![b'v'; 100],
            )
            .await;
            version_ids.push(vid);
        }

        // Delete first 3 objects via normal API (creates delete markers)
        let mut delete_marker_ids = Vec::new();
        for i in 0..NUM_DELETED {
            let dm_vid =
                create_delete_marker(&helper, &bucket, &format!("data/file{i}.dat")).await;
            delete_marker_ids.push(dm_vid);
        }

        // Verify pre-conditions: 5 versions + 3 delete markers = 8 items
        let pre_versions = helper.list_object_versions(&bucket).await;
        let pre_markers = pre_versions
            .iter()
            .filter(|(k, _)| k.starts_with("[delete-marker]"))
            .count();
        let pre_objects = pre_versions
            .iter()
            .filter(|(k, _)| !k.starts_with("[delete-marker]"))
            .count();
        assert_eq!(pre_markers, NUM_DELETED, "Should have {NUM_DELETED} delete markers before");
        assert_eq!(pre_objects, NUM_OBJECTS, "Should have {NUM_OBJECTS} object versions before");

        // Run with --filter-delete-marker-only
        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/data/"),
            "--filter-delete-marker-only",
            "--delete-all-versions",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(
            result.stats.stats_deleted_objects, NUM_DELETED as u64,
            "Should delete only {NUM_DELETED} delete markers"
        );

        // Verify: no delete markers remain
        let post_versions = helper.list_object_versions(&bucket).await;
        let post_markers: Vec<_> = post_versions
            .iter()
            .filter(|(k, _)| k.starts_with("[delete-marker]"))
            .collect();
        assert!(
            post_markers.is_empty(),
            "No delete markers should remain; found {}",
            post_markers.len()
        );

        // Verify: all original object versions are retained
        let post_objects: Vec<_> = post_versions
            .iter()
            .filter(|(k, _)| !k.starts_with("[delete-marker]"))
            .collect();
        assert_eq!(
            post_objects.len(),
            NUM_OBJECTS,
            "All {NUM_OBJECTS} object versions should be retained"
        );

        // Verify: retained version IDs match the originals
        let remaining_vids: Vec<&str> = post_objects.iter().map(|(_, vid)| vid.as_str()).collect();
        for (i, expected_vid) in version_ids.iter().enumerate() {
            assert!(
                remaining_vids.contains(&expected_vid.as_str()),
                "Version ID for file{i}.dat ({expected_vid}) should be retained"
            );
        }

        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// No delete markers: nothing is deleted
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_filter_delete_marker_only_no_markers() {
    e2e_timeout!(async {
        // Purpose: Verify that --filter-delete-marker-only with no delete markers
        //          results in zero deletions (all items are filtered out).
        // Setup:   Create versioned bucket; upload 5 objects with 2 versions each.
        //          No delete markers present. Total: 10 object versions.
        // Expected: 0 deletions; all 10 versions remain.

        const NUM_OBJECTS: usize = 5;

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_versioned_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        // Upload 2 versions per object
        for i in 0..NUM_OBJECTS {
            put_object_versioned(
                &helper,
                &bucket,
                &format!("data/file{i}.dat"),
                vec![b'v'; 100],
            )
            .await;
            put_object_versioned(
                &helper,
                &bucket,
                &format!("data/file{i}.dat"),
                vec![b'w'; 200],
            )
            .await;
        }

        let pre_versions = helper.list_object_versions(&bucket).await;
        assert_eq!(pre_versions.len(), 10, "Should have 10 versions before");

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/data/"),
            "--filter-delete-marker-only",
            "--delete-all-versions",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(
            result.stats.stats_deleted_objects, 0,
            "Should delete 0 objects (no delete markers)"
        );

        // Verify: all 10 versions remain
        let post_versions = helper.list_object_versions(&bucket).await;
        assert_eq!(
            post_versions.len(),
            10,
            "All 10 versions should remain unchanged"
        );

        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// All items are delete markers: all are deleted
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_filter_delete_marker_only_all_markers() {
    e2e_timeout!(async {
        // Purpose: Verify that when all items in the version listing are delete markers,
        //          --filter-delete-marker-only deletes all of them.
        // Setup:   Create versioned bucket; upload 5 objects; delete all via normal API
        //          (creates 5 delete markers); then delete ALL versions with --delete-all-versions
        //          first to remove the original versions, leaving only delete markers.
        //          Actually — simpler: upload 5 objects, delete each via normal API,
        //          then use --filter-delete-marker-only. This keeps both versions + markers.
        //          But we want ONLY markers — so we manually delete the object versions first.
        // Adjusted: Upload 5 objects, create delete markers for all, then permanently
        //           delete the original object versions via versioned delete.
        //           Remaining: 5 delete markers only.
        // Expected: 5 delete markers deleted; bucket is empty.

        const NUM_OBJECTS: usize = 5;

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_versioned_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        // Upload objects and capture version IDs
        let mut original_vids = Vec::new();
        for i in 0..NUM_OBJECTS {
            let vid = put_object_versioned(
                &helper,
                &bucket,
                &format!("data/file{i}.dat"),
                vec![b'v'; 100],
            )
            .await;
            original_vids.push((format!("data/file{i}.dat"), vid));
        }

        // Create delete markers for all objects
        for i in 0..NUM_OBJECTS {
            create_delete_marker(&helper, &bucket, &format!("data/file{i}.dat")).await;
        }

        // Permanently delete the original object versions (not the delete markers)
        for (key, vid) in &original_vids {
            helper
                .client()
                .delete_object()
                .bucket(&bucket)
                .key(key)
                .version_id(vid)
                .send()
                .await
                .unwrap_or_else(|e| panic!("Failed to permanently delete {key} version {vid}: {e}"));
        }

        // Verify: only delete markers remain
        let pre_versions = helper.list_object_versions(&bucket).await;
        assert_eq!(pre_versions.len(), NUM_OBJECTS, "Should have {NUM_OBJECTS} items");
        let pre_markers = pre_versions
            .iter()
            .filter(|(k, _)| k.starts_with("[delete-marker]"))
            .count();
        assert_eq!(
            pre_markers, NUM_OBJECTS,
            "All {NUM_OBJECTS} items should be delete markers"
        );

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/data/"),
            "--filter-delete-marker-only",
            "--delete-all-versions",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(
            result.stats.stats_deleted_objects, NUM_OBJECTS as u64,
            "Should delete all {NUM_OBJECTS} delete markers"
        );

        // Verify: bucket is empty
        let post_versions = helper.list_object_versions(&bucket).await;
        assert!(
            post_versions.is_empty(),
            "No items should remain; found {}",
            post_versions.len()
        );

        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// Combined with --filter-include-regex: only matching delete markers deleted
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_filter_delete_marker_only_with_include_regex() {
    e2e_timeout!(async {
        // Purpose: Verify that --filter-delete-marker-only combined with
        //          --filter-include-regex only deletes delete markers whose keys
        //          match the regex.
        // Setup:   Create versioned bucket; upload 5 .log and 5 .dat files;
        //          delete all via normal API (creates 10 delete markers).
        //          Use include regex to target only .log files.
        // Expected: Only 5 .log delete markers deleted; 5 .dat delete markers
        //           and all 10 original versions remain.

        const NUM_PER_TYPE: usize = 5;

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_versioned_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        // Upload .log files
        for i in 0..NUM_PER_TYPE {
            put_object_versioned(
                &helper,
                &bucket,
                &format!("data/file{i}.log"),
                vec![b'a'; 100],
            )
            .await;
        }

        // Upload .dat files
        for i in 0..NUM_PER_TYPE {
            put_object_versioned(
                &helper,
                &bucket,
                &format!("data/file{i}.dat"),
                vec![b'b'; 100],
            )
            .await;
        }

        // Create delete markers for all files
        for i in 0..NUM_PER_TYPE {
            create_delete_marker(&helper, &bucket, &format!("data/file{i}.log")).await;
        }
        for i in 0..NUM_PER_TYPE {
            create_delete_marker(&helper, &bucket, &format!("data/file{i}.dat")).await;
        }

        // Pre-conditions: 10 versions + 10 delete markers = 20 items
        let pre_versions = helper.list_object_versions(&bucket).await;
        assert_eq!(pre_versions.len(), 20, "Should have 20 items before pipeline");

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/data/"),
            "--filter-delete-marker-only",
            "--filter-include-regex",
            r"\.log$",
            "--delete-all-versions",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(
            result.stats.stats_deleted_objects, NUM_PER_TYPE as u64,
            "Should delete only {NUM_PER_TYPE} .log delete markers"
        );

        // Verify: .log delete markers are gone
        let post_versions = helper.list_object_versions(&bucket).await;
        let log_markers: Vec<_> = post_versions
            .iter()
            .filter(|(k, _)| k.starts_with("[delete-marker]") && k.contains(".log"))
            .collect();
        assert!(
            log_markers.is_empty(),
            ".log delete markers should be deleted; found {}",
            log_markers.len()
        );

        // Verify: .dat delete markers remain
        let dat_markers: Vec<_> = post_versions
            .iter()
            .filter(|(k, _)| k.starts_with("[delete-marker]") && k.contains(".dat"))
            .collect();
        assert_eq!(
            dat_markers.len(),
            NUM_PER_TYPE,
            ".dat delete markers should remain"
        );

        // Verify: all 10 original object versions remain
        let object_versions: Vec<_> = post_versions
            .iter()
            .filter(|(k, _)| !k.starts_with("[delete-marker]"))
            .collect();
        assert_eq!(
            object_versions.len(),
            NUM_PER_TYPE * 2,
            "All 10 original versions should remain"
        );

        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// Dry run: no delete markers are actually deleted
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_filter_delete_marker_only_dry_run() {
    e2e_timeout!(async {
        // Purpose: Verify that --filter-delete-marker-only with --dry-run reports
        //          what would be deleted without actually deleting anything.
        // Setup:   Create versioned bucket; upload 5 objects; delete 3 via normal API.
        //          Total: 5 versions + 3 markers = 8 items.
        // Expected: Dry run reports 3 deletions; S3 state unchanged (8 items remain).

        const NUM_OBJECTS: usize = 5;
        const NUM_DELETED: usize = 3;

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_versioned_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        for i in 0..NUM_OBJECTS {
            put_object_versioned(
                &helper,
                &bucket,
                &format!("data/file{i}.dat"),
                vec![b'v'; 100],
            )
            .await;
        }
        for i in 0..NUM_DELETED {
            create_delete_marker(&helper, &bucket, &format!("data/file{i}.dat")).await;
        }

        let pre_versions = helper.list_object_versions(&bucket).await;
        assert_eq!(pre_versions.len(), NUM_OBJECTS + NUM_DELETED, "Pre-condition check");

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/data/"),
            "--filter-delete-marker-only",
            "--delete-all-versions",
            "--dry-run",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Dry run should complete without errors");

        // Verify: S3 state is unchanged
        let post_versions = helper.list_object_versions(&bucket).await;
        assert_eq!(
            post_versions.len(),
            pre_versions.len(),
            "Dry run should not modify S3 state"
        );

        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// Multiple versions with delete markers: only markers removed
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_filter_delete_marker_only_multiple_versions_and_markers() {
    e2e_timeout!(async {
        // Purpose: Verify correct behavior with multiple object versions AND
        //          multiple delete markers per key (upload → delete → re-upload → delete).
        // Setup:   Create versioned bucket with 3 keys, each having:
        //            v1 (original) → dm1 (delete marker) → v2 (re-upload) → dm2 (delete marker)
        //          Total per key: 2 versions + 2 delete markers = 4 items.
        //          Total: 3 keys × 4 items = 12 items.
        // Expected: 6 delete markers deleted; 6 object versions remain.

        const NUM_KEYS: usize = 3;

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_versioned_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        let mut all_version_ids = Vec::new();
        let mut all_marker_ids = Vec::new();

        for i in 0..NUM_KEYS {
            let key = format!("data/file{i}.dat");

            // v1
            let v1 = put_object_versioned(&helper, &bucket, &key, vec![b'a'; 100]).await;
            all_version_ids.push(v1);

            // dm1
            let dm1 = create_delete_marker(&helper, &bucket, &key).await;
            all_marker_ids.push(dm1);

            // v2 (re-upload after delete)
            let v2 = put_object_versioned(&helper, &bucket, &key, vec![b'b'; 200]).await;
            all_version_ids.push(v2);

            // dm2
            let dm2 = create_delete_marker(&helper, &bucket, &key).await;
            all_marker_ids.push(dm2);
        }

        // Verify: 12 items total (6 versions + 6 markers)
        let pre_versions = helper.list_object_versions(&bucket).await;
        assert_eq!(pre_versions.len(), NUM_KEYS * 4, "Should have 12 items before");
        let pre_marker_count = pre_versions
            .iter()
            .filter(|(k, _)| k.starts_with("[delete-marker]"))
            .count();
        assert_eq!(pre_marker_count, NUM_KEYS * 2, "Should have 6 delete markers");

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/data/"),
            "--filter-delete-marker-only",
            "--delete-all-versions",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(
            result.stats.stats_deleted_objects,
            (NUM_KEYS * 2) as u64,
            "Should delete all 6 delete markers"
        );

        // Verify: no delete markers remain
        let post_versions = helper.list_object_versions(&bucket).await;
        let post_markers: Vec<_> = post_versions
            .iter()
            .filter(|(k, _)| k.starts_with("[delete-marker]"))
            .collect();
        assert!(
            post_markers.is_empty(),
            "No delete markers should remain; found {}",
            post_markers.len()
        );

        // Verify: all 6 object versions are retained
        let post_objects: Vec<_> = post_versions
            .iter()
            .filter(|(k, _)| !k.starts_with("[delete-marker]"))
            .collect();
        assert_eq!(
            post_objects.len(),
            NUM_KEYS * 2,
            "All 6 object versions should be retained"
        );

        // Verify: version IDs match
        let remaining_vids: Vec<&str> = post_objects.iter().map(|(_, vid)| vid.as_str()).collect();
        for vid in &all_version_ids {
            assert!(
                remaining_vids.contains(&vid.as_str()),
                "Version ID {vid} should be retained"
            );
        }

        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// Empty versioned bucket: completes with 0 deletions
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_filter_delete_marker_only_empty_bucket() {
    e2e_timeout!(async {
        // Purpose: Verify that --filter-delete-marker-only on an empty versioned bucket
        //          completes successfully with 0 deletions and no errors.
        // Setup:   Create versioned bucket, upload nothing.
        // Expected: Pipeline succeeds, 0 deletions, no errors.

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_versioned_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/"),
            "--filter-delete-marker-only",
            "--delete-all-versions",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(
            !result.has_error,
            "Pipeline should complete without errors on empty bucket"
        );
        assert_eq!(
            result.stats.stats_deleted_objects, 0,
            "No objects to delete in empty bucket"
        );
        assert_eq!(result.stats.stats_failed_objects, 0, "No failures expected");

        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// Prefix boundary: only delete markers under the target prefix are deleted
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_filter_delete_marker_only_respects_prefix() {
    e2e_timeout!(async {
        // Purpose: Verify that --filter-delete-marker-only only affects delete markers
        //          under the specified prefix and leaves other prefixes untouched.
        // Setup:   Create versioned bucket with two prefixes:
        //          - target/: 3 objects + 3 delete markers
        //          - other/:  3 objects + 3 delete markers
        // Expected: Only target/ delete markers deleted; other/ completely untouched.

        const NUM_PER_PREFIX: usize = 3;

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_versioned_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        // Setup target/ prefix
        for i in 0..NUM_PER_PREFIX {
            put_object_versioned(
                &helper,
                &bucket,
                &format!("target/file{i}.dat"),
                vec![b't'; 100],
            )
            .await;
        }
        for i in 0..NUM_PER_PREFIX {
            create_delete_marker(&helper, &bucket, &format!("target/file{i}.dat")).await;
        }

        // Setup other/ prefix
        for i in 0..NUM_PER_PREFIX {
            put_object_versioned(
                &helper,
                &bucket,
                &format!("other/file{i}.dat"),
                vec![b'o'; 100],
            )
            .await;
        }
        for i in 0..NUM_PER_PREFIX {
            create_delete_marker(&helper, &bucket, &format!("other/file{i}.dat")).await;
        }

        // Verify: 12 items total (6 per prefix: 3 versions + 3 markers)
        let pre_versions = helper.list_object_versions(&bucket).await;
        assert_eq!(pre_versions.len(), 12, "Should have 12 items before pipeline");

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/target/"),
            "--filter-delete-marker-only",
            "--delete-all-versions",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(
            result.stats.stats_deleted_objects, NUM_PER_PREFIX as u64,
            "Should delete only {NUM_PER_PREFIX} target/ delete markers"
        );

        // Verify: target/ delete markers are gone
        let post_versions = helper.list_object_versions(&bucket).await;
        let target_markers: Vec<_> = post_versions
            .iter()
            .filter(|(k, _)| k.starts_with("[delete-marker]") && k.contains("target/"))
            .collect();
        assert!(
            target_markers.is_empty(),
            "target/ delete markers should be deleted"
        );

        // Verify: target/ object versions remain
        let target_versions: Vec<_> = post_versions
            .iter()
            .filter(|(k, _)| !k.starts_with("[delete-marker]") && k.contains("target/"))
            .collect();
        assert_eq!(
            target_versions.len(),
            NUM_PER_PREFIX,
            "target/ object versions should be retained"
        );

        // Verify: other/ is completely untouched (3 versions + 3 markers)
        let other_items: Vec<_> = post_versions
            .iter()
            .filter(|(k, _)| {
                let key = k.strip_prefix("[delete-marker]").unwrap_or(k);
                key.starts_with("other/")
            })
            .collect();
        assert_eq!(
            other_items.len(),
            NUM_PER_PREFIX * 2,
            "other/ should be completely untouched (versions + markers)"
        );

        guard.cleanup().await;
    });
}
