//! E2E tests for error handling and access denial (Tests 29.48 - 29.50).
//!
//! Tests access denied with invalid credentials, nonexistent bucket,
//! --warn-as-error behavior, CLI exit codes, config validation, error chain
//! preservation, and error code granularity.

#![cfg(e2e_test)]

mod common;

use common::{CollectingEventCallback, TestHelper};
use s3rm_rs::EventType;
use std::sync::{Arc, Mutex};

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
        // TODO: Verification that error codes appear in log output (Req 4.10)
        //       requires tracing output capture, which is not available in E2E
        //       tests. Error code logging is covered by unit/property tests.
        //
        // Validates: Requirements 6.4, 10.5, 13.4

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

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
        guard.cleanup().await;
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

        let guard = helper.bucket_guard(&bucket);

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

        let config = TestHelper::build_config(vec![&format!("s3://{bucket}/"), "--force"]);
        let result = TestHelper::run_pipeline(config).await;

        // Revert the deny policy so cleanup can delete the protected/ objects.
        helper.delete_bucket_policy(&bucket).await;

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
        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// 29.50 Warn As Error
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_warn_as_error() {
    e2e_timeout!(async {
        // Purpose: Verify --warn-as-error flag is accepted by the CLI and does not
        //          crash. This test only validates flag acceptance, not actual warning
        //          promotion, because the test setup produces a clean successful
        //          deletion with no warnings to promote.
        //
        //          Actual warning-to-error promotion logic is covered by unit and
        //          property tests (Properties 21-24 in logging_properties.rs).
        //
        // TODO: A proper E2E test for warning promotion would need a partial
        //       failure scenario (e.g., some objects failing deletion) combined
        //       with --warn-as-error to verify has_error() returns true. This
        //       requires reliable control over which objects fail, which is
        //       difficult without the deny-policy approach used in 29.49a.
        //
        // Setup:   Upload 10 objects.
        // Expected: Pipeline completes normally; no warnings generated so no
        //           error promotion occurs.
        //
        // Validates: Requirement 10.5

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

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
        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// R-2: CLI Exit Code Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_exit_codes() {
    e2e_timeout!(async {
        // Purpose: Verify the CLI binary returns correct exit codes for different
        //          scenarios by spawning the binary as a subprocess.
        //
        // Test cases:
        //   1. Exit code 0: successful dry-run deletion against a real bucket
        //   2. Exit code 2: invalid arguments (unknown flag)
        //
        // Note: Exit codes 1 (runtime error) and 3 (warning/partial failure) are
        // harder to test at E2E level since they require specific AWS failure
        // conditions that are difficult to reliably reproduce via the CLI binary.
        //
        // Validates: Requirement 10.5

        let binary_path = env!("CARGO_BIN_EXE_s3rm");

        // --- Exit code 2: invalid arguments ---
        // Running with a completely unknown flag should cause clap to reject it.
        let invalid_args_output = std::process::Command::new(binary_path)
            .args(["--this-flag-does-not-exist"])
            .output()
            .expect("Failed to execute s3rm binary");

        assert_eq!(
            invalid_args_output.status.code(),
            Some(2),
            "Unknown flag should produce exit code 2; stderr: {}",
            String::from_utf8_lossy(&invalid_args_output.stderr)
        );

        // --- Exit code 0: successful dry-run ---
        // Create a real bucket, upload an object, and run with --dry-run --force.
        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        helper
            .put_object(&bucket, "exit-test/file0.dat", vec![b'e'; 100])
            .await;

        let success_output = std::process::Command::new(binary_path)
            .args([
                &format!("s3://{bucket}/exit-test/"),
                "--dry-run",
                "--force",
                "--target-profile",
                "s3rm-e2e-test",
            ])
            .output()
            .expect("Failed to execute s3rm binary");

        assert_eq!(
            success_output.status.code(),
            Some(0),
            "Dry-run on real bucket should produce exit code 0; stderr: {}",
            String::from_utf8_lossy(&success_output.stderr)
        );

        // Verify the object is still there (dry-run did not delete it)
        let remaining = helper.count_objects(&bucket, "exit-test/").await;
        assert_eq!(
            remaining, 1,
            "Object should still exist after dry-run via CLI"
        );
        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// R-5: Rate Limit < Batch Size Validation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_rate_limit_less_than_batch_size_rejected() {
    e2e_timeout!(async {
        // Purpose: Verify that --rate-limit-objects < --batch-size is rejected
        //          at configuration time. The validation in Config::try_from
        //          returns an error when rate_limit_objects < batch_size, since
        //          a single batch operation must not exceed the rate limit.
        // Setup:   Create a bucket (needed for a valid S3 target).
        // Expected: build_config_from_args returns Err containing an error
        //           message about rate-limit-objects being too small.
        //
        // Validates: Requirement 8.8

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        // Attempt to build config with rate-limit (10) < batch-size (200)
        let args: Vec<String> = vec![
            "s3rm".to_string(),
            format!("s3://{bucket}/"),
            "--rate-limit-objects".to_string(),
            "10".to_string(),
            "--batch-size".to_string(),
            "200".to_string(),
            "--force".to_string(),
            "--target-profile".to_string(),
            "s3rm-e2e-test".to_string(),
        ];

        let result = s3rm_rs::config::args::build_config_from_args(args);

        assert!(
            result.is_err(),
            "Config should be rejected when rate-limit-objects < batch-size"
        );

        let error_msg = result.unwrap_err();
        assert!(
            error_msg.contains("--rate-limit-objects"),
            "Error message should mention --rate-limit-objects; got: {error_msg}"
        );
        assert!(
            error_msg.contains("--batch-size"),
            "Error message should mention --batch-size; got: {error_msg}"
        );
        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// --warn-as-error with actual partial failure
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_warn_as_error_with_partial_failure() {
    e2e_timeout!(async {
        // Purpose: Verify that --warn-as-error promotes warnings from partial
        //          batch failures to errors. When some objects are protected by
        //          a deny policy and others are deletable, the pipeline generates
        //          warnings for the failures. With --warn-as-error, has_error
        //          should be true.
        // Setup:   Create bucket, upload 10 objects under deletable/ and 10 under
        //          protected/. Apply deny policy on protected/. Run with
        //          --warn-as-error --force.
        // Expected: has_error is true (warnings promoted to errors), deletable/
        //           objects deleted, protected/ objects remain.
        //
        // Validates: Requirement 10.5

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

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

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/"),
            "--warn-as-error",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        // Revert the deny policy so cleanup can delete the protected/ objects.
        helper.delete_bucket_policy(&bucket).await;

        // With --warn-as-error, the partial failure warnings should be promoted to errors
        assert!(
            result.has_error,
            "Pipeline should report error when --warn-as-error and partial failures occur"
        );

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

        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// Single deleter partial failure captures specific S3 error code in events
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_single_deleter_partial_failure_error_code_in_events() {
    e2e_timeout!(async {
        // Purpose: Verify that SingleDeleter (batch_size=1) extracts and reports
        //          specific S3 error codes (e.g., "AccessDenied") rather than a
        //          generic "DeleteObjectError" string. This validates fix #6
        //          (error code granularity in SingleDeleter).
        //
        // Setup:   1. Create a bucket and upload objects:
        //             - 5 under "ok/" (no restrictions)
        //             - 5 under "denied/" (deletion denied by bucket policy)
        //          2. Apply deny policy on "denied/"
        //          3. Run with --batch-size 1 (forces SingleDeleter) and event callback
        //
        // Expected: - DELETE_FAILED events for "denied/" objects contain
        //             "AccessDenied" in their error_message, not generic "DeleteObjectError"
        //           - "ok/" objects are successfully deleted
        //
        // Validates: Error code granularity (fix #6)

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        for i in 0..5 {
            helper
                .put_object(&bucket, &format!("ok/file{i}.dat"), vec![b'o'; 100])
                .await;
        }
        for i in 0..5 {
            helper
                .put_object(&bucket, &format!("denied/file{i}.dat"), vec![b'd'; 100])
                .await;
        }

        helper.deny_delete_on_prefix(&bucket, "denied/").await;

        let collected_events = Arc::new(Mutex::new(Vec::new()));
        let callback = CollectingEventCallback {
            events: Arc::clone(&collected_events),
        };

        let mut config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/"),
            "--batch-size",
            "1",
            "--force",
        ]);
        config
            .event_manager
            .register_callback(EventType::ALL_EVENTS, callback, false);

        let result = TestHelper::run_pipeline(config).await;

        helper.delete_bucket_policy(&bucket).await;

        // "ok/" objects should have been deleted
        let remaining_ok = helper.count_objects(&bucket, "ok/").await;
        assert_eq!(remaining_ok, 0, "All ok/ objects should have been deleted");

        // "denied/" objects should still exist
        let remaining_denied = helper.count_objects(&bucket, "denied/").await;
        assert_eq!(
            remaining_denied, 5,
            "All denied/ objects should remain due to AccessDenied"
        );

        // Check DELETE_FAILED events contain specific error codes
        {
            let events = collected_events.lock().unwrap();
            let failed_events: Vec<_> = events
                .iter()
                .filter(|e| e.event_type == EventType::DELETE_FAILED)
                .collect();

            assert!(
                !failed_events.is_empty(),
                "Should have DELETE_FAILED events for denied/ objects"
            );

            for event in &failed_events {
                if let Some(ref error_msg) = event.error_message {
                    assert!(
                        error_msg.contains("AccessDenied"),
                        "DELETE_FAILED event error_message should contain 'AccessDenied', got: {error_msg}"
                    );
                    assert!(
                        !error_msg.contains("DeleteObjectError"),
                        "DELETE_FAILED event should NOT use generic 'DeleteObjectError', got: {error_msg}"
                    );
                }
            }
        }

        assert!(
            result.has_warning || result.has_error,
            "Pipeline should report warning or error for failures"
        );

        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// Batch deleter partial failure captures specific S3 error codes in events
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_batch_deleter_error_code_in_events() {
    e2e_timeout!(async {
        // Purpose: Verify that BatchDeleter reports specific S3 error codes
        //          (e.g., "AccessDenied") in DELETE_FAILED events. This serves
        //          as a reference for comparing with SingleDeleter behavior.
        //
        // Setup:   Similar to the single-deleter test but with batch_size=100.
        //
        // Expected: DELETE_FAILED events contain "AccessDenied" error code.
        //
        // Validates: BatchDeleter error code consistency

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        for i in 0..5 {
            helper
                .put_object(&bucket, &format!("ok/file{i}.dat"), vec![b'o'; 100])
                .await;
        }
        for i in 0..5 {
            helper
                .put_object(&bucket, &format!("denied/file{i}.dat"), vec![b'd'; 100])
                .await;
        }

        helper.deny_delete_on_prefix(&bucket, "denied/").await;

        let collected_events = Arc::new(Mutex::new(Vec::new()));
        let callback = CollectingEventCallback {
            events: Arc::clone(&collected_events),
        };

        let mut config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/"),
            "--batch-size",
            "100",
            "--force",
        ]);
        config
            .event_manager
            .register_callback(EventType::ALL_EVENTS, callback, false);

        let result = TestHelper::run_pipeline(config).await;

        helper.delete_bucket_policy(&bucket).await;

        // Check DELETE_FAILED events contain specific error codes
        {
            let events = collected_events.lock().unwrap();
            let failed_events: Vec<_> = events
                .iter()
                .filter(|e| e.event_type == EventType::DELETE_FAILED)
                .collect();

            assert!(
                !failed_events.is_empty(),
                "Should have DELETE_FAILED events for denied/ objects"
            );

            for event in &failed_events {
                if let Some(ref error_msg) = event.error_message {
                    assert!(
                        error_msg.contains("AccessDenied"),
                        "BatchDeleter DELETE_FAILED event should contain 'AccessDenied', got: {error_msg}"
                    );
                }
            }
        }

        assert!(
            result.has_warning || result.has_error,
            "Pipeline should report warning or error for failures"
        );

        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// Smoke test: invalid credentials produce non-empty pipeline errors
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_invalid_credentials_pipeline_error_smoke_test() {
    e2e_timeout!(async {
        // Purpose: Smoke test verifying that invalid credentials produce a
        //          pipeline error with a non-empty error list. This does NOT
        //          test error chain preservation in the deletion stage (the
        //          failure occurs at listing, before any deletion). Error chain
        //          preservation through delete_buffered_objects and
        //          handle_api_error is covered by unit tests.
        //
        // Setup:   Create a bucket, upload objects, run with invalid credentials.
        // Expected: has_error is true, errors list is non-empty, objects remain.
        //
        // Validates: Pipeline surfaces listing-stage auth errors (Req 6.4)

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        for i in 0..3 {
            helper
                .put_object(&bucket, &format!("chain/file{i}.dat"), vec![b'c'; 100])
                .await;
        }

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/chain/"),
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
        assert!(
            !result.errors.is_empty(),
            "Errors list should not be empty when pipeline fails"
        );

        // Objects should still exist (failure was at listing, before deletion)
        let remaining = helper.count_objects(&bucket, "chain/").await;
        assert_eq!(
            remaining, 3,
            "All objects should remain after failed deletion"
        );

        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// Single deleter: warn-as-error with partial failure
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_warn_as_error_single_deleter_partial_failure() {
    e2e_timeout!(async {
        // Purpose: Verify that --warn-as-error works correctly with batch_size=1
        //          (SingleDeleter). When some objects fail to delete, the warning
        //          should be promoted to an error.
        //
        // Setup:   Upload deletable and protected objects, apply deny policy,
        //          run with --batch-size 1 --warn-as-error.
        // Expected: has_error is true (warnings promoted to errors).
        //
        // Validates: warn-as-error with SingleDeleter

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        for i in 0..5 {
            helper
                .put_object(&bucket, &format!("ok/file{i}.dat"), vec![b'o'; 100])
                .await;
        }
        for i in 0..5 {
            helper
                .put_object(&bucket, &format!("denied/file{i}.dat"), vec![b'd'; 100])
                .await;
        }

        helper.deny_delete_on_prefix(&bucket, "denied/").await;

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/"),
            "--batch-size",
            "1",
            "--warn-as-error",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        helper.delete_bucket_policy(&bucket).await;

        // With --warn-as-error, partial failures should be promoted to error
        assert!(
            result.has_error,
            "SingleDeleter with --warn-as-error should promote partial failures to error"
        );

        // denied/ objects should still exist
        let remaining_denied = helper.count_objects(&bucket, "denied/").await;
        assert_eq!(
            remaining_denied, 5,
            "All denied/ objects should remain due to AccessDenied"
        );

        guard.cleanup().await;
    });
}
