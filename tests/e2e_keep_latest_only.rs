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

// ---------------------------------------------------------------------------
// Keep Latest Only: Single version per key (nothing to delete)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_keep_latest_only_single_version_per_key() {
    e2e_timeout!(async {
        // Purpose: Verify that --keep-latest-only deletes nothing when every
        //          object has only a single version (which is always the latest).
        // Setup:   Create versioned bucket; upload 5 objects with one version each.
        // Expected: 0 deletions; all 5 versions remain with their original IDs.

        const NUM_OBJECTS: usize = 5;

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_versioned_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        // Upload one version per object (all are latest)
        let mut version_ids = Vec::new();
        for i in 0..NUM_OBJECTS {
            let vid = put_object_versioned(
                &helper,
                &bucket,
                &format!("data/file{i}.dat"),
                vec![b'a'; 100],
            )
            .await;
            version_ids.push(vid);
        }

        let pre_versions = helper.list_object_versions(&bucket).await;
        assert_eq!(
            pre_versions.len(),
            NUM_OBJECTS,
            "Should have {} versions before pipeline",
            NUM_OBJECTS
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
            result.stats.stats_deleted_objects, 0,
            "Should delete 0 objects (all are latest)"
        );

        // Verify: all original version IDs still present
        let post_versions = helper.list_object_versions(&bucket).await;
        assert_eq!(
            post_versions.len(),
            NUM_OBJECTS,
            "All {} versions should remain",
            NUM_OBJECTS
        );

        let remaining_vids: Vec<&str> = post_versions.iter().map(|(_, vid)| vid.as_str()).collect();
        for (i, vid) in version_ids.iter().enumerate() {
            assert!(
                remaining_vids.contains(&vid.as_str()),
                "Version ID for file{i}.dat ({vid}) should still exist"
            );
        }

        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// Keep Latest Only: Many versions per key (>2)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_keep_latest_only_many_versions_per_key() {
    e2e_timeout!(async {
        // Purpose: Verify that --keep-latest-only correctly deletes ALL non-latest
        //          versions when objects have more than 2 versions each.
        // Setup:   Create versioned bucket; upload 3 objects with 4 versions each
        //          (12 total). Only the 4th version of each key is latest.
        // Expected: 9 old versions deleted; 3 latest versions remain.

        const NUM_OBJECTS: usize = 3;
        const VERSIONS_PER_OBJECT: usize = 4;

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_versioned_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        // Upload 4 versions per object, track all IDs
        let mut old_version_ids: Vec<String> = Vec::new();
        let mut latest_version_ids: Vec<String> = Vec::new();

        for i in 0..NUM_OBJECTS {
            let key = format!("data/file{i}.dat");
            for v in 0..VERSIONS_PER_OBJECT {
                let vid = put_object_versioned(
                    &helper,
                    &bucket,
                    &key,
                    vec![b'a' + v as u8; 100 * (v + 1)],
                )
                .await;
                if v < VERSIONS_PER_OBJECT - 1 {
                    old_version_ids.push(vid);
                } else {
                    latest_version_ids.push(vid);
                }
            }
        }

        let pre_versions = helper.list_object_versions(&bucket).await;
        assert_eq!(
            pre_versions.len(),
            NUM_OBJECTS * VERSIONS_PER_OBJECT,
            "Should have {} versions before pipeline",
            NUM_OBJECTS * VERSIONS_PER_OBJECT
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
            result.stats.stats_deleted_objects,
            (NUM_OBJECTS * (VERSIONS_PER_OBJECT - 1)) as u64,
            "Should delete {} old versions",
            NUM_OBJECTS * (VERSIONS_PER_OBJECT - 1)
        );

        let post_versions = helper.list_object_versions(&bucket).await;
        assert_eq!(
            post_versions.len(),
            NUM_OBJECTS,
            "Should have {} versions remaining (one per key)",
            NUM_OBJECTS
        );

        let remaining_vids: Vec<&str> = post_versions.iter().map(|(_, vid)| vid.as_str()).collect();

        // Verify: latest version IDs retained
        for (i, vid) in latest_version_ids.iter().enumerate() {
            assert!(
                remaining_vids.contains(&vid.as_str()),
                "Latest version ID for file{i}.dat ({vid}) should be retained"
            );
        }

        // Verify: ALL old version IDs gone
        for (idx, vid) in old_version_ids.iter().enumerate() {
            assert!(
                !remaining_vids.contains(&vid.as_str()),
                "Old version ID #{idx} ({vid}) should have been deleted"
            );
        }

        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// Keep Latest Only: Dry-run mode composition
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_keep_latest_only_dry_run() {
    e2e_timeout!(async {
        // Purpose: Verify that --dry-run with --keep-latest-only simulates
        //          deletions without actually removing any versions.
        // Setup:   Create versioned bucket; upload 5 objects with 2 versions each.
        // Expected: Stats report 5 simulated deletions; all 10 versions remain
        //           with their original version IDs.

        const NUM_OBJECTS: usize = 5;

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_versioned_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        let mut all_version_ids = Vec::new();
        for i in 0..NUM_OBJECTS {
            let key = format!("data/file{i}.dat");
            let v1 = put_object_versioned(&helper, &bucket, &key, vec![b'v'; 100]).await;
            all_version_ids.push(v1);
            let v2 = put_object_versioned(&helper, &bucket, &key, vec![b'w'; 200]).await;
            all_version_ids.push(v2);
        }

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/data/"),
            "--keep-latest-only",
            "--delete-all-versions",
            "--dry-run",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(
            !result.has_error,
            "Dry-run pipeline should complete without errors"
        );
        assert_eq!(
            result.stats.stats_deleted_objects, NUM_OBJECTS as u64,
            "Dry-run stats should report {} simulated deletions",
            NUM_OBJECTS
        );

        // Verify: ALL versions still exist (dry-run made no actual deletions)
        let post_versions = helper.list_object_versions(&bucket).await;
        assert_eq!(
            post_versions.len(),
            NUM_OBJECTS * 2,
            "All {} versions must still exist after dry-run",
            NUM_OBJECTS * 2
        );

        let remaining_vids: Vec<&str> = post_versions.iter().map(|(_, vid)| vid.as_str()).collect();
        for vid in &all_version_ids {
            assert!(
                remaining_vids.contains(&vid.as_str()),
                "Version ID {vid} must still exist after dry-run"
            );
        }

        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// Keep Latest Only: --max-delete interaction
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_keep_latest_only_max_delete() {
    e2e_timeout!(async {
        // Purpose: Verify that --max-delete correctly limits the number of
        //          non-latest versions deleted by --keep-latest-only.
        // Setup:   Create versioned bucket; upload 10 objects with 2 versions each
        //          (10 non-latest versions). Set --max-delete 5 with --batch-size 1
        //          for deterministic enforcement.
        // Expected: Exactly 5 non-latest versions deleted; pipeline stops at limit.

        const NUM_OBJECTS: usize = 10;
        const MAX_DELETE: u64 = 5;

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_versioned_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        for i in 0..NUM_OBJECTS {
            let key = format!("data/file{i}.dat");
            put_object_versioned(&helper, &bucket, &key, vec![b'v'; 100]).await;
            put_object_versioned(&helper, &bucket, &key, vec![b'w'; 200]).await;
        }

        let pre_versions = helper.list_object_versions(&bucket).await;
        assert_eq!(
            pre_versions.len(),
            NUM_OBJECTS * 2,
            "Should have 20 versions before pipeline"
        );

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/data/"),
            "--keep-latest-only",
            "--delete-all-versions",
            "--max-delete",
            &MAX_DELETE.to_string(),
            "--batch-size",
            "1",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert_eq!(
            result.stats.stats_deleted_objects, MAX_DELETE,
            "Exactly {MAX_DELETE} objects should be deleted (batch-size=1 ensures precise enforcement)"
        );

        // Verify: at least (total - max_delete) versions remain
        let post_versions = helper.list_object_versions(&bucket).await;
        assert!(
            post_versions.len() >= (NUM_OBJECTS * 2 - MAX_DELETE as usize),
            "At least {} versions should remain; got {}",
            NUM_OBJECTS * 2 - MAX_DELETE as usize,
            post_versions.len()
        );

        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// Keep Latest Only: Empty prefix (bucket-wide)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_keep_latest_only_bucket_wide() {
    e2e_timeout!(async {
        // Purpose: Verify that --keep-latest-only works at bucket scope
        //          (no prefix) and processes objects across all prefixes.
        // Setup:   Create versioned bucket; upload objects under varied prefixes
        //          (a/, b/, root-level); overwrite each once.
        // Expected: All old versions across all prefixes deleted; latest retained.

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_versioned_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        let mut old_vids = Vec::new();
        let mut latest_vids = Vec::new();

        // Upload objects across varied prefixes with 2 versions each
        let keys: Vec<String> = (0..3)
            .map(|i| format!("a/file{i}.dat"))
            .chain((0..3).map(|i| format!("b/file{i}.dat")))
            .chain((0..2).map(|i| format!("root-file{i}.dat")))
            .collect();

        for key in &keys {
            let old_vid = put_object_versioned(&helper, &bucket, key, vec![b'x'; 100]).await;
            old_vids.push(old_vid);
            let latest_vid = put_object_versioned(&helper, &bucket, key, vec![b'y'; 200]).await;
            latest_vids.push(latest_vid);
        }

        let total_keys = keys.len();
        let pre_versions = helper.list_object_versions(&bucket).await;
        assert_eq!(
            pre_versions.len(),
            total_keys * 2,
            "Should have {} versions before pipeline",
            total_keys * 2
        );

        // Target the entire bucket (no trailing slash, matching existing pattern)
        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}"),
            "--keep-latest-only",
            "--delete-all-versions",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(
            result.stats.stats_deleted_objects, total_keys as u64,
            "Should delete {} old versions",
            total_keys
        );

        let post_versions = helper.list_object_versions(&bucket).await;
        assert_eq!(
            post_versions.len(),
            total_keys,
            "Should have {} versions remaining (one per key)",
            total_keys
        );

        let remaining_vids: Vec<&str> = post_versions.iter().map(|(_, vid)| vid.as_str()).collect();

        for (i, vid) in latest_vids.iter().enumerate() {
            assert!(
                remaining_vids.contains(&vid.as_str()),
                "Latest version ID for {} ({vid}) should be retained",
                keys[i]
            );
        }
        for (i, vid) in old_vids.iter().enumerate() {
            assert!(
                !remaining_vids.contains(&vid.as_str()),
                "Old version ID for {} ({vid}) should have been deleted",
                keys[i]
            );
        }

        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// Keep Latest Only: --if-match with versioned deletion fails gracefully
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_keep_latest_only_with_if_match() {
    e2e_timeout!(async {
        // Purpose: Verify the interaction between --keep-latest-only and --if-match.
        //          S3's DeleteObject API does not support If-Match conditional
        //          headers when deleting a specific version by version_id. All
        //          versioned deletions with If-Match fail at the API level.
        // Setup:   Create versioned bucket; upload 5 objects with 2 versions each.
        //          Use --if-match --batch-size 1.
        // Expected: All 5 conditional deletion attempts fail (stats_failed_objects=5);
        //           0 actual deletions; all 10 versions remain untouched.

        const NUM_OBJECTS: usize = 5;

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_versioned_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        let mut all_vids = Vec::new();
        for i in 0..NUM_OBJECTS {
            let key = format!("data/file{i}.dat");
            let old_vid = put_object_versioned(&helper, &bucket, &key, vec![b'v'; 100]).await;
            all_vids.push(old_vid);
            let latest_vid = put_object_versioned(&helper, &bucket, &key, vec![b'w'; 200]).await;
            all_vids.push(latest_vid);
        }

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/data/"),
            "--keep-latest-only",
            "--delete-all-versions",
            "--if-match",
            "--batch-size",
            "1",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        // All conditional deletion attempts fail at the S3 API level
        assert_eq!(
            result.stats.stats_deleted_objects, 0,
            "No objects should be deleted (If-Match fails for versioned deletes)"
        );
        assert_eq!(
            result.stats.stats_failed_objects, NUM_OBJECTS as u64,
            "All {} conditional deletion attempts should fail",
            NUM_OBJECTS
        );

        // Verify: all versions remain untouched
        let post_versions = helper.list_object_versions(&bucket).await;
        assert_eq!(
            post_versions.len(),
            NUM_OBJECTS * 2,
            "All {} versions should remain (no deletions occurred)",
            NUM_OBJECTS * 2
        );

        let remaining_vids: Vec<&str> = post_versions.iter().map(|(_, vid)| vid.as_str()).collect();
        for vid in &all_vids {
            assert!(
                remaining_vids.contains(&vid.as_str()),
                "Version ID {vid} should still exist (deletion failed)"
            );
        }

        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// Keep Latest Only: 1000 objects across multiple workers
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_keep_latest_only_1000_objects_multi_worker() {
    // Use a longer timeout for the large-scale test (3 minutes)
    tokio::time::timeout(std::time::Duration::from_secs(180), async {
        // Purpose: Verify that --keep-latest-only correctly handles 1000 objects
        //          distributed across multiple workers. This tests concurrency
        //          safety and batch processing at scale.
        // Setup:   Create versioned bucket; upload 1000 objects with 2 versions each
        //          (2000 total). Use --worker-size 8 for parallel deletion.
        // Expected: 1000 old versions deleted; 1000 latest versions remain;
        //           every latest version ID retained, every old version ID gone.

        const NUM_OBJECTS: usize = 1000;

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_versioned_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        // Upload old versions in parallel for speed
        let old_objects: Vec<(String, Vec<u8>)> = (0..NUM_OBJECTS)
            .map(|i| (format!("data/file{i:04}.dat"), vec![b'v'; 100]))
            .collect();
        helper.put_objects_parallel(&bucket, old_objects).await;

        // Capture old version IDs
        let old_versions = helper.list_object_versions(&bucket).await;
        let old_vids: std::collections::HashSet<String> =
            old_versions.iter().map(|(_, vid)| vid.clone()).collect();
        assert_eq!(
            old_vids.len(),
            NUM_OBJECTS,
            "Should have {NUM_OBJECTS} old version IDs"
        );

        // Upload latest versions in parallel
        let latest_objects: Vec<(String, Vec<u8>)> = (0..NUM_OBJECTS)
            .map(|i| (format!("data/file{i:04}.dat"), vec![b'w'; 200]))
            .collect();
        helper.put_objects_parallel(&bucket, latest_objects).await;

        // Capture latest version IDs (new ones that weren't in old_vids)
        let all_versions = helper.list_object_versions(&bucket).await;
        assert_eq!(
            all_versions.len(),
            NUM_OBJECTS * 2,
            "Should have {} versions before pipeline",
            NUM_OBJECTS * 2
        );
        let latest_vids: std::collections::HashSet<String> = all_versions
            .iter()
            .map(|(_, vid)| vid.clone())
            .filter(|vid| !old_vids.contains(vid))
            .collect();
        assert_eq!(
            latest_vids.len(),
            NUM_OBJECTS,
            "Should have {NUM_OBJECTS} latest version IDs"
        );

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/data/"),
            "--keep-latest-only",
            "--delete-all-versions",
            "--worker-size",
            "8",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(
            result.stats.stats_deleted_objects, NUM_OBJECTS as u64,
            "Should delete {} old versions",
            NUM_OBJECTS
        );

        let post_versions = helper.list_object_versions(&bucket).await;
        assert_eq!(
            post_versions.len(),
            NUM_OBJECTS,
            "Should have {} versions remaining (one per key)",
            NUM_OBJECTS
        );

        let remaining_vids: std::collections::HashSet<String> =
            post_versions.iter().map(|(_, vid)| vid.clone()).collect();

        // Verify: all latest version IDs retained
        for vid in &latest_vids {
            assert!(
                remaining_vids.contains(vid),
                "Latest version ID {vid} should be retained"
            );
        }

        // Verify: all old version IDs gone
        for vid in &old_vids {
            assert!(
                !remaining_vids.contains(vid),
                "Old version ID {vid} should have been deleted"
            );
        }

        guard.cleanup().await;
    })
    .await
    .expect("1000-object test timed out (3 min limit)");
}
