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

/// Listing mode for S3 object enumeration.
///
/// Used to unify the parallel and sequential listing logic for both
/// ListObjectsV2 and ListObjectVersions APIs.
#[derive(Clone, Copy)]
enum ListingMode {
    /// List current objects (non-versioned) via ListObjectsV2 API.
    Objects,
    /// List all object versions and delete markers via ListObjectVersions API.
    Versions,
}

impl ListingMode {
    /// Returns a label for log messages (e.g. "object" or "object version").
    fn label(&self) -> &'static str {
        match self {
            Self::Objects => "object",
            Self::Versions => "object version",
        }
    }
}

/// A normalized page of listing results from either S3 listing API.
///
/// Abstracts over the differences between ListObjectsV2Output and
/// ListObjectVersionsOutput so that paging/recursion logic can be shared.
#[derive(Debug)]
struct ListPage {
    /// S3 objects found at this level (converted to S3Object).
    objects: Vec<S3Object>,
    /// Sub-prefixes (common prefixes / "directories") discovered via delimiter.
    sub_prefixes: Vec<String>,
    /// Whether there are more results to fetch.
    is_truncated: bool,
    /// Next continuation token (ListObjectsV2 only).
    continuation_token: Option<String>,
    /// Next key marker (ListObjectVersions only).
    key_marker: Option<String>,
    /// Next version ID marker (ListObjectVersions only).
    version_id_marker: Option<String>,
}

#[async_trait]
impl StorageTrait for S3Storage {
    fn is_express_onezone_storage(&self) -> bool {
        is_express_onezone_bucket(&self.bucket)
    }

    async fn list_objects(&self, sender: &Sender<S3Object>, max_keys: i32) -> Result<()> {
        self.list_dispatch(ListingMode::Objects, sender, max_keys)
            .await
    }

    async fn list_object_versions(&self, sender: &Sender<S3Object>, max_keys: i32) -> Result<()> {
        self.list_dispatch(ListingMode::Versions, sender, max_keys)
            .await
    }

    async fn head_object(&self, key: &str, version_id: Option<String>) -> Result<HeadObjectOutput> {
        self.exec_rate_limit_objects_per_sec().await;

        self.client
            .as_ref()
            .expect("S3 client not initialized")
            .head_object()
            .set_request_payer(self.request_payer.clone())
            .bucket(&self.bucket)
            .key(key)
            .set_version_id(version_id.clone())
            .send()
            .await
            .map_err(|e| {
                let (s3_error_code, s3_error_message) = extract_sdk_error_details(&e);
                tracing::error!(
                    bucket = self.bucket,
                    key = %key,
                    version_id = version_id,
                    s3_error_code = s3_error_code,
                    s3_error_message = s3_error_message,
                    "S3 HeadObject API call failed for s3://{}/{}: {} ({}).",
                    self.bucket, key, s3_error_code, s3_error_message,
                );
                anyhow::anyhow!(e).context("aws_sdk_s3::client::head_object() failed.")
            })
    }

    async fn get_object_tagging(
        &self,
        key: &str,
        version_id: Option<String>,
    ) -> Result<GetObjectTaggingOutput> {
        self.exec_rate_limit_objects_per_sec().await;

        self.client
            .as_ref()
            .expect("S3 client not initialized")
            .get_object_tagging()
            .set_request_payer(self.request_payer.clone())
            .bucket(&self.bucket)
            .key(key)
            .set_version_id(version_id.clone())
            .send()
            .await
            .map_err(|e| {
                let (s3_error_code, s3_error_message) = extract_sdk_error_details(&e);
                tracing::error!(
                    bucket = self.bucket,
                    key = %key,
                    version_id = version_id,
                    s3_error_code = s3_error_code,
                    s3_error_message = s3_error_message,
                    "S3 GetObjectTagging API call failed for s3://{}/{}: {} ({}).",
                    self.bucket, key, s3_error_code, s3_error_message,
                );
                anyhow::anyhow!(e).context("aws_sdk_s3::client::get_object_tagging() failed.")
            })
    }

    async fn delete_object(
        &self,
        key: &str,
        version_id: Option<String>,
        if_match: Option<String>,
    ) -> Result<DeleteObjectOutput> {
        self.exec_rate_limit_objects_per_sec().await;

        self.client
            .as_ref()
            .expect("S3 client not initialized")
            .delete_object()
            .set_request_payer(self.request_payer.clone())
            .bucket(&self.bucket)
            .key(key)
            .set_version_id(version_id.clone())
            .set_if_match(if_match)
            .send()
            .await
            .map_err(|e| {
                let (s3_error_code, s3_error_message) = extract_sdk_error_details(&e);
                tracing::warn!(
                    bucket = self.bucket,
                    key = %key,
                    version_id = version_id,
                    s3_error_code = s3_error_code,
                    s3_error_message = s3_error_message,
                    "S3 DeleteObject API call failed for s3://{}/{}: {} ({}).",
                    self.bucket, key, s3_error_code, s3_error_message,
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
            .expect("S3 client not initialized")
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
        // Express One Zone directory buckets don't support versioning and the
        // GetBucketVersioning API returns MethodNotAllowed for them.
        if self.is_express_onezone_storage() {
            return Ok(false);
        }

        let response = self
            .client
            .as_ref()
            .expect("S3 client not initialized")
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

    /// Dispatch listing to parallel or sequential based on configuration.
    ///
    /// Adapted from s3sync's parallel/sequential dispatch pattern.
    async fn list_dispatch(
        &self,
        mode: ListingMode,
        sender: &Sender<S3Object>,
        max_keys: i32,
    ) -> Result<()> {
        if self.config.max_parallel_listings > 1 {
            let use_parallel = !self.is_express_onezone_storage()
                || self.config.allow_parallel_listings_in_express_one_zone;

            if use_parallel {
                let express_suffix = if self.is_express_onezone_storage() {
                    " (Express One Zone)"
                } else {
                    ""
                };
                tracing::debug!(
                    "Using parallel {} listing with {} workers{}.",
                    mode.label(),
                    self.config.max_parallel_listings,
                    express_suffix,
                );
                let permit = self
                    .listing_worker_semaphore
                    .clone()
                    .acquire_owned()
                    .await
                    .expect("listing semaphore closed unexpectedly");
                return self
                    .list_with_parallel(mode, "", sender, max_keys, 1, permit)
                    .await
                    .context(format!("Failed to parallel {} listing.", mode.label()));
            }
        }

        // Sequential listing fallback
        tracing::debug!("Disabled parallel {} listing.", mode.label());
        self.list_sequential(mode, sender, max_keys).await
    }

    /// Sequential listing fallback for both objects and object versions.
    async fn list_sequential(
        &self,
        mode: ListingMode,
        sender: &Sender<S3Object>,
        max_keys: i32,
    ) -> Result<()> {
        let mut continuation_token: Option<String> = None;
        let mut key_marker: Option<String> = None;
        let mut version_id_marker: Option<String> = None;

        loop {
            if self.cancellation_token.is_cancelled() {
                tracing::info!("{} listing cancelled", mode.label());
                break;
            }

            self.exec_rate_limit_objects_per_sec().await;

            let page = self
                .fetch_list_page(
                    mode,
                    &self.prefix,
                    None,
                    max_keys,
                    continuation_token.clone(),
                    key_marker.clone(),
                    version_id_marker.clone(),
                )
                .await?;

            if self.send_listed_objects(page.objects, sender).await? {
                return Ok(());
            }

            if page.is_truncated {
                continuation_token = page.continuation_token;
                key_marker = page.key_marker;
                version_id_marker = page.version_id_marker;
            } else {
                break;
            }
        }

        Ok(())
    }

    /// Fetch a single page of listing results from the appropriate S3 API.
    ///
    /// Normalizes the response into a `ListPage` regardless of which listing
    /// API was called, so callers don't need to handle the two output types.
    #[allow(clippy::too_many_arguments)]
    async fn fetch_list_page(
        &self,
        mode: ListingMode,
        prefix: &str,
        delimiter: Option<String>,
        max_keys: i32,
        continuation_token: Option<String>,
        key_marker: Option<String>,
        version_id_marker: Option<String>,
    ) -> Result<ListPage> {
        let client = self.client.as_ref().expect("S3 client not initialized");

        match mode {
            ListingMode::Objects => {
                let output = client
                    .list_objects_v2()
                    .set_request_payer(self.request_payer.clone())
                    .bucket(&self.bucket)
                    .prefix(prefix)
                    .set_delimiter(delimiter)
                    .set_continuation_token(continuation_token)
                    .max_keys(max_keys)
                    .send()
                    .await
                    .map_err(|e| {
                        let (code, msg) = extract_sdk_error_details(&e);
                        tracing::error!(
                            bucket = self.bucket,
                            prefix = prefix,
                            s3_error_code = code,
                            s3_error_message = msg,
                            "S3 ListObjectsV2 API call failed for s3://{}/{}: {} ({}).",
                            self.bucket,
                            prefix,
                            code,
                            msg,
                        );
                        anyhow::anyhow!(e).context(format!(
                            "aws_sdk_s3::client::list_objects_v2() failed. s3://{}/{}",
                            self.bucket, prefix
                        ))
                    })?;

                let objects = output
                    .contents()
                    .iter()
                    .map(|o| S3Object::NotVersioning(aws_sdk_s3::types::Object::clone(o)))
                    .collect();
                let sub_prefixes = output
                    .common_prefixes()
                    .iter()
                    .filter_map(|cp| cp.prefix().map(String::from))
                    .collect();

                Ok(ListPage {
                    objects,
                    sub_prefixes,
                    is_truncated: output.is_truncated() == Some(true),
                    continuation_token: output.next_continuation_token().map(String::from),
                    key_marker: None,
                    version_id_marker: None,
                })
            }
            ListingMode::Versions => {
                let output = client
                    .list_object_versions()
                    .set_request_payer(self.request_payer.clone())
                    .bucket(&self.bucket)
                    .prefix(prefix)
                    .set_delimiter(delimiter)
                    .set_key_marker(key_marker)
                    .set_version_id_marker(version_id_marker)
                    .max_keys(max_keys)
                    .send()
                    .await
                    .map_err(|e| {
                        let (code, msg) = extract_sdk_error_details(&e);
                        tracing::error!(
                            bucket = self.bucket,
                            prefix = prefix,
                            s3_error_code = code,
                            s3_error_message = msg,
                            "S3 ListObjectVersions API call failed for s3://{}/{}: {} ({}).",
                            self.bucket,
                            prefix,
                            code,
                            msg,
                        );
                        anyhow::anyhow!(e).context(format!(
                            "aws_sdk_s3::client::list_object_versions() failed. s3://{}/{}",
                            self.bucket, prefix
                        ))
                    })?;

                let mut objects: Vec<S3Object> = output
                    .versions()
                    .iter()
                    .map(|v| S3Object::Versioning(aws_sdk_s3::types::ObjectVersion::clone(v)))
                    .collect();
                objects.extend(output.delete_markers().iter().map(|m| {
                    S3Object::DeleteMarker(aws_sdk_s3::types::DeleteMarkerEntry::clone(m))
                }));
                let sub_prefixes = output
                    .common_prefixes()
                    .iter()
                    .filter_map(|cp| cp.prefix().map(String::from))
                    .collect();

                Ok(ListPage {
                    objects,
                    sub_prefixes,
                    is_truncated: output.is_truncated() == Some(true),
                    continuation_token: None,
                    key_marker: output.next_key_marker().map(String::from),
                    version_id_marker: output.next_version_id_marker().map(String::from),
                })
            }
        }
    }

    /// Send listed objects to the channel, checking for cancellation.
    ///
    /// Returns `Ok(true)` if listing should stop (cancellation or channel closed),
    /// `Ok(false)` if all objects were sent successfully.
    async fn send_listed_objects(
        &self,
        objects: Vec<S3Object>,
        sender: &Sender<S3Object>,
    ) -> Result<bool> {
        for s3_object in objects {
            if self.cancellation_token.is_cancelled() {
                return Ok(true);
            }
            if let Err(e) = sender
                .send(s3_object)
                .await
                .context("async_channel::Sender::send() failed.")
            {
                return if !sender.is_closed() {
                    Err(e)
                } else {
                    Ok(true)
                };
            }
        }
        Ok(false)
    }

    /// Recursive parallel listing using S3 delimiter-based prefix partitioning.
    ///
    /// Adapted from s3sync's parallel listing algorithm:
    /// 1. Up to `max_parallel_listing_max_depth`, uses `Delimiter="/"` to discover
    ///    sub-prefixes (common prefixes) alongside objects at the current level.
    /// 2. Objects at the current level are sent directly to the channel.
    /// 3. Each sub-prefix spawns a new task via `JoinSet` that recursively calls
    ///    this method with `current_depth + 1`.
    /// 4. Concurrency is bounded by `listing_worker_semaphore` (a tokio Semaphore
    ///    initialized to `config.max_parallel_listings`).
    /// 5. Beyond `max_parallel_listing_max_depth`, no delimiter is set, so listing
    ///    enumerates all objects under the prefix sequentially.
    fn list_with_parallel<'a>(
        &'a self,
        mode: ListingMode,
        prefix: &'a str,
        sender: &'a Sender<S3Object>,
        max_keys: i32,
        current_depth: usize,
        permit: tokio::sync::OwnedSemaphorePermit,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            let prefix = if prefix.is_empty() {
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
            let mut key_marker: Option<String> = None;
            let mut version_id_marker: Option<String> = None;

            loop {
                if self.cancellation_token.is_cancelled() {
                    break;
                }

                self.exec_rate_limit_objects_per_sec().await;

                let page = self
                    .fetch_list_page(
                        mode,
                        &prefix,
                        delimiter.clone(),
                        max_keys,
                        continuation_token.clone(),
                        key_marker.clone(),
                        version_id_marker.clone(),
                    )
                    .await?;

                // Send objects found at this level
                if self.send_listed_objects(page.objects, sender).await? {
                    return Ok(());
                }

                // For each common prefix (sub-directory), spawn a parallel task
                if !page.sub_prefixes.is_empty() {
                    let mut join_set = JoinSet::new();

                    for sub_prefix in page.sub_prefixes {
                        if self.cancellation_token.is_cancelled() {
                            break;
                        }

                        let storage = self.clone();
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
                            .expect("listing semaphore closed unexpectedly");

                        join_set.spawn(async move {
                            storage
                                .list_with_parallel(
                                    mode,
                                    &sub_prefix,
                                    &sender,
                                    max_keys,
                                    current_depth + 1,
                                    new_permit,
                                )
                                .await
                                .context(format!("Failed to list {}s in sub-prefix", mode.label()))
                        });
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

                if !page.is_truncated {
                    break;
                }

                continuation_token = page.continuation_token;
                key_marker = page.key_marker;
                version_id_marker = page.version_id_marker;
            }

            if let Some(permit) = current_permit {
                drop(permit);
            }

            Ok(())
        })
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
    use crate::test_utils::init_dummy_tracing_subscriber;
    use aws_smithy_types::checksum_config::RequestChecksumCalculation;

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
            lua_callback_timeout_milliseconds: 0,
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

    #[test]
    fn extract_sdk_error_details_service_error() {
        use aws_sdk_s3::operation::head_object::HeadObjectError;
        use aws_smithy_types::body::SdkBody;

        // Build a service error with code and message
        let service_err = HeadObjectError::NotFound(
            aws_sdk_s3::types::error::NotFound::builder()
                .message("Not Found")
                .build(),
        );

        let raw_response = aws_smithy_runtime_api::http::Response::new(
            aws_smithy_runtime_api::http::StatusCode::try_from(404).unwrap(),
            SdkBody::from(""),
        );

        let sdk_error = SdkError::service_error(service_err, raw_response);
        let (code, message) = extract_sdk_error_details(&sdk_error);

        // NotFound should have a code from the metadata
        assert_ne!(code, "N/A", "service errors should extract a code");
        assert!(!message.is_empty(), "service errors should have a message");
    }

    /// Creates a ClientConfig with short timeouts and 1 retry attempt for fast-failing tests.
    fn make_fast_failing_client_config() -> ClientConfig {
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
                aws_max_attempts: 1,
                initial_backoff_milliseconds: 100,
            },
            cli_timeout_config: crate::config::CLITimeoutConfig {
                operation_timeout_milliseconds: None,
                operation_attempt_timeout_milliseconds: Some(2000),
                connect_timeout_milliseconds: Some(1000),
                read_timeout_milliseconds: None,
            },
            disable_stalled_stream_protection: false,
            request_checksum_calculation: RequestChecksumCalculation::WhenRequired,
            accelerate: false,
            request_payer: None,
        }
    }

    #[test]
    fn extract_sdk_error_details_non_service_error() {
        use aws_sdk_s3::operation::head_object::HeadObjectError;
        use aws_smithy_types::body::SdkBody;

        // Build a timeout error (not a service error)
        let sdk_error: SdkError<HeadObjectError, aws_smithy_runtime_api::http::Response<SdkBody>> =
            SdkError::timeout_error("connection timed out");
        let (code, message) = extract_sdk_error_details(&sdk_error);

        assert_eq!(code, "N/A", "non-service errors should return N/A code");
        assert!(
            !message.is_empty(),
            "non-service errors should have a message from Display"
        );
    }

    #[tokio::test]
    async fn list_objects_returns_error_on_api_failure() {
        init_dummy_tracing_subscriber();

        let mut config = make_test_config("test-bucket", "prefix/");
        config.target_client_config = Some(make_fast_failing_client_config());
        let (storage, _, _) =
            create_test_storage(&config, Some(make_fast_failing_client_config())).await;

        let (sender, _receiver) = async_channel::unbounded();
        let result = storage.list_objects(&sender, 1000).await;

        assert!(result.is_err());
        let err_msg = format!("{:#}", result.unwrap_err());
        assert!(
            err_msg.contains("list_objects_v2() failed"),
            "Expected error to contain 'list_objects_v2() failed', got: {err_msg}"
        );
    }

    #[tokio::test]
    async fn list_object_versions_returns_error_on_api_failure() {
        init_dummy_tracing_subscriber();

        let mut config = make_test_config("test-bucket", "prefix/");
        config.target_client_config = Some(make_fast_failing_client_config());
        let (storage, _, _) =
            create_test_storage(&config, Some(make_fast_failing_client_config())).await;

        let (sender, _receiver) = async_channel::unbounded();
        let result = storage.list_object_versions(&sender, 1000).await;

        assert!(result.is_err());
        let err_msg = format!("{:#}", result.unwrap_err());
        assert!(
            err_msg.contains("list_object_versions() failed"),
            "Expected error to contain 'list_object_versions() failed', got: {err_msg}"
        );
    }

    #[tokio::test]
    async fn head_object_returns_error_on_api_failure() {
        init_dummy_tracing_subscriber();

        let mut config = make_test_config("test-bucket", "prefix/");
        config.target_client_config = Some(make_fast_failing_client_config());
        let (storage, _, _) =
            create_test_storage(&config, Some(make_fast_failing_client_config())).await;

        let result = storage.head_object("test-key", None).await;

        assert!(result.is_err());
        let err_msg = format!("{:#}", result.unwrap_err());
        assert!(
            err_msg.contains("head_object() failed"),
            "Expected error to contain 'head_object() failed', got: {err_msg}"
        );
    }

    #[tokio::test]
    async fn get_object_tagging_returns_error_on_api_failure() {
        init_dummy_tracing_subscriber();

        let mut config = make_test_config("test-bucket", "prefix/");
        config.target_client_config = Some(make_fast_failing_client_config());
        let (storage, _, _) =
            create_test_storage(&config, Some(make_fast_failing_client_config())).await;

        let result = storage.get_object_tagging("test-key", None).await;

        assert!(result.is_err());
        let err_msg = format!("{:#}", result.unwrap_err());
        assert!(
            err_msg.contains("get_object_tagging() failed"),
            "Expected error to contain 'get_object_tagging() failed', got: {err_msg}"
        );
    }

    #[tokio::test]
    async fn delete_object_returns_error_on_api_failure() {
        init_dummy_tracing_subscriber();

        let mut config = make_test_config("test-bucket", "prefix/");
        config.target_client_config = Some(make_fast_failing_client_config());
        let (storage, _, _) =
            create_test_storage(&config, Some(make_fast_failing_client_config())).await;

        let result = storage.delete_object("test-key", None, None).await;

        assert!(result.is_err());
        let err_msg = format!("{:#}", result.unwrap_err());
        assert!(
            err_msg.contains("delete_object() failed"),
            "Expected error to contain 'delete_object() failed', got: {err_msg}"
        );
    }

    #[tokio::test]
    async fn delete_objects_returns_error_on_api_failure() {
        init_dummy_tracing_subscriber();

        let mut config = make_test_config("test-bucket", "prefix/");
        config.target_client_config = Some(make_fast_failing_client_config());
        let (storage, _, _) =
            create_test_storage(&config, Some(make_fast_failing_client_config())).await;

        let objects = vec![ObjectIdentifier::builder().key("test-key").build().unwrap()];
        let result = storage.delete_objects(objects).await;

        assert!(result.is_err());
        let err_msg = format!("{:#}", result.unwrap_err());
        assert!(
            err_msg.contains("delete_objects() failed"),
            "Expected error to contain 'delete_objects() failed', got: {err_msg}"
        );
    }

    #[tokio::test]
    async fn is_versioning_enabled_returns_error_on_api_failure() {
        init_dummy_tracing_subscriber();

        let mut config = make_test_config("test-bucket", "prefix/");
        config.target_client_config = Some(make_fast_failing_client_config());
        let (storage, _, _) =
            create_test_storage(&config, Some(make_fast_failing_client_config())).await;

        let result = storage.is_versioning_enabled().await;

        assert!(result.is_err());
        let err_msg = format!("{:#}", result.unwrap_err());
        assert!(
            err_msg.contains("get_bucket_versioning() failed"),
            "Expected error to contain 'get_bucket_versioning() failed', got: {err_msg}"
        );
    }

    // ---------------------------------------------------------------
    // exec_rate_limit_objects_per_sec_n: count == 0 early-return
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn exec_rate_limit_objects_per_sec_n_zero_skips_limiter() {
        let config = make_test_config("test-bucket", "prefix/");
        let (stats_sender, _) = async_channel::unbounded();
        let cancellation_token = crate::types::token::create_pipeline_cancellation_token();

        // Limiter with max=1: if count=0 did NOT early-return and tried
        // to acquire(0), the leaky-bucket crate would panic.
        let limiter = Arc::new(
            leaky_bucket::RateLimiter::builder()
                .max(1)
                .initial(1)
                .refill(1)
                .build(),
        );

        let storage = S3Storage {
            config,
            bucket: "test-bucket".to_string(),
            prefix: "prefix/".to_string(),
            cancellation_token,
            client: None,
            request_payer: None,
            stats_sender,
            rate_limit_objects_per_sec: Some(limiter),
            has_warning: Arc::new(AtomicBool::new(false)),
            listing_worker_semaphore: Arc::new(tokio::sync::Semaphore::new(1)),
        };

        // Must return immediately without touching the rate limiter.
        storage.exec_rate_limit_objects_per_sec_n(0).await;
    }

    // ---------------------------------------------------------------
    // ListingMode tests
    // ---------------------------------------------------------------

    #[test]
    fn listing_mode_label_objects() {
        assert_eq!(ListingMode::Objects.label(), "object");
    }

    #[test]
    fn listing_mode_label_versions() {
        assert_eq!(ListingMode::Versions.label(), "object version");
    }

    // ---------------------------------------------------------------
    // Helper: create S3Storage with access to cancellation token
    // ---------------------------------------------------------------

    fn make_s3_storage_with_token(
        bucket: &str,
        prefix: &str,
    ) -> (S3Storage, crate::types::token::PipelineCancellationToken) {
        let config = make_test_config(bucket, prefix);
        let cancellation_token = crate::types::token::create_pipeline_cancellation_token();
        let ct = cancellation_token.clone();
        let (stats_sender, _) = async_channel::unbounded();
        let storage = S3Storage {
            config,
            bucket: bucket.to_string(),
            prefix: prefix.to_string(),
            cancellation_token,
            client: None,
            request_payer: None,
            stats_sender,
            rate_limit_objects_per_sec: None,
            has_warning: Arc::new(AtomicBool::new(false)),
            listing_worker_semaphore: Arc::new(tokio::sync::Semaphore::new(1)),
        };
        (storage, ct)
    }

    // ---------------------------------------------------------------
    // send_listed_objects tests
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn send_listed_objects_empty_vec() {
        let (storage, _ct) = make_s3_storage_with_token("b", "p/");
        let (sender, _receiver) = async_channel::unbounded();

        let result = storage.send_listed_objects(vec![], &sender).await.unwrap();
        assert!(!result, "empty vec should return false (not stopped)");
    }

    #[tokio::test]
    async fn send_listed_objects_sends_all_not_versioned() {
        let (storage, _ct) = make_s3_storage_with_token("b", "p/");
        let (sender, receiver) = async_channel::unbounded();

        let objects = vec![
            S3Object::NotVersioning(
                aws_sdk_s3::types::Object::builder()
                    .key("file1.txt")
                    .build(),
            ),
            S3Object::NotVersioning(
                aws_sdk_s3::types::Object::builder()
                    .key("file2.txt")
                    .build(),
            ),
            S3Object::NotVersioning(
                aws_sdk_s3::types::Object::builder()
                    .key("file3.txt")
                    .build(),
            ),
        ];

        let stopped = storage.send_listed_objects(objects, &sender).await.unwrap();
        assert!(!stopped);

        // All three should be received
        let r1 = receiver.recv().await.unwrap();
        assert_eq!(r1.key(), "file1.txt");
        let r2 = receiver.recv().await.unwrap();
        assert_eq!(r2.key(), "file2.txt");
        let r3 = receiver.recv().await.unwrap();
        assert_eq!(r3.key(), "file3.txt");
        assert!(receiver.try_recv().is_err(), "no more objects");
    }

    #[tokio::test]
    async fn send_listed_objects_sends_versioned_and_delete_markers() {
        let (storage, _ct) = make_s3_storage_with_token("b", "p/");
        let (sender, receiver) = async_channel::unbounded();

        let objects = vec![
            S3Object::Versioning(
                aws_sdk_s3::types::ObjectVersion::builder()
                    .key("v1.txt")
                    .version_id("vid-1")
                    .build(),
            ),
            S3Object::DeleteMarker(
                aws_sdk_s3::types::DeleteMarkerEntry::builder()
                    .key("dm1.txt")
                    .version_id("vid-dm")
                    .build(),
            ),
        ];

        let stopped = storage.send_listed_objects(objects, &sender).await.unwrap();
        assert!(!stopped);

        let r1 = receiver.recv().await.unwrap();
        assert!(matches!(r1, S3Object::Versioning(_)));
        assert_eq!(r1.key(), "v1.txt");

        let r2 = receiver.recv().await.unwrap();
        assert!(matches!(r2, S3Object::DeleteMarker(_)));
        assert_eq!(r2.key(), "dm1.txt");
    }

    #[tokio::test]
    async fn send_listed_objects_cancelled_before_send() {
        let (storage, ct) = make_s3_storage_with_token("b", "p/");
        let (sender, receiver) = async_channel::unbounded();

        ct.cancel();

        let objects = vec![S3Object::NotVersioning(
            aws_sdk_s3::types::Object::builder()
                .key("should-not-send.txt")
                .build(),
        )];

        let stopped = storage.send_listed_objects(objects, &sender).await.unwrap();
        assert!(stopped, "should stop when cancelled");
        assert!(
            receiver.try_recv().is_err(),
            "nothing should have been sent"
        );
    }

    #[tokio::test]
    async fn send_listed_objects_closed_channel_returns_ok_true() {
        let (storage, _ct) = make_s3_storage_with_token("b", "p/");
        let (sender, receiver) = async_channel::unbounded::<S3Object>();

        // Close the channel
        receiver.close();

        let objects = vec![S3Object::NotVersioning(
            aws_sdk_s3::types::Object::builder()
                .key("ignored.txt")
                .build(),
        )];

        let stopped = storage.send_listed_objects(objects, &sender).await.unwrap();
        assert!(stopped, "should stop when channel is closed");
    }

    #[tokio::test]
    async fn send_listed_objects_bounded_channel_close_mid_send() {
        let (storage, _ct) = make_s3_storage_with_token("b", "p/");
        // Bounded channel with capacity 1
        let (sender, receiver) = async_channel::bounded::<S3Object>(1);

        let objects = vec![
            S3Object::NotVersioning(
                aws_sdk_s3::types::Object::builder()
                    .key("first.txt")
                    .build(),
            ),
            S3Object::NotVersioning(
                aws_sdk_s3::types::Object::builder()
                    .key("second.txt")
                    .build(),
            ),
        ];

        // Spawn a task that receives first object then closes channel
        let recv_handle = tokio::spawn(async move {
            let obj = receiver.recv().await.unwrap();
            assert_eq!(obj.key(), "first.txt");
            receiver.close();
        });

        let stopped = storage.send_listed_objects(objects, &sender).await.unwrap();
        assert!(stopped, "should stop when channel is closed mid-send");
        recv_handle.await.unwrap();
    }

    // ---------------------------------------------------------------
    // list_sequential: cancelled immediately
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn list_sequential_objects_cancelled_immediately() {
        let (storage, ct) = make_s3_storage_with_token("b", "p/");
        let (sender, _receiver) = async_channel::unbounded();

        ct.cancel();

        // Should return Ok(()) without trying to call S3 API (no client needed)
        let result = storage
            .list_sequential(ListingMode::Objects, &sender, 1000)
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn list_sequential_versions_cancelled_immediately() {
        let (storage, ct) = make_s3_storage_with_token("b", "p/");
        let (sender, _receiver) = async_channel::unbounded();

        ct.cancel();

        let result = storage
            .list_sequential(ListingMode::Versions, &sender, 1000)
            .await;
        assert!(result.is_ok());
    }

    // ---------------------------------------------------------------
    // list_dispatch: parallel vs sequential routing
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn list_dispatch_sequential_when_single_worker() {
        // max_parallel_listings=1 → sequential path → cancel immediately → Ok(())
        let (storage, ct) = make_s3_storage_with_token("b", "p/");
        assert_eq!(storage.config.max_parallel_listings, 1);
        let (sender, _receiver) = async_channel::unbounded();

        ct.cancel();

        let result = storage
            .list_dispatch(ListingMode::Objects, &sender, 1000)
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn list_dispatch_express_onezone_blocks_parallel_by_default() {
        // Express One Zone + max_parallel>1 but no allow flag → sequential fallback.
        // We verify by checking the error message: the parallel path wraps with
        // "Failed to parallel object listing." while the sequential path does not.
        let mut config = make_test_config("bucket--az1--x-s3", "prefix/");
        config.max_parallel_listings = 4;
        config.max_parallel_listing_max_depth = 1;
        config.allow_parallel_listings_in_express_one_zone = false;
        config.target_client_config = Some(make_fast_failing_client_config());

        let (storage, _, _) =
            create_test_storage(&config, Some(make_fast_failing_client_config())).await;

        let (sender, _receiver) = async_channel::unbounded();
        let result = storage.list_objects(&sender, 1000).await;

        assert!(result.is_err());
        let err_msg = format!("{:#}", result.unwrap_err());
        // Sequential path error: "list_objects_v2() failed" without parallel wrapper
        assert!(
            err_msg.contains("list_objects_v2() failed"),
            "Expected sequential path error, got: {err_msg}"
        );
        assert!(
            !err_msg.contains("Failed to parallel"),
            "Should NOT take parallel path for express one zone without allow flag, got: {err_msg}"
        );
    }

    #[tokio::test]
    async fn list_dispatch_parallel_objects_api_failure() {
        // max_parallel>1, non-express, with a real fast-failing client → parallel path
        let mut config = make_test_config("test-bucket", "prefix/");
        config.max_parallel_listings = 4;
        config.max_parallel_listing_max_depth = 1;
        config.target_client_config = Some(make_fast_failing_client_config());

        let (storage, _, _) =
            create_test_storage(&config, Some(make_fast_failing_client_config())).await;

        let (sender, _receiver) = async_channel::unbounded();
        let result = storage.list_objects(&sender, 1000).await;

        assert!(result.is_err());
        let err_msg = format!("{:#}", result.unwrap_err());
        // The parallel path wraps with "Failed to parallel object listing."
        assert!(
            err_msg.contains("Failed to parallel object listing"),
            "Expected parallel path error context, got: {err_msg}"
        );
    }

    #[tokio::test]
    async fn list_dispatch_parallel_versions_api_failure() {
        let mut config = make_test_config("test-bucket", "prefix/");
        config.max_parallel_listings = 4;
        config.max_parallel_listing_max_depth = 1;
        config.target_client_config = Some(make_fast_failing_client_config());

        let (storage, _, _) =
            create_test_storage(&config, Some(make_fast_failing_client_config())).await;

        let (sender, _receiver) = async_channel::unbounded();
        let result = storage.list_object_versions(&sender, 1000).await;

        assert!(result.is_err());
        let err_msg = format!("{:#}", result.unwrap_err());
        assert!(
            err_msg.contains("Failed to parallel object version listing"),
            "Expected parallel path error context, got: {err_msg}"
        );
    }

    #[tokio::test]
    async fn list_dispatch_express_onezone_allows_parallel_when_configured() {
        // Express One Zone + allow flag + max_parallel>1 → parallel path (fails on API)
        let mut config = make_test_config("bucket--az1--x-s3", "prefix/");
        config.max_parallel_listings = 2;
        config.max_parallel_listing_max_depth = 1;
        config.allow_parallel_listings_in_express_one_zone = true;
        config.target_client_config = Some(make_fast_failing_client_config());

        let (storage, _, _) =
            create_test_storage(&config, Some(make_fast_failing_client_config())).await;

        let (sender, _receiver) = async_channel::unbounded();
        let result = storage.list_objects(&sender, 1000).await;

        assert!(result.is_err());
        let err_msg = format!("{:#}", result.unwrap_err());
        // Should take parallel path
        assert!(
            err_msg.contains("Failed to parallel object listing"),
            "Expected parallel path error context, got: {err_msg}"
        );
    }

    // ---------------------------------------------------------------
    // fetch_list_page: error messages
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn fetch_list_page_objects_error_message() {
        let mut config = make_test_config("test-bucket", "prefix/");
        config.target_client_config = Some(make_fast_failing_client_config());

        let cancellation_token = crate::types::token::create_pipeline_cancellation_token();
        let (stats_sender, _) = async_channel::unbounded();

        let client_config = make_fast_failing_client_config();
        let client = Some(Arc::new(client_config.create_client().await));

        let storage = S3Storage {
            config,
            bucket: "test-bucket".to_string(),
            prefix: "prefix/".to_string(),
            cancellation_token,
            client,
            request_payer: None,
            stats_sender,
            rate_limit_objects_per_sec: None,
            has_warning: Arc::new(AtomicBool::new(false)),
            listing_worker_semaphore: Arc::new(tokio::sync::Semaphore::new(1)),
        };

        let result = storage
            .fetch_list_page(
                ListingMode::Objects,
                "prefix/",
                None,
                1000,
                None,
                None,
                None,
            )
            .await;
        assert!(result.is_err());
        let err_msg = format!("{:#}", result.unwrap_err());
        assert!(
            err_msg.contains("list_objects_v2() failed"),
            "Expected list_objects_v2 error, got: {err_msg}"
        );
    }

    #[tokio::test]
    async fn fetch_list_page_versions_error_message() {
        let mut config = make_test_config("test-bucket", "prefix/");
        config.target_client_config = Some(make_fast_failing_client_config());

        let cancellation_token = crate::types::token::create_pipeline_cancellation_token();
        let (stats_sender, _) = async_channel::unbounded();

        let client_config = make_fast_failing_client_config();
        let client = Some(Arc::new(client_config.create_client().await));

        let storage = S3Storage {
            config,
            bucket: "test-bucket".to_string(),
            prefix: "prefix/".to_string(),
            cancellation_token,
            client,
            request_payer: None,
            stats_sender,
            rate_limit_objects_per_sec: None,
            has_warning: Arc::new(AtomicBool::new(false)),
            listing_worker_semaphore: Arc::new(tokio::sync::Semaphore::new(1)),
        };

        let result = storage
            .fetch_list_page(
                ListingMode::Versions,
                "prefix/",
                None,
                1000,
                None,
                None,
                None,
            )
            .await;
        assert!(result.is_err());
        let err_msg = format!("{:#}", result.unwrap_err());
        assert!(
            err_msg.contains("list_object_versions() failed"),
            "Expected list_object_versions error, got: {err_msg}"
        );
    }

    // ---------------------------------------------------------------
    // list_with_parallel: cancelled immediately
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn list_with_parallel_cancelled_immediately() {
        let (mut storage, ct) = make_s3_storage_with_token("b", "p/");
        storage.config.max_parallel_listings = 2;
        storage.listing_worker_semaphore = Arc::new(tokio::sync::Semaphore::new(2));
        let (sender, _receiver) = async_channel::unbounded();

        ct.cancel();

        let permit = storage
            .listing_worker_semaphore
            .clone()
            .acquire_owned()
            .await
            .unwrap();

        let result = storage
            .list_with_parallel(ListingMode::Objects, "", &sender, 1000, 1, permit)
            .await;
        assert!(
            result.is_ok(),
            "cancelled parallel listing should return Ok"
        );
    }

    #[tokio::test]
    async fn list_with_parallel_versions_cancelled_immediately() {
        let (mut storage, ct) = make_s3_storage_with_token("b", "p/");
        storage.config.max_parallel_listings = 2;
        storage.listing_worker_semaphore = Arc::new(tokio::sync::Semaphore::new(2));
        let (sender, _receiver) = async_channel::unbounded();

        ct.cancel();

        let permit = storage
            .listing_worker_semaphore
            .clone()
            .acquire_owned()
            .await
            .unwrap();

        let result = storage
            .list_with_parallel(ListingMode::Versions, "", &sender, 1000, 1, permit)
            .await;
        assert!(result.is_ok());
    }

    // ---------------------------------------------------------------
    // list_with_parallel: empty prefix uses self.prefix
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn list_with_parallel_empty_prefix_uses_storage_prefix() {
        // With an empty prefix and a fast-failing client, the error message
        // should reference the storage's own prefix (verifying prefix resolution).
        let mut config = make_test_config("test-bucket", "my-prefix/");
        config.max_parallel_listings = 2;
        config.max_parallel_listing_max_depth = 1;
        config.target_client_config = Some(make_fast_failing_client_config());

        let cancellation_token = crate::types::token::create_pipeline_cancellation_token();
        let (stats_sender, _) = async_channel::unbounded();
        let client_config = make_fast_failing_client_config();
        let client = Some(Arc::new(client_config.create_client().await));

        let storage = S3Storage {
            config,
            bucket: "test-bucket".to_string(),
            prefix: "my-prefix/".to_string(),
            cancellation_token,
            client,
            request_payer: None,
            stats_sender,
            rate_limit_objects_per_sec: None,
            has_warning: Arc::new(AtomicBool::new(false)),
            listing_worker_semaphore: Arc::new(tokio::sync::Semaphore::new(2)),
        };

        let (sender, _receiver) = async_channel::unbounded();
        let permit = storage
            .listing_worker_semaphore
            .clone()
            .acquire_owned()
            .await
            .unwrap();

        // Empty prefix "" → should resolve to "my-prefix/"
        let result = storage
            .list_with_parallel(ListingMode::Objects, "", &sender, 1000, 1, permit)
            .await;
        assert!(result.is_err());
        let err_msg = format!("{:#}", result.unwrap_err());
        assert!(
            err_msg.contains("my-prefix/"),
            "Error should reference the resolved prefix 'my-prefix/', got: {err_msg}"
        );
    }
}
