pub mod client_builder;

use anyhow::{Context, Result};
use async_channel::Sender;
use async_trait::async_trait;
use aws_sdk_s3::Client;
use aws_sdk_s3::error::SdkError;
use aws_sdk_s3::operation::delete_object::DeleteObjectOutput;
use aws_sdk_s3::operation::delete_objects::DeleteObjectsOutput;
use aws_sdk_s3::operation::get_object_tagging::GetObjectTaggingOutput;
use aws_sdk_s3::operation::head_object::HeadObjectOutput;
use aws_sdk_s3::types::{Delete, ObjectIdentifier, RequestPayer};
use aws_smithy_types::error::metadata::ProvideErrorMetadata;
use leaky_bucket::RateLimiter;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::task::JoinSet;

use crate::config::{ClientConfig, Config};
use crate::storage::{Storage, StorageFactory, StorageTrait};
use crate::types::token::PipelineCancellationToken;
use crate::types::{DeletionStatistics, S3Object, StoragePath};

const EXPRESS_ONEZONE_STORAGE_SUFFIX: &str = "--x-s3";

/// Extracts the S3 error code and message from an AWS SDK error.
///
/// For service errors (S3 API responses), returns the S3 error code
/// (e.g. "AccessDenied", "InternalError") and the human-readable error
/// message from the response. For other error types (network, timeout,
/// construction failure), returns "N/A" as the code and the full error
/// description as the message.
fn extract_sdk_error_details<E: std::fmt::Display + ProvideErrorMetadata>(
    e: &SdkError<E>,
) -> (String, String) {
    if let Some(service_err) = e.as_service_error() {
        (
            service_err.code().unwrap_or("unknown").to_string(),
            service_err.message().unwrap_or("no message").to_string(),
        )
    } else {
        ("N/A".to_string(), e.to_string())
    }
}

/// Factory for creating S3 storage instances.
///
/// Adapted from s3sync's S3StorageFactory.
pub struct S3StorageFactory;

#[async_trait]
impl StorageFactory for S3StorageFactory {
    async fn create(
        config: Config,
        path: StoragePath,
        cancellation_token: PipelineCancellationToken,
        stats_sender: Sender<DeletionStatistics>,
        client_config: Option<ClientConfig>,
        request_payer: Option<RequestPayer>,
        rate_limit_objects_per_sec: Option<Arc<RateLimiter>>,
        has_warning: Arc<AtomicBool>,
    ) -> Storage {
        let StoragePath::S3 { bucket, prefix } = path;

        let client = if let Some(ref client_config) = client_config {
            Some(Arc::new(client_config.create_client().await))
        } else {
            None
        };

        let listing_semaphore_size = config.max_parallel_listings as usize;

        Box::new(S3Storage {
            config,
            bucket,
            prefix,
            cancellation_token,
            client,
            request_payer,
            stats_sender,
            rate_limit_objects_per_sec,
            has_warning,
            listing_worker_semaphore: Arc::new(tokio::sync::Semaphore::new(listing_semaphore_size)),
        })
    }
}

/// S3 storage implementation for the deletion pipeline.
///
/// Adapted from s3sync's S3Storage, providing S3 operations needed for
/// deletion: listing, deleting, head object, and tagging operations.
///
/// Key differences from s3sync's S3Storage:
/// - No upload/download methods (not needed for deletion)
/// - No local storage support
/// - Simplified trait surface focused on deletion operations
/// - Uses `DeletionStatistics` instead of `SyncStatistics`
#[derive(Clone)]
struct S3Storage {
    #[allow(dead_code)] // Used by later tasks (pipeline, lister)
    config: Config,
    bucket: String,
    prefix: String,
    cancellation_token: PipelineCancellationToken,
    client: Option<Arc<Client>>,
    request_payer: Option<RequestPayer>,
    stats_sender: Sender<DeletionStatistics>,
    rate_limit_objects_per_sec: Option<Arc<RateLimiter>>,
    has_warning: Arc<AtomicBool>,
    listing_worker_semaphore: Arc<tokio::sync::Semaphore>,
}

#[async_trait]
impl StorageTrait for S3Storage {
    fn is_express_onezone_storage(&self) -> bool {
        is_express_onezone_bucket(&self.bucket)
    }

    async fn list_objects(&self, sender: &Sender<S3Object>, max_keys: i32) -> Result<()> {
        // Dispatch to parallel listing if configured (adapted from s3sync)
        if self.config.max_parallel_listings > 1 {
            if !self.is_express_onezone_storage() {
                tracing::debug!(
                    "Using parallel listing with {} workers.",
                    self.config.max_parallel_listings
                );
                let permit = self
                    .listing_worker_semaphore
                    .clone()
                    .acquire_owned()
                    .await
                    .unwrap();
                return self
                    .list_objects_with_parallel("", sender, max_keys, 1, permit)
                    .await
                    .context("Failed to parallel object listing.");
            } else if self.config.allow_parallel_listings_in_express_one_zone {
                tracing::debug!(
                    "Using parallel listing with {} workers (Express One Zone).",
                    self.config.max_parallel_listings
                );
                let permit = self
                    .listing_worker_semaphore
                    .clone()
                    .acquire_owned()
                    .await
                    .unwrap();
                return self
                    .list_objects_with_parallel("", sender, max_keys, 1, permit)
                    .await
                    .context("Failed to parallel object listing.");
            }
        }

        // Sequential listing fallback
        tracing::debug!("Disabled parallel listing.");
        let mut continuation_token: Option<String> = None;

        loop {
            if self.cancellation_token.is_cancelled() {
                tracing::info!("Listing cancelled");
                break;
            }

            self.exec_rate_limit_objects_per_sec().await;

            let output = self
                .client
                .as_ref()
                .unwrap()
                .list_objects_v2()
                .set_request_payer(self.request_payer.clone())
                .bucket(&self.bucket)
                .prefix(&self.prefix)
                .set_continuation_token(continuation_token.clone())
                .max_keys(max_keys)
                .send()
                .await
                .map_err(|e| {
                    let (s3_error_code, s3_error_message) = extract_sdk_error_details(&e);
                    tracing::error!(
                        bucket = self.bucket,
                        prefix = self.prefix,
                        s3_error_code = s3_error_code,
                        s3_error_message = s3_error_message,
                        "S3 ListObjectsV2 API call failed for s3://{}/{}: {} ({}).",
                        self.bucket,
                        self.prefix,
                        s3_error_code,
                        s3_error_message,
                    );
                    anyhow::anyhow!(e).context("aws_sdk_s3::client::list_objects_v2() failed.")
                })?;

            for object in output.contents() {
                if self.cancellation_token.is_cancelled() {
                    return Ok(());
                }

                let s3_object = S3Object::NotVersioning(aws_sdk_s3::types::Object::clone(object));
                if let Err(e) = sender
                    .send(s3_object)
                    .await
                    .context("async_channel::Sender::send() failed.")
                {
                    return if !sender.is_closed() { Err(e) } else { Ok(()) };
                }
            }

            if output.is_truncated() == Some(true) {
                continuation_token = output.next_continuation_token().map(String::from);
            } else {
                break;
            }
        }

        Ok(())
    }

    async fn list_object_versions(&self, sender: &Sender<S3Object>, max_keys: i32) -> Result<()> {
        // Dispatch to parallel listing if configured
        if self.config.max_parallel_listings > 1 {
            if !self.is_express_onezone_storage() {
                tracing::debug!(
                    "Using parallel version listing with {} workers.",
                    self.config.max_parallel_listings
                );
                let permit = self
                    .listing_worker_semaphore
                    .clone()
                    .acquire_owned()
                    .await
                    .unwrap();
                return self
                    .list_object_versions_with_parallel("", sender, max_keys, 1, permit)
                    .await
                    .context("Failed to parallel object version listing.");
            } else if self.config.allow_parallel_listings_in_express_one_zone {
                tracing::debug!(
                    "Using parallel version listing with {} workers (Express One Zone).",
                    self.config.max_parallel_listings
                );
                let permit = self
                    .listing_worker_semaphore
                    .clone()
                    .acquire_owned()
                    .await
                    .unwrap();
                return self
                    .list_object_versions_with_parallel("", sender, max_keys, 1, permit)
                    .await
                    .context("Failed to parallel object version listing.");
            }
        }

        // Sequential listing fallback
        tracing::debug!("Disabled parallel version listing.");
        let mut key_marker: Option<String> = None;
        let mut version_id_marker: Option<String> = None;

        loop {
            if self.cancellation_token.is_cancelled() {
                tracing::info!("Version listing cancelled");
                break;
            }

            self.exec_rate_limit_objects_per_sec().await;

            let output = self
                .client
                .as_ref()
                .unwrap()
                .list_object_versions()
                .set_request_payer(self.request_payer.clone())
                .bucket(&self.bucket)
                .prefix(&self.prefix)
                .set_key_marker(key_marker.clone())
                .set_version_id_marker(version_id_marker.clone())
                .max_keys(max_keys)
                .send()
                .await
                .map_err(|e| {
                    let (s3_error_code, s3_error_message) = extract_sdk_error_details(&e);
                    tracing::error!(
                        bucket = self.bucket,
                        prefix = self.prefix,
                        s3_error_code = s3_error_code,
                        s3_error_message = s3_error_message,
                        "S3 ListObjectVersions API call failed for s3://{}/{}: {} ({}).",
                        self.bucket,
                        self.prefix,
                        s3_error_code,
                        s3_error_message,
                    );
                    anyhow::anyhow!(e).context("aws_sdk_s3::client::list_object_versions() failed.")
                })?;

            // Send object versions
            for version in output.versions() {
                if self.cancellation_token.is_cancelled() {
                    return Ok(());
                }

                let s3_object =
                    S3Object::Versioning(aws_sdk_s3::types::ObjectVersion::clone(version));
                if let Err(e) = sender
                    .send(s3_object)
                    .await
                    .context("async_channel::Sender::send() failed.")
                {
                    return if !sender.is_closed() { Err(e) } else { Ok(()) };
                }
            }

            // Send delete markers
            for marker in output.delete_markers() {
                if self.cancellation_token.is_cancelled() {
                    return Ok(());
                }

                let s3_object =
                    S3Object::DeleteMarker(aws_sdk_s3::types::DeleteMarkerEntry::clone(marker));
                if let Err(e) = sender
                    .send(s3_object)
                    .await
                    .context("async_channel::Sender::send() failed.")
                {
                    return if !sender.is_closed() { Err(e) } else { Ok(()) };
                }
            }

            if output.is_truncated() == Some(true) {
                key_marker = output.next_key_marker().map(String::from);
                version_id_marker = output.next_version_id_marker().map(String::from);
            } else {
                break;
            }
        }

        Ok(())
    }

    async fn head_object(
        &self,
        relative_key: &str,
        version_id: Option<String>,
    ) -> Result<HeadObjectOutput> {
        self.exec_rate_limit_objects_per_sec().await;

        let full_key = prepend_prefix(&self.prefix, relative_key);
        self.client
            .as_ref()
            .unwrap()
            .head_object()
            .set_request_payer(self.request_payer.clone())
            .bucket(&self.bucket)
            .key(&full_key)
            .set_version_id(version_id.clone())
            .send()
            .await
            .map_err(|e| {
                let (s3_error_code, s3_error_message) = extract_sdk_error_details(&e);
                tracing::error!(
                    bucket = self.bucket,
                    key = %full_key,
                    version_id = version_id,
                    s3_error_code = s3_error_code,
                    s3_error_message = s3_error_message,
                    "S3 HeadObject API call failed for s3://{}/{}: {} ({}).",
                    self.bucket, full_key, s3_error_code, s3_error_message,
                );
                anyhow::anyhow!(e).context("aws_sdk_s3::client::head_object() failed.")
            })
    }

    async fn get_object_tagging(
        &self,
        relative_key: &str,
        version_id: Option<String>,
    ) -> Result<GetObjectTaggingOutput> {
        self.exec_rate_limit_objects_per_sec().await;

        let full_key = prepend_prefix(&self.prefix, relative_key);
        self.client
            .as_ref()
            .unwrap()
            .get_object_tagging()
            .set_request_payer(self.request_payer.clone())
            .bucket(&self.bucket)
            .key(&full_key)
            .set_version_id(version_id.clone())
            .send()
            .await
            .map_err(|e| {
                let (s3_error_code, s3_error_message) = extract_sdk_error_details(&e);
                tracing::error!(
                    bucket = self.bucket,
                    key = %full_key,
                    version_id = version_id,
                    s3_error_code = s3_error_code,
                    s3_error_message = s3_error_message,
                    "S3 GetObjectTagging API call failed for s3://{}/{}: {} ({}).",
                    self.bucket, full_key, s3_error_code, s3_error_message,
                );
                anyhow::anyhow!(e).context("aws_sdk_s3::client::get_object_tagging() failed.")
            })
    }

    async fn delete_object(
        &self,
        relative_key: &str,
        version_id: Option<String>,
        if_match: Option<String>,
    ) -> Result<DeleteObjectOutput> {
        self.exec_rate_limit_objects_per_sec().await;

        let full_key = prepend_prefix(&self.prefix, relative_key);
        self.client
            .as_ref()
            .unwrap()
            .delete_object()
            .set_request_payer(self.request_payer.clone())
            .bucket(&self.bucket)
            .key(&full_key)
            .set_version_id(version_id.clone())
            .set_if_match(if_match)
            .send()
            .await
            .map_err(|e| {
                let (s3_error_code, s3_error_message) = extract_sdk_error_details(&e);
                tracing::warn!(
                    bucket = self.bucket,
                    key = %full_key,
                    version_id = version_id,
                    s3_error_code = s3_error_code,
                    s3_error_message = s3_error_message,
                    "S3 DeleteObject API call failed for s3://{}/{}: {} ({}).",
                    self.bucket, full_key, s3_error_code, s3_error_message,
                );
                anyhow::anyhow!(e).context("aws_sdk_s3::client::delete_object() failed.")
            })
    }

    async fn delete_objects(&self, objects: Vec<ObjectIdentifier>) -> Result<DeleteObjectsOutput> {
        self.exec_rate_limit_objects_per_sec_n(objects.len()).await;

        let object_count = objects.len();

        let delete = Delete::builder()
            .set_objects(Some(objects))
            .build()
            .context("Failed to build Delete request")?;

        self.client
            .as_ref()
            .unwrap()
            .delete_objects()
            .set_request_payer(self.request_payer.clone())
            .bucket(&self.bucket)
            .delete(delete)
            .send()
            .await
            .map_err(|e| {
                let (s3_error_code, s3_error_message) = extract_sdk_error_details(&e);
                tracing::error!(
                    bucket = self.bucket,
                    prefix = self.prefix,
                    object_count = object_count,
                    s3_error_code = s3_error_code,
                    s3_error_message = s3_error_message,
                    "S3 DeleteObjects API call failed for {} objects in s3://{}/{}: {} ({}).",
                    object_count,
                    self.bucket,
                    self.prefix,
                    s3_error_code,
                    s3_error_message,
                );
                anyhow::anyhow!(e).context("aws_sdk_s3::client::delete_objects() failed.")
            })
    }

    async fn is_versioning_enabled(&self) -> Result<bool> {
        let response = self
            .client
            .as_ref()
            .unwrap()
            .get_bucket_versioning()
            .bucket(&self.bucket)
            .send()
            .await
            .map_err(|e| {
                let (s3_error_code, s3_error_message) = extract_sdk_error_details(&e);
                tracing::error!(
                    bucket = self.bucket,
                    s3_error_code = s3_error_code,
                    s3_error_message = s3_error_message,
                    "S3 GetBucketVersioning API call failed for bucket '{}': {} ({}).",
                    self.bucket,
                    s3_error_code,
                    s3_error_message,
                );
                anyhow::anyhow!(e).context("aws_sdk_s3::client::get_bucket_versioning() failed.")
            })?;

        Ok(response.status() == Some(&aws_sdk_s3::types::BucketVersioningStatus::Enabled))
    }

    fn get_client(&self) -> Option<Arc<Client>> {
        self.client.clone()
    }

    fn get_stats_sender(&self) -> Sender<DeletionStatistics> {
        self.stats_sender.clone()
    }

    async fn send_stats(&self, stats: DeletionStatistics) {
        let _ = self.stats_sender.send(stats).await;
    }

    fn set_warning(&self) {
        self.has_warning.store(true, Ordering::SeqCst);
    }
}

impl S3Storage {
    /// Apply rate limiting for objects per second if configured.
    ///
    /// Acquires a single token. Appropriate for single-object operations
    /// (delete_object, head_object, get_object_tagging, list_objects).
    ///
    /// Reused from s3sync's rate limiting pattern.
    async fn exec_rate_limit_objects_per_sec(&self) {
        if let Some(ref rate_limiter) = self.rate_limit_objects_per_sec {
            rate_limiter.acquire_one().await;
        }
    }

    /// Apply rate limiting proportional to the number of objects being processed.
    ///
    /// Used for batch operations (delete_objects) where a single API call
    /// processes multiple objects. Acquires `count` tokens to accurately
    /// enforce the configured objects-per-second rate.
    async fn exec_rate_limit_objects_per_sec_n(&self, count: usize) {
        if count == 0 {
            return;
        }
        if let Some(ref rate_limiter) = self.rate_limit_objects_per_sec {
            rate_limiter.acquire(count).await;
        }
    }

    /// Recursive parallel listing using S3 delimiter-based prefix partitioning.
    ///
    /// Adapted from s3sync's `list_objects_with_parallel` algorithm:
    /// 1. Up to `max_parallel_listing_max_depth`, uses `Delimiter="/"` to discover
    ///    sub-prefixes (common prefixes) alongside objects at the current level.
    /// 2. Objects at the current level are sent directly to the channel.
    /// 3. Each sub-prefix spawns a new task via `JoinSet` that recursively calls
    ///    this method with `current_depth + 1`.
    /// 4. Concurrency is bounded by `listing_worker_semaphore` (a tokio Semaphore
    ///    initialized to `config.max_parallel_listings`).
    /// 5. Beyond `max_parallel_listing_max_depth`, no delimiter is set, so listing
    ///    enumerates all objects under the prefix sequentially.
    fn list_objects_with_parallel<'a>(
        &'a self,
        prefix: &'a str,
        sender: &'a Sender<S3Object>,
        max_keys: i32,
        current_depth: usize,
        permit: tokio::sync::OwnedSemaphorePermit,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            let is_root_prefix = prefix.is_empty();
            let prefix = if is_root_prefix {
                self.prefix.clone()
            } else {
                prefix.to_string()
            };

            // Use delimiter "/" to discover sub-prefixes up to max depth
            let delimiter = if current_depth <= self.config.max_parallel_listing_max_depth as usize
            {
                Some("/".to_string())
            } else {
                None
            };

            let mut current_permit = Some(permit);
            let mut continuation_token: Option<String> = None;

            loop {
                if self.cancellation_token.is_cancelled() {
                    break;
                }

                self.exec_rate_limit_objects_per_sec().await;

                let output = self
                    .client
                    .as_ref()
                    .unwrap()
                    .list_objects_v2()
                    .set_request_payer(self.request_payer.clone())
                    .bucket(&self.bucket)
                    .prefix(&prefix)
                    .set_delimiter(delimiter.clone())
                    .set_continuation_token(continuation_token.clone())
                    .max_keys(max_keys)
                    .send()
                    .await
                    .map_err(|e| {
                        let (s3_error_code, s3_error_message) = extract_sdk_error_details(&e);
                        tracing::error!(
                            bucket = self.bucket,
                            prefix = prefix,
                            s3_error_code = s3_error_code,
                            s3_error_message = s3_error_message,
                            "S3 ListObjectsV2 API call failed for s3://{}/{}: {} ({}).",
                            self.bucket,
                            prefix,
                            s3_error_code,
                            s3_error_message,
                        );
                        anyhow::anyhow!(e).context("Failed to list objects in parallel listing")
                    })?;

                // Send objects found at this level
                for object in output.contents() {
                    if self.cancellation_token.is_cancelled() {
                        return Ok(());
                    }

                    let s3_object =
                        S3Object::NotVersioning(aws_sdk_s3::types::Object::clone(object));
                    if let Err(e) = sender
                        .send(s3_object)
                        .await
                        .context("async_channel::Sender::send() failed.")
                    {
                        return if !sender.is_closed() { Err(e) } else { Ok(()) };
                    }
                }

                // For each common prefix (sub-directory), spawn a parallel task
                let common_prefixes = output.common_prefixes();
                if !common_prefixes.is_empty() {
                    let mut join_set = JoinSet::new();

                    for common_prefix in common_prefixes {
                        if self.cancellation_token.is_cancelled() {
                            break;
                        }

                        if let Some(sub_prefix) = common_prefix.prefix() {
                            let storage = self.clone();
                            let sub_prefix = sub_prefix.to_string();
                            let sender = sender.clone();

                            // Release current permit before acquiring new ones for sub-tasks
                            if let Some(p) = current_permit.take() {
                                drop(p);
                            }

                            // Acquire a new permit (bounded by max_parallel_listings)
                            let new_permit = self
                                .listing_worker_semaphore
                                .clone()
                                .acquire_owned()
                                .await
                                .unwrap();

                            join_set.spawn(async move {
                                storage
                                    .list_objects_with_parallel(
                                        &sub_prefix,
                                        &sender,
                                        max_keys,
                                        current_depth + 1,
                                        new_permit,
                                    )
                                    .await
                                    .context("Failed to list objects in sub-prefix")
                            });
                        }
                    }

                    // Wait for all sub-prefix tasks to complete
                    while let Some(join_result) = join_set.join_next().await {
                        match join_result {
                            Err(join_error) => {
                                self.cancellation_token.cancel();
                                return Err(anyhow::anyhow!(join_error));
                            }
                            Ok(Err(task_error)) => {
                                self.cancellation_token.cancel();
                                return Err(task_error);
                            }
                            Ok(Ok(())) => {}
                        }
                    }
                }

                if output.is_truncated() != Some(true) {
                    break;
                }

                continuation_token = output.next_continuation_token().map(String::from);
            }

            if let Some(permit) = current_permit {
                drop(permit);
            }

            Ok(())
        })
    }

    fn list_object_versions_with_parallel<'a>(
        &'a self,
        prefix: &'a str,
        sender: &'a Sender<S3Object>,
        max_keys: i32,
        current_depth: usize,
        permit: tokio::sync::OwnedSemaphorePermit,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            let is_root_prefix = prefix.is_empty();
            let prefix = if is_root_prefix {
                self.prefix.clone()
            } else {
                prefix.to_string()
            };

            // Use delimiter "/" to discover sub-prefixes up to max depth
            let delimiter = if current_depth <= self.config.max_parallel_listing_max_depth as usize
            {
                Some("/".to_string())
            } else {
                None
            };

            let mut current_permit = Some(permit);
            let mut key_marker: Option<String> = None;
            let mut version_id_marker: Option<String> = None;

            loop {
                if self.cancellation_token.is_cancelled() {
                    break;
                }

                self.exec_rate_limit_objects_per_sec().await;

                let output = self
                    .client
                    .as_ref()
                    .unwrap()
                    .list_object_versions()
                    .set_request_payer(self.request_payer.clone())
                    .bucket(&self.bucket)
                    .prefix(&prefix)
                    .set_delimiter(delimiter.clone())
                    .set_key_marker(key_marker.clone())
                    .set_version_id_marker(version_id_marker.clone())
                    .max_keys(max_keys)
                    .send()
                    .await
                    .map_err(|e| {
                        let (s3_error_code, s3_error_message) = extract_sdk_error_details(&e);
                        tracing::error!(
                            bucket = self.bucket,
                            prefix = prefix,
                            s3_error_code = s3_error_code,
                            s3_error_message = s3_error_message,
                            "S3 ListObjectVersions API call failed for s3://{}/{}: {} ({}).",
                            self.bucket,
                            prefix,
                            s3_error_code,
                            s3_error_message,
                        );
                        anyhow::anyhow!(e)
                            .context("Failed to list object versions in parallel listing")
                    })?;

                // Send object versions found at this level
                for version in output.versions() {
                    if self.cancellation_token.is_cancelled() {
                        return Ok(());
                    }

                    let s3_object =
                        S3Object::Versioning(aws_sdk_s3::types::ObjectVersion::clone(version));
                    if let Err(e) = sender
                        .send(s3_object)
                        .await
                        .context("async_channel::Sender::send() failed.")
                    {
                        return if !sender.is_closed() { Err(e) } else { Ok(()) };
                    }
                }

                // Send delete markers found at this level
                for marker in output.delete_markers() {
                    if self.cancellation_token.is_cancelled() {
                        return Ok(());
                    }

                    let s3_object =
                        S3Object::DeleteMarker(aws_sdk_s3::types::DeleteMarkerEntry::clone(marker));
                    if let Err(e) = sender
                        .send(s3_object)
                        .await
                        .context("async_channel::Sender::send() failed.")
                    {
                        return if !sender.is_closed() { Err(e) } else { Ok(()) };
                    }
                }

                // For each common prefix (sub-directory), spawn a parallel task
                let common_prefixes = output.common_prefixes();
                if !common_prefixes.is_empty() {
                    let mut join_set = JoinSet::new();

                    for common_prefix in common_prefixes {
                        if self.cancellation_token.is_cancelled() {
                            break;
                        }

                        if let Some(sub_prefix) = common_prefix.prefix() {
                            let storage = self.clone();
                            let sub_prefix = sub_prefix.to_string();
                            let sender = sender.clone();

                            // Release current permit before acquiring new ones for sub-tasks
                            if let Some(p) = current_permit.take() {
                                drop(p);
                            }

                            // Acquire a new permit (bounded by max_parallel_listings)
                            let new_permit = self
                                .listing_worker_semaphore
                                .clone()
                                .acquire_owned()
                                .await
                                .unwrap();

                            join_set.spawn(async move {
                                storage
                                    .list_object_versions_with_parallel(
                                        &sub_prefix,
                                        &sender,
                                        max_keys,
                                        current_depth + 1,
                                        new_permit,
                                    )
                                    .await
                                    .context("Failed to list object versions in sub-prefix")
                            });
                        }
                    }

                    // Wait for all sub-prefix tasks to complete
                    while let Some(join_result) = join_set.join_next().await {
                        match join_result {
                            Err(join_error) => {
                                self.cancellation_token.cancel();
                                return Err(anyhow::anyhow!(join_error));
                            }
                            Ok(Err(task_error)) => {
                                self.cancellation_token.cancel();
                                return Err(task_error);
                            }
                            Ok(Ok(())) => {}
                        }
                    }
                }

                if output.is_truncated() != Some(true) {
                    break;
                }

                key_marker = output.next_key_marker().map(String::from);
                version_id_marker = output.next_version_id_marker().map(String::from);
            }

            if let Some(permit) = current_permit {
                drop(permit);
            }

            Ok(())
        })
    }
}

/// Prepend the storage prefix to a relative key to form the full S3 key.
///
/// If the prefix is empty, returns the relative key as-is.
/// Reused from s3sync's key generation pattern.
fn prepend_prefix(prefix: &str, relative_key: &str) -> String {
    if prefix.is_empty() {
        relative_key.to_string()
    } else {
        format!("{prefix}{relative_key}")
    }
}

/// Check if a bucket name indicates Express One Zone storage.
///
/// Express One Zone bucket names end with `--x-s3`.
/// Reused from s3sync's detection logic.
fn is_express_onezone_bucket(bucket: &str) -> bool {
    bucket.ends_with(EXPRESS_ONEZONE_STORAGE_SUFFIX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aws_smithy_types::checksum_config::RequestChecksumCalculation;

    fn init_dummy_tracing_subscriber() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter("dummy=trace")
            .try_init();
    }

    fn make_test_config(bucket: &str, prefix: &str) -> Config {
        use crate::callback::event_manager::EventManager;
        use crate::callback::filter_manager::FilterManager;

        Config {
            target: StoragePath::S3 {
                bucket: bucket.to_string(),
                prefix: prefix.to_string(),
            },
            show_no_progress: false,
            target_client_config: None,
            force_retry_config: crate::config::ForceRetryConfig {
                force_retry_count: 0,
                force_retry_interval_milliseconds: 0,
            },
            tracing_config: None,
            worker_size: 1,
            warn_as_error: false,
            dry_run: false,
            rate_limit_objects: None,
            max_parallel_listings: 1,
            object_listing_queue_size: 1000,
            max_parallel_listing_max_depth: 0,
            allow_parallel_listings_in_express_one_zone: false,
            filter_config: crate::config::FilterConfig::default(),
            max_keys: 1000,
            auto_complete_shell: None,
            event_callback_lua_script: None,
            filter_callback_lua_script: None,
            allow_lua_os_library: false,
            allow_lua_unsafe_vm: false,
            lua_vm_memory_limit: 0,
            if_match: false,
            max_delete: None,
            filter_manager: FilterManager::new(),
            event_manager: EventManager::new(),
            batch_size: 1000,
            delete_all_versions: false,
            force: false,
            test_user_defined_callback: false,
        }
    }

    #[test]
    fn prepend_prefix_with_prefix() {
        init_dummy_tracing_subscriber();

        assert_eq!(prepend_prefix("logs/", "file.txt"), "logs/file.txt");
        assert_eq!(prepend_prefix("a/b/c/", "key.json"), "a/b/c/key.json");
    }

    #[test]
    fn prepend_prefix_empty_prefix() {
        init_dummy_tracing_subscriber();

        assert_eq!(prepend_prefix("", "file.txt"), "file.txt");
        assert_eq!(prepend_prefix("", "a/b/c"), "a/b/c");
    }

    #[test]
    fn is_express_onezone_bucket_detection() {
        init_dummy_tracing_subscriber();

        assert!(is_express_onezone_bucket("my-bucket--usw2-az1--x-s3"));
        assert!(is_express_onezone_bucket("test--x-s3"));
        assert!(!is_express_onezone_bucket("my-bucket"));
        assert!(!is_express_onezone_bucket("my-bucket--x-s3-extra"));
        assert!(!is_express_onezone_bucket(""));
    }

    fn make_test_client_config() -> ClientConfig {
        ClientConfig {
            client_config_location: crate::types::ClientConfigLocation {
                aws_config_file: None,
                aws_shared_credentials_file: None,
            },
            credential: crate::types::S3Credentials::Credentials {
                access_keys: crate::types::AccessKeys {
                    access_key: "test".to_string(),
                    secret_access_key: "test".to_string(),
                    session_token: None,
                },
            },
            region: Some("us-east-1".to_string()),
            endpoint_url: Some("https://localhost:9000".to_string()),
            force_path_style: true,
            retry_config: crate::config::RetryConfig {
                aws_max_attempts: 3,
                initial_backoff_milliseconds: 100,
            },
            cli_timeout_config: crate::config::CLITimeoutConfig {
                operation_timeout_milliseconds: None,
                operation_attempt_timeout_milliseconds: None,
                connect_timeout_milliseconds: None,
                read_timeout_milliseconds: None,
            },
            disable_stalled_stream_protection: false,
            request_checksum_calculation: RequestChecksumCalculation::WhenRequired,
            accelerate: false,
            request_payer: None,
        }
    }

    async fn create_test_storage(
        config: &Config,
        client_config: Option<ClientConfig>,
    ) -> (
        Storage,
        Arc<AtomicBool>,
        async_channel::Receiver<DeletionStatistics>,
    ) {
        let (stats_sender, stats_receiver) = async_channel::unbounded();
        let has_warning = Arc::new(AtomicBool::new(false));
        let has_warning_clone = has_warning.clone();
        let cancellation_token = crate::types::token::create_pipeline_cancellation_token();

        let storage = S3StorageFactory::create(
            config.clone(),
            config.target.clone(),
            cancellation_token,
            stats_sender,
            client_config,
            None,
            None,
            has_warning_clone,
        )
        .await;

        (storage, has_warning, stats_receiver)
    }

    #[tokio::test]
    async fn s3_storage_factory_creates_with_client() {
        init_dummy_tracing_subscriber();

        let mut config = make_test_config("test-bucket", "prefix/");
        config.target_client_config = Some(make_test_client_config());
        config.max_parallel_listings = 2;
        config.worker_size = 4;

        let (storage, _, _) = create_test_storage(&config, Some(make_test_client_config())).await;

        assert!(storage.get_client().is_some());
        assert!(!storage.is_express_onezone_storage());
    }

    #[tokio::test]
    async fn s3_storage_factory_creates_without_client() {
        init_dummy_tracing_subscriber();

        let config = make_test_config("test-bucket", "");
        let (storage, _, _) = create_test_storage(&config, None).await;

        assert!(storage.get_client().is_none());
    }

    #[tokio::test]
    async fn s3_storage_express_one_zone_detection() {
        init_dummy_tracing_subscriber();

        let config = make_test_config("my-bucket--usw2-az1--x-s3", "");
        let (storage, _, _) = create_test_storage(&config, None).await;

        assert!(storage.is_express_onezone_storage());
    }

    #[tokio::test]
    async fn s3_storage_warning_flag() {
        init_dummy_tracing_subscriber();

        let config = make_test_config("test-bucket", "");
        let (storage, has_warning, _) = create_test_storage(&config, None).await;

        assert!(!has_warning.load(Ordering::SeqCst));
        storage.set_warning();
        assert!(has_warning.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn s3_storage_stats_sender() {
        init_dummy_tracing_subscriber();

        let config = make_test_config("test-bucket", "");
        let (storage, _, stats_receiver) = create_test_storage(&config, None).await;

        storage
            .send_stats(DeletionStatistics::DeleteBytes(512))
            .await;

        let received = stats_receiver.recv().await.unwrap();
        assert!(matches!(received, DeletionStatistics::DeleteBytes(512)));
    }
}
