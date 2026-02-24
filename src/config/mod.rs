pub mod args;

use crate::callback::event_manager::EventManager;
use crate::callback::filter_manager::FilterManager;
use crate::types::{ClientConfigLocation, S3Credentials, StoragePath};
use aws_sdk_s3::types::RequestPayer;
use aws_smithy_types::checksum_config::RequestChecksumCalculation;
use chrono::{DateTime, Utc};
use fancy_regex::Regex;

/// Main configuration for the s3rm-rs deletion pipeline.
///
/// Holds all settings needed to configure and run a [`DeletionPipeline`](crate::DeletionPipeline):
/// target bucket/prefix, AWS credentials, worker pool size, filter rules,
/// safety flags (dry-run, force, max-delete), and callback registrations.
///
/// Adapted from s3sync's Config, removing source-specific and sync-specific options.
/// Only target-related configuration is retained since s3rm-rs operates on a single S3 target.
///
/// # Quick Start
///
/// Use [`Config::for_target`] for a minimal configuration with sensible defaults:
///
/// ```
/// use s3rm_rs::Config;
///
/// let config = Config::for_target("my-bucket", "logs/2024/");
/// assert_eq!(config.worker_size, 24);
/// assert_eq!(config.batch_size, 200);
/// ```
///
/// Then customize fields as needed:
///
/// ```
/// use s3rm_rs::Config;
///
/// let mut config = Config::for_target("my-bucket", "logs/2024/");
/// config.dry_run = true;
/// config.force = true;
/// config.worker_size = 100;
/// config.max_delete = Some(10_000);
/// ```
///
/// # Default
///
/// [`Config::default()`] creates a configuration targeting an empty bucket/prefix.
/// You must set the `target` field before running a pipeline.
///
/// ```
/// use s3rm_rs::Config;
/// use s3rm_rs::types::StoragePath;
///
/// let mut config = Config::default();
/// config.target = StoragePath::S3 {
///     bucket: "my-bucket".into(),
///     prefix: "prefix/".into(),
/// };
/// ```
#[derive(Debug, Clone)]
pub struct Config {
    pub target: StoragePath,
    pub show_no_progress: bool,
    pub log_deletion_summary: bool,
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
    // Testing flag: enables user-defined callbacks (for library testing)
    pub test_user_defined_callback: bool,
}

impl Config {
    /// Create a `Config` with sensible defaults for the given S3 bucket and prefix.
    ///
    /// This is the recommended way to construct a `Config` for library usage.
    /// All fields are set to production-ready defaults matching the CLI defaults
    /// (24 workers, batch size 200, etc.). The `force` flag is set to `true` to
    /// skip interactive confirmation prompts, which is appropriate for programmatic use.
    ///
    /// # Examples
    ///
    /// ```
    /// use s3rm_rs::Config;
    ///
    /// let config = Config::for_target("my-bucket", "logs/");
    /// assert_eq!(config.batch_size, 200);
    /// assert!(config.force); // no interactive prompts
    /// ```
    pub fn for_target(bucket: &str, prefix: &str) -> Self {
        Config {
            target: StoragePath::S3 {
                bucket: bucket.to_string(),
                prefix: prefix.to_string(),
            },
            force: true,
            ..Config::default()
        }
    }
}

impl Default for Config {
    /// Create a `Config` with sensible defaults.
    ///
    /// The `target` defaults to an empty bucket/prefix — set it before running
    /// a pipeline. All other fields use production defaults matching the CLI.
    fn default() -> Self {
        Config {
            target: StoragePath::S3 {
                bucket: String::new(),
                prefix: String::new(),
            },
            show_no_progress: false,
            log_deletion_summary: true,
            target_client_config: None,
            force_retry_config: ForceRetryConfig::default(),
            tracing_config: None,
            worker_size: 24,
            warn_as_error: false,
            dry_run: false,
            rate_limit_objects: None,
            max_parallel_listings: 16,
            object_listing_queue_size: 200_000,
            max_parallel_listing_max_depth: 2,
            allow_parallel_listings_in_express_one_zone: false,
            filter_config: FilterConfig::default(),
            max_keys: 1000,
            auto_complete_shell: None,
            event_callback_lua_script: None,
            filter_callback_lua_script: None,
            allow_lua_os_library: false,
            allow_lua_unsafe_vm: false,
            lua_vm_memory_limit: 64 * 1024 * 1024,
            if_match: false,
            max_delete: None,
            filter_manager: FilterManager::new(),
            event_manager: EventManager::new(),
            batch_size: 200,
            delete_all_versions: false,
            force: false,
            test_user_defined_callback: false,
        }
    }
}

impl Default for ForceRetryConfig {
    fn default() -> Self {
        ForceRetryConfig {
            force_retry_count: 0,
            force_retry_interval_milliseconds: 1000,
        }
    }
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
    use crate::test_utils::init_dummy_tracing_subscriber;

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

    // ------------------------------------------------------------------
    // Config::for_target and Config::default tests
    // (Covers uncovered constructors — lines 113-166)
    // ------------------------------------------------------------------

    #[test]
    fn config_for_target_sets_bucket_and_prefix() {
        init_dummy_tracing_subscriber();

        let config = Config::for_target("my-bucket", "logs/2024/");
        let StoragePath::S3 { bucket, prefix } = &config.target;
        assert_eq!(bucket, "my-bucket");
        assert_eq!(prefix, "logs/2024/");
    }

    #[test]
    fn config_for_target_sets_force_true() {
        // Library usage should skip interactive prompts by default.
        let config = Config::for_target("bucket", "prefix/");
        assert!(config.force);
    }

    #[test]
    fn config_for_target_uses_default_worker_and_batch_size() {
        let config = Config::for_target("bucket", "");
        assert_eq!(config.worker_size, 24);
        assert_eq!(config.batch_size, 200);
    }

    #[test]
    fn config_for_target_has_sensible_defaults() {
        let config = Config::for_target("bucket", "prefix/");
        assert!(!config.dry_run);
        assert!(!config.delete_all_versions);
        assert!(!config.if_match);
        assert!(config.max_delete.is_none());
        assert!(config.tracing_config.is_none());
        assert!(config.target_client_config.is_none());
        assert!(config.rate_limit_objects.is_none());
        assert!(!config.warn_as_error);
        assert!(!config.test_user_defined_callback);
    }

    #[test]
    fn config_default_has_empty_target() {
        let config = Config::default();
        let StoragePath::S3 { bucket, prefix } = &config.target;
        assert!(bucket.is_empty());
        assert!(prefix.is_empty());
    }

    #[test]
    fn config_default_does_not_set_force() {
        // Default config (not library convenience) should NOT skip prompts.
        let config = Config::default();
        assert!(!config.force);
    }

    #[test]
    fn config_default_field_values() {
        let config = Config::default();
        assert_eq!(config.worker_size, 24);
        assert_eq!(config.batch_size, 200);
        assert!(!config.show_no_progress);
        assert!(config.log_deletion_summary);
        assert!(!config.dry_run);
        assert!(!config.warn_as_error);
        assert_eq!(config.max_parallel_listings, 16);
        assert_eq!(config.object_listing_queue_size, 200_000);
        assert_eq!(config.max_parallel_listing_max_depth, 2);
        assert!(!config.allow_parallel_listings_in_express_one_zone);
        assert_eq!(config.max_keys, 1000);
        assert!(config.auto_complete_shell.is_none());
        assert!(config.event_callback_lua_script.is_none());
        assert!(config.filter_callback_lua_script.is_none());
        assert!(!config.allow_lua_os_library);
        assert!(!config.allow_lua_unsafe_vm);
        assert_eq!(config.lua_vm_memory_limit, 64 * 1024 * 1024);
        assert!(!config.if_match);
        assert!(!config.delete_all_versions);
    }

    #[test]
    fn force_retry_config_default_values() {
        let frc = ForceRetryConfig::default();
        assert_eq!(frc.force_retry_count, 0);
        assert_eq!(frc.force_retry_interval_milliseconds, 1000);
    }
}
