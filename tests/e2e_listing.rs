//! E2E tests for unified listing refactoring.
//!
//! Tests that the refactored listing logic (unified ListingMode dispatch,
//! sequential/parallel paths, pagination) works correctly against real S3
//! for both object listing and version listing modes.

#![cfg(e2e_test)]

mod common;

use common::TestHelper;

// ---------------------------------------------------------------------------
// Parallel listing with versioned bucket + delete-all-versions
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_parallel_listing_versioned_objects() {
    e2e_timeout!(async {
        // Purpose: Verify parallel listing works correctly with versioned buckets
        //          when using --delete-all-versions. The list_object_versions API
        //          returns both versions() and delete_markers(); the unified
        //          list_with_parallel(Versions, ...) must handle both.
        //
        // Setup:   Versioned bucket, 60 keys across 3 sub-prefixes (20 each).
        //          1. Upload v1 (60 versions)
        //          2. Delete without --delete-all-versions (creates 60 delete markers)
        //          3. Upload v2 on same keys (60 new versions)
        //          Per-key history: v1 → delete-marker → v2
        //          Total entries: 120 versions + 60 delete markers = 180
        //
        // Expected: All 180 entries permanently deleted; nothing remains.

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_versioned_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        let make_objects = |body: u8| -> Vec<(String, Vec<u8>)> {
            (0..3)
                .flat_map(|prefix_idx| {
                    (0..20).map(move |i| {
                        (
                            format!("par-ver/sub{prefix_idx}/file{i:02}.dat"),
                            vec![body; 50],
                        )
                    })
                })
                .collect()
        };

        // Step 1: Upload v1
        helper
            .put_objects_parallel(&bucket, make_objects(b'1'))
            .await;

        // Step 2: Delete without --delete-all-versions → creates delete markers
        // in the middle of the version history
        let config_markers =
            TestHelper::build_config(vec![&format!("s3://{bucket}/par-ver/"), "--force"]);
        let marker_result = TestHelper::run_pipeline(config_markers).await;
        assert!(!marker_result.has_error);
        assert_eq!(marker_result.stats.stats_deleted_objects, 60);

        // Step 3: Upload v2 on same keys → new versions on top of delete markers
        helper
            .put_objects_parallel(&bucket, make_objects(b'2'))
            .await;

        // Verify: 120 versions + 60 delete markers = 180 entries
        let pre_versions = helper.list_object_versions(&bucket).await;
        let version_count = pre_versions
            .iter()
            .filter(|(k, _)| !k.starts_with("[delete-marker]") && k.starts_with("par-ver/"))
            .count();
        let marker_count = pre_versions
            .iter()
            .filter(|(k, _)| k.starts_with("[delete-marker]par-ver/"))
            .count();
        assert_eq!(version_count, 120, "Should have 120 versions (v1 + v2)");
        assert_eq!(
            marker_count, 60,
            "Should have 60 delete markers in the middle"
        );

        // Now delete everything with parallel versioned listing
        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/par-ver/"),
            "--delete-all-versions",
            "--max-parallel-listings",
            "3",
            "--max-parallel-listing-max-depth",
            "2",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        // 120 versions + 60 delete markers = 180 total deletions
        assert_eq!(
            result.stats.stats_deleted_objects, 180,
            "Should delete all 120 versions + 60 delete markers"
        );

        // Verify nothing remains
        let versions = helper.list_object_versions(&bucket).await;
        let par_ver_versions: Vec<_> = versions
            .iter()
            .filter(|(k, _)| k.starts_with("par-ver/") || k.starts_with("[delete-marker]par-ver/"))
            .collect();
        assert!(
            par_ver_versions.is_empty(),
            "No versions or delete markers should remain under par-ver/"
        );
        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// Deep nesting with parallel listing
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_parallel_listing_deep_nesting() {
    e2e_timeout!(async {
        // Purpose: Verify parallel listing handles deep prefix hierarchies correctly.
        //          With max_depth=2, sub-prefixes at levels 1-2 should be discovered
        //          and parallelized; level 3+ should be listed sequentially within
        //          their parent task.
        // Setup:   50 objects across a 3-level deep hierarchy.
        // Expected: All 50 objects deleted.

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        let objects: Vec<(String, Vec<u8>)> = (0..2)
            .flat_map(|a| {
                (0..5).flat_map(move |b| {
                    (0..5).map(move |c| {
                        (
                            format!("deep/l1-{a}/l2-{b}/l3-{c}/file.dat"),
                            vec![b'd'; 50],
                        )
                    })
                })
            })
            .collect();
        assert_eq!(objects.len(), 50);
        helper.put_objects_parallel(&bucket, objects).await;

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/deep/"),
            "--max-parallel-listings",
            "4",
            "--max-parallel-listing-max-depth",
            "2",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(
            !result.has_error,
            "Pipeline should complete without errors: {:?}",
            result.errors
        );
        assert_eq!(
            result.stats.stats_deleted_objects, 50,
            "Should delete all 50 deeply nested objects"
        );

        let remaining = helper.count_objects(&bucket, "deep/").await;
        assert_eq!(remaining, 0, "No objects should remain under deep/");
        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// Sequential non-versioned listing with pagination
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_sequential_listing_pagination() {
    e2e_timeout!(async {
        // Purpose: Verify sequential non-versioned listing (max_parallel_listings=1)
        //          handles pagination correctly with small max_keys. This exercises
        //          the list_sequential(Objects, ...) path with multiple pages.
        // Setup:   60 objects under a single prefix. max_keys=7 forces 9 pages
        //          (8 full pages of 7 + 1 partial page of 4).
        // Expected: All 60 objects deleted across multiple list pages.

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        let objects: Vec<(String, Vec<u8>)> = (0..60)
            .map(|i| (format!("seq-page/file{i:02}.dat"), vec![b's'; 50]))
            .collect();
        helper.put_objects_parallel(&bucket, objects).await;

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/seq-page/"),
            "--max-parallel-listings",
            "1",
            "--max-keys",
            "7",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(
            !result.has_error,
            "Pipeline should complete without errors: {:?}",
            result.errors
        );
        assert_eq!(
            result.stats.stats_deleted_objects, 60,
            "Should delete all 60 objects across multiple pages"
        );
        assert_eq!(
            result.stats.stats_failed_objects, 0,
            "No objects should fail"
        );

        let remaining = helper.count_objects(&bucket, "seq-page/").await;
        assert_eq!(remaining, 0, "No objects should remain under seq-page/");
        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// Sequential versioned listing with pagination
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_sequential_versioned_listing_pagination() {
    e2e_timeout!(async {
        // Purpose: Verify sequential version listing (max_parallel_listings=1) handles
        //          pagination correctly with small max_keys. This exercises the
        //          list_sequential(Versions, ...) path with multiple pages.
        // Setup:   Versioned bucket with 30 objects (2 versions each = 60 versions).
        //          max_keys=10 forces pagination.
        // Expected: All 60 versions permanently deleted.

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_versioned_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        // Upload v1
        for i in 0..30 {
            helper
                .put_object(&bucket, &format!("seq-ver/file{i:02}.dat"), vec![b'1'; 50])
                .await;
        }
        // Upload v2
        for i in 0..30 {
            helper
                .put_object(&bucket, &format!("seq-ver/file{i:02}.dat"), vec![b'2'; 50])
                .await;
        }

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/seq-ver/"),
            "--delete-all-versions",
            "--max-parallel-listings",
            "1",
            "--max-keys",
            "10",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(
            result.stats.stats_deleted_objects, 60,
            "Should delete all 60 versions"
        );

        let versions = helper.list_object_versions(&bucket).await;
        let remaining: Vec<_> = versions
            .iter()
            .filter(|(k, _)| k.starts_with("seq-ver/") || k.starts_with("[delete-marker]seq-ver/"))
            .collect();
        assert!(remaining.is_empty(), "No versions should remain");
        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// Parallel listing with pagination (small max_keys)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_parallel_listing_with_pagination() {
    e2e_timeout!(async {
        // Purpose: Verify parallel listing handles pagination correctly when
        //          max_keys is small. Each parallel task must paginate through
        //          its sub-prefix independently.
        // Setup:   80 objects across 4 sub-prefixes (20 each). max_keys=5 forces
        //          multiple pages per sub-prefix.
        // Expected: All 80 objects deleted.

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        let objects: Vec<(String, Vec<u8>)> = (0..4)
            .flat_map(|p| {
                (0..20).map(move |i| (format!("par-page/dir{p}/file{i:02}.dat"), vec![b'p'; 50]))
            })
            .collect();
        helper.put_objects_parallel(&bucket, objects).await;

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/par-page/"),
            "--max-parallel-listings",
            "3",
            "--max-parallel-listing-max-depth",
            "1",
            "--max-keys",
            "5",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(
            !result.has_error,
            "Pipeline should complete without errors: {:?}",
            result.errors
        );
        assert_eq!(
            result.stats.stats_deleted_objects, 80,
            "Should delete all 80 objects"
        );

        let remaining = helper.count_objects(&bucket, "par-page/").await;
        assert_eq!(remaining, 0, "No objects should remain under par-page/");
        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// Parallel versioned listing with pagination
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_parallel_versioned_listing_with_pagination() {
    e2e_timeout!(async {
        // Purpose: Verify parallel version listing handles pagination correctly.
        //          Combines: versioned bucket + parallel listing + small max_keys.
        //          This is the most complex listing path (list_with_parallel(Versions, ...)
        //          with pagination in each parallel task).
        // Setup:   Versioned bucket with 40 objects across 2 sub-prefixes,
        //          2 versions each = 80 version entries. max_keys=7.
        // Expected: All 80 versions permanently deleted.

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_versioned_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        // Upload v1
        let v1: Vec<(String, Vec<u8>)> = (0..2)
            .flat_map(|p| {
                (0..20).map(move |i| (format!("pv-page/dir{p}/file{i:02}.dat"), vec![b'1'; 50]))
            })
            .collect();
        helper.put_objects_parallel(&bucket, v1).await;

        // Upload v2
        let v2: Vec<(String, Vec<u8>)> = (0..2)
            .flat_map(|p| {
                (0..20).map(move |i| (format!("pv-page/dir{p}/file{i:02}.dat"), vec![b'2'; 50]))
            })
            .collect();
        helper.put_objects_parallel(&bucket, v2).await;

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/pv-page/"),
            "--delete-all-versions",
            "--max-parallel-listings",
            "2",
            "--max-parallel-listing-max-depth",
            "1",
            "--max-keys",
            "7",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(
            result.stats.stats_deleted_objects, 80,
            "Should delete all 80 versions"
        );

        let versions = helper.list_object_versions(&bucket).await;
        let remaining: Vec<_> = versions
            .iter()
            .filter(|(k, _)| k.starts_with("pv-page/") || k.starts_with("[delete-marker]pv-page/"))
            .collect();
        assert!(remaining.is_empty(), "No versions should remain");
        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// Parallel listing with single prefix (no sub-prefix discovery)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_parallel_listing_flat_prefix() {
    e2e_timeout!(async {
        // Purpose: Verify parallel listing works when there are no sub-prefixes
        //          to discover (all objects are directly under one prefix with no
        //          "/" delimiter matches). The parallel listing should degrade
        //          gracefully to a single-task listing.
        // Setup:   50 objects all directly under flat/ (no sub-directories).
        // Expected: All 50 objects deleted.

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        let objects: Vec<(String, Vec<u8>)> = (0..50)
            .map(|i| (format!("flat/file{i:02}.dat"), vec![b'f'; 50]))
            .collect();
        helper.put_objects_parallel(&bucket, objects).await;

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/flat/"),
            "--max-parallel-listings",
            "4",
            "--max-parallel-listing-max-depth",
            "2",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(
            result.stats.stats_deleted_objects, 50,
            "Should delete all 50 objects"
        );

        let remaining = helper.count_objects(&bucket, "flat/").await;
        assert_eq!(remaining, 0, "No objects should remain under flat/");
        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// Sequential listing with delete markers (versioned, no --delete-all-versions)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_sequential_versioned_creates_delete_markers_then_parallel_cleans() {
    e2e_timeout!(async {
        // Purpose: Verify the full versioned listing workflow:
        //          1. Sequential list_objects → delete → creates delete markers
        //          2. Parallel list_object_versions (--delete-all-versions) → permanently removes all
        //          This validates both listing modes work correctly on the same bucket.
        // Setup:   Versioned bucket with 20 objects.
        // Expected: Step 1 creates delete markers; Step 2 removes all versions + markers.

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_versioned_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        for i in 0..20 {
            helper
                .put_object(&bucket, &format!("two-pass/file{i:02}.dat"), vec![b't'; 50])
                .await;
        }

        // Step 1: Sequential listing (default) → creates delete markers
        let config1 =
            TestHelper::build_config(vec![&format!("s3://{bucket}/two-pass/"), "--force"]);
        let result1 = TestHelper::run_pipeline(config1).await;
        assert!(!result1.has_error);
        assert_eq!(result1.stats.stats_deleted_objects, 20);

        // Verify delete markers were created
        let versions = helper.list_object_versions(&bucket).await;
        let markers: Vec<_> = versions
            .iter()
            .filter(|(k, _)| k.starts_with("[delete-marker]two-pass/"))
            .collect();
        assert_eq!(markers.len(), 20, "Should have 20 delete markers");

        // Step 2: Parallel versioned listing → permanently remove everything
        let config2 = TestHelper::build_config(vec![
            &format!("s3://{bucket}/two-pass/"),
            "--delete-all-versions",
            "--max-parallel-listings",
            "2",
            "--max-parallel-listing-max-depth",
            "1",
            "--force",
        ]);
        let result2 = TestHelper::run_pipeline(config2).await;
        assert!(!result2.has_error);
        // Should delete 20 original versions + 20 delete markers = 40
        assert_eq!(result2.stats.stats_deleted_objects, 40);

        let remaining = helper.list_object_versions(&bucket).await;
        let two_pass_remaining: Vec<_> = remaining
            .iter()
            .filter(|(k, _)| {
                k.starts_with("two-pass/") || k.starts_with("[delete-marker]two-pass/")
            })
            .collect();
        assert!(
            two_pass_remaining.is_empty(),
            "No versions or markers should remain"
        );
        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// 5,000 complex hierarchical objects: with prefix and without prefix
// ---------------------------------------------------------------------------

/// Extended timeout for the 5k-object hierarchy test (3 minutes).
const LARGE_HIERARCHY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(180);

/// Generate 5,000 objects arranged in a deeply nested, non-uniform hierarchy.
///
/// Structure (6 levels max):
/// ```text
/// {root}/dept-{0..1}/team-{0..4}/proj-{0..4}/
///     src/mod-{0..4}/f{0..9}.rs          = 2*5*5 * 5*10 = 2,500
///     test/t{0..4}.rs                    = 2*5*5 * 5     =   250
///     doc/d{0..4}.md                     = 2*5*5 * 5     =   250
///     data/s{0..3}/r{0..9}.json          = 2*5*5 * 4*10  = 2,000
///                                                  Total = 5,000
/// ```
fn generate_hierarchy_objects(root: &str) -> Vec<(String, Vec<u8>)> {
    let mut objects = Vec::with_capacity(5_000);
    let body = vec![b'H'; 64]; // minimal body to keep upload fast

    for dept in 0..2 {
        for team in 0..5 {
            for proj in 0..5 {
                let base = format!("{root}/dept-{dept}/team-{team}/proj-{proj}");

                // src/mod-{m}/f{f}.rs — 5 modules × 10 files = 50 per project
                for m in 0..5 {
                    for f in 0..10 {
                        objects.push((format!("{base}/src/mod-{m}/f{f}.rs"), body.clone()));
                    }
                }

                // test/t{t}.rs — 5 files per project
                for t in 0..5 {
                    objects.push((format!("{base}/test/t{t}.rs"), body.clone()));
                }

                // doc/d{d}.md — 5 files per project
                for d in 0..5 {
                    objects.push((format!("{base}/doc/d{d}.md"), body.clone()));
                }

                // data/s{s}/r{r}.json — 4 shards × 10 records = 40 per project
                for s in 0..4 {
                    for r in 0..10 {
                        objects.push((format!("{base}/data/s{s}/r{r}.json"), body.clone()));
                    }
                }
            }
        }
    }

    assert_eq!(
        objects.len(),
        5_000,
        "hierarchy must produce exactly 5,000 objects"
    );
    objects
}

#[tokio::test]
async fn e2e_5k_hierarchy_delete_with_prefix() {
    tokio::time::timeout(LARGE_HIERARCHY_TIMEOUT, async {
        // Purpose: Verify that 5,000 objects in a deeply nested hierarchy (up to 6
        //          levels) are correctly listed and deleted when targeting a sub-prefix.
        //          Exercises the unified list_with_parallel(Objects, ...) with many
        //          sub-prefix discoveries at multiple depths.
        //
        // Setup:   Upload 5,000 objects under "hier/". Target only "hier/dept-0/"
        //          which contains exactly 2,500 objects (1/2 of total).
        //          Use parallel listing with 8 workers and max-depth 4 to exercise
        //          recursive prefix discovery across the deep hierarchy.
        //
        // Expected: Exactly 2,500 objects deleted; 2,500 remain under dept-1/.

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        // Upload 5,000 objects
        let objects = generate_hierarchy_objects("hier");
        helper.put_objects_parallel(&bucket, objects).await;

        // Verify pre-state
        let pre_count = helper.count_objects(&bucket, "hier/").await;
        assert_eq!(pre_count, 5_000, "Should start with 5,000 objects");

        // Delete only dept-0 (2,500 objects = 5 teams * 5 projects * 100 files)
        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/hier/dept-0/"),
            "--max-parallel-listings",
            "8",
            "--max-parallel-listing-max-depth",
            "4",
            "--worker-size",
            "16",
            "--batch-size",
            "500",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(
            !result.has_error,
            "Pipeline should complete without errors: {:?}",
            result.errors
        );
        assert_eq!(
            result.stats.stats_deleted_objects, 2_500,
            "Should delete exactly 2,500 objects under dept-0/"
        );
        assert_eq!(
            result.stats.stats_failed_objects, 0,
            "No objects should fail"
        );

        // Verify dept-0 is empty
        let dept0_remaining = helper.count_objects(&bucket, "hier/dept-0/").await;
        assert_eq!(dept0_remaining, 0, "dept-0/ should be completely empty");

        // Verify the other 2,500 objects remain
        let total_remaining = helper.count_objects(&bucket, "hier/").await;
        assert_eq!(
            total_remaining, 2_500,
            "2,500 objects should remain in dept-1/"
        );

        guard.cleanup().await;
    })
    .await
    .expect("e2e_5k_hierarchy_delete_with_prefix timed out");
}

#[tokio::test]
async fn e2e_5k_hierarchy_delete_without_prefix() {
    tokio::time::timeout(LARGE_HIERARCHY_TIMEOUT, async {
        // Purpose: Verify that 5,000 objects in a deeply nested hierarchy are
        //          correctly listed and deleted when targeting the entire bucket
        //          (empty prefix). This exercises the root-level parallel listing
        //          entry point where prefix="" resolves to self.prefix (bucket root).
        //
        // Setup:   Upload 5,000 objects directly under the bucket root (no common
        //          prefix wrapper). Target "s3://bucket/" so the listing starts from
        //          the root and must discover all top-level dept-* prefixes.
        //          Use parallel listing with 8 workers and max-depth 3.
        //
        // Expected: All 5,000 objects deleted; bucket is empty.

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        // Upload 5,000 objects at bucket root (no wrapping prefix)
        let objects = generate_hierarchy_objects("root");
        helper.put_objects_parallel(&bucket, objects).await;

        // Verify pre-state
        let pre_count = helper.count_objects(&bucket, "").await;
        assert_eq!(pre_count, 5_000, "Should start with 5,000 objects");

        // Delete everything in the bucket (empty prefix)
        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/"),
            "--max-parallel-listings",
            "8",
            "--max-parallel-listing-max-depth",
            "3",
            "--worker-size",
            "16",
            "--batch-size",
            "500",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(
            !result.has_error,
            "Pipeline should complete without errors: {:?}",
            result.errors
        );
        assert_eq!(
            result.stats.stats_deleted_objects, 5_000,
            "Should delete all 5,000 objects"
        );
        assert_eq!(
            result.stats.stats_failed_objects, 0,
            "No objects should fail"
        );

        // Verify bucket is empty
        let remaining = helper.count_objects(&bucket, "").await;
        assert_eq!(remaining, 0, "Bucket should be completely empty");

        guard.cleanup().await;
    })
    .await
    .expect("e2e_5k_hierarchy_delete_without_prefix timed out");
}

// ---------------------------------------------------------------------------
// Express One Zone: 5,000 complex hierarchical objects with parallel listing
// ---------------------------------------------------------------------------

/// Returns the availability zone ID for Express One Zone directory buckets.
///
/// Reads `S3RM_E2E_AZ_ID` from the environment; falls back to `apne1-az4`
/// (ap-northeast-1 AZ 4) which is a commonly available Express One Zone AZ.
fn express_az_id() -> String {
    std::env::var("S3RM_E2E_AZ_ID").unwrap_or_else(|_| "apne1-az4".to_string())
}

#[tokio::test]
async fn e2e_5k_hierarchy_express_one_zone_delete_with_prefix() {
    tokio::time::timeout(LARGE_HIERARCHY_TIMEOUT, async {
        // Purpose: Verify that 5,000 objects in a deeply nested hierarchy are
        //          correctly listed and deleted on an Express One Zone directory
        //          bucket when targeting a sub-prefix with parallel listing.
        //          Express One Zone buckets require --allow-parallel-listings-in-
        //          express-one-zone to enable parallel listing; without it the
        //          pipeline falls back to sequential listing.
        //
        // Setup:   Express One Zone directory bucket with 5,000 objects under "hier/".
        //          Target only "hier/dept-0/" (2,500 objects).
        //          Use --allow-parallel-listings-in-express-one-zone with 4 workers.
        //
        // Expected: Exactly 2,500 objects deleted; 2,500 remain under dept-1/.

        let az_id = express_az_id();
        let helper = TestHelper::new().await;
        let bucket = helper.generate_directory_bucket_name(&az_id);
        helper.create_directory_bucket(&bucket, &az_id).await;

        let guard = helper.bucket_guard(&bucket);

        // Upload 5,000 objects
        let objects = generate_hierarchy_objects("hier");
        helper.put_objects_parallel(&bucket, objects).await;

        // Verify pre-state
        let pre_count = helper.count_objects(&bucket, "hier/").await;
        assert_eq!(pre_count, 5_000, "Should start with 5,000 objects");

        // Delete only dept-0 (2,500 objects)
        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/hier/dept-0/"),
            "--allow-parallel-listings-in-express-one-zone",
            "--max-parallel-listings",
            "4",
            "--max-parallel-listing-max-depth",
            "4",
            "--worker-size",
            "16",
            "--batch-size",
            "500",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(
            !result.has_error,
            "Pipeline should complete without errors: {:?}",
            result.errors
        );
        assert_eq!(
            result.stats.stats_deleted_objects, 2_500,
            "Should delete exactly 2,500 objects under dept-0/"
        );
        assert_eq!(
            result.stats.stats_failed_objects, 0,
            "No objects should fail"
        );

        // Verify dept-0 is empty
        let dept0_remaining = helper.count_objects(&bucket, "hier/dept-0/").await;
        assert_eq!(dept0_remaining, 0, "dept-0/ should be completely empty");

        // Verify the other 2,500 objects remain
        let total_remaining = helper.count_objects(&bucket, "hier/").await;
        assert_eq!(
            total_remaining, 2_500,
            "2,500 objects should remain in dept-1/"
        );

        guard.cleanup().await;
    })
    .await
    .expect("e2e_5k_hierarchy_express_one_zone_delete_with_prefix timed out");
}

#[tokio::test]
async fn e2e_5k_hierarchy_express_one_zone_delete_without_prefix() {
    tokio::time::timeout(LARGE_HIERARCHY_TIMEOUT, async {
        // Purpose: Verify that 5,000 objects in a deeply nested hierarchy are
        //          correctly listed and deleted on an Express One Zone directory
        //          bucket when targeting the entire bucket (empty prefix).
        //          Uses --allow-parallel-listings-in-express-one-zone for parallel
        //          listing from the root.
        //
        // Setup:   Express One Zone directory bucket with 5,000 objects.
        //          Target "s3://bucket/" (empty prefix).
        //          Use --allow-parallel-listings-in-express-one-zone with 4 workers.
        //
        // Expected: All 5,000 objects deleted; bucket is empty.

        let az_id = express_az_id();
        let helper = TestHelper::new().await;
        let bucket = helper.generate_directory_bucket_name(&az_id);
        helper.create_directory_bucket(&bucket, &az_id).await;

        let guard = helper.bucket_guard(&bucket);

        // Upload 5,000 objects
        let objects = generate_hierarchy_objects("root");
        helper.put_objects_parallel(&bucket, objects).await;

        // Verify pre-state
        let pre_count = helper.count_objects(&bucket, "").await;
        assert_eq!(pre_count, 5_000, "Should start with 5,000 objects");

        // Delete everything in the bucket (empty prefix)
        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/"),
            "--allow-parallel-listings-in-express-one-zone",
            "--max-parallel-listings",
            "4",
            "--max-parallel-listing-max-depth",
            "3",
            "--worker-size",
            "16",
            "--batch-size",
            "500",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(
            !result.has_error,
            "Pipeline should complete without errors: {:?}",
            result.errors
        );
        assert_eq!(
            result.stats.stats_deleted_objects, 5_000,
            "Should delete all 5,000 objects"
        );
        assert_eq!(
            result.stats.stats_failed_objects, 0,
            "No objects should fail"
        );

        // Verify bucket is empty
        let remaining = helper.count_objects(&bucket, "").await;
        assert_eq!(remaining, 0, "Bucket should be completely empty");

        guard.cleanup().await;
    })
    .await
    .expect("e2e_5k_hierarchy_express_one_zone_delete_without_prefix timed out");
}
