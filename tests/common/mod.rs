//! Shared E2E test infrastructure for s3rm-rs.
//!
//! Provides `TestHelper` for bucket management, object operations, and pipeline
//! execution against real AWS S3. All helpers use the `s3rm-e2e-test` AWS profile.

#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::Arc;

use aws_config::BehaviorVersion;
use aws_sdk_s3::Client;
use aws_sdk_s3::types::{
    BucketLocationConstraint, BucketVersioningStatus, CreateBucketConfiguration, Delete,
    ObjectIdentifier, VersioningConfiguration,
};
use s3rm_rs::config::args::build_config_from_args;
use s3rm_rs::{Config, DeletionPipeline, DeletionStats, create_pipeline_cancellation_token};
use uuid::Uuid;

/// AWS profile used for all E2E tests.
const AWS_PROFILE: &str = "s3rm-e2e-test";

/// Default region for E2E tests (used when creating buckets).
/// The actual region is determined by the AWS profile, but we need a
/// location constraint for bucket creation outside us-east-1.
const DEFAULT_REGION: &str = "us-east-1";

/// Result of running a deletion pipeline.
#[derive(Debug)]
pub struct PipelineResult {
    /// Deletion statistics snapshot after pipeline completion.
    pub stats: DeletionStats,
    /// Whether the pipeline encountered any error.
    pub has_error: bool,
    /// Whether a pipeline stage panicked (task panic detected).
    pub has_panic: bool,
    /// Whether the pipeline encountered any warning.
    pub has_warning: bool,
    /// Error messages collected from the pipeline (empty if no errors).
    pub errors: Vec<String>,
}

/// RAII guard that deletes all objects and the bucket when dropped.
///
/// This ensures cleanup ALWAYS runs, even if the test panics. Call
/// `TestHelper::bucket_guard()` to create one after creating a bucket.
pub struct BucketGuard {
    helper: Arc<TestHelper>,
    bucket: String,
}

impl Drop for BucketGuard {
    fn drop(&mut self) {
        let helper = self.helper.clone();
        let bucket = self.bucket.clone();
        // Catch panics from block_on() to avoid double-panic abort when the
        // runtime is shutting down (e.g., if the test already panicked).
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            tokio::runtime::Handle::current().block_on(async move {
                helper.delete_bucket_cascade(&bucket).await;
            });
        }));
    }
}

/// Shared test helper for E2E tests.
///
/// Wraps an AWS S3 `Client` built with the `s3rm-e2e-test` profile and provides
/// convenience methods for bucket management, object operations, and pipeline execution.
pub struct TestHelper {
    client: Client,
    region: String,
}

impl TestHelper {
    /// Create a new TestHelper with an S3 client configured via the e2e test profile.
    pub async fn new() -> Arc<Self> {
        let sdk_config = aws_config::defaults(BehaviorVersion::latest())
            .profile_name(AWS_PROFILE)
            .load()
            .await;

        let region = sdk_config
            .region()
            .map(|r| r.to_string())
            .unwrap_or_else(|| DEFAULT_REGION.to_string());

        let client = Client::new(&sdk_config);

        Arc::new(Self { client, region })
    }

    /// Create a RAII guard that cleans up the bucket on drop.
    ///
    /// The guard deletes all objects (including versions and delete markers)
    /// and then deletes the bucket itself. This runs even if the test panics.
    pub fn bucket_guard(self: &Arc<Self>, bucket: &str) -> BucketGuard {
        BucketGuard {
            helper: Arc::clone(self),
            bucket: bucket.to_string(),
        }
    }

    /// Generate a unique bucket name for test isolation.
    ///
    /// Returns a name like `s3rm-e2e-<uuid>` which is guaranteed unique
    /// across parallel test runs.
    pub fn generate_bucket_name(&self) -> String {
        format!("s3rm-e2e-{}", Uuid::new_v4())
    }

    /// Return the AWS region this helper is configured for.
    pub fn region(&self) -> &str {
        &self.region
    }

    // -----------------------------------------------------------------------
    // Bucket management
    // -----------------------------------------------------------------------

    /// Create a standard (non-versioned) S3 bucket.
    pub async fn create_bucket(&self, bucket: &str) {
        let mut builder = self.client.create_bucket().bucket(bucket);

        // us-east-1 must NOT specify a location constraint
        if self.region != "us-east-1" {
            let constraint = BucketLocationConstraint::from(self.region.as_str());
            let config = CreateBucketConfiguration::builder()
                .location_constraint(constraint)
                .build();
            builder = builder.create_bucket_configuration(config);
        }

        builder
            .send()
            .await
            .unwrap_or_else(|e| panic!("Failed to create bucket {bucket}: {e}"));
    }

    /// Create a versioned S3 bucket (create bucket + enable versioning).
    pub async fn create_versioned_bucket(&self, bucket: &str) {
        self.create_bucket(bucket).await;

        let versioning_config = VersioningConfiguration::builder()
            .status(BucketVersioningStatus::Enabled)
            .build();

        self.client
            .put_bucket_versioning()
            .bucket(bucket)
            .versioning_configuration(versioning_config)
            .send()
            .await
            .unwrap_or_else(|e| panic!("Failed to enable versioning on {bucket}: {e}"));
    }

    /// Delete all objects (including versions and delete markers) and then delete the bucket.
    ///
    /// This is the cleanup function that MUST always run, regardless of test outcome.
    pub async fn delete_bucket_cascade(&self, bucket: &str) {
        // First try to delete all object versions (handles versioned buckets)
        self.delete_all_versions(bucket).await;

        // Then delete any remaining non-versioned objects
        self.delete_all_objects(bucket).await;

        // Finally delete the bucket itself
        let _ = self.client.delete_bucket().bucket(bucket).send().await;
    }

    /// Delete all object versions and delete markers from a bucket.
    async fn delete_all_versions(&self, bucket: &str) {
        let mut key_marker: Option<String> = None;
        let mut version_id_marker: Option<String> = None;

        loop {
            let mut req = self.client.list_object_versions().bucket(bucket);
            if let Some(ref km) = key_marker {
                req = req.key_marker(km);
            }
            if let Some(ref vim) = version_id_marker {
                req = req.version_id_marker(vim);
            }

            let resp = match req.send().await {
                Ok(r) => r,
                Err(_) => return, // Bucket may not exist or no permission
            };

            let mut objects_to_delete: Vec<ObjectIdentifier> = Vec::new();

            // Collect versions (returns &[ObjectVersion])
            for v in resp.versions() {
                if let (Some(key), Some(vid)) = (v.key(), v.version_id()) {
                    objects_to_delete.push(
                        ObjectIdentifier::builder()
                            .key(key)
                            .version_id(vid)
                            .build()
                            .unwrap(),
                    );
                }
            }

            // Collect delete markers (returns &[DeleteMarkerEntry])
            for m in resp.delete_markers() {
                if let (Some(key), Some(vid)) = (m.key(), m.version_id()) {
                    objects_to_delete.push(
                        ObjectIdentifier::builder()
                            .key(key)
                            .version_id(vid)
                            .build()
                            .unwrap(),
                    );
                }
            }

            // Batch delete in chunks of 1000
            for chunk in objects_to_delete.chunks(1000) {
                let delete = Delete::builder()
                    .set_objects(Some(chunk.to_vec()))
                    .quiet(true)
                    .build()
                    .unwrap();
                let _ = self
                    .client
                    .delete_objects()
                    .bucket(bucket)
                    .delete(delete)
                    .send()
                    .await;
            }

            if resp.is_truncated() == Some(true) {
                key_marker = resp.next_key_marker().map(|s| s.to_string());
                version_id_marker = resp.next_version_id_marker().map(|s| s.to_string());
            } else {
                break;
            }
        }
    }

    /// Delete all non-versioned objects from a bucket.
    async fn delete_all_objects(&self, bucket: &str) {
        let mut continuation_token: Option<String> = None;

        loop {
            let mut req = self.client.list_objects_v2().bucket(bucket);
            if let Some(ref token) = continuation_token {
                req = req.continuation_token(token);
            }

            let resp = match req.send().await {
                Ok(r) => r,
                Err(_) => return,
            };

            let contents = resp.contents();
            if contents.is_empty() {
                break;
            }

            let objects: Vec<ObjectIdentifier> = contents
                .iter()
                .filter_map(|obj| {
                    obj.key()
                        .map(|k| ObjectIdentifier::builder().key(k).build().unwrap())
                })
                .collect();

            if !objects.is_empty() {
                let delete = Delete::builder()
                    .set_objects(Some(objects))
                    .quiet(true)
                    .build()
                    .unwrap();
                let _ = self
                    .client
                    .delete_objects()
                    .bucket(bucket)
                    .delete(delete)
                    .send()
                    .await;
            }

            if resp.is_truncated() == Some(true) {
                continuation_token = resp.next_continuation_token().map(|s| s.to_string());
            } else {
                break;
            }
        }
    }

    // -----------------------------------------------------------------------
    // Object operations
    // -----------------------------------------------------------------------

    /// Upload an object with the given body bytes.
    pub async fn put_object(&self, bucket: &str, key: &str, body: Vec<u8>) {
        self.client
            .put_object()
            .bucket(bucket)
            .key(key)
            .body(body.into())
            .send()
            .await
            .unwrap_or_else(|e| panic!("Failed to put object {key} in {bucket}: {e}"));
    }

    /// Upload an object with an explicit Content-Type.
    pub async fn put_object_with_content_type(
        &self,
        bucket: &str,
        key: &str,
        body: Vec<u8>,
        content_type: &str,
    ) {
        self.client
            .put_object()
            .bucket(bucket)
            .key(key)
            .body(body.into())
            .content_type(content_type)
            .send()
            .await
            .unwrap_or_else(|e| {
                panic!("Failed to put object {key} with content-type {content_type}: {e}")
            });
    }

    /// Upload an object with user-defined metadata.
    pub async fn put_object_with_metadata(
        &self,
        bucket: &str,
        key: &str,
        body: Vec<u8>,
        metadata: HashMap<String, String>,
    ) {
        let mut builder = self
            .client
            .put_object()
            .bucket(bucket)
            .key(key)
            .body(body.into());

        for (k, v) in &metadata {
            builder = builder.metadata(k, v);
        }

        builder
            .send()
            .await
            .unwrap_or_else(|e| panic!("Failed to put object {key} with metadata: {e}"));
    }

    /// Upload an object with S3 tags.
    pub async fn put_object_with_tags(
        &self,
        bucket: &str,
        key: &str,
        body: Vec<u8>,
        tags: HashMap<String, String>,
    ) {
        // S3 PutObject tagging uses the x-amz-tagging header with URL-encoded key=value pairs
        let tag_string: String = tags
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("&");

        self.client
            .put_object()
            .bucket(bucket)
            .key(key)
            .body(body.into())
            .tagging(&tag_string)
            .send()
            .await
            .unwrap_or_else(|e| panic!("Failed to put object {key} with tags: {e}"));
    }

    /// Upload an object with content-type, user-defined metadata, AND tags all at once.
    pub async fn put_object_full(
        &self,
        bucket: &str,
        key: &str,
        body: Vec<u8>,
        content_type: &str,
        metadata: HashMap<String, String>,
        tags: HashMap<String, String>,
    ) {
        let tag_string: String = tags
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("&");

        let mut builder = self
            .client
            .put_object()
            .bucket(bucket)
            .key(key)
            .body(body.into())
            .content_type(content_type)
            .tagging(&tag_string);

        for (k, v) in &metadata {
            builder = builder.metadata(k, v);
        }

        builder
            .send()
            .await
            .unwrap_or_else(|e| panic!("Failed to put object {key} with full properties: {e}"));
    }

    /// Upload multiple objects in parallel for faster test setup.
    ///
    /// Each entry is a `(key, body)` pair. Uploads run concurrently using a
    /// `JoinSet`, significantly reducing wall-clock time compared to sequential
    /// uploads.
    pub async fn put_objects_parallel(&self, bucket: &str, objects: Vec<(String, Vec<u8>)>) {
        let mut set = tokio::task::JoinSet::new();
        let client = self.client.clone();
        let bucket = bucket.to_string();

        for (key, body) in objects {
            let client = client.clone();
            let bucket = bucket.clone();
            set.spawn(async move {
                client
                    .put_object()
                    .bucket(&bucket)
                    .key(&key)
                    .body(body.into())
                    .send()
                    .await
                    .unwrap_or_else(|e| panic!("Failed to put object {key} in {bucket}: {e}"));
            });
        }

        while let Some(result) = set.join_next().await {
            result.expect("Upload task panicked");
        }
    }

    /// List remaining object keys under the given prefix.
    pub async fn list_objects(&self, bucket: &str, prefix: &str) -> Vec<String> {
        let mut keys = Vec::new();
        let mut continuation_token: Option<String> = None;

        loop {
            let mut req = self.client.list_objects_v2().bucket(bucket).prefix(prefix);
            if let Some(ref token) = continuation_token {
                req = req.continuation_token(token);
            }

            let resp = req
                .send()
                .await
                .unwrap_or_else(|e| panic!("Failed to list objects in {bucket}/{prefix}: {e}"));

            for obj in resp.contents() {
                if let Some(key) = obj.key() {
                    keys.push(key.to_string());
                }
            }

            if resp.is_truncated() == Some(true) {
                continuation_token = resp.next_continuation_token().map(|s| s.to_string());
            } else {
                break;
            }
        }

        keys
    }

    /// List object versions in a bucket. Returns (key, version_id) pairs.
    /// Delete markers are returned with a "[delete-marker]" prefix on the key.
    pub async fn list_object_versions(&self, bucket: &str) -> Vec<(String, String)> {
        let mut result: Vec<(String, String)> = Vec::new();
        let mut key_marker: Option<String> = None;
        let mut version_id_marker: Option<String> = None;

        loop {
            let mut req = self.client.list_object_versions().bucket(bucket);
            if let Some(ref km) = key_marker {
                req = req.key_marker(km);
            }
            if let Some(ref vim) = version_id_marker {
                req = req.version_id_marker(vim);
            }

            let resp = req
                .send()
                .await
                .unwrap_or_else(|e| panic!("Failed to list object versions in {bucket}: {e}"));

            for v in resp.versions() {
                if let (Some(key), Some(vid)) = (v.key(), v.version_id()) {
                    result.push((key.to_string(), vid.to_string()));
                }
            }

            for m in resp.delete_markers() {
                if let (Some(key), Some(vid)) = (m.key(), m.version_id()) {
                    result.push((format!("[delete-marker]{key}"), vid.to_string()));
                }
            }

            if resp.is_truncated() == Some(true) {
                key_marker = resp.next_key_marker().map(|s| s.to_string());
                version_id_marker = resp.next_version_id_marker().map(|s| s.to_string());
            } else {
                break;
            }
        }

        result
    }

    /// Count objects remaining under the given prefix.
    pub async fn count_objects(&self, bucket: &str, prefix: &str) -> usize {
        self.list_objects(bucket, prefix).await.len()
    }

    /// Return a reference to the underlying S3 client for advanced operations.
    pub fn client(&self) -> &Client {
        &self.client
    }

    /// Apply a bucket policy that denies `s3:DeleteObject` for the given key prefix.
    ///
    /// This causes batch deletions of objects under the prefix to fail with AccessDenied,
    /// while objects outside the prefix can still be deleted normally.
    pub async fn deny_delete_on_prefix(&self, bucket: &str, prefix: &str) {
        // We need the caller's account ID for the Principal. Using "*" for the
        // principal with a condition on the prefix is simpler and works for our purpose.
        let policy = format!(
            r#"{{
  "Version": "2012-10-17",
  "Statement": [
    {{
      "Sid": "DenyDeleteOnPrefix",
      "Effect": "Deny",
      "Principal": "*",
      "Action": ["s3:DeleteObject"],
      "Resource": "arn:aws:s3:::{bucket}/{prefix}*"
    }}
  ]
}}"#
        );

        self.client
            .put_bucket_policy()
            .bucket(bucket)
            .policy(policy)
            .send()
            .await
            .unwrap_or_else(|e| panic!("Failed to put deny policy on {bucket}/{prefix}: {e}"));
    }

    /// Remove the bucket policy (reverting any deny rules).
    pub async fn delete_bucket_policy(&self, bucket: &str) {
        self.client
            .delete_bucket_policy()
            .bucket(bucket)
            .send()
            .await
            .unwrap_or_else(|e| panic!("Failed to delete bucket policy on {bucket}: {e}"));
    }

    // -----------------------------------------------------------------------
    // Pipeline helpers
    // -----------------------------------------------------------------------

    /// Build a `Config` from the given CLI-style arguments.
    ///
    /// Automatically prepends the binary name ("s3rm") and appends
    /// `--target-profile s3rm-e2e-test` unless the args already contain
    /// `--target-profile` or `--target-access-key`.
    pub fn build_config(args: Vec<&str>) -> Config {
        let mut full_args: Vec<String> = vec!["s3rm".to_string()];
        full_args.extend(args.iter().map(|s| s.to_string()));

        // Add profile unless custom credentials are specified
        let has_profile = full_args.iter().any(|a| a.starts_with("--target-profile"));
        let has_access_key = full_args
            .iter()
            .any(|a| a.starts_with("--target-access-key"));
        if !has_profile && !has_access_key {
            full_args.push("--target-profile".to_string());
            full_args.push(AWS_PROFILE.to_string());
        }

        build_config_from_args(full_args)
            .unwrap_or_else(|e| panic!("Failed to build config from args: {e}"))
    }

    /// Run the deletion pipeline with the given config and return results.
    ///
    /// Creates a cancellation token, closes the stats sender (no progress
    /// reporter in tests), runs the pipeline, and collects results.
    pub async fn run_pipeline(config: Config) -> PipelineResult {
        let token = create_pipeline_cancellation_token();
        let mut pipeline = DeletionPipeline::new(config, token).await;

        // Close stats sender since we don't have a progress reporter in tests
        pipeline.close_stats_sender();

        // Run the pipeline
        pipeline.run().await;

        // Collect results
        let stats = pipeline.get_deletion_stats();
        let has_error = pipeline.has_error();
        let has_panic = pipeline.has_panic();
        let has_warning = pipeline.has_warning();
        let errors = if has_error {
            pipeline
                .get_errors_and_consume()
                .unwrap_or_default()
                .into_iter()
                .map(|e| format!("{e:?}"))
                .collect()
        } else {
            Vec::new()
        };

        PipelineResult {
            stats,
            has_error,
            has_panic,
            has_warning,
            errors,
        }
    }
}

/// Default timeout for E2E tests (5 minutes).
///
/// Each E2E test creates a bucket, uploads objects, runs the pipeline, and
/// cleans up. 5 minutes is generous but prevents indefinite hangs.
pub const E2E_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);

/// Wraps an async E2E test body with a timeout.
///
/// Usage:
/// ```ignore
/// #[tokio::test]
/// async fn e2e_my_test() {
///     e2e_timeout!(async {
///         // test body here
///     });
/// }
/// ```
#[macro_export]
macro_rules! e2e_timeout {
    ($body:expr) => {
        tokio::time::timeout(common::E2E_TIMEOUT, $body)
            .await
            .expect("E2E test timed out")
    };
}
