use crate::callback::event_manager::EventManager;
use crate::callback::filter_manager::FilterManager;
use crate::types::{ClientConfigLocation, S3Credentials, StoragePath};
use aws_sdk_s3::types::RequestPayer;
use aws_smithy_types::checksum_config::RequestChecksumCalculation;
use chrono::{DateTime, Utc};
use fancy_regex::Regex;
use std::sync::Arc;
use tokio::sync::Semaphore;

/// Main configuration for the s3rm-rs deletion pipeline.
///
/// Holds all settings needed to configure and run a [`DeletionPipeline`](crate::DeletionPipeline):
/// target bucket/prefix, AWS credentials, worker pool size, filter rules,
/// safety flags (dry-run, force, max-delete), and callback registrations.
///
/// Adapted from s3sync's Config, removing source-specific and sync-specific options.
/// Only target-related configuration is retained since s3rm-rs operates on a single S3 target.
///
/// ## Programmatic Construction
///
/// Build a `Config` directly when using the library API:
///
/// ```no_run
/// use s3rm_rs::config::{Config, FilterConfig, ForceRetryConfig, ClientConfig, RetryConfig, CLITimeoutConfig, TracingConfig};
/// use s3rm_rs::types::{StoragePath, S3Credentials, ClientConfigLocation};
/// use s3rm_rs::{FilterManager, EventManager};
/// use std::sync::Arc;
///
/// let config = Config {
///     target: StoragePath::S3 {
///         bucket: "my-bucket".into(),
///         prefix: "logs/2024/".into(),
///     },
///     worker_size: 100,
///     batch_size: 1000,
///     dry_run: true,
///     force: true,
///     delete_all_versions: false,
///     max_delete: Some(10_000),
///     filter_config: FilterConfig::default(),
///     filter_manager: FilterManager::new(),
///     event_manager: EventManager::new(),
///     // ... other fields
///     # show_no_progress: false,
///     # target_client_config: None,
///     # force_retry_config: ForceRetryConfig { force_retry_count: 3, force_retry_interval_milliseconds: 1000 },
///     # tracing_config: None,
///     # warn_as_error: false,
///     # rate_limit_objects: None,
///     # max_parallel_listings: 10,
///     # object_listing_queue_size: 1000,
///     # max_parallel_listing_max_depth: 5,
///     # allow_parallel_listings_in_express_one_zone: false,
///     # max_keys: 1000,
///     # auto_complete_shell: None,
///     # event_callback_lua_script: None,
///     # filter_callback_lua_script: None,
///     # allow_lua_os_library: false,
///     # allow_lua_unsafe_vm: false,
///     # lua_vm_memory_limit: 50 * 1024 * 1024,
///     # if_match: false,
/// };
/// ```
#[derive(Debug, Clone)]
pub struct Config {
    pub target: StoragePath,
    pub show_no_progress: bool,
    pub target_client_config: Option<ClientConfig>,
    pub force_retry_config: ForceRetryConfig,
    pub tracing_config: Option<TracingConfig>,
    pub worker_size: u16,
    pub warn_as_error: bool,
    pub dry_run: bool,
    pub rate_limit_objects: Option<u32>,
    pub max_parallel_listings: u16,
    pub object_listing_queue_size: u32,
    pub max_parallel_listing_max_depth: u16,
    pub allow_parallel_listings_in_express_one_zone: bool,
    pub filter_config: FilterConfig,
    pub max_keys: i32,
    pub auto_complete_shell: Option<clap_complete::shells::Shell>,
    pub event_callback_lua_script: Option<String>,
    pub filter_callback_lua_script: Option<String>,
    pub allow_lua_os_library: bool,
    pub allow_lua_unsafe_vm: bool,
    pub lua_vm_memory_limit: usize,
    pub if_match: bool,
    pub max_delete: Option<u64>,
    // Callback managers
    pub filter_manager: FilterManager,
    pub event_manager: EventManager,
    // Deletion-specific options
    pub batch_size: u16,
    pub delete_all_versions: bool,
    pub force: bool,
}

/// AWS S3 client configuration.
///
/// Reused from s3sync's ClientConfig with credential loading,
/// region configuration, endpoint setup, retry config, and timeout config.
#[derive(Debug, Clone)]
pub struct ClientConfig {
    pub client_config_location: ClientConfigLocation,
    pub credential: S3Credentials,
    pub region: Option<String>,
    pub endpoint_url: Option<String>,
    pub force_path_style: bool,
    pub accelerate: bool,
    pub request_payer: Option<RequestPayer>,
    pub retry_config: RetryConfig,
    pub cli_timeout_config: CLITimeoutConfig,
    pub disable_stalled_stream_protection: bool,
    pub request_checksum_calculation: RequestChecksumCalculation,
    pub parallel_upload_semaphore: Arc<Semaphore>,
}

/// Retry configuration for AWS SDK operations.
///
/// Reused from s3sync's retry configuration with exponential backoff.
#[derive(Debug, Clone)]
pub struct RetryConfig {
    pub aws_max_attempts: u32,
    pub initial_backoff_milliseconds: u64,
}

/// Timeout configuration for AWS SDK operations.
///
/// Reused from s3sync's CLI timeout configuration.
#[derive(Debug, Clone)]
pub struct CLITimeoutConfig {
    pub operation_timeout_milliseconds: Option<u64>,
    pub operation_attempt_timeout_milliseconds: Option<u64>,
    pub connect_timeout_milliseconds: Option<u64>,
    pub read_timeout_milliseconds: Option<u64>,
}

/// Tracing (logging) configuration.
///
/// Reused from s3sync's tracing configuration supporting verbosity levels,
/// JSON format, color control, and AWS SDK tracing.
#[derive(Debug, Clone, Copy)]
pub struct TracingConfig {
    pub tracing_level: log::Level,
    pub json_tracing: bool,
    pub aws_sdk_tracing: bool,
    pub span_events_tracing: bool,
    pub disable_color_tracing: bool,
}

/// Force retry configuration for application-level retries
/// (in addition to AWS SDK retries).
///
/// Reused from s3sync's force retry configuration.
#[derive(Debug, Clone, Copy)]
pub struct ForceRetryConfig {
    pub force_retry_count: u32,
    pub force_retry_interval_milliseconds: u64,
}

/// Filter configuration for object selection.
///
/// Adapted from s3sync's FilterConfig, removing sync-specific filters
/// (check_size, check_etag, etc.) and retaining deletion-relevant filters.
#[derive(Debug, Clone, Default)]
pub struct FilterConfig {
    pub before_time: Option<DateTime<Utc>>,
    pub after_time: Option<DateTime<Utc>>,
    pub include_regex: Option<Regex>,
    pub exclude_regex: Option<Regex>,
    pub include_content_type_regex: Option<Regex>,
    pub exclude_content_type_regex: Option<Regex>,
    pub include_metadata_regex: Option<Regex>,
    pub exclude_metadata_regex: Option<Regex>,
    pub include_tag_regex: Option<Regex>,
    pub exclude_tag_regex: Option<Regex>,
    pub larger_size: Option<u64>,
    pub smaller_size: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn init_dummy_tracing_subscriber() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter("dummy=trace")
            .try_init();
    }

    #[test]
    fn retry_config_creation() {
        init_dummy_tracing_subscriber();

        let retry_config = RetryConfig {
            aws_max_attempts: 3,
            initial_backoff_milliseconds: 100,
        };
        assert_eq!(retry_config.aws_max_attempts, 3);
        assert_eq!(retry_config.initial_backoff_milliseconds, 100);
    }

    #[test]
    fn cli_timeout_config_creation() {
        init_dummy_tracing_subscriber();

        let timeout_config = CLITimeoutConfig {
            operation_timeout_milliseconds: Some(30000),
            operation_attempt_timeout_milliseconds: Some(10000),
            connect_timeout_milliseconds: Some(5000),
            read_timeout_milliseconds: Some(5000),
        };
        assert_eq!(timeout_config.operation_timeout_milliseconds, Some(30000));
        assert_eq!(
            timeout_config.operation_attempt_timeout_milliseconds,
            Some(10000)
        );
    }

    #[test]
    fn cli_timeout_config_no_timeouts() {
        init_dummy_tracing_subscriber();

        let timeout_config = CLITimeoutConfig {
            operation_timeout_milliseconds: None,
            operation_attempt_timeout_milliseconds: None,
            connect_timeout_milliseconds: None,
            read_timeout_milliseconds: None,
        };
        assert!(timeout_config.operation_timeout_milliseconds.is_none());
    }

    #[test]
    fn tracing_config_creation() {
        init_dummy_tracing_subscriber();

        let tracing_config = TracingConfig {
            tracing_level: log::Level::Info,
            json_tracing: false,
            aws_sdk_tracing: false,
            span_events_tracing: false,
            disable_color_tracing: false,
        };
        assert_eq!(tracing_config.tracing_level, log::Level::Info);
        assert!(!tracing_config.json_tracing);
    }

    #[test]
    fn force_retry_config_creation() {
        init_dummy_tracing_subscriber();

        let force_retry = ForceRetryConfig {
            force_retry_count: 3,
            force_retry_interval_milliseconds: 1000,
        };
        assert_eq!(force_retry.force_retry_count, 3);
        assert_eq!(force_retry.force_retry_interval_milliseconds, 1000);
    }

    #[test]
    fn filter_config_default() {
        init_dummy_tracing_subscriber();

        let filter_config = FilterConfig::default();
        assert!(filter_config.before_time.is_none());
        assert!(filter_config.after_time.is_none());
        assert!(filter_config.include_regex.is_none());
        assert!(filter_config.exclude_regex.is_none());
        assert!(filter_config.larger_size.is_none());
        assert!(filter_config.smaller_size.is_none());
    }
}
