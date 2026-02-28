//! E2E tests for optimistic locking (If-Match) support (Tests 29.31 - 29.32a).
//!
//! Tests the --if-match flag with both single and batch deletion modes,
//! including ETag mismatch handling for modified objects.

#![cfg(e2e_test)]

mod common;

use common::TestHelper;
use s3rm_rs::{FilterCallback, S3Object};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

// ---------------------------------------------------------------------------
// 29.31 If-Match Single Deletion
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_if_match_single_deletion() {
    e2e_timeout!(async {
        // Purpose: Verify --if-match with single deletion mode (--batch-size 1).
        //          When if-match is enabled, the pipeline uses each object's ETag
        //          in the conditional deletion request. Since objects are not
        //          modified between listing and deletion, all deletions should
        //          succeed.
        // Setup:   Upload 10 objects.
        // Expected: All 10 objects deleted; stats show 10 deleted, 0 failed.
        //
        // Validates: Requirements 11.1, 11.3

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        for i in 0..10 {
            helper
                .put_object(&bucket, &format!("ifmatch/file{i}.dat"), vec![b'i'; 100])
                .await;
        }

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/ifmatch/"),
            "--if-match",
            "--batch-size",
            "1",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(
            result.stats.stats_deleted_objects, 10,
            "Should delete all 10 objects with if-match"
        );
        assert_eq!(result.stats.stats_failed_objects, 0, "No failures expected");

        let remaining = helper.count_objects(&bucket, "ifmatch/").await;
        assert_eq!(
            remaining, 0,
            "All ifmatch/ objects should be removed from S3"
        );
        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// 29.32 If-Match Batch Deletion
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_if_match_batch_deletion() {
    e2e_timeout!(async {
        // Purpose: Verify --if-match with batch deletion mode. Per-object ETags
        //          are included in the batch DeleteObjects request for conditional
        //          deletion.
        // Setup:   Upload 20 objects.
        // Expected: All 20 objects deleted; ETags match for unmodified objects.
        //
        // Validates: Requirements 11.1, 11.4

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        for i in 0..20 {
            helper
                .put_object(
                    &bucket,
                    &format!("ifmatch-batch/file{i:02}.dat"),
                    vec![b'b'; 200],
                )
                .await;
        }

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/ifmatch-batch/"),
            "--if-match",
            "--batch-size",
            "10",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(
            result.stats.stats_deleted_objects, 20,
            "Should delete all 20 objects with if-match in batch mode"
        );

        let remaining = helper.count_objects(&bucket, "ifmatch-batch/").await;
        assert_eq!(
            remaining, 0,
            "All ifmatch-batch/ objects should be removed from S3"
        );
        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// If-Match on Versioned Bucket with --delete-all-versions — Single Deletion
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_if_match_versioned_bucket_single_deletion() {
    e2e_timeout!(async {
        // Purpose: Verify --if-match + --delete-all-versions on a versioned bucket
        //          with --batch-size 1 (single deleter path). S3's DeleteObject API
        //          does not support If-Match conditional headers when deleting a
        //          specific version by version_id. All versioned conditional deletion
        //          attempts fail at the API level.
        // Setup:   Create versioned bucket; upload 10 objects (1 version each).
        // Expected: All 10 conditional deletion attempts fail (stats_failed_objects=10);
        //           0 actual deletions; all 10 versions remain untouched.
        //
        // Validates: Requirements 11.1, 11.2, 11.3

        const NUM_OBJECTS: usize = 10;

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_versioned_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        for i in 0..NUM_OBJECTS {
            helper
                .put_object(
                    &bucket,
                    &format!("ifmatch-ver-single/file{i:02}.dat"),
                    vec![b'v'; 150],
                )
                .await;
        }

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/ifmatch-ver-single/"),
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

        // All versions must remain untouched
        let versions = helper.list_object_versions(&bucket).await;
        assert_eq!(
            versions.len(),
            NUM_OBJECTS,
            "All {} versions should remain after failed conditional deletes",
            NUM_OBJECTS
        );

        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// If-Match on Versioned Bucket with --delete-all-versions — Batch Deletion
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_if_match_versioned_bucket_batch_deletion() {
    e2e_timeout!(async {
        // Purpose: Verify --if-match + --delete-all-versions on a versioned bucket
        //          with the default batch size (batch deleter path). S3's DeleteObjects
        //          API does not support If-Match conditional headers when deleting
        //          specific versions by version_id. All versioned conditional deletion
        //          attempts fail at the API level.
        // Setup:   Create versioned bucket; upload 20 objects (1 version each).
        // Expected: All 20 conditional deletion attempts fail (stats_failed_objects=20);
        //           0 actual deletions; all 20 versions remain untouched.
        //
        // Validates: Requirements 11.1, 11.2, 11.4

        const NUM_OBJECTS: usize = 20;

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_versioned_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        for i in 0..NUM_OBJECTS {
            helper
                .put_object(
                    &bucket,
                    &format!("ifmatch-ver-batch/file{i:02}.dat"),
                    vec![b'b'; 200],
                )
                .await;
        }

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/ifmatch-ver-batch/"),
            "--delete-all-versions",
            "--if-match",
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

        // All versions must remain untouched
        let versions = helper.list_object_versions(&bucket).await;
        assert_eq!(
            versions.len(),
            NUM_OBJECTS,
            "All {} versions should remain after failed conditional deletes",
            NUM_OBJECTS
        );

        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// Helper: Filter callback that modifies objects during filtering
// ---------------------------------------------------------------------------

/// A Rust filter callback that overwrites specific objects (changing their ETags)
/// during the filtering phase, then returns `true` for all objects so they all
/// proceed to the deletion stage. Objects modified here will have stale ETags
/// in the pipeline, causing If-Match deletions to fail.
struct ModifyDuringFilterCallback {
    client: aws_sdk_s3::Client,
    bucket: String,
    /// Keys of objects to overwrite during filtering.
    keys_to_modify: HashSet<String>,
    /// Track which keys were actually modified (for assertions).
    modified: Arc<Mutex<Vec<String>>>,
}

#[async_trait::async_trait]
impl FilterCallback for ModifyDuringFilterCallback {
    async fn filter(&mut self, object: &S3Object) -> anyhow::Result<bool> {
        let key = object.key().to_string();
        if self.keys_to_modify.contains(&key) {
            // Overwrite the object with new content, changing its ETag
            self.client
                .put_object()
                .bucket(&self.bucket)
                .key(&key)
                .body(vec![b'M'; 999].into())
                .send()
                .await
                .map_err(|e| anyhow::anyhow!("Failed to modify object {key}: {e}"))?;
            self.modified.lock().unwrap().push(key);
        }
        Ok(true) // Pass all objects to deletion
    }
}

// ---------------------------------------------------------------------------
// 29.32a If-Match ETag Mismatch Skips Modified Objects
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_if_match_etag_mismatch_skips_modified_objects() {
    e2e_timeout!(async {
        // Purpose: Verify --if-match correctly skips objects whose ETags changed
        //          after listing. A filter callback overwrites 3 objects during the
        //          filtering phase, changing their ETags. When the pipeline then
        //          attempts to delete these objects with the stale ETags, the
        //          If-Match condition should fail and the objects should be skipped.
        // Setup:   Upload 10 objects. Register a filter callback that overwrites 3
        //          specific objects (changing their ETags) during filtering, then
        //          returns true for all objects.
        // Expected: 7 unmodified objects deleted; 3 modified objects remain (ETag
        //           mismatch causes skip); stats show 7 deleted; the modified
        //           objects still exist in the bucket.
        //
        // Validates: Requirement 11.2

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        for i in 0..10 {
            helper
                .put_object(
                    &bucket,
                    &format!("ifmatch-mismatch/file{i:02}.dat"),
                    vec![b'O'; 200],
                )
                .await;
        }

        // These 3 objects will be overwritten during the filter phase
        let keys_to_modify: HashSet<String> = [
            "ifmatch-mismatch/file02.dat",
            "ifmatch-mismatch/file05.dat",
            "ifmatch-mismatch/file08.dat",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        let modified_keys = Arc::new(Mutex::new(Vec::new()));
        let filter_callback = ModifyDuringFilterCallback {
            client: helper.client().clone(),
            bucket: bucket.clone(),
            keys_to_modify,
            modified: Arc::clone(&modified_keys),
        };

        let mut config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/ifmatch-mismatch/"),
            "--if-match",
            "--batch-size",
            "1",
            "--force",
        ]);
        config.filter_manager.register_callback(filter_callback);

        let result = TestHelper::run_pipeline(config).await;

        // Verify the filter callback actually modified 3 objects
        {
            let modified = modified_keys.lock().unwrap();
            assert_eq!(modified.len(), 3, "Should have modified 3 objects");
        }

        // 7 objects with matching ETags should be deleted
        assert_eq!(
            result.stats.stats_deleted_objects, 7,
            "Should delete 7 unmodified objects (3 skipped due to ETag mismatch)"
        );

        // 3 modified objects should have failed (ETag mismatch)
        assert_eq!(
            result.stats.stats_failed_objects, 3,
            "Should have 3 failed deletions due to ETag mismatch"
        );

        // The 3 modified objects should still exist in the bucket
        let remaining = helper.list_objects(&bucket, "ifmatch-mismatch/").await;
        assert_eq!(
            remaining.len(),
            3,
            "3 modified objects should remain in the bucket"
        );
        assert!(remaining.contains(&"ifmatch-mismatch/file02.dat".to_string()));
        assert!(remaining.contains(&"ifmatch-mismatch/file05.dat".to_string()));
        assert!(remaining.contains(&"ifmatch-mismatch/file08.dat".to_string()));
        guard.cleanup().await;
    });
}
