//! E2E tests for optimistic locking (If-Match) support (Tests 29.31 - 29.32).
//!
//! Tests the --if-match flag with both single and batch deletion modes.

#![cfg(e2e_test)]

mod common;

use common::TestHelper;

// ---------------------------------------------------------------------------
// 29.31 If-Match Single Deletion
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_if_match_single_deletion() {
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

    let _guard = helper.bucket_guard(&bucket);

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
}

// ---------------------------------------------------------------------------
// 29.32 If-Match Batch Deletion
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_if_match_batch_deletion() {
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

    let _guard = helper.bucket_guard(&bucket);

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
}
