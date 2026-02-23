//! E2E tests for AWS configuration options (Tests 29.51 - 29.53a).
//!
//! Tests target-region override, force-path-style, request-payer, and
//! Lua unsafe VM options.

#![cfg(e2e_test)]

mod common;

use common::TestHelper;
use std::io::Write as IoWrite;

// ---------------------------------------------------------------------------
// 29.51 Target Region Override
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_target_region_override() {
    e2e_timeout!(async {
        // Purpose: Verify --target-region overrides the region from the AWS
        //          profile without breaking access to the bucket.
        // Setup:   Upload 10 objects in the default region.
        // Expected: Pipeline completes; all 10 objects deleted (region override
        //           does not break access when the same region is specified).
        //
        // Validates: Requirement 8.5

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let _guard = helper.bucket_guard(&bucket);

        for i in 0..10 {
            helper
                .put_object(&bucket, &format!("region/file{i}.dat"), vec![b'r'; 100])
                .await;
        }

        let region = helper.region().to_string();
        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/region/"),
            "--target-region",
            &region,
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(
            result.stats.stats_deleted_objects, 10,
            "Should delete all 10 objects"
        );
    });
}

// ---------------------------------------------------------------------------
// 29.52 Target Force Path Style
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_target_force_path_style() {
    e2e_timeout!(async {
        // Purpose: Verify --target-force-path-style uses path-style addressing
        //          for S3 requests (e.g., s3.amazonaws.com/bucket/key instead of
        //          bucket.s3.amazonaws.com/key). Should work with standard S3.
        // Setup:   Upload 10 objects.
        // Expected: Pipeline completes; all 10 objects deleted.
        //
        // Validates: Requirement 8.6

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let _guard = helper.bucket_guard(&bucket);

        for i in 0..10 {
            helper
                .put_object(&bucket, &format!("pathstyle/file{i}.dat"), vec![b'p'; 100])
                .await;
        }

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/pathstyle/"),
            "--target-force-path-style",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(
            result.stats.stats_deleted_objects, 10,
            "Should delete all 10 objects"
        );
    });
}

// ---------------------------------------------------------------------------
// 29.53 Target Request Payer
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_target_request_payer() {
    e2e_timeout!(async {
        // Purpose: Verify --target-request-payer adds the requester-pays header
        //          to S3 requests without breaking normal operations.
        // Setup:   Upload 10 objects.
        // Expected: Pipeline completes; all 10 objects deleted (request payer
        //           header does not break normal operations).
        //
        // Validates: Requirement 8.3

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let _guard = helper.bucket_guard(&bucket);

        for i in 0..10 {
            helper
                .put_object(&bucket, &format!("payer/file{i}.dat"), vec![b'y'; 100])
                .await;
        }

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/payer/"),
            "--target-request-payer",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(
            result.stats.stats_deleted_objects, 10,
            "Should delete all 10 objects"
        );
    });
}

// ---------------------------------------------------------------------------
// 29.53a Allow Lua Unsafe VM
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_allow_lua_unsafe_vm() {
    e2e_timeout!(async {
        // Purpose: Verify --allow-lua-unsafe-vm disables the Lua sandbox entirely,
        //          allowing access to OS library functions that would otherwise be
        //          blocked by the default sandbox.
        // Setup:   Upload 5 objects. Write a Lua filter script that uses os.clock()
        //          (an OS library function).
        // Expected: Pipeline completes without Lua sandbox error; all 5 matching
        //           objects deleted.
        //
        // Validates: Requirement 2.14

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let _guard = helper.bucket_guard(&bucket);

        for i in 0..5 {
            helper
                .put_object(&bucket, &format!("unsafe-vm/file{i}.dat"), vec![b'u'; 100])
                .await;
        }

        // Write a Lua filter script that uses os.clock() (requires unsafe VM)
        let lua_script = tempfile::NamedTempFile::new().unwrap();
        writeln!(
            lua_script.as_file(),
            r#"
    function filter(object)
        local t = os.clock()
        return true
    end
    "#
        )
        .unwrap();

        let lua_path = lua_script.path().to_str().unwrap();
        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/unsafe-vm/"),
            "--filter-callback-lua-script",
            lua_path,
            "--allow-lua-unsafe-vm",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(
            !result.has_error,
            "Pipeline with unsafe VM should complete without errors"
        );
        assert_eq!(
            result.stats.stats_deleted_objects, 5,
            "Should delete all 5 objects"
        );
    });
}
