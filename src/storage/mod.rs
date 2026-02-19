use anyhow::Result;
use async_channel::Sender;
use async_trait::async_trait;
use aws_sdk_s3::Client;
use aws_sdk_s3::operation::delete_object::DeleteObjectOutput;
use aws_sdk_s3::operation::delete_objects::DeleteObjectsOutput;
use aws_sdk_s3::operation::get_object_tagging::GetObjectTaggingOutput;
use aws_sdk_s3::operation::head_object::HeadObjectOutput;
use aws_sdk_s3::types::{ObjectIdentifier, RequestPayer};
use dyn_clone::DynClone;
use leaky_bucket::RateLimiter;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use crate::config::{ClientConfig, Config};
use crate::types::token::PipelineCancellationToken;
use crate::types::{DeletionStatistics, S3Object, StoragePath};

pub mod s3;

/// Type alias for a boxed Storage trait object.
///
/// Reused from s3sync's Storage type pattern.
pub type Storage = Box<dyn StorageTrait + Send + Sync>;

/// Factory trait for creating Storage instances.
///
/// Adapted from s3sync's StorageFactory - simplified for S3-only usage
/// (no local storage needed since s3rm-rs only deletes from S3).
#[async_trait]
pub trait StorageFactory {
    #[allow(clippy::too_many_arguments)]
    async fn create(
        config: Config,
        path: StoragePath,
        cancellation_token: PipelineCancellationToken,
        stats_sender: Sender<DeletionStatistics>,
        client_config: Option<ClientConfig>,
        request_payer: Option<RequestPayer>,
        rate_limit_objects_per_sec: Option<Arc<RateLimiter>>,
        has_warning: Arc<AtomicBool>,
    ) -> Storage;
}

/// Core storage trait for S3 operations needed by the deletion pipeline.
///
/// Adapted from s3sync's StorageTrait, keeping only the methods required
/// for deletion operations. Removed sync-specific methods (get_object,
/// put_object, copy_object, etc.).
///
/// Methods retained:
/// - `list_objects` / `list_object_versions`: For listing objects to delete
/// - `delete_object`: For single-object deletion
/// - `head_object`: For content-type and metadata filtering in ObjectDeleter
/// - `get_object_tagging`: For tag filtering in ObjectDeleter
/// - `is_versioning_enabled`: For versioning detection
/// - `get_client`: For direct client access (batch DeleteObjects API)
/// - `is_express_onezone_storage`: For Express One Zone handling
/// - Stats and warning methods: For pipeline statistics tracking
#[async_trait]
pub trait StorageTrait: DynClone {
    /// Returns true if this is an Express One Zone storage bucket.
    fn is_express_onezone_storage(&self) -> bool;

    /// List objects (non-versioned) and send them to the provided channel.
    ///
    /// Listing failures are treated as unrecoverable errors.
    async fn list_objects(&self, sender: &Sender<S3Object>, max_keys: i32) -> Result<()>;

    /// List object versions (versioned bucket) and send them to the channel.
    ///
    /// Listing failures are treated as unrecoverable errors.
    async fn list_object_versions(&self, sender: &Sender<S3Object>, max_keys: i32) -> Result<()>;

    /// Get object metadata via HeadObject API.
    ///
    /// `relative_key` is the object key relative to the storage prefix;
    /// the prefix is prepended internally before calling S3.
    /// Used by ObjectDeleter for content-type and metadata filtering.
    async fn head_object(
        &self,
        relative_key: &str,
        version_id: Option<String>,
    ) -> Result<HeadObjectOutput>;

    /// Get object tags via GetObjectTagging API.
    ///
    /// `relative_key` is the object key relative to the storage prefix;
    /// the prefix is prepended internally before calling S3.
    /// Used by ObjectDeleter for tag filtering.
    async fn get_object_tagging(
        &self,
        relative_key: &str,
        version_id: Option<String>,
    ) -> Result<GetObjectTaggingOutput>;

    /// Delete a single object via DeleteObject API.
    ///
    /// `relative_key` is the object key relative to the storage prefix;
    /// the prefix is prepended internally before calling S3.
    /// Supports version_id for versioned deletions and if_match for
    /// optimistic locking (ETag-based conditional deletion).
    async fn delete_object(
        &self,
        relative_key: &str,
        version_id: Option<String>,
        if_match: Option<String>,
    ) -> Result<DeleteObjectOutput>;

    /// Delete multiple objects in a single request via DeleteObjects batch API.
    ///
    /// Takes a list of ObjectIdentifier whose keys are **full S3 keys**
    /// (already include any prefix). The prefix is NOT prepended.
    /// Supports up to 1000 objects per request.
    /// The caller is responsible for batching into groups of 1000.
    ///
    /// Returns DeleteObjectsOutput containing both successfully deleted objects
    /// and any errors for objects that failed to delete (partial failure).
    async fn delete_objects(&self, objects: Vec<ObjectIdentifier>) -> Result<DeleteObjectsOutput>;

    /// Check if versioning is enabled on the bucket.
    async fn is_versioning_enabled(&self) -> Result<bool>;

    /// Get the underlying AWS S3 Client for direct API access.
    fn get_client(&self) -> Option<Arc<Client>>;

    /// Get the statistics sender channel.
    fn get_stats_sender(&self) -> Sender<DeletionStatistics>;

    /// Send a statistics event through the channel.
    async fn send_stats(&self, stats: DeletionStatistics);

    /// Set the warning flag to indicate a warning occurred.
    fn set_warning(&self);
}

dyn_clone::clone_trait_object!(StorageTrait);

// Default refill interval 100ms (same as s3sync)
const REFILL_PER_INTERVAL_DIVIDER: usize = 10;

/// Create a single S3 storage instance for the deletion pipeline.
///
/// Adapted from s3sync's storage_factory::create_storage_pair - simplified
/// to create only one storage (target) since s3rm-rs doesn't need a source.
/// Also simplified to omit bandwidth rate limiting (not needed for deletion).
pub async fn create_storage(
    config: Config,
    cancellation_token: PipelineCancellationToken,
    stats_sender: Sender<DeletionStatistics>,
    has_warning: Arc<AtomicBool>,
) -> Storage {
    let rate_limit_objects_per_sec = config.rate_limit_objects.map(|rate_limit_value| {
        let refill = if (rate_limit_value as usize) <= REFILL_PER_INTERVAL_DIVIDER {
            1
        } else {
            rate_limit_value as usize / REFILL_PER_INTERVAL_DIVIDER
        };
        Arc::new(
            RateLimiter::builder()
                .max(rate_limit_value as usize)
                .initial(rate_limit_value as usize)
                .refill(refill)
                .fair(true)
                .build(),
        )
    });

    let client_config = config.target_client_config.clone();
    let request_payer = client_config.as_ref().and_then(|c| c.request_payer.clone());

    let target = config.target.clone();

    s3::S3StorageFactory::create(
        config,
        target,
        cancellation_token,
        stats_sender,
        client_config,
        request_payer,
        rate_limit_objects_per_sec,
        has_warning,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        CLITimeoutConfig, FilterConfig, ForceRetryConfig, RetryConfig, TracingConfig,
    };
    use crate::types::{AccessKeys, ClientConfigLocation, S3Credentials};
    use aws_smithy_types::checksum_config::RequestChecksumCalculation;
    use std::sync::atomic::AtomicBool;
    use tokio::sync::Semaphore;

    fn init_dummy_tracing_subscriber() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter("dummy=trace")
            .try_init();
    }

    fn make_test_client_config() -> ClientConfig {
        ClientConfig {
            client_config_location: ClientConfigLocation {
                aws_config_file: None,
                aws_shared_credentials_file: None,
            },
            credential: S3Credentials::Credentials {
                access_keys: AccessKeys {
                    access_key: "test_key".to_string(),
                    secret_access_key: "test_secret".to_string(),
                    session_token: None,
                },
            },
            region: Some("us-east-1".to_string()),
            endpoint_url: Some("https://localhost:9000".to_string()),
            force_path_style: true,
            retry_config: RetryConfig {
                aws_max_attempts: 3,
                initial_backoff_milliseconds: 100,
            },
            cli_timeout_config: CLITimeoutConfig {
                operation_timeout_milliseconds: None,
                operation_attempt_timeout_milliseconds: None,
                connect_timeout_milliseconds: None,
                read_timeout_milliseconds: None,
            },
            disable_stalled_stream_protection: false,
            request_checksum_calculation: RequestChecksumCalculation::WhenRequired,
            parallel_upload_semaphore: Arc::new(Semaphore::new(1)),
            accelerate: false,
            request_payer: None,
        }
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
            target_client_config: Some(make_test_client_config()),
            force_retry_config: ForceRetryConfig {
                force_retry_count: 0,
                force_retry_interval_milliseconds: 0,
            },
            tracing_config: Some(TracingConfig {
                tracing_level: log::Level::Info,
                json_tracing: false,
                aws_sdk_tracing: false,
                span_events_tracing: false,
                disable_color_tracing: false,
            }),
            worker_size: 4,
            warn_as_error: false,
            dry_run: false,
            rate_limit_objects: None,
            max_parallel_listings: 1,
            object_listing_queue_size: 1000,
            max_parallel_listing_max_depth: 0,
            allow_parallel_listings_in_express_one_zone: false,
            filter_config: FilterConfig::default(),
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
        }
    }

    #[tokio::test]
    async fn create_s3_storage_with_credentials() {
        init_dummy_tracing_subscriber();

        let config = make_test_config("test-bucket", "prefix/");
        let cancellation_token = crate::types::token::create_pipeline_cancellation_token();
        let (stats_sender, _stats_receiver) = async_channel::unbounded();
        let has_warning = Arc::new(AtomicBool::new(false));

        let storage = create_storage(config, cancellation_token, stats_sender, has_warning).await;

        // Should have a client
        assert!(storage.get_client().is_some());
        // Should not be express one zone
        assert!(!storage.is_express_onezone_storage());
    }

    #[tokio::test]
    async fn create_s3_storage_with_rate_limiting() {
        init_dummy_tracing_subscriber();

        let mut config = make_test_config("test-bucket", "prefix/");
        config.rate_limit_objects = Some(100);

        let cancellation_token = crate::types::token::create_pipeline_cancellation_token();
        let (stats_sender, _stats_receiver) = async_channel::unbounded();
        let has_warning = Arc::new(AtomicBool::new(false));

        let storage = create_storage(config, cancellation_token, stats_sender, has_warning).await;

        assert!(storage.get_client().is_some());
    }

    #[tokio::test]
    async fn create_s3_storage_with_small_rate_limit() {
        init_dummy_tracing_subscriber();

        let mut config = make_test_config("test-bucket", "prefix/");
        config.rate_limit_objects = Some(5); // <= REFILL_PER_INTERVAL_DIVIDER

        let cancellation_token = crate::types::token::create_pipeline_cancellation_token();
        let (stats_sender, _stats_receiver) = async_channel::unbounded();
        let has_warning = Arc::new(AtomicBool::new(false));

        let storage = create_storage(config, cancellation_token, stats_sender, has_warning).await;

        assert!(storage.get_client().is_some());
    }

    #[tokio::test]
    async fn create_s3_storage_express_one_zone() {
        init_dummy_tracing_subscriber();

        // Express One Zone buckets have --x-s3 suffix
        let config = make_test_config("test-bucket--usw2-az1--x-s3", "prefix/");
        let cancellation_token = crate::types::token::create_pipeline_cancellation_token();
        let (stats_sender, _stats_receiver) = async_channel::unbounded();
        let has_warning = Arc::new(AtomicBool::new(false));

        let storage = create_storage(config, cancellation_token, stats_sender, has_warning).await;

        assert!(storage.is_express_onezone_storage());
    }

    #[tokio::test]
    async fn create_s3_storage_no_client_config() {
        init_dummy_tracing_subscriber();

        let mut config = make_test_config("test-bucket", "prefix/");
        config.target_client_config = None;

        let cancellation_token = crate::types::token::create_pipeline_cancellation_token();
        let (stats_sender, _stats_receiver) = async_channel::unbounded();
        let has_warning = Arc::new(AtomicBool::new(false));

        let storage = create_storage(config, cancellation_token, stats_sender, has_warning).await;

        // No client config means no client
        assert!(storage.get_client().is_none());
    }

    #[tokio::test]
    async fn storage_stats_sender_works() {
        init_dummy_tracing_subscriber();

        let config = make_test_config("test-bucket", "prefix/");
        let cancellation_token = crate::types::token::create_pipeline_cancellation_token();
        let (stats_sender, stats_receiver) = async_channel::unbounded();
        let has_warning = Arc::new(AtomicBool::new(false));

        let storage = create_storage(config, cancellation_token, stats_sender, has_warning).await;

        // Send stats through storage
        let sender = storage.get_stats_sender();
        sender
            .send(DeletionStatistics::DeleteComplete {
                key: "test/key".to_string(),
            })
            .await
            .unwrap();

        let received = stats_receiver.recv().await.unwrap();
        assert!(matches!(
            received,
            DeletionStatistics::DeleteComplete { .. }
        ));
    }

    #[tokio::test]
    async fn storage_set_warning() {
        init_dummy_tracing_subscriber();

        let config = make_test_config("test-bucket", "prefix/");
        let cancellation_token = crate::types::token::create_pipeline_cancellation_token();
        let (stats_sender, _stats_receiver) = async_channel::unbounded();
        let has_warning = Arc::new(AtomicBool::new(false));
        let has_warning_clone = has_warning.clone();

        let storage =
            create_storage(config, cancellation_token, stats_sender, has_warning_clone).await;

        assert!(!has_warning.load(std::sync::atomic::Ordering::SeqCst));
        storage.set_warning();
        assert!(has_warning.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[tokio::test]
    async fn storage_send_stats_async() {
        init_dummy_tracing_subscriber();

        let config = make_test_config("test-bucket", "prefix/");
        let cancellation_token = crate::types::token::create_pipeline_cancellation_token();
        let (stats_sender, stats_receiver) = async_channel::unbounded();
        let has_warning = Arc::new(AtomicBool::new(false));

        let storage = create_storage(config, cancellation_token, stats_sender, has_warning).await;

        storage
            .send_stats(DeletionStatistics::DeleteBytes(1024))
            .await;

        let received = stats_receiver.recv().await.unwrap();
        assert!(matches!(received, DeletionStatistics::DeleteBytes(1024)));
    }

    #[tokio::test]
    async fn create_s3_storage_with_request_payer() {
        init_dummy_tracing_subscriber();

        let mut config = make_test_config("test-bucket", "prefix/");
        if let Some(ref mut cc) = config.target_client_config {
            cc.request_payer = Some(RequestPayer::Requester);
        }

        let cancellation_token = crate::types::token::create_pipeline_cancellation_token();
        let (stats_sender, _stats_receiver) = async_channel::unbounded();
        let has_warning = Arc::new(AtomicBool::new(false));

        let storage = create_storage(config, cancellation_token, stats_sender, has_warning).await;

        assert!(storage.get_client().is_some());
    }
}
