//! E2E tests for performance configuration options (Tests 29.33 - 29.37).
//!
//! Tests worker-size, rate-limit, parallel listings, queue size, and max-keys.

#![cfg(e2e_test)]

mod common;

use common::TestHelper;

// ---------------------------------------------------------------------------
// 29.33 Worker Size Configuration
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_worker_size_configuration() {
    e2e_timeout!(async {
        // Purpose: Verify --worker-size correctly configures the number of
        //          concurrent deletion workers. The pipeline should complete
        //          successfully with a custom worker count.
        // Setup:   Upload 100 objects.
        // Expected: All 100 objects deleted; stats show 100 deleted.
        //
        // Validates: Requirements 1.3, 1.4

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let _guard = helper.bucket_guard(&bucket);

        for i in 0..100 {
            helper
                .put_object(&bucket, &format!("workers/file{i:03}.dat"), vec![b'w'; 100])
                .await;
        }

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/workers/"),
            "--worker-size",
            "4",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(
            result.stats.stats_deleted_objects, 100,
            "Should delete all 100 objects"
        );
    });
}

// ---------------------------------------------------------------------------
// 29.34 Rate Limit Objects
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_rate_limit_objects() {
    e2e_timeout!(async {
        // Purpose: Verify --rate-limit-objects enforces a rate limit on deletions
        //          without preventing pipeline completion. The rate limiter should
        //          slow down deletions but not block them.
        // Setup:   Upload 50 objects.
        // Expected: All 50 objects deleted; pipeline completes (rate limiting
        //           does not prevent completion).
        //
        // Validates: Requirements 8.7, 8.8

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let _guard = helper.bucket_guard(&bucket);

        for i in 0..50 {
            helper
                .put_object(
                    &bucket,
                    &format!("ratelimit/file{i:02}.dat"),
                    vec![b'r'; 100],
                )
                .await;
        }

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/ratelimit/"),
            "--rate-limit-objects",
            "200",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(
            result.stats.stats_deleted_objects, 50,
            "Should delete all 50 objects"
        );
    });
}

// ---------------------------------------------------------------------------
// 29.35 Max Parallel Listings
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_max_parallel_listings() {
    e2e_timeout!(async {
        // Purpose: Verify --max-parallel-listings and --max-parallel-listing-max-depth
        //          configure parallel listing without causing errors.
        // Setup:   Upload 100 objects with 5 different top-level prefixes (20 each).
        // Expected: All 100 objects deleted; parallel listing config does not
        //           cause errors.
        //
        // Validates: Requirements 1.5, 1.6, 1.7

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let _guard = helper.bucket_guard(&bucket);

        for prefix_idx in 0..5 {
            for i in 0..20 {
                helper
                    .put_object(
                        &bucket,
                        &format!("prefix{prefix_idx}/file{i:02}.dat"),
                        vec![b'p'; 100],
                    )
                    .await;
            }
        }

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/"),
            "--max-parallel-listings",
            "2",
            "--max-parallel-listing-max-depth",
            "1",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(
            result.stats.stats_deleted_objects, 100,
            "Should delete all 100 objects"
        );
    });
}

// ---------------------------------------------------------------------------
// 29.36 Object Listing Queue Size
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_object_listing_queue_size() {
    e2e_timeout!(async {
        // Purpose: Verify --object-listing-queue-size controls the internal queue
        //          size for listed objects without preventing pipeline completion.
        // Setup:   Upload 50 objects.
        // Expected: All 50 objects deleted (small queue size does not prevent
        //           completion).
        //
        // Validates: Requirement 1.8

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let _guard = helper.bucket_guard(&bucket);

        for i in 0..50 {
            helper
                .put_object(&bucket, &format!("queue/file{i:02}.dat"), vec![b'q'; 100])
                .await;
        }

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/queue/"),
            "--object-listing-queue-size",
            "5",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(
            result.stats.stats_deleted_objects, 50,
            "Should delete all 50 objects"
        );
    });
}

// ---------------------------------------------------------------------------
// 29.37 Max Keys Listing
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_max_keys_listing() {
    e2e_timeout!(async {
        // Purpose: Verify --max-keys controls the number of objects returned per
        //          S3 list request, forcing pagination. The pipeline should still
        //          delete all objects across multiple pages.
        // Setup:   Upload 100 objects.
        // Expected: All 100 objects deleted (pagination with small page size
        //           works correctly).
        //
        // Validates: Requirement 1.5

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let _guard = helper.bucket_guard(&bucket);

        for i in 0..100 {
            helper
                .put_object(&bucket, &format!("maxkeys/file{i:03}.dat"), vec![b'k'; 100])
                .await;
        }

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/maxkeys/"),
            "--max-keys",
            "10",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(
            result.stats.stats_deleted_objects, 100,
            "Should delete all 100 objects"
        );
    });
}
