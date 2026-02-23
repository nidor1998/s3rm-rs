//! E2E tests for error handling and access denial (Tests 29.48 - 29.50).
//!
//! Tests access denied with invalid credentials, nonexistent bucket,
//! and --warn-as-error behavior.

#![cfg(e2e_test)]

mod common;

use common::TestHelper;

// ---------------------------------------------------------------------------
// 29.48 Access Denied Invalid Credentials
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_access_denied_invalid_credentials() {
    e2e_timeout!(async {
        // Purpose: Verify that the pipeline reports an error when given invalid
        //          AWS credentials. Access denied or invalid credentials errors
        //          should be surfaced through the pipeline's error reporting.
        // Setup:   Create a bucket and upload 5 objects using valid credentials
        //          (for setup only). Then run the pipeline with invalid credentials.
        // Expected: Pipeline reports error; has_error() returns true; error is
        //           an AWS SDK error (access denied or invalid credentials).
        //
        // Validates: Requirements 6.4, 10.5, 13.4

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let _guard = helper.bucket_guard(&bucket);

        for i in 0..5 {
            helper
                .put_object(&bucket, &format!("access/file{i}.dat"), vec![b'a'; 100])
                .await;
        }

        // Build config with invalid credentials
        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/access/"),
            "--target-access-key",
            "INVALIDACCESSKEY123456",
            "--target-secret-access-key",
            "INVALIDSECRETKEY1234567890abcdefghijklmn",
            "--target-region",
            helper.region(),
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(
            result.has_error,
            "Pipeline should report error for invalid credentials"
        );

        // Objects should still exist (deletion was not performed)
        let remaining = helper.count_objects(&bucket, "access/").await;
        assert_eq!(
            remaining, 5,
            "All objects should remain after access denial"
        );
    });
}

// ---------------------------------------------------------------------------
// 29.49 Nonexistent Bucket Error
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_nonexistent_bucket_error() {
    e2e_timeout!(async {
        // Purpose: Verify that targeting a nonexistent bucket produces an error.
        //          The pipeline should detect that the bucket does not exist and
        //          report the error (NoSuchBucket or similar).
        // Setup:   No bucket creation; use a random non-existent bucket name.
        // Expected: Pipeline reports error; has_error() returns true.
        //
        // Validates: Requirements 6.4, 10.5

        let helper = TestHelper::new().await;
        let fake_bucket = helper.generate_bucket_name();

        // No cleanup needed since bucket is never created
        let config = TestHelper::build_config(vec![&format!("s3://{fake_bucket}/"), "--force"]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(
            result.has_error,
            "Pipeline should report error for nonexistent bucket"
        );
    });
}

// ---------------------------------------------------------------------------
// 29.49a Batch Deletion Partial Failure via Access Denial
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_batch_partial_failure_access_denied() {
    e2e_timeout!(async {
        // Purpose: Verify that when some objects in a batch cannot be deleted due
        //          to access denial (bucket policy denying s3:DeleteObject on a
        //          prefix), the pipeline reports partial failure — successfully
        //          deleted objects are counted, and failed objects are recorded.
        //
        // Setup:   1. Create a bucket and upload 20 objects:
        //             - 10 under "deletable/" (no restrictions)
        //             - 10 under "protected/" (deletion will be denied)
        //          2. Apply a bucket policy that denies s3:DeleteObject on
        //             "protected/*"
        //          3. Run the pipeline to delete ALL objects
        //
        // Expected: - The 10 "deletable/" objects are deleted
        //           - The 10 "protected/" objects remain (AccessDenied)
        //           - Stats show ~10 deleted, ~10 failed
        //           - Pipeline reports has_warning or has_error
        //
        // Cleanup:  Remove the deny policy, then delete all objects and bucket.
        //
        // Validates: Requirements 1.9, 6.4, 6.5

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        // Upload 10 deletable objects
        for i in 0..10 {
            helper
                .put_object(&bucket, &format!("deletable/file{i}.dat"), vec![b'd'; 100])
                .await;
        }
        // Upload 10 protected objects
        for i in 0..10 {
            helper
                .put_object(&bucket, &format!("protected/file{i}.dat"), vec![b'p'; 100])
                .await;
        }

        // Apply deny policy on the "protected/" prefix
        helper.deny_delete_on_prefix(&bucket, "protected/").await;

        // Run pipeline to delete everything
        let config = TestHelper::build_config(vec![&format!("s3://{bucket}/"), "--force"]);
        let result = TestHelper::run_pipeline(config).await;

        // Revert the deny policy BEFORE assertions so cleanup always succeeds
        helper.delete_bucket_policy(&bucket).await;

        // Now set up the guard for cleanup (after policy removal)
        let _guard = helper.bucket_guard(&bucket);

        // The deletable/ objects should have been deleted
        let remaining_deletable = helper.count_objects(&bucket, "deletable/").await;
        assert_eq!(
            remaining_deletable, 0,
            "All deletable/ objects should have been deleted"
        );

        // The protected/ objects should still exist (AccessDenied)
        let remaining_protected = helper.count_objects(&bucket, "protected/").await;
        assert_eq!(
            remaining_protected, 10,
            "All protected/ objects should remain due to AccessDenied"
        );

        // Stats should reflect the partial failure
        assert_eq!(
            result.stats.stats_deleted_objects, 10,
            "Should report 10 successfully deleted objects"
        );
        assert!(
            result.stats.stats_failed_objects > 0,
            "Should report failed objects due to access denial"
        );

        // Pipeline should report warning or error for the failures
        assert!(
            result.has_warning || result.has_error,
            "Pipeline should report warning or error for partial batch failure"
        );
    });
}

// ---------------------------------------------------------------------------
// 29.50 Warn As Error
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_warn_as_error() {
    e2e_timeout!(async {
        // Purpose: Verify --warn-as-error promotes warnings to errors. If the
        //          pipeline encounters any warnings during execution, has_error()
        //          should return true. If no warnings occur (normal successful
        //          deletion), the pipeline should complete normally.
        // Setup:   Upload 10 objects.
        // Expected: Pipeline completes; if any warnings occurred, has_error()
        //           returns true; otherwise normal completion.
        //
        // Validates: Requirement 10.5

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let _guard = helper.bucket_guard(&bucket);

        for i in 0..10 {
            helper
                .put_object(&bucket, &format!("warn/file{i}.dat"), vec![b'w'; 100])
                .await;
        }

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/warn/"),
            "--warn-as-error",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        // With --warn-as-error, the presence of errors depends on whether any
        // warnings were generated during the deletion. For a successful deletion,
        // there should be no warnings and thus no error promotion.
        // The key behavior being tested: the flag is accepted and does not crash.
        assert_eq!(
            result.stats.stats_deleted_objects, 10,
            "Should delete all 10 objects"
        );

        // If there were no warnings, has_error should be false
        if !result.has_warning {
            assert!(
                !result.has_error,
                "No warnings means --warn-as-error should not promote to error"
            );
        }
    });
}
