//! E2E tests for combined/advanced scenarios (Tests 29.54 - 29.59).
//!
//! Tests multiple filters combined, dry-run with versioning/filters,
//! max-delete with filters, Lua filter + event callback, and large
//! object count.

#![cfg(e2e_test)]

mod common;

use common::TestHelper;
use std::collections::HashMap;
use std::io::Write as IoWrite;

// ---------------------------------------------------------------------------
// 29.54 Multiple Filters Combined
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_multiple_filters_combined() {
    e2e_timeout!(async {
        // Purpose: Verify multiple filters combine with AND logic. An object must
        //          pass ALL filters (prefix, regex, AND size) to be deleted.
        // Setup:   Upload 30 objects:
        //          - 10 with key logs/small{i}.txt (100B)
        //          - 10 with key logs/large{i}.txt (10KB)
        //          - 10 with key data/large{i}.dat (10KB)
        // Expected: Only 10 logs/large{i}.txt objects deleted (matching prefix
        //           logs/, regex \.txt$, AND size > 1KB). 20 others remain.
        //
        // Validates: Requirement 2.11

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        // Small text files under logs/
        for i in 0..10 {
            helper
                .put_object(&bucket, &format!("logs/small{i}.txt"), vec![b's'; 100])
                .await;
        }
        // Large text files under logs/
        for i in 0..10 {
            helper
                .put_object(
                    &bucket,
                    &format!("logs/large{i}.txt"),
                    vec![b'L'; 10 * 1024],
                )
                .await;
        }
        // Large data files under data/
        for i in 0..10 {
            helper
                .put_object(
                    &bucket,
                    &format!("data/large{i}.dat"),
                    vec![b'D'; 10 * 1024],
                )
                .await;
        }

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/logs/"),
            "--filter-include-regex",
            r"\.txt$",
            "--filter-larger-size",
            "1KB",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(
            result.stats.stats_deleted_objects, 10,
            "Should delete exactly 10 logs/large*.txt objects"
        );

        // Verify remaining objects
        let remaining_logs_small = helper.list_objects(&bucket, "logs/small").await;
        assert_eq!(
            remaining_logs_small.len(),
            10,
            "Small txt files should remain"
        );

        let remaining_data = helper.list_objects(&bucket, "data/").await;
        assert_eq!(remaining_data.len(), 10, "Data files should remain");
        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// 29.55 Dry Run with Delete All Versions
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_dry_run_with_delete_all_versions() {
    e2e_timeout!(async {
        // Purpose: Verify dry-run mode combined with --delete-all-versions
        //          simulates deletion of all versions without actually removing
        //          anything.
        // Setup:   Create versioned bucket; upload 10 objects; overwrite each
        //          once (creates 20 versions total).
        // Expected: All 20 versions still exist (dry-run); stats show 20
        //           simulated deletions; no actual S3 deletions.
        //
        // Validates: Requirements 3.1, 5.2, 5.4

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_versioned_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        // Upload initial versions
        for i in 0..10 {
            helper
                .put_object(&bucket, &format!("dryver/file{i}.dat"), vec![b'v'; 100])
                .await;
        }
        // Overwrite to create second versions
        for i in 0..10 {
            helper
                .put_object(&bucket, &format!("dryver/file{i}.dat"), vec![b'w'; 200])
                .await;
        }

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/dryver/"),
            "--dry-run",
            "--delete-all-versions",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(
            !result.has_error,
            "Dry-run pipeline should complete without errors"
        );
        assert_eq!(
            result.stats.stats_deleted_objects, 20,
            "Dry-run stats should report 20 simulated deletions"
        );

        // Verify all versions still exist
        let versions = helper.list_object_versions(&bucket).await;
        let real_versions: Vec<_> = versions
            .iter()
            .filter(|(k, _)| !k.starts_with("[delete-marker]"))
            .collect();
        assert_eq!(
            real_versions.len(),
            20,
            "All 20 versions must still exist after dry-run"
        );
        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// 29.56 Dry Run with Filters
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_dry_run_with_filters() {
    e2e_timeout!(async {
        // Purpose: Verify dry-run combined with filters accurately simulates
        //          filtered deletion. Only matching objects should be reported
        //          in stats, but no objects should actually be deleted.
        // Setup:   Upload 20 objects: 10 matching ^temp/, 10 not matching.
        // Expected: All 20 objects still exist; stats show 10 simulated deletions
        //           (only the matching ones).
        //
        // Validates: Requirements 3.1, 2.2

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        for i in 0..10 {
            helper
                .put_object(&bucket, &format!("temp/file{i}.dat"), vec![b't'; 100])
                .await;
        }
        for i in 0..10 {
            helper
                .put_object(&bucket, &format!("perm/file{i}.dat"), vec![b'p'; 100])
                .await;
        }

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/"),
            "--dry-run",
            "--filter-include-regex",
            "^temp/",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(
            !result.has_error,
            "Dry-run pipeline should complete without errors"
        );
        assert_eq!(
            result.stats.stats_deleted_objects, 10,
            "Dry-run stats should show 10 simulated deletions"
        );

        // Verify ALL objects still exist
        let total_remaining = helper.count_objects(&bucket, "").await;
        assert_eq!(
            total_remaining, 20,
            "All 20 objects must still exist after dry-run"
        );
        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// 29.57 Max Delete with Filters
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_max_delete_with_filters() {
    e2e_timeout!(async {
        // Purpose: Verify --max-delete interacts correctly with filters. The
        //          max-delete threshold should be applied AFTER filtering, so
        //          only filtered-in objects count toward the limit.
        // Setup:   Upload 50 objects: 30 matching ^to-delete/, 20 not matching.
        // Expected: Exactly 5 of the to-delete/ objects deleted; 25 to-delete/
        //           and 20 other objects remain.
        //
        // Note:    --batch-size 1 is essential for deterministic behavior. Without
        //          it, a single large batch could delete all filtered objects before
        //          the max-delete check fires, making assertions unreliable.
        //
        // Validates: Requirements 3.6, 2.2

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        let max_delete_value: u64 = 5;

        for i in 0..30 {
            helper
                .put_object(
                    &bucket,
                    &format!("to-delete/file{i:02}.dat"),
                    vec![b'd'; 100],
                )
                .await;
        }
        for i in 0..20 {
            helper
                .put_object(&bucket, &format!("to-keep/file{i:02}.dat"), vec![b'k'; 100])
                .await;
        }

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/"),
            "--filter-include-regex",
            "^to-delete/",
            "--max-delete",
            "5",
            "--batch-size",
            "1",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        // Check that max-delete was honored
        let to_delete_remaining = helper.count_objects(&bucket, "to-delete/").await;
        let to_keep_remaining = helper.count_objects(&bucket, "to-keep/").await;

        assert_eq!(to_keep_remaining, 20, "All to-keep/ objects should remain");
        assert!(
            to_delete_remaining >= 25,
            "At least 25 to-delete/ objects should remain (max-delete 5); got {to_delete_remaining}"
        );
        assert!(
            result.stats.stats_deleted_objects >= max_delete_value,
            "At least {max_delete_value} objects should be deleted (the check fires after the limit is hit); got {}",
            result.stats.stats_deleted_objects
        );
        assert!(
            result.stats.stats_deleted_objects <= max_delete_value,
            "At most {max_delete_value} objects should be deleted with batch-size=1; got {}",
            result.stats.stats_deleted_objects
        );
        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// 29.58 Lua Filter with Event Callback
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_lua_filter_with_event_callback() {
    e2e_timeout!(async {
        // Purpose: Verify Lua filter callback combined with Lua event callback.
        //          The filter controls which objects are deleted, and the event
        //          script receives events about the operation.
        // Setup:   Upload 20 objects: 10 large (1KB), 10 small (100B). Write
        //          Lua filter (delete objects > 500B). Write Lua event script.
        // Expected: Only 10 large objects deleted; event script executes.
        //
        // Validates: Requirements 2.8, 2.12, 7.6

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        for i in 0..10 {
            helper
                .put_object(
                    &bucket,
                    &format!("lua-combo/large{i}.dat"),
                    vec![b'L'; 1024],
                )
                .await;
        }
        for i in 0..10 {
            helper
                .put_object(&bucket, &format!("lua-combo/small{i}.dat"), vec![b's'; 100])
                .await;
        }

        // Lua filter script: delete objects > 500 bytes
        let filter_script = tempfile::NamedTempFile::new().unwrap();
        writeln!(
            filter_script.as_file(),
            r#"
    function filter(object)
        return object["size"] > 500
    end
    "#
        )
        .unwrap();

        // Lua event script (no-op, just verifies it doesn't crash)
        let event_script = tempfile::NamedTempFile::new().unwrap();
        writeln!(
            event_script.as_file(),
            r#"
    function on_event(event_data)
        -- no-op event handler
    end
    "#
        )
        .unwrap();

        let filter_path = filter_script.path().to_str().unwrap();
        let event_path = event_script.path().to_str().unwrap();
        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/lua-combo/"),
            "--filter-callback-lua-script",
            filter_path,
            "--event-callback-lua-script",
            event_path,
            "--allow-lua-os-library",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(
            result.stats.stats_deleted_objects, 10,
            "Should delete exactly 10 large objects"
        );

        let remaining_small = helper.list_objects(&bucket, "lua-combo/small").await;
        assert_eq!(remaining_small.len(), 10, "All small objects should remain");
        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// 29.59 Large Object Count
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_large_object_count() {
    e2e_timeout!(async {
        // Purpose: Verify the pipeline can handle deleting the maximum allowed
        //          number of objects (1000) in a single test. This tests the
        //          pipeline's ability to handle moderate scale.
        // Setup:   Upload 1000 objects (each 1KB).
        // Expected: All 1000 objects deleted; stats show 1000 deleted, 0 failed.
        //
        // Validates: Requirements 1.1, 1.8

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        let objects: Vec<(String, Vec<u8>)> = (0..1000)
            .map(|i| (format!("large-count/file{i:04}.dat"), vec![b'x'; 1024]))
            .collect();
        helper.put_objects_parallel(&bucket, objects).await;

        let config =
            TestHelper::build_config(vec![&format!("s3://{bucket}/large-count/"), "--force"]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(
            result.stats.stats_deleted_objects, 1000,
            "Should delete all 1000 objects"
        );
        assert_eq!(
            result.stats.stats_failed_objects, 0,
            "No objects should fail"
        );
        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// 29.59a All Filters Combined — Comprehensive AND Logic
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_all_filters_combined() {
    e2e_timeout!(async {
        // Purpose: Verify that ALL filter types combine with AND logic. Upload 20
        //          objects with diverse properties. Configure every available filter
        //          (include/exclude regex, size bounds, content-type, metadata, and
        //          tags). Only 3 objects that pass EVERY filter should be deleted;
        //          the remaining 17 survive because each is excluded by at least one
        //          filter.
        //
        // Filter chain (all must pass):
        //   1. Prefix:                      data/
        //   2. --filter-include-regex        \.json$
        //   3. --filter-exclude-regex        archive
        //   4. --filter-larger-size          500B
        //   5. --filter-smaller-size         5KB
        //   6. --filter-include-content-type-regex  application/json
        //   7. --filter-include-metadata-regex      env=production
        //   8. --filter-exclude-metadata-regex      retain=true
        //   9. --filter-include-tag-regex           deletable=yes
        //  10. --filter-exclude-tag-regex           protected=true
        //
        // Objects and why they're excluded:
        //   [1-3]  data/report{i}.json  1KB json env=production  deletable=yes
        //          → PASS ALL FILTERS (deleted)
        //   [4-6]  data/report{i}.json  1KB json env=production,retain=true  deletable=yes
        //          → FAIL: exclude-metadata "retain=true"
        //   [7-9]  data/archive{i}.json  1KB json env=production  deletable=yes
        //          → FAIL: exclude-regex "archive"
        //   [10-12] data/report{i}.csv  1KB text/csv env=production  deletable=yes
        //          → FAIL: include-regex ".json$" (key is .csv)
        //   [13-15] data/tiny{i}.json  100B json env=production  deletable=yes
        //          → FAIL: larger-size 500B (too small)
        //   [16-17] data/staging{i}.json  1KB json env=staging  deletable=yes
        //          → FAIL: include-metadata "env=production"
        //   [18-19] data/guarded{i}.json  1KB json env=production  protected=true
        //          → FAIL: exclude-tag "protected=true"
        //   [20]   other/report0.json  1KB json env=production  deletable=yes
        //          → FAIL: prefix "data/" (key is "other/")
        //
        // Expected: 3 deleted, 17 remain.
        //
        // Validates: Requirements 2.2, 2.3, 2.4, 2.5, 2.6, 2.11

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        let meta_prod = || {
            let mut m = HashMap::new();
            m.insert("env".to_string(), "production".to_string());
            m
        };
        let meta_prod_retain = || {
            let mut m = HashMap::new();
            m.insert("env".to_string(), "production".to_string());
            m.insert("retain".to_string(), "true".to_string());
            m
        };
        let meta_staging = || {
            let mut m = HashMap::new();
            m.insert("env".to_string(), "staging".to_string());
            m
        };
        let tag_deletable = || {
            let mut t = HashMap::new();
            t.insert("deletable".to_string(), "yes".to_string());
            t
        };
        let tag_protected = || {
            let mut t = HashMap::new();
            t.insert("protected".to_string(), "true".to_string());
            t
        };

        // [1-3] PASS ALL — will be deleted
        for i in 0..3 {
            helper
                .put_object_full(
                    &bucket,
                    &format!("data/report{i}.json"),
                    vec![b'R'; 1024],
                    "application/json",
                    meta_prod(),
                    tag_deletable(),
                )
                .await;
        }

        // [4-6] FAIL: exclude-metadata "retain=true"
        for i in 0..3 {
            helper
                .put_object_full(
                    &bucket,
                    &format!("data/retained{i}.json"),
                    vec![b'T'; 1024],
                    "application/json",
                    meta_prod_retain(),
                    tag_deletable(),
                )
                .await;
        }

        // [7-9] FAIL: exclude-regex "archive"
        for i in 0..3 {
            helper
                .put_object_full(
                    &bucket,
                    &format!("data/archive{i}.json"),
                    vec![b'A'; 1024],
                    "application/json",
                    meta_prod(),
                    tag_deletable(),
                )
                .await;
        }

        // [10-12] FAIL: include-regex ".json$" (key is .csv)
        for i in 0..3 {
            helper
                .put_object_full(
                    &bucket,
                    &format!("data/report{i}.csv"),
                    vec![b'C'; 1024],
                    "text/csv",
                    meta_prod(),
                    tag_deletable(),
                )
                .await;
        }

        // [13-15] FAIL: larger-size 500B (object is only 100B)
        for i in 0..3 {
            helper
                .put_object_full(
                    &bucket,
                    &format!("data/tiny{i}.json"),
                    vec![b't'; 100],
                    "application/json",
                    meta_prod(),
                    tag_deletable(),
                )
                .await;
        }

        // [16-17] FAIL: include-metadata "env=production" (metadata is env=staging)
        for i in 0..2 {
            helper
                .put_object_full(
                    &bucket,
                    &format!("data/staging{i}.json"),
                    vec![b'S'; 1024],
                    "application/json",
                    meta_staging(),
                    tag_deletable(),
                )
                .await;
        }

        // [18-19] FAIL: exclude-tag "protected=true"
        for i in 0..2 {
            helper
                .put_object_full(
                    &bucket,
                    &format!("data/guarded{i}.json"),
                    vec![b'G'; 1024],
                    "application/json",
                    meta_prod(),
                    tag_protected(),
                )
                .await;
        }

        // [20] FAIL: prefix "data/" (key is "other/")
        helper
            .put_object_full(
                &bucket,
                "other/report0.json",
                vec![b'O'; 1024],
                "application/json",
                meta_prod(),
                tag_deletable(),
            )
            .await;

        // Verify setup: 20 total objects
        let total_before = helper.count_objects(&bucket, "").await;
        assert_eq!(total_before, 20, "Should have uploaded 20 objects");

        // Run pipeline with ALL filters
        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/data/"),
            "--filter-include-regex",
            r"\.json$",
            "--filter-exclude-regex",
            "archive",
            "--filter-larger-size",
            "500B",
            "--filter-smaller-size",
            "5KB",
            "--filter-include-content-type-regex",
            "application/json",
            "--filter-include-metadata-regex",
            "env=production",
            "--filter-exclude-metadata-regex",
            "retain=true",
            "--filter-include-tag-regex",
            "deletable=yes",
            "--filter-exclude-tag-regex",
            "protected=true",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(
            result.stats.stats_deleted_objects, 3,
            "Only 3 objects should pass all filters and be deleted"
        );

        // The 3 deleted objects should be gone
        let remaining_reports = helper.list_objects(&bucket, "data/report").await;
        // data/report0..2.json were deleted, but data/report0..2.csv still exist
        let json_reports: Vec<_> = remaining_reports
            .iter()
            .filter(|k| k.ends_with(".json"))
            .collect();
        assert_eq!(
            json_reports.len(),
            0,
            "All data/report*.json should be deleted"
        );
        let csv_reports: Vec<_> = remaining_reports
            .iter()
            .filter(|k| k.ends_with(".csv"))
            .collect();
        assert_eq!(csv_reports.len(), 3, "All data/report*.csv should remain");

        // Objects excluded by each filter should remain
        let remaining_retained = helper.list_objects(&bucket, "data/retained").await;
        assert_eq!(
            remaining_retained.len(),
            3,
            "Retained objects should remain"
        );

        let remaining_archive = helper.list_objects(&bucket, "data/archive").await;
        assert_eq!(remaining_archive.len(), 3, "Archive objects should remain");

        let remaining_tiny = helper.list_objects(&bucket, "data/tiny").await;
        assert_eq!(remaining_tiny.len(), 3, "Tiny objects should remain");

        let remaining_staging = helper.list_objects(&bucket, "data/staging").await;
        assert_eq!(remaining_staging.len(), 2, "Staging objects should remain");

        let remaining_guarded = helper.list_objects(&bucket, "data/guarded").await;
        assert_eq!(remaining_guarded.len(), 2, "Guarded objects should remain");

        let remaining_other = helper.list_objects(&bucket, "other/").await;
        assert_eq!(
            remaining_other.len(),
            1,
            "Object outside prefix should remain"
        );

        // Total remaining: 17
        let total_after = helper.count_objects(&bucket, "").await;
        assert_eq!(total_after, 17, "17 objects should remain after deletion");
        guard.cleanup().await;
    });
}
