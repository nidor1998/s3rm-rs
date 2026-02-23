//! E2E tests for filtering options (Tests 29.1 - 29.12).
//!
//! Each test uploads objects with varying properties, runs the pipeline with
//! a specific filter, and asserts that only matching objects were deleted while
//! non-matching objects remain.

#![cfg(e2e_test)]

mod common;

use common::TestHelper;
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// 29.1 Include Regex Filter
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_filter_include_regex() {
    e2e_timeout!(async {
        // Purpose: Verify --filter-include-regex deletes only objects whose keys
        //          match the given regular expression pattern.
        // Setup:   Upload 20 objects: 10 under logs/*.txt, 10 under data/*.csv.
        // Expected: Only the 10 logs/*.txt objects are deleted; 10 data/ objects remain.
        //
        // Validates: Requirement 2.2

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        // Upload 10 logs/*.txt objects
        for i in 0..10 {
            helper
                .put_object(&bucket, &format!("logs/file{i}.txt"), vec![b'a'; 100])
                .await;
        }
        // Upload 10 data/*.csv objects
        for i in 0..10 {
            helper
                .put_object(&bucket, &format!("data/file{i}.csv"), vec![b'b'; 100])
                .await;
        }

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/"),
            "--filter-include-regex",
            r"^logs/.*\.txt$",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(
            result.stats.stats_deleted_objects, 10,
            "Should delete exactly 10 logs/*.txt objects"
        );

        let remaining = helper.count_objects(&bucket, "").await;
        assert_eq!(remaining, 10, "10 data/ objects should remain");

        let data_objects = helper.list_objects(&bucket, "data/").await;
        assert_eq!(data_objects.len(), 10, "All data/ objects should remain");
        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// 29.2 Exclude Regex Filter
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_filter_exclude_regex() {
    e2e_timeout!(async {
        // Purpose: Verify --filter-exclude-regex prevents matching objects from
        //          being deleted, while non-matching objects are deleted.
        // Setup:   Upload 20 objects: 10 under keep/, 10 under delete/.
        // Expected: 10 delete/ objects deleted; 10 keep/ objects remain.
        //
        // Validates: Requirement 2.2

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        for i in 0..10 {
            helper
                .put_object(&bucket, &format!("keep/file{i}.dat"), vec![b'k'; 100])
                .await;
        }
        for i in 0..10 {
            helper
                .put_object(&bucket, &format!("delete/file{i}.dat"), vec![b'd'; 100])
                .await;
        }

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/"),
            "--filter-exclude-regex",
            "^keep/",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(
            result.stats.stats_deleted_objects, 10,
            "Should delete exactly 10 delete/ objects"
        );

        let remaining = helper.list_objects(&bucket, "keep/").await;
        assert_eq!(remaining.len(), 10, "All keep/ objects should remain");
        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// 29.3 Include Content-Type Regex Filter
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_filter_include_content_type_regex() {
    e2e_timeout!(async {
        // Purpose: Verify --filter-include-content-type-regex deletes only objects
        //          whose Content-Type matches the pattern.
        // Setup:   Upload 10 objects with text/plain, 10 with application/json.
        // Expected: 10 text/plain objects deleted; 10 application/json objects remain.
        //
        // Validates: Requirement 2.3

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        for i in 0..10 {
            helper
                .put_object_with_content_type(
                    &bucket,
                    &format!("text/file{i}.txt"),
                    vec![b't'; 100],
                    "text/plain",
                )
                .await;
        }
        for i in 0..10 {
            helper
                .put_object_with_content_type(
                    &bucket,
                    &format!("json/file{i}.json"),
                    vec![b'j'; 100],
                    "application/json",
                )
                .await;
        }

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/"),
            "--filter-include-content-type-regex",
            "text/plain",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(
            result.stats.stats_deleted_objects, 10,
            "Should delete exactly 10 text/plain objects"
        );

        let remaining = helper.list_objects(&bucket, "json/").await;
        assert_eq!(
            remaining.len(),
            10,
            "All application/json objects should remain"
        );
        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// 29.4 Exclude Content-Type Regex Filter
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_filter_exclude_content_type_regex() {
    e2e_timeout!(async {
        // Purpose: Verify --filter-exclude-content-type-regex excludes objects
        //          whose Content-Type matches the pattern from deletion.
        // Setup:   Upload 10 objects with image/png, 10 with text/html.
        // Expected: 10 text/html objects deleted; 10 image/png objects remain.
        //
        // Validates: Requirement 2.3

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        for i in 0..10 {
            helper
                .put_object_with_content_type(
                    &bucket,
                    &format!("images/file{i}.png"),
                    vec![b'p'; 100],
                    "image/png",
                )
                .await;
        }
        for i in 0..10 {
            helper
                .put_object_with_content_type(
                    &bucket,
                    &format!("html/file{i}.html"),
                    vec![b'h'; 100],
                    "text/html",
                )
                .await;
        }

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/"),
            "--filter-exclude-content-type-regex",
            "image/png",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(
            result.stats.stats_deleted_objects, 10,
            "Should delete exactly 10 text/html objects"
        );

        let remaining = helper.list_objects(&bucket, "images/").await;
        assert_eq!(remaining.len(), 10, "All image/png objects should remain");
        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// 29.5 Include Metadata Regex Filter
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_filter_include_metadata_regex() {
    e2e_timeout!(async {
        // Purpose: Verify --filter-include-metadata-regex deletes only objects
        //          whose user-defined metadata matches the pattern.
        // Setup:   Upload 10 objects with metadata env=production, 10 with env=staging.
        // Expected: 10 env=production objects deleted; 10 env=staging objects remain.
        //
        // Validates: Requirement 2.4

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        for i in 0..10 {
            let mut metadata = HashMap::new();
            metadata.insert("env".to_string(), "production".to_string());
            helper
                .put_object_with_metadata(
                    &bucket,
                    &format!("prod/file{i}.dat"),
                    vec![b'p'; 100],
                    metadata,
                )
                .await;
        }
        for i in 0..10 {
            let mut metadata = HashMap::new();
            metadata.insert("env".to_string(), "staging".to_string());
            helper
                .put_object_with_metadata(
                    &bucket,
                    &format!("stage/file{i}.dat"),
                    vec![b's'; 100],
                    metadata,
                )
                .await;
        }

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/"),
            "--filter-include-metadata-regex",
            "env=production",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(
            result.stats.stats_deleted_objects, 10,
            "Should delete exactly 10 env=production objects"
        );

        let remaining = helper.list_objects(&bucket, "stage/").await;
        assert_eq!(remaining.len(), 10, "All env=staging objects should remain");
        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// 29.6 Exclude Metadata Regex Filter
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_filter_exclude_metadata_regex() {
    e2e_timeout!(async {
        // Purpose: Verify --filter-exclude-metadata-regex excludes objects whose
        //          metadata matches the pattern from deletion.
        // Setup:   Upload 10 objects with metadata tier=archive, 10 with tier=hot.
        // Expected: 10 tier=hot objects deleted; 10 tier=archive objects remain.
        //
        // Validates: Requirement 2.4

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        for i in 0..10 {
            let mut metadata = HashMap::new();
            metadata.insert("tier".to_string(), "archive".to_string());
            helper
                .put_object_with_metadata(
                    &bucket,
                    &format!("archive/file{i}.dat"),
                    vec![b'a'; 100],
                    metadata,
                )
                .await;
        }
        for i in 0..10 {
            let mut metadata = HashMap::new();
            metadata.insert("tier".to_string(), "hot".to_string());
            helper
                .put_object_with_metadata(
                    &bucket,
                    &format!("hot/file{i}.dat"),
                    vec![b'h'; 100],
                    metadata,
                )
                .await;
        }

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/"),
            "--filter-exclude-metadata-regex",
            "tier=archive",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(
            result.stats.stats_deleted_objects, 10,
            "Should delete exactly 10 tier=hot objects"
        );

        let remaining = helper.list_objects(&bucket, "archive/").await;
        assert_eq!(
            remaining.len(),
            10,
            "All tier=archive objects should remain"
        );

        let deleted_remaining = helper.count_objects(&bucket, "hot/").await;
        assert_eq!(
            deleted_remaining, 0,
            "All tier=hot objects should be removed from S3"
        );
        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// 29.6a Include Metadata Regex Filter — Multiple Entries (3+)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_filter_include_metadata_regex_multiple_entries() {
    e2e_timeout!(async {
        // Purpose: Verify --filter-include-metadata-regex works correctly when
        //          objects have 3 or more metadata entries. Metadata is serialized
        //          sorted alphabetically by key and comma-separated, e.g.
        //          "env=production,team=backend,version=v2". The regex must match
        //          across the sorted representation.
        // Setup:   Upload 20 objects:
        //          - 10 with metadata {env=production, team=backend, version=v2}
        //          - 10 with metadata {env=staging, team=frontend, version=v1}
        // Expected: Only the 10 env=production objects are deleted; 10 env=staging remain.
        //
        // Validates: Requirement 2.4

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        // Upload 10 objects with matching multi-metadata
        for i in 0..10 {
            let mut metadata = HashMap::new();
            metadata.insert("env".to_string(), "production".to_string());
            metadata.insert("team".to_string(), "backend".to_string());
            metadata.insert("version".to_string(), "v2".to_string());
            helper
                .put_object_with_metadata(
                    &bucket,
                    &format!("match/file{i}.dat"),
                    vec![b'm'; 100],
                    metadata,
                )
                .await;
        }
        // Upload 10 objects with non-matching multi-metadata
        for i in 0..10 {
            let mut metadata = HashMap::new();
            metadata.insert("env".to_string(), "staging".to_string());
            metadata.insert("team".to_string(), "frontend".to_string());
            metadata.insert("version".to_string(), "v1".to_string());
            helper
                .put_object_with_metadata(
                    &bucket,
                    &format!("nomatch/file{i}.dat"),
                    vec![b'n'; 100],
                    metadata,
                )
                .await;
        }

        // Regex uses comma-separated format per spec: "key1=value1,key2=value2"
        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/"),
            "--filter-include-metadata-regex",
            "env=production,team=backend,version=v2",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(
            result.stats.stats_deleted_objects, 10,
            "Should delete exactly 10 env=production objects"
        );

        let remaining_match = helper.list_objects(&bucket, "match/").await;
        assert_eq!(
            remaining_match.len(),
            0,
            "All matching objects should be deleted"
        );

        let remaining_nomatch = helper.list_objects(&bucket, "nomatch/").await;
        assert_eq!(
            remaining_nomatch.len(),
            10,
            "All non-matching objects should remain"
        );
        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// 29.6b Exclude Metadata Regex Filter — Multiple Entries with Alternation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_filter_exclude_metadata_regex_multiple_entries_alternation() {
    e2e_timeout!(async {
        // Purpose: Verify --filter-exclude-metadata-regex with the spec's alternation
        //          pattern "key1=(value1|value2),key2=value2" correctly excludes objects
        //          whose metadata matches. Metadata is serialized sorted alphabetically
        //          by key and comma-separated. This test uses the alternation syntax
        //          documented in the CLI help.
        // Setup:   Upload 30 objects:
        //          - 10 with metadata {env=production, team=backend, version=v2}
        //          - 10 with metadata {env=staging, team=backend, version=v2}
        //          - 10 with metadata {env=development, team=frontend, version=v1}
        // Regex:   "env=(production|staging),team=backend" excludes the first 20 objects.
        // Expected: Only the 10 env=development objects are deleted; 20 excluded remain.
        //
        // Validates: Requirement 2.4

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        // Upload 10 objects with metadata env=production,team=backend,version=v2
        for i in 0..10 {
            let mut metadata = HashMap::new();
            metadata.insert("env".to_string(), "production".to_string());
            metadata.insert("team".to_string(), "backend".to_string());
            metadata.insert("version".to_string(), "v2".to_string());
            helper
                .put_object_with_metadata(
                    &bucket,
                    &format!("prod/file{i}.dat"),
                    vec![b'p'; 100],
                    metadata,
                )
                .await;
        }
        // Upload 10 objects with metadata env=staging,team=backend,version=v2
        for i in 0..10 {
            let mut metadata = HashMap::new();
            metadata.insert("env".to_string(), "staging".to_string());
            metadata.insert("team".to_string(), "backend".to_string());
            metadata.insert("version".to_string(), "v2".to_string());
            helper
                .put_object_with_metadata(
                    &bucket,
                    &format!("staging/file{i}.dat"),
                    vec![b's'; 100],
                    metadata,
                )
                .await;
        }
        // Upload 10 objects with metadata env=development,team=frontend,version=v1
        for i in 0..10 {
            let mut metadata = HashMap::new();
            metadata.insert("env".to_string(), "development".to_string());
            metadata.insert("team".to_string(), "frontend".to_string());
            metadata.insert("version".to_string(), "v1".to_string());
            helper
                .put_object_with_metadata(
                    &bucket,
                    &format!("dev/file{i}.dat"),
                    vec![b'd'; 100],
                    metadata,
                )
                .await;
        }

        // Alternation pattern per spec: "key1=(value1|value2),key2=value2"
        // Matches sorted metadata: env=(production|staging),team=backend
        // This excludes prod/ and staging/ objects, leaving only dev/ for deletion
        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/"),
            "--filter-exclude-metadata-regex",
            "env=(production|staging),team=backend",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(
            result.stats.stats_deleted_objects, 10,
            "Should delete exactly 10 env=development objects"
        );

        let remaining_prod = helper.list_objects(&bucket, "prod/").await;
        assert_eq!(
            remaining_prod.len(),
            10,
            "All prod/ objects should remain (excluded by regex)"
        );

        let remaining_staging = helper.list_objects(&bucket, "staging/").await;
        assert_eq!(
            remaining_staging.len(),
            10,
            "All staging/ objects should remain (excluded by regex)"
        );

        let remaining_dev = helper.list_objects(&bucket, "dev/").await;
        assert_eq!(
            remaining_dev.len(),
            0,
            "All dev/ objects should be deleted (not excluded)"
        );
        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// 29.7 Include Tag Regex Filter
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_filter_include_tag_regex() {
    e2e_timeout!(async {
        // Purpose: Verify --filter-include-tag-regex deletes only objects whose
        //          S3 tags match the given pattern.
        // Setup:   Upload 10 objects tagged status=expired, 10 tagged status=active.
        // Expected: 10 status=expired objects deleted; 10 status=active objects remain.
        //
        // Validates: Requirement 2.5

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        for i in 0..10 {
            let mut tags = HashMap::new();
            tags.insert("status".to_string(), "expired".to_string());
            helper
                .put_object_with_tags(
                    &bucket,
                    &format!("expired/file{i}.dat"),
                    vec![b'e'; 100],
                    tags,
                )
                .await;
        }
        for i in 0..10 {
            let mut tags = HashMap::new();
            tags.insert("status".to_string(), "active".to_string());
            helper
                .put_object_with_tags(
                    &bucket,
                    &format!("active/file{i}.dat"),
                    vec![b'a'; 100],
                    tags,
                )
                .await;
        }

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/"),
            "--filter-include-tag-regex",
            "status=expired",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(
            result.stats.stats_deleted_objects, 10,
            "Should delete exactly 10 status=expired objects"
        );

        let remaining = helper.list_objects(&bucket, "active/").await;
        assert_eq!(
            remaining.len(),
            10,
            "All status=active objects should remain"
        );

        let deleted_remaining = helper.count_objects(&bucket, "expired/").await;
        assert_eq!(
            deleted_remaining, 0,
            "All status=expired objects should be removed from S3"
        );
        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// 29.8 Exclude Tag Regex Filter
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_filter_exclude_tag_regex() {
    e2e_timeout!(async {
        // Purpose: Verify --filter-exclude-tag-regex excludes objects whose tags
        //          match the pattern from deletion.
        // Setup:   Upload 10 objects tagged retain=true, 10 tagged retain=false.
        // Expected: 10 retain=false objects deleted; 10 retain=true objects remain.
        //
        // Validates: Requirement 2.5

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        for i in 0..10 {
            let mut tags = HashMap::new();
            tags.insert("retain".to_string(), "true".to_string());
            helper
                .put_object_with_tags(
                    &bucket,
                    &format!("retained/file{i}.dat"),
                    vec![b'r'; 100],
                    tags,
                )
                .await;
        }
        for i in 0..10 {
            let mut tags = HashMap::new();
            tags.insert("retain".to_string(), "false".to_string());
            helper
                .put_object_with_tags(
                    &bucket,
                    &format!("disposable/file{i}.dat"),
                    vec![b'd'; 100],
                    tags,
                )
                .await;
        }

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/"),
            "--filter-exclude-tag-regex",
            "retain=true",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(
            result.stats.stats_deleted_objects, 10,
            "Should delete exactly 10 retain=false objects"
        );

        let remaining = helper.list_objects(&bucket, "retained/").await;
        assert_eq!(remaining.len(), 10, "All retain=true objects should remain");

        let deleted_remaining = helper.count_objects(&bucket, "disposable/").await;
        assert_eq!(
            deleted_remaining, 0,
            "All retain=false objects should be removed from S3"
        );
        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// 29.8a Include Tag Regex Filter — Multiple Tags (3+)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_filter_include_tag_regex_multiple_tags() {
    e2e_timeout!(async {
        // Purpose: Verify --filter-include-tag-regex works correctly when objects
        //          have 3 or more tags. Tags are serialized sorted alphabetically
        //          by key and &-separated, e.g.
        //          "env=production&retain=false&team=backend". The regex must match
        //          across the sorted representation.
        // Setup:   Upload 20 objects:
        //          - 10 tagged {env=production, retain=false, team=backend}
        //          - 10 tagged {env=staging, retain=true, team=frontend}
        // Expected: Only the 10 env=production objects are deleted; 10 env=staging remain.
        //
        // Validates: Requirement 2.5

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        // Upload 10 objects with matching multi-tags
        for i in 0..10 {
            let mut tags = HashMap::new();
            tags.insert("env".to_string(), "production".to_string());
            tags.insert("retain".to_string(), "false".to_string());
            tags.insert("team".to_string(), "backend".to_string());
            helper
                .put_object_with_tags(
                    &bucket,
                    &format!("match/file{i}.dat"),
                    vec![b'm'; 100],
                    tags,
                )
                .await;
        }
        // Upload 10 objects with non-matching multi-tags
        for i in 0..10 {
            let mut tags = HashMap::new();
            tags.insert("env".to_string(), "staging".to_string());
            tags.insert("retain".to_string(), "true".to_string());
            tags.insert("team".to_string(), "frontend".to_string());
            helper
                .put_object_with_tags(
                    &bucket,
                    &format!("nomatch/file{i}.dat"),
                    vec![b'n'; 100],
                    tags,
                )
                .await;
        }

        // Regex uses &-separated format per spec: "key1=value1&key2=value2"
        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/"),
            "--filter-include-tag-regex",
            "env=production&retain=false&team=backend",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(
            result.stats.stats_deleted_objects, 10,
            "Should delete exactly 10 env=production objects"
        );

        let remaining_match = helper.list_objects(&bucket, "match/").await;
        assert_eq!(
            remaining_match.len(),
            0,
            "All matching objects should be deleted"
        );

        let remaining_nomatch = helper.list_objects(&bucket, "nomatch/").await;
        assert_eq!(
            remaining_nomatch.len(),
            10,
            "All non-matching objects should remain"
        );
        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// 29.8b Exclude Tag Regex Filter — Multiple Tags with Alternation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_filter_exclude_tag_regex_multiple_tags_alternation() {
    e2e_timeout!(async {
        // Purpose: Verify --filter-exclude-tag-regex with the spec's alternation
        //          pattern "key1=(value1|value2)&key2=value2" correctly excludes
        //          objects whose tags match. Tags are serialized sorted alphabetically
        //          by key and &-separated. This test uses the alternation syntax
        //          documented in the CLI help.
        // Setup:   Upload 30 objects:
        //          - 10 tagged {env=production, retain=true, team=backend}
        //          - 10 tagged {env=staging, retain=true, team=backend}
        //          - 10 tagged {env=development, retain=false, team=frontend}
        // Regex:   "env=(production|staging)&retain=true" excludes the first 20.
        // Expected: Only the 10 env=development objects are deleted; 20 excluded remain.
        //
        // Validates: Requirement 2.5

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        // Upload 10 objects tagged env=production,retain=true,team=backend
        for i in 0..10 {
            let mut tags = HashMap::new();
            tags.insert("env".to_string(), "production".to_string());
            tags.insert("retain".to_string(), "true".to_string());
            tags.insert("team".to_string(), "backend".to_string());
            helper
                .put_object_with_tags(&bucket, &format!("prod/file{i}.dat"), vec![b'p'; 100], tags)
                .await;
        }
        // Upload 10 objects tagged env=staging,retain=true,team=backend
        for i in 0..10 {
            let mut tags = HashMap::new();
            tags.insert("env".to_string(), "staging".to_string());
            tags.insert("retain".to_string(), "true".to_string());
            tags.insert("team".to_string(), "backend".to_string());
            helper
                .put_object_with_tags(
                    &bucket,
                    &format!("staging/file{i}.dat"),
                    vec![b's'; 100],
                    tags,
                )
                .await;
        }
        // Upload 10 objects tagged env=development,retain=false,team=frontend
        for i in 0..10 {
            let mut tags = HashMap::new();
            tags.insert("env".to_string(), "development".to_string());
            tags.insert("retain".to_string(), "false".to_string());
            tags.insert("team".to_string(), "frontend".to_string());
            helper
                .put_object_with_tags(&bucket, &format!("dev/file{i}.dat"), vec![b'd'; 100], tags)
                .await;
        }

        // Alternation pattern per spec: "key1=(value1|value2)&key2=value2"
        // Matches sorted tags: env=(production|staging)&retain=true
        // This excludes prod/ and staging/ objects, leaving only dev/ for deletion
        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/"),
            "--filter-exclude-tag-regex",
            "env=(production|staging)&retain=true",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(
            result.stats.stats_deleted_objects, 10,
            "Should delete exactly 10 env=development objects"
        );

        let remaining_prod = helper.list_objects(&bucket, "prod/").await;
        assert_eq!(
            remaining_prod.len(),
            10,
            "All prod/ objects should remain (excluded by regex)"
        );

        let remaining_staging = helper.list_objects(&bucket, "staging/").await;
        assert_eq!(
            remaining_staging.len(),
            10,
            "All staging/ objects should remain (excluded by regex)"
        );

        let remaining_dev = helper.list_objects(&bucket, "dev/").await;
        assert_eq!(
            remaining_dev.len(),
            0,
            "All dev/ objects should be deleted (not excluded)"
        );
        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// 29.9 Mtime Before Filter
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_filter_mtime_before() {
    e2e_timeout!(async {
        // Purpose: Verify --filter-mtime-before deletes only objects modified
        //          before the given timestamp.
        // Setup:   Upload 10 objects, sleep briefly, record timestamp T, then
        //          upload 10 more objects.
        // Expected: Only the first 10 (older) objects are deleted; newer 10 remain.
        //
        // Validates: Requirement 2.7

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        // Upload first batch (older objects)
        for i in 0..10 {
            helper
                .put_object(&bucket, &format!("old/file{i}.dat"), vec![b'o'; 100])
                .await;
        }

        // Sleep to create a clear timestamp boundary between "old" and "new" objects.
        // S3 last-modified timestamps have second-level granularity, so we need enough
        // margin to ensure no overlap. 3 seconds provides a safe buffer even on slow CI.
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;

        // Record the boundary timestamp
        let boundary = chrono::Utc::now().to_rfc3339();

        // Wait again to ensure new objects are strictly after the boundary
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;

        // Upload second batch (newer objects)
        for i in 0..10 {
            helper
                .put_object(&bucket, &format!("new/file{i}.dat"), vec![b'n'; 100])
                .await;
        }

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/"),
            "--filter-mtime-before",
            &boundary,
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(
            result.stats.stats_deleted_objects, 10,
            "Should delete exactly 10 older objects"
        );

        let remaining_new = helper.list_objects(&bucket, "new/").await;
        assert_eq!(remaining_new.len(), 10, "All newer objects should remain");

        let remaining_old = helper.list_objects(&bucket, "old/").await;
        assert_eq!(
            remaining_old.len(),
            0,
            "All older objects should be deleted"
        );
        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// 29.10 Mtime After Filter
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_filter_mtime_after() {
    e2e_timeout!(async {
        // Purpose: Verify --filter-mtime-after deletes only objects modified
        //          after the given timestamp.
        // Setup:   Upload 10 objects, sleep briefly, record timestamp T, then
        //          upload 10 more objects.
        // Expected: Only the second 10 (newer) objects are deleted; older 10 remain.
        //
        // Validates: Requirement 2.7

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        // Upload first batch (older objects)
        for i in 0..10 {
            helper
                .put_object(&bucket, &format!("old/file{i}.dat"), vec![b'o'; 100])
                .await;
        }

        // Sleep to create a clear timestamp boundary between "old" and "new" objects.
        // S3 last-modified timestamps have second-level granularity, so we need enough
        // margin to ensure no overlap. 3 seconds provides a safe buffer even on slow CI.
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;

        // Record the boundary timestamp
        let boundary = chrono::Utc::now().to_rfc3339();

        // Wait again to ensure new objects are strictly after the boundary
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;

        // Upload second batch (newer objects)
        for i in 0..10 {
            helper
                .put_object(&bucket, &format!("new/file{i}.dat"), vec![b'n'; 100])
                .await;
        }

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/"),
            "--filter-mtime-after",
            &boundary,
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(
            result.stats.stats_deleted_objects, 10,
            "Should delete exactly 10 newer objects"
        );

        let remaining_old = helper.list_objects(&bucket, "old/").await;
        assert_eq!(remaining_old.len(), 10, "All older objects should remain");

        let remaining_new = helper.list_objects(&bucket, "new/").await;
        assert_eq!(
            remaining_new.len(),
            0,
            "All newer objects should be deleted"
        );
        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// 29.11 Smaller Size Filter
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_filter_smaller_size() {
    e2e_timeout!(async {
        // Purpose: Verify --filter-smaller-size deletes only objects smaller than
        //          the given size threshold.
        // Setup:   Upload 10 objects of 100 bytes each, 10 objects of 10KB each.
        // Expected: 10 small (100B) objects deleted; 10 large (10KB) objects remain.
        //
        // Validates: Requirement 2.6

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        // Upload small objects (100 bytes)
        for i in 0..10 {
            helper
                .put_object(&bucket, &format!("small/file{i}.dat"), vec![b's'; 100])
                .await;
        }
        // Upload large objects (10KB)
        for i in 0..10 {
            helper
                .put_object(
                    &bucket,
                    &format!("large/file{i}.dat"),
                    vec![b'L'; 10 * 1024],
                )
                .await;
        }

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/"),
            "--filter-smaller-size",
            "1KB",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(
            result.stats.stats_deleted_objects, 10,
            "Should delete exactly 10 small objects"
        );

        let remaining_large = helper.list_objects(&bucket, "large/").await;
        assert_eq!(remaining_large.len(), 10, "All large objects should remain");

        let remaining_small = helper.list_objects(&bucket, "small/").await;
        assert_eq!(
            remaining_small.len(),
            0,
            "All small objects should be deleted"
        );
        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// 29.12 Larger Size Filter
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_filter_larger_size() {
    e2e_timeout!(async {
        // Purpose: Verify --filter-larger-size deletes only objects larger than
        //          the given size threshold.
        // Setup:   Upload 10 objects of 100 bytes each, 10 objects of 10KB each.
        // Expected: 10 large (10KB) objects deleted; 10 small (100B) objects remain.
        //
        // Validates: Requirement 2.6

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let guard = helper.bucket_guard(&bucket);

        // Upload small objects (100 bytes)
        for i in 0..10 {
            helper
                .put_object(&bucket, &format!("small/file{i}.dat"), vec![b's'; 100])
                .await;
        }
        // Upload large objects (10KB)
        for i in 0..10 {
            helper
                .put_object(
                    &bucket,
                    &format!("large/file{i}.dat"),
                    vec![b'L'; 10 * 1024],
                )
                .await;
        }

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/"),
            "--filter-larger-size",
            "1KB",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(
            result.stats.stats_deleted_objects, 10,
            "Should delete exactly 10 large objects"
        );

        let remaining_small = helper.list_objects(&bucket, "small/").await;
        assert_eq!(remaining_small.len(), 10, "All small objects should remain");

        let remaining_large = helper.list_objects(&bucket, "large/").await;
        assert_eq!(
            remaining_large.len(),
            0,
            "All large objects should be deleted"
        );
        guard.cleanup().await;
    });
}

// ---------------------------------------------------------------------------
// Prefix-scoped filter tests: verify HeadObject/GetObjectTagging-dependent
// filters work correctly when a non-empty prefix is specified.
//
// The S3 listing API returns full keys (e.g., "data/file.txt"). The
// head_object, get_object_tagging, and delete_object methods must use
// these keys as-is without re-prepending the prefix. These tests exercise
// that code path for each filter type.
// ---------------------------------------------------------------------------

// -- Key regex filters with prefix ------------------------------------------

#[tokio::test]
async fn e2e_filter_include_regex_with_prefix() {
    e2e_timeout!(async {
        // Purpose: Verify --filter-include-regex works with a non-empty prefix.
        //          Only objects under the prefix whose keys match the regex are
        //          deleted; objects outside the prefix are untouched.
        // Setup:   5 .log + 5 .dat under data/, 5 .log under outside/.
        // Expected: 5 data/*.log deleted; 5 data/*.dat + 5 outside/*.log remain.
        //
        // Validates: Requirement 2.2 (with prefix)

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;
        let guard = helper.bucket_guard(&bucket);

        for i in 0..5 {
            helper
                .put_object(&bucket, &format!("data/app{i}.log"), vec![b'L'; 100])
                .await;
            helper
                .put_object(&bucket, &format!("data/app{i}.dat"), vec![b'D'; 100])
                .await;
        }
        for i in 0..5 {
            helper
                .put_object(&bucket, &format!("outside/app{i}.log"), vec![b'O'; 100])
                .await;
        }

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/data/"),
            "--filter-include-regex",
            r"\.log$",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error);
        assert_eq!(result.stats.stats_deleted_objects, 5);
        assert_eq!(
            helper.count_objects(&bucket, "data/").await,
            5,
            "data/*.dat should remain"
        );
        assert_eq!(
            helper.count_objects(&bucket, "outside/").await,
            5,
            "outside/ untouched"
        );
        guard.cleanup().await;
    });
}

#[tokio::test]
async fn e2e_filter_exclude_regex_with_prefix() {
    e2e_timeout!(async {
        // Purpose: Verify --filter-exclude-regex works with a non-empty prefix.
        //          Objects under the prefix whose keys match the regex are kept;
        //          non-matching objects are deleted. Objects outside the prefix
        //          are untouched.
        // Setup:   5 .log + 5 .dat under data/, 5 .dat under outside/.
        // Expected: 5 data/*.dat deleted (not excluded); 5 data/*.log remain
        //           (excluded); 5 outside/*.dat untouched.
        //
        // Validates: Requirement 2.2 (with prefix)

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;
        let guard = helper.bucket_guard(&bucket);

        for i in 0..5 {
            helper
                .put_object(&bucket, &format!("data/keep{i}.log"), vec![b'L'; 100])
                .await;
            helper
                .put_object(&bucket, &format!("data/remove{i}.dat"), vec![b'D'; 100])
                .await;
        }
        for i in 0..5 {
            helper
                .put_object(&bucket, &format!("outside/file{i}.dat"), vec![b'O'; 100])
                .await;
        }

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/data/"),
            "--filter-exclude-regex",
            r"\.log$",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error);
        assert_eq!(result.stats.stats_deleted_objects, 5);
        assert_eq!(
            helper.count_objects(&bucket, "data/").await,
            5,
            "data/*.log should remain"
        );
        assert_eq!(
            helper.count_objects(&bucket, "outside/").await,
            5,
            "outside/ untouched"
        );
        guard.cleanup().await;
    });
}

// -- Content-type regex filters with prefix (requires HeadObject) -----------

#[tokio::test]
async fn e2e_filter_include_content_type_regex_with_prefix() {
    e2e_timeout!(async {
        // Purpose: Verify --filter-include-content-type-regex works with a
        //          non-empty prefix. This exercises the HeadObject call path
        //          that was affected by the double-prefix bug.
        // Setup:   5 text/plain + 5 application/json under data/,
        //          5 text/plain under outside/.
        // Expected: 5 data/ text/plain deleted; 5 data/ json + 5 outside/ remain.
        //
        // Validates: Requirement 2.3 (with prefix)

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;
        let guard = helper.bucket_guard(&bucket);

        for i in 0..5 {
            helper
                .put_object_with_content_type(
                    &bucket,
                    &format!("data/text{i}.txt"),
                    vec![b't'; 100],
                    "text/plain",
                )
                .await;
            helper
                .put_object_with_content_type(
                    &bucket,
                    &format!("data/doc{i}.json"),
                    vec![b'j'; 100],
                    "application/json",
                )
                .await;
        }
        for i in 0..5 {
            helper
                .put_object_with_content_type(
                    &bucket,
                    &format!("outside/text{i}.txt"),
                    vec![b'o'; 100],
                    "text/plain",
                )
                .await;
        }

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/data/"),
            "--filter-include-content-type-regex",
            "text/plain",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(
            result.stats.stats_deleted_objects, 5,
            "Should delete 5 text/plain under data/"
        );
        assert_eq!(
            helper.count_objects(&bucket, "data/").await,
            5,
            "data/ json objects remain"
        );
        assert_eq!(
            helper.count_objects(&bucket, "outside/").await,
            5,
            "outside/ untouched"
        );
        guard.cleanup().await;
    });
}

#[tokio::test]
async fn e2e_filter_exclude_content_type_regex_with_prefix() {
    e2e_timeout!(async {
        // Purpose: Verify --filter-exclude-content-type-regex works with a
        //          non-empty prefix. Objects matching the content-type pattern
        //          are excluded (kept); the rest under the prefix are deleted.
        // Setup:   5 text/plain + 5 application/json under data/,
        //          5 application/json under outside/.
        // Expected: 5 data/ text/plain deleted (not excluded);
        //           5 data/ json kept (excluded); 5 outside/ untouched.
        //
        // Validates: Requirement 2.3 (with prefix)

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;
        let guard = helper.bucket_guard(&bucket);

        for i in 0..5 {
            helper
                .put_object_with_content_type(
                    &bucket,
                    &format!("data/remove{i}.txt"),
                    vec![b't'; 100],
                    "text/plain",
                )
                .await;
            helper
                .put_object_with_content_type(
                    &bucket,
                    &format!("data/keep{i}.json"),
                    vec![b'j'; 100],
                    "application/json",
                )
                .await;
        }
        for i in 0..5 {
            helper
                .put_object_with_content_type(
                    &bucket,
                    &format!("outside/doc{i}.json"),
                    vec![b'o'; 100],
                    "application/json",
                )
                .await;
        }

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/data/"),
            "--filter-exclude-content-type-regex",
            "application/json",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(
            result.stats.stats_deleted_objects, 5,
            "Should delete 5 text/plain under data/"
        );
        assert_eq!(
            helper.count_objects(&bucket, "data/").await,
            5,
            "data/ json objects remain"
        );
        assert_eq!(
            helper.count_objects(&bucket, "outside/").await,
            5,
            "outside/ untouched"
        );
        guard.cleanup().await;
    });
}

// -- Metadata regex filters with prefix (requires HeadObject) ---------------

#[tokio::test]
async fn e2e_filter_include_metadata_regex_with_prefix() {
    e2e_timeout!(async {
        // Purpose: Verify --filter-include-metadata-regex works with a non-empty
        //          prefix. This exercises the HeadObject call path for metadata
        //          retrieval with prefixed keys.
        // Setup:   5 objects with env=production + 5 with env=staging under data/,
        //          5 with env=production under outside/.
        // Expected: 5 data/ env=production deleted; 5 data/ staging +
        //           5 outside/ remain.
        //
        // Validates: Requirement 2.4 (with prefix)

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;
        let guard = helper.bucket_guard(&bucket);

        for i in 0..5 {
            let mut meta_prod = HashMap::new();
            meta_prod.insert("env".to_string(), "production".to_string());
            helper
                .put_object_with_metadata(
                    &bucket,
                    &format!("data/prod{i}.dat"),
                    vec![b'p'; 100],
                    meta_prod,
                )
                .await;

            let mut meta_stage = HashMap::new();
            meta_stage.insert("env".to_string(), "staging".to_string());
            helper
                .put_object_with_metadata(
                    &bucket,
                    &format!("data/stage{i}.dat"),
                    vec![b's'; 100],
                    meta_stage,
                )
                .await;
        }
        for i in 0..5 {
            let mut meta_prod = HashMap::new();
            meta_prod.insert("env".to_string(), "production".to_string());
            helper
                .put_object_with_metadata(
                    &bucket,
                    &format!("outside/prod{i}.dat"),
                    vec![b'o'; 100],
                    meta_prod,
                )
                .await;
        }

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/data/"),
            "--filter-include-metadata-regex",
            "env=production",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(
            result.stats.stats_deleted_objects, 5,
            "Should delete 5 env=production under data/"
        );
        assert_eq!(
            helper.count_objects(&bucket, "data/").await,
            5,
            "data/ staging objects remain"
        );
        assert_eq!(
            helper.count_objects(&bucket, "outside/").await,
            5,
            "outside/ untouched"
        );
        guard.cleanup().await;
    });
}

#[tokio::test]
async fn e2e_filter_exclude_metadata_regex_with_prefix() {
    e2e_timeout!(async {
        // Purpose: Verify --filter-exclude-metadata-regex works with a non-empty
        //          prefix. Objects matching the metadata pattern are excluded (kept).
        // Setup:   5 objects with env=production + 5 with env=staging under data/,
        //          5 with env=staging under outside/.
        // Expected: 5 data/ env=production deleted (not excluded);
        //           5 data/ staging kept (excluded); 5 outside/ untouched.
        //
        // Validates: Requirement 2.4 (with prefix)

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;
        let guard = helper.bucket_guard(&bucket);

        for i in 0..5 {
            let mut meta_prod = HashMap::new();
            meta_prod.insert("env".to_string(), "production".to_string());
            helper
                .put_object_with_metadata(
                    &bucket,
                    &format!("data/prod{i}.dat"),
                    vec![b'p'; 100],
                    meta_prod,
                )
                .await;

            let mut meta_stage = HashMap::new();
            meta_stage.insert("env".to_string(), "staging".to_string());
            helper
                .put_object_with_metadata(
                    &bucket,
                    &format!("data/stage{i}.dat"),
                    vec![b's'; 100],
                    meta_stage,
                )
                .await;
        }
        for i in 0..5 {
            let mut meta_stage = HashMap::new();
            meta_stage.insert("env".to_string(), "staging".to_string());
            helper
                .put_object_with_metadata(
                    &bucket,
                    &format!("outside/stage{i}.dat"),
                    vec![b'o'; 100],
                    meta_stage,
                )
                .await;
        }

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/data/"),
            "--filter-exclude-metadata-regex",
            "env=staging",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(
            result.stats.stats_deleted_objects, 5,
            "Should delete 5 env=production under data/"
        );
        assert_eq!(
            helper.count_objects(&bucket, "data/stage").await,
            5,
            "data/ staging objects remain (excluded)"
        );
        assert_eq!(
            helper.count_objects(&bucket, "data/prod").await,
            0,
            "data/ production objects should be removed from S3"
        );
        assert_eq!(
            helper.count_objects(&bucket, "outside/").await,
            5,
            "outside/ untouched"
        );
        guard.cleanup().await;
    });
}

// -- Tag regex filters with prefix (requires GetObjectTagging) --------------

#[tokio::test]
async fn e2e_filter_include_tag_regex_with_prefix() {
    e2e_timeout!(async {
        // Purpose: Verify --filter-include-tag-regex works with a non-empty prefix.
        //          This exercises the GetObjectTagging call path with prefixed keys.
        // Setup:   5 objects tagged status=expired + 5 tagged status=active under
        //          data/, 5 tagged status=expired under outside/.
        // Expected: 5 data/ status=expired deleted; 5 data/ active +
        //           5 outside/ remain.
        //
        // Validates: Requirement 2.5 (with prefix)

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;
        let guard = helper.bucket_guard(&bucket);

        for i in 0..5 {
            let mut tags_expired = HashMap::new();
            tags_expired.insert("status".to_string(), "expired".to_string());
            helper
                .put_object_with_tags(
                    &bucket,
                    &format!("data/old{i}.dat"),
                    vec![b'e'; 100],
                    tags_expired,
                )
                .await;

            let mut tags_active = HashMap::new();
            tags_active.insert("status".to_string(), "active".to_string());
            helper
                .put_object_with_tags(
                    &bucket,
                    &format!("data/current{i}.dat"),
                    vec![b'a'; 100],
                    tags_active,
                )
                .await;
        }
        for i in 0..5 {
            let mut tags_expired = HashMap::new();
            tags_expired.insert("status".to_string(), "expired".to_string());
            helper
                .put_object_with_tags(
                    &bucket,
                    &format!("outside/old{i}.dat"),
                    vec![b'o'; 100],
                    tags_expired,
                )
                .await;
        }

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/data/"),
            "--filter-include-tag-regex",
            "status=expired",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(
            result.stats.stats_deleted_objects, 5,
            "Should delete 5 status=expired under data/"
        );
        assert_eq!(
            helper.count_objects(&bucket, "data/current").await,
            5,
            "data/ active objects remain"
        );
        assert_eq!(
            helper.count_objects(&bucket, "data/old").await,
            0,
            "data/ expired objects should be removed from S3"
        );
        assert_eq!(
            helper.count_objects(&bucket, "outside/").await,
            5,
            "outside/ untouched"
        );
        guard.cleanup().await;
    });
}

#[tokio::test]
async fn e2e_filter_exclude_tag_regex_with_prefix() {
    e2e_timeout!(async {
        // Purpose: Verify --filter-exclude-tag-regex works with a non-empty prefix.
        //          Objects matching the tag pattern are excluded (kept).
        // Setup:   5 objects tagged status=expired + 5 tagged status=active under
        //          data/, 5 tagged status=active under outside/.
        // Expected: 5 data/ status=expired deleted (not excluded);
        //           5 data/ active kept (excluded); 5 outside/ untouched.
        //
        // Validates: Requirement 2.5 (with prefix)

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;
        let guard = helper.bucket_guard(&bucket);

        for i in 0..5 {
            let mut tags_expired = HashMap::new();
            tags_expired.insert("status".to_string(), "expired".to_string());
            helper
                .put_object_with_tags(
                    &bucket,
                    &format!("data/old{i}.dat"),
                    vec![b'e'; 100],
                    tags_expired,
                )
                .await;

            let mut tags_active = HashMap::new();
            tags_active.insert("status".to_string(), "active".to_string());
            helper
                .put_object_with_tags(
                    &bucket,
                    &format!("data/current{i}.dat"),
                    vec![b'a'; 100],
                    tags_active,
                )
                .await;
        }
        for i in 0..5 {
            let mut tags_active = HashMap::new();
            tags_active.insert("status".to_string(), "active".to_string());
            helper
                .put_object_with_tags(
                    &bucket,
                    &format!("outside/current{i}.dat"),
                    vec![b'o'; 100],
                    tags_active,
                )
                .await;
        }

        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/data/"),
            "--filter-exclude-tag-regex",
            "status=active",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(
            result.stats.stats_deleted_objects, 5,
            "Should delete 5 status=expired under data/"
        );
        assert_eq!(
            helper.count_objects(&bucket, "data/current").await,
            5,
            "data/ active objects remain (excluded)"
        );
        assert_eq!(
            helper.count_objects(&bucket, "data/old").await,
            0,
            "data/ expired objects should be removed from S3"
        );
        assert_eq!(
            helper.count_objects(&bucket, "outside/").await,
            5,
            "outside/ untouched"
        );
        guard.cleanup().await;
    });
}
