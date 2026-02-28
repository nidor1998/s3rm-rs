//! E2E tests for the --keep-latest-only feature.
//!
//! Tests that --keep-latest-only deletes all non-latest versions while
//! retaining the latest version of each object in versioned buckets.

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

// ---------------------------------------------------------------------------
// Keep Latest Only: Deletes old versions, retains latest
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_keep_latest_only_deletes_old_versions() {
    e2e_timeout!(async {
        // Purpose: Verify that --keep-latest-only deletes all non-latest versions
        //          and retains only the latest version of each object.
        // Setup:   Create versioned bucket; upload 5 objects; overwrite each once
        //          (creates 2 versions per key = 10 versions total).
        // Expected: 5 old versions deleted; 5 latest versions remain; each key
        //           has exactly 1 version left with the correct version ID.

        const NUM_OBJECTS: usize = 5;
        const VERSIONS_PER_OBJECT: usize = 2;

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_versioned_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        // Upload initial versions (old — should be deleted)
        let mut old_version_ids = Vec::new();
        for i in 0..NUM_OBJECTS {
            let vid = put_object_versioned(
                &helper,
                &bucket,
                &format!("data/file{i}.dat"),
                vec![b'v'; 100],
            )
            .await;
            old_version_ids.push(vid);
        }

        // Overwrite each object to create a second version (latest — should be kept)
        let mut latest_version_ids = Vec::new();
        for i in 0..NUM_OBJECTS {
            let vid = put_object_versioned(
                &helper,
                &bucket,
                &format!("data/file{i}.dat"),
                vec![b'w'; 200],
            )
            .await;
            latest_version_ids.push(vid);
        }

        // Verify: 10 versions before pipeline
        let pre_versions = helper.list_object_versions(&bucket).await;
        assert_eq!(
            pre_versions.len(),
            NUM_OBJECTS * VERSIONS_PER_OBJECT,
            "Should have {} versions before pipeline",
            NUM_OBJECTS * VERSIONS_PER_OBJECT
        );

        // Run with --keep-latest-only --delete-all-versions
        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/data/"),
            "--keep-latest-only",
            "--delete-all-versions",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(
            result.stats.stats_deleted_objects, NUM_OBJECTS as u64,
            "Should delete {} old versions",
            NUM_OBJECTS
        );

        // Verify: only latest versions remain (one per key)
        let post_versions = helper.list_object_versions(&bucket).await;
        assert_eq!(
            post_versions.len(),
            NUM_OBJECTS,
            "Should have exactly {} versions remaining (one per key)",
            NUM_OBJECTS
        );

        // Verify: retained version IDs are exactly the latest ones
        let remaining_vids: Vec<&str> = post_versions.iter().map(|(_, vid)| vid.as_str()).collect();
        for (i, expected_vid) in latest_version_ids.iter().enumerate() {
            assert!(
                remaining_vids.contains(&expected_vid.as_str()),
                "Latest version ID for file{i}.dat ({expected_vid}) should be retained"
            );
        }

        // Verify: old version IDs are gone
        for (i, old_vid) in old_version_ids.iter().enumerate() {
            assert!(
                !remaining_vids.contains(&old_vid.as_str()),
                "Old version ID for file{i}.dat ({old_vid}) should have been deleted"
            );
        }

        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// Keep Latest Only: Also removes non-latest delete markers
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_keep_latest_only_with_delete_markers() {
    e2e_timeout!(async {
        // Purpose: Verify that --keep-latest-only handles delete markers correctly.
        //          Non-latest delete markers should be deleted; latest delete markers
        //          should be retained.
        // Setup:   Create versioned bucket; upload 5 objects; delete 3 via normal API
        //          (creates delete markers as latest); overwrite the remaining 2.
        //          Version state:
        //            file0-file2: original version (non-latest) + delete marker (latest)
        //            file3-file4: original version (non-latest) + v2 (latest)
        //          Total: 10 items, 5 non-latest, 5 latest
        // Expected: Non-latest versions are deleted. Latest items (delete markers
        //           and v2 objects) remain, verified by version ID.

        const NUM_OBJECTS: usize = 5;
        const NUM_DELETED: usize = 3;

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_versioned_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        // Upload initial versions for all objects (non-latest — should be deleted)
        let mut old_version_ids = Vec::new();
        for i in 0..NUM_OBJECTS {
            let vid = put_object_versioned(
                &helper,
                &bucket,
                &format!("data/file{i}.dat"),
                vec![b'v'; 100],
            )
            .await;
            old_version_ids.push(vid);
        }

        // Delete first 3 objects via normal API (creates delete markers as latest)
        let mut delete_marker_ids = Vec::new();
        for i in 0..NUM_DELETED {
            let resp = helper
                .client()
                .delete_object()
                .bucket(&bucket)
                .key(format!("data/file{i}.dat"))
                .send()
                .await
                .unwrap_or_else(|e| panic!("Failed to create delete marker: {e}"));
            let dm_vid = resp
                .version_id()
                .unwrap_or_else(|| panic!("No version ID for delete marker file{i}.dat"))
                .to_string();
            delete_marker_ids.push(dm_vid);
        }

        // Overwrite remaining 2 objects to create a second version (latest)
        let mut latest_version_ids = Vec::new();
        for i in NUM_DELETED..NUM_OBJECTS {
            let vid = put_object_versioned(
                &helper,
                &bucket,
                &format!("data/file{i}.dat"),
                vec![b'w'; 200],
            )
            .await;
            latest_version_ids.push(vid);
        }

        // Version state:
        //   file0: v1(non-latest) + dm(latest) = 2 items
        //   file1: v1(non-latest) + dm(latest) = 2 items
        //   file2: v1(non-latest) + dm(latest) = 2 items
        //   file3: v1(non-latest) + v2(latest) = 2 items
        //   file4: v1(non-latest) + v2(latest) = 2 items
        // Total: 10 items, 5 non-latest, 5 latest

        let pre_versions = helper.list_object_versions(&bucket).await;
        assert_eq!(
            pre_versions.len(),
            10,
            "Should have 10 items before pipeline"
        );

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/data/"),
            "--keep-latest-only",
            "--delete-all-versions",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(
            result.stats.stats_deleted_objects, 5,
            "Should delete 5 non-latest items"
        );

        // Verify: 5 latest items remain (3 delete markers + 2 v2 objects)
        let post_versions = helper.list_object_versions(&bucket).await;
        assert_eq!(
            post_versions.len(),
            5,
            "Should have 5 latest items remaining"
        );

        // Collect all remaining version IDs (both versions and delete markers)
        let remaining_vids: Vec<&str> = post_versions.iter().map(|(_, vid)| vid.as_str()).collect();

        // Verify: delete marker version IDs are retained
        for (i, dm_vid) in delete_marker_ids.iter().enumerate() {
            assert!(
                remaining_vids.contains(&dm_vid.as_str()),
                "Delete marker version ID for file{i}.dat ({dm_vid}) should be retained"
            );
        }

        // Verify: latest v2 version IDs (file3, file4) are retained
        for (idx, latest_vid) in latest_version_ids.iter().enumerate() {
            let file_idx = NUM_DELETED + idx;
            assert!(
                remaining_vids.contains(&latest_vid.as_str()),
                "Latest version ID for file{file_idx}.dat ({latest_vid}) should be retained"
            );
        }

        // Verify: old version IDs are gone
        for (i, old_vid) in old_version_ids.iter().enumerate() {
            assert!(
                !remaining_vids.contains(&old_vid.as_str()),
                "Old version ID for file{i}.dat ({old_vid}) should have been deleted"
            );
        }

        // Verify: remaining version entries include 3 delete markers
        let dm_count = post_versions
            .iter()
            .filter(|(key, _)| key.starts_with("[delete-marker]"))
            .count();
        assert_eq!(
            dm_count, NUM_DELETED,
            "Should have {} delete markers remaining",
            NUM_DELETED
        );

        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// Keep Latest Only: Works with --filter-include-regex
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_keep_latest_only_with_include_regex() {
    e2e_timeout!(async {
        // Purpose: Verify that --keep-latest-only works with --filter-include-regex.
        //          Only objects matching the regex should be processed; among those,
        //          only non-latest versions should be deleted.
        // Setup:   Create versioned bucket; upload 5 .log and 5 .dat files;
        //          overwrite each once. Use include regex to target only .log files.
        // Expected: Only old .log versions deleted; latest .log versions and all
        //           .dat versions remain, verified by version ID.

        const NUM_PER_TYPE: usize = 5;

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_versioned_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        // Upload .log files (2 versions each), capture version IDs
        let mut old_log_vids = Vec::new();
        let mut latest_log_vids = Vec::new();
        for i in 0..NUM_PER_TYPE {
            let old_vid = put_object_versioned(
                &helper,
                &bucket,
                &format!("data/file{i}.log"),
                vec![b'a'; 100],
            )
            .await;
            old_log_vids.push(old_vid);
            let latest_vid = put_object_versioned(
                &helper,
                &bucket,
                &format!("data/file{i}.log"),
                vec![b'b'; 200],
            )
            .await;
            latest_log_vids.push(latest_vid);
        }

        // Upload .dat files (2 versions each), capture version IDs
        let mut old_dat_vids = Vec::new();
        let mut latest_dat_vids = Vec::new();
        for i in 0..NUM_PER_TYPE {
            let old_vid = put_object_versioned(
                &helper,
                &bucket,
                &format!("data/file{i}.dat"),
                vec![b'c'; 100],
            )
            .await;
            old_dat_vids.push(old_vid);
            let latest_vid = put_object_versioned(
                &helper,
                &bucket,
                &format!("data/file{i}.dat"),
                vec![b'd'; 200],
            )
            .await;
            latest_dat_vids.push(latest_vid);
        }

        // Total: 20 versions (10 .log + 10 .dat)
        let pre_versions = helper.list_object_versions(&bucket).await;
        assert_eq!(
            pre_versions.len(),
            20,
            "Should have 20 versions before pipeline"
        );

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/data/"),
            "--keep-latest-only",
            "--delete-all-versions",
            "--filter-include-regex",
            r".*\.log$",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(
            result.stats.stats_deleted_objects, NUM_PER_TYPE as u64,
            "Should delete {} old .log versions",
            NUM_PER_TYPE
        );

        // Verify: .dat files untouched (10 versions), .log files have 5 versions left
        let post_versions = helper.list_object_versions(&bucket).await;
        assert_eq!(
            post_versions.len(),
            15,
            "Should have 15 versions remaining (5 latest .log + 10 .dat)"
        );

        let remaining_vids: Vec<&str> = post_versions.iter().map(|(_, vid)| vid.as_str()).collect();

        // Verify: latest .log version IDs are retained
        for (i, vid) in latest_log_vids.iter().enumerate() {
            assert!(
                remaining_vids.contains(&vid.as_str()),
                "Latest .log version ID for file{i}.log ({vid}) should be retained"
            );
        }

        // Verify: old .log version IDs are gone
        for (i, vid) in old_log_vids.iter().enumerate() {
            assert!(
                !remaining_vids.contains(&vid.as_str()),
                "Old .log version ID for file{i}.log ({vid}) should have been deleted"
            );
        }

        // Verify: ALL .dat version IDs (both old and latest) are retained (untouched)
        for (i, vid) in old_dat_vids.iter().enumerate() {
            assert!(
                remaining_vids.contains(&vid.as_str()),
                "Old .dat version ID for file{i}.dat ({vid}) should be retained (untouched)"
            );
        }
        for (i, vid) in latest_dat_vids.iter().enumerate() {
            assert!(
                remaining_vids.contains(&vid.as_str()),
                "Latest .dat version ID for file{i}.dat ({vid}) should be retained (untouched)"
            );
        }

        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// Keep Latest Only: Only affects objects under the target prefix
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_keep_latest_only_respects_prefix_boundary() {
    e2e_timeout!(async {
        // Purpose: Verify that --keep-latest-only only processes objects under
        //          the target prefix. Objects outside the prefix must be
        //          completely untouched (all versions retained).
        // Setup:   Create versioned bucket; upload 3 objects under "target/"
        //          and 3 objects under "other/"; overwrite each once.
        //          Run pipeline targeting only "target/".
        // Expected: Old versions under "target/" deleted; latest retained.
        //           ALL versions under "other/" remain untouched.

        const NUM_PER_PREFIX: usize = 3;

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_versioned_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        // Upload objects under "target/" prefix (2 versions each)
        let mut target_old_vids = Vec::new();
        let mut target_latest_vids = Vec::new();
        for i in 0..NUM_PER_PREFIX {
            let old_vid = put_object_versioned(
                &helper,
                &bucket,
                &format!("target/file{i}.dat"),
                vec![b'a'; 100],
            )
            .await;
            target_old_vids.push(old_vid);
            let latest_vid = put_object_versioned(
                &helper,
                &bucket,
                &format!("target/file{i}.dat"),
                vec![b'b'; 200],
            )
            .await;
            target_latest_vids.push(latest_vid);
        }

        // Upload objects under "other/" prefix (2 versions each)
        let mut other_old_vids = Vec::new();
        let mut other_latest_vids = Vec::new();
        for i in 0..NUM_PER_PREFIX {
            let old_vid = put_object_versioned(
                &helper,
                &bucket,
                &format!("other/file{i}.dat"),
                vec![b'c'; 100],
            )
            .await;
            other_old_vids.push(old_vid);
            let latest_vid = put_object_versioned(
                &helper,
                &bucket,
                &format!("other/file{i}.dat"),
                vec![b'd'; 200],
            )
            .await;
            other_latest_vids.push(latest_vid);
        }

        // Total: 12 versions (6 target + 6 other)
        let pre_versions = helper.list_object_versions(&bucket).await;
        assert_eq!(
            pre_versions.len(),
            NUM_PER_PREFIX * 2 * 2,
            "Should have 12 versions before pipeline"
        );

        // Run targeting only "target/" prefix
        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/target/"),
            "--keep-latest-only",
            "--delete-all-versions",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(
            result.stats.stats_deleted_objects, NUM_PER_PREFIX as u64,
            "Should delete {} old versions under target/",
            NUM_PER_PREFIX
        );

        // Verify: 9 versions remain (3 latest target + 6 untouched other)
        let post_versions = helper.list_object_versions(&bucket).await;
        assert_eq!(
            post_versions.len(),
            NUM_PER_PREFIX + NUM_PER_PREFIX * 2,
            "Should have 9 versions remaining"
        );

        let remaining_vids: Vec<&str> = post_versions.iter().map(|(_, vid)| vid.as_str()).collect();

        // Verify: target/ latest version IDs are retained
        for (i, vid) in target_latest_vids.iter().enumerate() {
            assert!(
                remaining_vids.contains(&vid.as_str()),
                "Latest target version ID for file{i}.dat ({vid}) should be retained"
            );
        }

        // Verify: target/ old version IDs are gone
        for (i, vid) in target_old_vids.iter().enumerate() {
            assert!(
                !remaining_vids.contains(&vid.as_str()),
                "Old target version ID for file{i}.dat ({vid}) should have been deleted"
            );
        }

        // Verify: ALL other/ version IDs are retained (completely untouched)
        for (i, vid) in other_old_vids.iter().enumerate() {
            assert!(
                remaining_vids.contains(&vid.as_str()),
                "Old other/ version ID for file{i}.dat ({vid}) should be retained (untouched)"
            );
        }
        for (i, vid) in other_latest_vids.iter().enumerate() {
            assert!(
                remaining_vids.contains(&vid.as_str()),
                "Latest other/ version ID for file{i}.dat ({vid}) should be retained (untouched)"
            );
        }

        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// Keep Latest Only: Rejects non-versioned bucket
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_keep_latest_only_rejects_non_versioned_bucket() {
    e2e_timeout!(async {
        // Purpose: Verify that --keep-latest-only returns an error when the
        //          target bucket does not have versioning enabled.
        // Setup:   Create standard (non-versioned) bucket; upload 5 objects.
        // Expected: Pipeline reports error; objects remain untouched.

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
            "--keep-latest-only",
            "--delete-all-versions",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(
            result.has_error,
            "Pipeline should report error for --keep-latest-only on non-versioned bucket"
        );

        // Objects should still exist (deletion was not performed)
        let remaining = helper.count_objects(&bucket, "data/").await;
        assert_eq!(remaining, 5, "All objects should remain untouched");

        guard.cleanup().await;
    });
}
