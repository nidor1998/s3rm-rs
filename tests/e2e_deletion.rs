//! E2E tests for core deletion modes (Tests 29.13 - 29.17).
//!
//! Tests basic prefix deletion, batch vs single deletion modes, deleting
//! all bucket contents, and handling empty buckets.

#![cfg(e2e_test)]

mod common;

use common::TestHelper;

// ---------------------------------------------------------------------------
// 29.13 Basic Prefix Deletion
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_basic_prefix_deletion() {
    e2e_timeout!(async {
        // Purpose: Verify that deletion by prefix (the positional target argument)
        //          removes only objects under the specified prefix.
        // Setup:   Upload 30 objects under prefix data/, 10 objects under prefix other/.
        // Expected: All 30 data/ objects deleted; 10 other/ objects remain;
        //           stats show 30 deleted, 0 failed.
        //
        // Validates: Requirements 1.1, 2.1

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        for i in 0..30 {
            helper
                .put_object(&bucket, &format!("data/file{i:03}.dat"), vec![b'd'; 200])
                .await;
        }
        for i in 0..10 {
            helper
                .put_object(&bucket, &format!("other/file{i:03}.dat"), vec![b'o'; 200])
                .await;
        }

        let config = TestHelper::build_config(vec![&format!("s3://{bucket}/data/"), "--force"]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(
            result.stats.stats_deleted_objects, 30,
            "Should delete all 30 data/ objects"
        );
        assert_eq!(
            result.stats.stats_failed_objects, 0,
            "No objects should fail"
        );

        let remaining = helper.list_objects(&bucket, "other/").await;
        assert_eq!(remaining.len(), 10, "All 10 other/ objects should remain");

        let data_remaining = helper.count_objects(&bucket, "data/").await;
        assert_eq!(data_remaining, 0, "No data/ objects should remain");
        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// 29.14 Batch Deletion Mode
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_batch_deletion_mode() {
    e2e_timeout!(async {
        // Purpose: Verify batch deletion mode works correctly with a custom
        //          batch size. The --batch-size option controls how many objects
        //          are sent in each S3 DeleteObjects API call.
        // Setup:   Upload 500 objects under a single prefix.
        // Expected: All 500 objects deleted; stats show 500 deleted, 0 failed.
        //
        // Validates: Requirements 1.1, 1.9

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        let objects: Vec<(String, Vec<u8>)> = (0..500)
            .map(|i| (format!("batch/file{i:04}.dat"), vec![b'b'; 100]))
            .collect();
        helper.put_objects_parallel(&bucket, objects).await;

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/batch/"),
            "--batch-size",
            "100",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(
            result.stats.stats_deleted_objects, 500,
            "Should delete all 500 objects"
        );
        assert_eq!(
            result.stats.stats_failed_objects, 0,
            "No objects should fail"
        );

        let remaining = helper.count_objects(&bucket, "batch/").await;
        assert_eq!(remaining, 0, "No objects should remain");
        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// 29.15 Single Deletion Mode
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_single_deletion_mode() {
    e2e_timeout!(async {
        // Purpose: Verify single-object deletion mode (--batch-size 1) uses the
        //          S3 DeleteObject API (one object at a time) instead of batch.
        // Setup:   Upload 20 objects.
        // Expected: All 20 objects deleted; stats show 20 deleted, 0 failed.
        //           Objects are actually removed from S3.
        //
        // Validates: Requirement 1.2

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        for i in 0..20 {
            helper
                .put_object(&bucket, &format!("single/file{i:02}.dat"), vec![b's'; 100])
                .await;
        }

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/single/"),
            "--batch-size",
            "1",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(
            result.stats.stats_deleted_objects, 20,
            "Should delete all 20 objects"
        );
        assert_eq!(
            result.stats.stats_failed_objects, 0,
            "No objects should fail"
        );

        // Verify objects are actually removed from S3
        let remaining = helper.count_objects(&bucket, "single/").await;
        assert_eq!(
            remaining, 0,
            "All objects should be removed from S3 after single deletion"
        );

        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// 29.16 Delete Entire Bucket Contents
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_delete_entire_bucket_contents() {
    e2e_timeout!(async {
        // Purpose: Verify that targeting a bucket without a prefix (s3://bucket)
        //          deletes all objects in the bucket, regardless of key structure.
        // Setup:   Upload 50 objects with varied prefixes (a/, b/, c/, root-level).
        // Expected: All 50 objects deleted; bucket is empty; stats show 50 deleted.
        //
        // Validates: Requirement 2.10

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        // Upload objects across varied prefixes
        for i in 0..15 {
            helper
                .put_object(&bucket, &format!("a/file{i}.dat"), vec![b'a'; 100])
                .await;
        }
        for i in 0..15 {
            helper
                .put_object(&bucket, &format!("b/file{i}.dat"), vec![b'b'; 100])
                .await;
        }
        for i in 0..10 {
            helper
                .put_object(&bucket, &format!("c/file{i}.dat"), vec![b'c'; 100])
                .await;
        }
        for i in 0..10 {
            helper
                .put_object(&bucket, &format!("root-file{i}.dat"), vec![b'r'; 100])
                .await;
        }

        let config = TestHelper::build_config(vec![&format!("s3://{bucket}"), "--force"]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(
            result.stats.stats_deleted_objects, 50,
            "Should delete all 50 objects"
        );

        let remaining = helper.count_objects(&bucket, "").await;
        assert_eq!(remaining, 0, "Bucket should be empty");
        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// 29.17 Empty Bucket No Error
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_empty_bucket_no_error() {
    e2e_timeout!(async {
        // Purpose: Verify that running the pipeline against an empty bucket
        //          completes successfully without errors. Edge case: no objects
        //          to process should not be treated as an error.
        // Setup:   Create empty bucket (no objects uploaded).
        // Expected: Pipeline completes without error; stats show 0 deleted, 0 failed.
        //
        // Validates: Requirement 1.8

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        let config = TestHelper::build_config(vec![&format!("s3://{bucket}"), "--force"]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(
            !result.has_error,
            "Pipeline on empty bucket should not error"
        );
        assert_eq!(
            result.stats.stats_deleted_objects, 0,
            "No objects to delete"
        );
        assert_eq!(
            result.stats.stats_failed_objects, 0,
            "No objects should fail"
        );
        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// Prefix-scoped deletion: verify only targeted objects are deleted
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_batch_deletion_respects_prefix_boundary() {
    e2e_timeout!(async {
        // Purpose: Verify that batch deletion (default mode) with a prefix only
        //          deletes objects under that prefix and leaves objects outside
        //          completely untouched — including objects with overlapping key
        //          prefixes (e.g., "data/" vs "data-archive/").
        // Setup:   Upload objects under three prefixes: data/, data-archive/, other/.
        // Expected: Only data/ objects are deleted; data-archive/ and other/ remain.
        //           Actual S3 state is verified, not just stats.
        //
        // Validates: Requirements 1.1, 2.1 (batch mode)

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

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

        // Verify data/ objects are actually gone
        let data_remaining = helper.count_objects(&bucket, "data/").await;
        assert_eq!(
            data_remaining, 0,
            "All data/ objects should be removed from S3"
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

        guard.cleanup().await;
    });
}

#[tokio::test]
async fn e2e_single_deletion_respects_prefix_boundary() {
    e2e_timeout!(async {
        // Purpose: Verify that single deletion mode (--batch-size 1) with a prefix
        //          only deletes objects under that prefix. This exercises the
        //          SingleDeleter code path with delete_object (not delete_objects).
        // Setup:   Upload objects under three prefixes: logs/, logs-backup/, keep/.
        // Expected: Only logs/ objects are deleted; logs-backup/ and keep/ remain.
        //           Actual S3 state is verified, not just stats.
        //
        // Validates: Requirements 1.2, 2.1 (single mode)

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        // Target prefix: 5 objects under logs/
        for i in 0..5 {
            helper
                .put_object(&bucket, &format!("logs/entry{i}.log"), vec![b'L'; 128])
                .await;
        }
        // Overlapping prefix name: 3 objects under logs-backup/
        for i in 0..3 {
            helper
                .put_object(
                    &bucket,
                    &format!("logs-backup/entry{i}.log"),
                    vec![b'B'; 128],
                )
                .await;
        }
        // Unrelated prefix: 2 objects under keep/
        for i in 0..2 {
            helper
                .put_object(&bucket, &format!("keep/file{i}.dat"), vec![b'K'; 128])
                .await;
        }

        let total_before = helper.count_objects(&bucket, "").await;
        assert_eq!(total_before, 10, "Should start with 10 objects");

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/logs/"),
            "--batch-size",
            "1",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(
            result.stats.stats_deleted_objects, 5,
            "Should delete all 5 logs/ objects"
        );
        assert_eq!(
            result.stats.stats_failed_objects, 0,
            "No objects should fail"
        );

        // Verify logs/ objects are actually gone
        let logs_remaining = helper.count_objects(&bucket, "logs/").await;
        assert_eq!(
            logs_remaining, 0,
            "All logs/ objects should be removed from S3"
        );

        // Verify logs-backup/ objects are untouched
        let backup_remaining = helper.count_objects(&bucket, "logs-backup/").await;
        assert_eq!(
            backup_remaining, 3,
            "logs-backup/ objects must not be affected by logs/ deletion"
        );

        // Verify keep/ objects are untouched
        let keep_remaining = helper.count_objects(&bucket, "keep/").await;
        assert_eq!(keep_remaining, 2, "keep/ objects must remain");

        // Total remaining = 3 + 2 = 5
        let total_after = helper.count_objects(&bucket, "").await;
        assert_eq!(
            total_after, 5,
            "Only 5 logs/ objects should have been removed"
        );

        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// Literal prefix: '?', '#', percent escapes and dot segments
// ---------------------------------------------------------------------------

/// Deletes each of `prefixes` in turn and checks that exactly the objects under that
/// prefix are removed, while the objects under every other prefix remain.
async fn assert_prefixes_are_deleted_literally(
    all_prefixes: &[&str],
    prefixes_to_delete: &[&str],
    extra_args: &[&str],
) {
    const OBJECTS_PER_PREFIX: usize = 2;

    let helper = TestHelper::new().await;
    let bucket = helper.generate_bucket_name();
    helper.create_bucket(&bucket).await;

    let guard = helper.bucket_guard(&bucket);

    for prefix in all_prefixes {
        for i in 0..OBJECTS_PER_PREFIX {
            helper
                .put_object(&bucket, &format!("{prefix}file{i}.dat"), vec![b'x'; 10])
                .await;
        }
    }

    let mut expected_keys: Vec<String> = helper.list_objects(&bucket, "").await;
    expected_keys.sort();
    assert_eq!(
        expected_keys.len(),
        all_prefixes.len() * OBJECTS_PER_PREFIX,
        "Every test object should be created under its own key"
    );

    for prefix in prefixes_to_delete {
        let target = format!("s3://{bucket}/{prefix}");
        let mut args = vec![target.as_str(), "--force"];
        args.extend_from_slice(extra_args);

        let result = TestHelper::run_pipeline(TestHelper::build_config(args)).await;

        assert!(
            !result.has_error,
            "Pipeline should complete without errors for prefix {prefix}: {:?}",
            result.errors
        );
        assert_eq!(
            result.stats.stats_deleted_objects, OBJECTS_PER_PREFIX as u64,
            "Only the objects under {prefix} should be deleted"
        );
        assert_eq!(
            result.stats.stats_failed_objects, 0,
            "No objects should fail for prefix {prefix}"
        );

        expected_keys.retain(|key| !key.starts_with(prefix));
        let mut remaining_keys = helper.list_objects(&bucket, "").await;
        remaining_keys.sort();
        assert_eq!(
            remaining_keys, expected_keys,
            "Objects under other prefixes must not be affected by deleting {prefix}"
        );
    }

    guard.cleanup().await;
}

// Each prefix to delete has a neighbor that URL parsing would have confused it with:
// "data#old/" and "data?old/" would be truncated to "data", "a%20b/" would be decoded
// to "a b/", and "data%23old/" would be decoded to "data#old/".
const LITERAL_PREFIXES: [&str; 7] = [
    "data#old/",
    "data?old/",
    "data/",
    "data%23old/",
    "a%20b/",
    "a b/",
    "%FF/",
];

const LITERAL_PREFIXES_TO_DELETE: [&str; 4] = ["data#old/", "data?old/", "a%20b/", "%FF/"];

#[tokio::test]
async fn e2e_batch_deletion_uses_prefix_literally() {
    e2e_timeout!(async {
        // Purpose: Verify that '?', '#' and percent escapes in the target are part of
        //          the prefix and are not interpreted as URL syntax (DeleteObjects).
        // Setup:   2 objects under each of 7 prefixes that differ only in such characters.
        // Expected: Deleting a prefix removes its 2 objects and nothing else.

        assert_prefixes_are_deleted_literally(&LITERAL_PREFIXES, &LITERAL_PREFIXES_TO_DELETE, &[])
            .await;
    });
}

#[tokio::test]
async fn e2e_single_deletion_uses_prefix_literally() {
    e2e_timeout!(async {
        // Purpose: Same as the batch test, with the DeleteObject API (--batch-size 1),
        //          where the key is sent in the request path instead of the body.
        // Setup:   2 objects under each of 7 prefixes that differ only in such characters.
        // Expected: Deleting a prefix removes its 2 objects and nothing else.

        assert_prefixes_are_deleted_literally(
            &LITERAL_PREFIXES,
            &LITERAL_PREFIXES_TO_DELETE,
            &["--batch-size", "1"],
        )
        .await;
    });
}

#[tokio::test]
async fn e2e_deletion_does_not_collapse_dot_segments_in_prefix() {
    e2e_timeout!(async {
        // Purpose: Verify that "." and ".." segments in the target are part of the
        //          prefix and are not collapsed ("a/../b/" must not become "b/").
        // Setup:   2 objects under each of "a/../b/", "b/", "./c/" and "c/".
        // Expected: Deleting "a/../b/" and "./c/" leaves "b/" and "c/" untouched.

        assert_prefixes_are_deleted_literally(
            &["a/../b/", "b/", "./c/", "c/"],
            &["a/../b/", "./c/"],
            &[],
        )
        .await;
    });
}
