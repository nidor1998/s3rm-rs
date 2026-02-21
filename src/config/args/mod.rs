use crate::callback::event_manager::EventManager;
use crate::callback::filter_manager::FilterManager;
use crate::config::{
    CLITimeoutConfig, ClientConfig, Config, FilterConfig, ForceRetryConfig, RetryConfig,
    TracingConfig,
};
use crate::types::{AccessKeys, ClientConfigLocation, S3Credentials, StoragePath};
use aws_sdk_s3::types::RequestPayer;
use aws_smithy_types::checksum_config::RequestChecksumCalculation;
use chrono::{DateTime, Utc};
use clap::Parser;
use clap::builder::NonEmptyStringValueParser;
use clap_verbosity_flag::{Verbosity, WarnLevel};
use fancy_regex::Regex;
use std::ffi::OsString;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::Semaphore;

#[cfg(test)]
mod tests;

// ---------------------------------------------------------------------------
// Default constants (aligned with s3sync)
// ---------------------------------------------------------------------------

const EXPRESS_ONEZONE_STORAGE_SUFFIX: &str = "--x-s3";

const DEFAULT_WORKER_SIZE: u16 = 16;
const DEFAULT_BATCH_SIZE: u16 = 1000;
const DEFAULT_AWS_MAX_ATTEMPTS: u32 = 10;
const DEFAULT_FORCE_RETRY_COUNT: u32 = 5;
const DEFAULT_FORCE_RETRY_INTERVAL_MILLISECONDS: u64 = 1000;
const DEFAULT_INITIAL_BACKOFF_MILLISECONDS: u64 = 100;
const DEFAULT_JSON_TRACING: bool = false;
const DEFAULT_AWS_SDK_TRACING: bool = false;
const DEFAULT_SPAN_EVENTS_TRACING: bool = false;
const DEFAULT_DISABLE_COLOR_TRACING: bool = false;
const DEFAULT_WARN_AS_ERROR: bool = false;
const DEFAULT_FORCE_PATH_STYLE: bool = false;
const DEFAULT_DRY_RUN: bool = false;
const DEFAULT_MAX_KEYS: i32 = 1000;
const DEFAULT_DISABLE_STALLED_STREAM_PROTECTION: bool = false;
const DEFAULT_MAX_PARALLEL_LISTINGS: u16 = 16;
const DEFAULT_OBJECT_LISTING_QUEUE_SIZE: u32 = 200000;
const DEFAULT_PARALLEL_LISTING_MAX_DEPTH: u16 = 2;
const DEFAULT_ALLOW_PARALLEL_LISTINGS_IN_EXPRESS_ONE_ZONE: bool = false;
const DEFAULT_ACCELERATE: bool = false;
const DEFAULT_REQUEST_PAYER: bool = false;
const DEFAULT_SHOW_NO_PROGRESS: bool = false;
const DEFAULT_IF_MATCH: bool = false;
#[allow(dead_code)]
const DEFAULT_ALLOW_LUA_OS_LIBRARY: bool = false;
#[allow(dead_code)]
const DEFAULT_ALLOW_LUA_UNSAFE_VM: bool = false;
#[allow(dead_code)]
const DEFAULT_LUA_VM_MEMORY_LIMIT: &str = "64MiB";
const DEFAULT_DELETE_ALL_VERSIONS: bool = false;
const DEFAULT_FORCE: bool = false;

// ---------------------------------------------------------------------------
// Error messages
// ---------------------------------------------------------------------------

const ERROR_MESSAGE_INVALID_TARGET: &str =
    "Target must be an S3 path starting with 's3://' (e.g., s3://bucket/prefix).";
const ERROR_MESSAGE_INVALID_REGEX: &str = "Invalid regular expression pattern";
const ERROR_MESSAGE_WORKER_SIZE_ZERO: &str = "Worker size must be at least 1.";
const ERROR_MESSAGE_BATCH_SIZE_ZERO: &str = "Batch size must be at least 1.";
const ERROR_MESSAGE_BATCH_SIZE_TOO_LARGE: &str = "Batch size must be at most 1000 (S3 API limit).";
const ERROR_MESSAGE_MAX_PARALLEL_LISTINGS_ZERO: &str = "Max parallel listings must be at least 1.";
const ERROR_MESSAGE_OBJECT_LISTING_QUEUE_SIZE_ZERO: &str =
    "Object listing queue size must be at least 1.";
const ERROR_MESSAGE_MAX_PARALLEL_LISTING_MAX_DEPTH_ZERO: &str =
    "Max parallel listing max depth must be at least 1.";

// ---------------------------------------------------------------------------
// Value parser helpers
// ---------------------------------------------------------------------------

fn check_s3_target(s: &str) -> Result<String, String> {
    if s.starts_with("s3://") && s.len() > 5 {
        Ok(s.to_string())
    } else {
        Err(ERROR_MESSAGE_INVALID_TARGET.to_string())
    }
}

fn parse_human_bytes(s: &str) -> Result<usize, String> {
    let byte = byte_unit::Byte::from_str(s.trim()).map_err(|e| e.to_string())?;
    usize::try_from(byte.as_u128()).map_err(|e| e.to_string())
}

/// Clap value_parser that validates a human-readable byte string without consuming it.
fn check_human_bytes(s: &str) -> Result<String, String> {
    byte_unit::Byte::from_str(s.trim()).map_err(|e| e.to_string())?;
    Ok(s.to_string())
}

// ---------------------------------------------------------------------------
// CLIArgs (clap-derived argument struct)
// ---------------------------------------------------------------------------

/// s3rm - Extremely fast Amazon S3 object deletion tool.
///
/// Delete objects from S3 buckets with powerful filtering,
/// safety features, and versioning support.
///
/// Example:
///   s3rm s3://my-bucket/logs/2023/ --dry-run
///   s3rm s3://my-bucket/temp/ --filter-include-regex '.*\.tmp$' --force
///   s3rm s3://my-bucket/old-data/ --delete-all-versions -vv
#[derive(Parser, Clone, Debug)]
#[command(name = "s3rm", version, about, long_about = None)]
pub struct CLIArgs {
    /// S3 target path: s3://<BUCKET_NAME>[/prefix]
    #[arg(
        env,
        help = "s3://<BUCKET_NAME>[/prefix]",
        value_parser = check_s3_target,
        default_value_if("auto_complete_shell", clap::builder::ArgPredicate::IsPresent, "s3://ignored"),
        required = false,
    )]
    pub target: String,

    // -----------------------------------------------------------------------
    // General options
    // -----------------------------------------------------------------------
    /// Simulation mode. Lists and filters objects but does not actually delete.
    #[arg(short = 'd', long, env, default_value_t = DEFAULT_DRY_RUN, help_heading = "General")]
    pub dry_run: bool,

    /// Don't show the progress bar.
    #[arg(long, env, default_value_t = DEFAULT_SHOW_NO_PROGRESS, help_heading = "General")]
    pub show_no_progress: bool,

    // -----------------------------------------------------------------------
    // Deletion options
    // -----------------------------------------------------------------------
    /// Number of objects per batch deletion request (1–1000). Default: 1000.
    /// When set to 1, uses single-object deletion (DeleteObject API).
    #[arg(long, env, default_value_t = DEFAULT_BATCH_SIZE, help_heading = "Deletion")]
    pub batch_size: u16,

    /// Delete all versions of matching objects including delete markers.
    #[arg(long, env, default_value_t = DEFAULT_DELETE_ALL_VERSIONS, help_heading = "Deletion")]
    pub delete_all_versions: bool,

    // -----------------------------------------------------------------------
    // Safety options
    // -----------------------------------------------------------------------
    /// Skip confirmation prompt before deleting.
    #[arg(short = 'f', long, env, default_value_t = DEFAULT_FORCE, help_heading = "Safety")]
    pub force: bool,

    /// Cancel the pipeline when deletion count exceeds this limit.
    #[arg(long, env, help_heading = "Safety")]
    pub max_delete: Option<u64>,

    // -----------------------------------------------------------------------
    // Filter options (same as s3sync)
    // -----------------------------------------------------------------------
    /// Include only objects whose key matches this regex pattern.
    #[arg(long, env, value_parser = NonEmptyStringValueParser::new(), help_heading = "Filter")]
    pub filter_include_regex: Option<String>,

    /// Exclude objects whose key matches this regex pattern.
    #[arg(long, env, value_parser = NonEmptyStringValueParser::new(), help_heading = "Filter")]
    pub filter_exclude_regex: Option<String>,

    /// Include only objects whose content-type matches this regex pattern.
    #[arg(long, env, value_parser = NonEmptyStringValueParser::new(), help_heading = "Filter")]
    pub filter_include_content_type_regex: Option<String>,

    /// Exclude objects whose content-type matches this regex pattern.
    #[arg(long, env, value_parser = NonEmptyStringValueParser::new(), help_heading = "Filter")]
    pub filter_exclude_content_type_regex: Option<String>,

    /// Include only objects whose user-defined metadata matches this regex.
    #[arg(long, env, value_parser = NonEmptyStringValueParser::new(), help_heading = "Filter",
        long_help = r#"Delete only objects that have metadata matching a given regular expression.
Keys(lowercase) must be sorted in alphabetical order, and comma separated.
This filter is applied after all other filters(except tag filters).
It may take an extra API call to get metadata of the object.

Example: "key1=(value1|value2),key2=value2""#)]
    pub filter_include_metadata_regex: Option<String>,

    /// Exclude objects whose user-defined metadata matches this regex.
    #[arg(long, env, value_parser = NonEmptyStringValueParser::new(), help_heading = "Filter",
        long_help = r#"Do not delete objects that have metadata matching a given regular expression.
Keys(lowercase) must be sorted in alphabetical order, and comma separated.
This filter is applied after all other filters(except tag filters).
It may take an extra API call to get metadata of the object.

Example: "key1=(value1|value2),key2=value2""#)]
    pub filter_exclude_metadata_regex: Option<String>,

    /// Include only objects whose tags match this regex pattern.
    #[arg(long, env, value_parser = NonEmptyStringValueParser::new(), help_heading = "Filter",
        long_help = r#"Delete only objects that have tag matching a given regular expression.
Keys must be sorted in alphabetical order, and '&' separated.
This filter is applied after all other filters.
It takes an extra API call to get tags of the object.

Example: "key1=(value1|value2)&key2=value2""#)]
    pub filter_include_tag_regex: Option<String>,

    /// Exclude objects whose tags match this regex pattern.
    #[arg(long, env, value_parser = NonEmptyStringValueParser::new(), help_heading = "Filter",
        long_help = r#"Do not delete objects that have tag matching a given regular expression.
Keys must be sorted in alphabetical order, and '&' separated.
This filter is applied after all other filters.
It takes an extra API call to get tags of the object.

Example: "key1=(value1|value2)&key2=value2""#)]
    pub filter_exclude_tag_regex: Option<String>,

    /// Include only objects modified before this date (ISO 8601 / RFC 3339).
    #[arg(
        long,
        env,
        help_heading = "Filter",
        long_help = r#"Delete only objects older than given time (RFC3339 datetime).
Example: 2023-02-19T12:00:00Z"#
    )]
    pub filter_mtime_before: Option<DateTime<Utc>>,

    /// Include only objects modified after this date (ISO 8601 / RFC 3339).
    #[arg(
        long,
        env,
        help_heading = "Filter",
        long_help = r#"Delete only objects newer than or equal to given time (RFC3339 datetime).
Example: 2023-02-19T12:00:00Z"#
    )]
    pub filter_mtime_after: Option<DateTime<Utc>>,

    /// Include only objects smaller than this size.
    #[arg(
        long,
        env,
        value_parser = check_human_bytes,
        help_heading = "Filter",
        long_help = r#"Delete only objects smaller than given size.
Allow suffixes: KB, KiB, MB, MiB, GB, GiB, TB, TiB"#
    )]
    pub filter_smaller_size: Option<String>,

    /// Include only objects larger than this size.
    #[arg(
        long,
        env,
        value_parser = check_human_bytes,
        help_heading = "Filter",
        long_help = r#"Delete only objects larger than or equal to given size.
Allow suffixes: KB, KiB, MB, MiB, GB, GiB, TB, TiB"#
    )]
    pub filter_larger_size: Option<String>,

    /// Path to the Lua script that is executed as filter callback.
    #[cfg(feature = "lua_support")]
    #[arg(
        long,
        env,
        value_parser = NonEmptyStringValueParser::new(),
        help_heading = "Lua",
        long_help = r#"Path to the Lua script that is executed as filter callback"#
    )]
    pub filter_callback_lua_script: Option<String>,

    // -----------------------------------------------------------------------
    // Performance options (same as s3sync)
    // -----------------------------------------------------------------------
    /// Number of concurrent deletion workers (1–65535). Default: 16.
    #[arg(long, env, default_value_t = DEFAULT_WORKER_SIZE, help_heading = "Performance")]
    pub worker_size: u16,

    /// Number of concurrent listing operations. Default: 16.
    #[arg(long, env, default_value_t = DEFAULT_MAX_PARALLEL_LISTINGS, help_heading = "Performance")]
    pub max_parallel_listings: u16,

    /// Maximum depth for parallel listing operations. Default: 2.
    #[arg(long, env, default_value_t = DEFAULT_PARALLEL_LISTING_MAX_DEPTH, help_heading = "Performance")]
    pub max_parallel_listing_max_depth: u16,

    /// Maximum objects per second for rate limiting.
    #[arg(long, env, help_heading = "Performance")]
    pub rate_limit_objects: Option<u32>,

    /// Object listing channel queue size. Default: 200000.
    #[arg(long, env, default_value_t = DEFAULT_OBJECT_LISTING_QUEUE_SIZE, help_heading = "Performance")]
    pub object_listing_queue_size: u32,

    /// Allow parallel listings in Express One Zone storage.
    #[arg(long, env, default_value_t = DEFAULT_ALLOW_PARALLEL_LISTINGS_IN_EXPRESS_ONE_ZONE, help_heading = "Performance")]
    pub allow_parallel_listings_in_express_one_zone: bool,

    // -----------------------------------------------------------------------
    // Logging options (same as s3sync)
    // -----------------------------------------------------------------------
    /// Verbosity level. -q (quiet), default (normal), -v, -vv, -vvv.
    #[command(flatten)]
    pub verbosity: Verbosity<WarnLevel>,

    /// Output logs in JSON format.
    #[arg(long, env, default_value_t = DEFAULT_JSON_TRACING, help_heading = "Logging")]
    pub json_tracing: bool,

    /// Enable AWS SDK tracing.
    #[arg(long, env, default_value_t = DEFAULT_AWS_SDK_TRACING, help_heading = "Logging")]
    pub aws_sdk_tracing: bool,

    /// Enable tracing span events.
    #[arg(long, env, default_value_t = DEFAULT_SPAN_EVENTS_TRACING, help_heading = "Logging")]
    pub span_events_tracing: bool,

    /// Disable colored output in logs.
    #[arg(long, env, default_value_t = DEFAULT_DISABLE_COLOR_TRACING, help_heading = "Logging")]
    pub disable_color_tracing: bool,

    // -----------------------------------------------------------------------
    // Retry options (same as s3sync)
    // -----------------------------------------------------------------------
    /// Maximum retry attempts for AWS SDK operations. Default: 10.
    #[arg(long, env, default_value_t = DEFAULT_AWS_MAX_ATTEMPTS, help_heading = "Retry")]
    pub aws_max_attempts: u32,

    /// Initial backoff in milliseconds for retries. Default: 100.
    #[arg(long, env, default_value_t = DEFAULT_INITIAL_BACKOFF_MILLISECONDS, help_heading = "Retry")]
    pub initial_backoff_milliseconds: u64,

    /// Number of application-level force retries (after SDK retries). Default: 5.
    #[arg(long, env, default_value_t = DEFAULT_FORCE_RETRY_COUNT, help_heading = "Retry")]
    pub force_retry_count: u32,

    /// Interval in ms between force retries. Default: 1000.
    #[arg(long, env, default_value_t = DEFAULT_FORCE_RETRY_INTERVAL_MILLISECONDS, help_heading = "Retry")]
    pub force_retry_interval_milliseconds: u64,

    // -----------------------------------------------------------------------
    // Timeout options (same as s3sync)
    // -----------------------------------------------------------------------
    /// Overall operation timeout in milliseconds.
    #[arg(long, env, help_heading = "Timeout")]
    pub operation_timeout_milliseconds: Option<u64>,

    /// Per-attempt operation timeout in milliseconds.
    #[arg(long, env, help_heading = "Timeout")]
    pub operation_attempt_timeout_milliseconds: Option<u64>,

    /// Connection timeout in milliseconds.
    #[arg(long, env, help_heading = "Timeout")]
    pub connect_timeout_milliseconds: Option<u64>,

    /// Read timeout in milliseconds.
    #[arg(long, env, help_heading = "Timeout")]
    pub read_timeout_milliseconds: Option<u64>,

    // -----------------------------------------------------------------------
    // AWS configuration (target-only, adapted from s3sync)
    // -----------------------------------------------------------------------
    /// AWS config file path.
    #[arg(long, env, help_heading = "AWS")]
    pub aws_config_file: Option<PathBuf>,

    /// AWS shared credentials file path.
    #[arg(long, env, help_heading = "AWS")]
    pub aws_shared_credentials_file: Option<PathBuf>,

    /// AWS profile for the target. If not set, uses the default profile.
    #[arg(long, env, value_parser = NonEmptyStringValueParser::new(), help_heading = "AWS")]
    pub target_profile: Option<String>,

    /// AWS access key ID for the target.
    #[arg(long, env, value_parser = NonEmptyStringValueParser::new(), help_heading = "AWS")]
    pub target_access_key: Option<String>,

    /// AWS secret access key for the target.
    #[arg(long, env, value_parser = NonEmptyStringValueParser::new(), help_heading = "AWS")]
    pub target_secret_key: Option<String>,

    /// AWS session token for the target.
    #[arg(long, env, value_parser = NonEmptyStringValueParser::new(), help_heading = "AWS")]
    pub target_session_token: Option<String>,

    /// AWS region for the target.
    #[arg(long, env, value_parser = NonEmptyStringValueParser::new(), help_heading = "AWS")]
    pub target_region: Option<String>,

    /// Custom S3-compatible endpoint URL (e.g. MinIO, Wasabi).
    #[arg(long, env, value_parser = NonEmptyStringValueParser::new(), help_heading = "AWS")]
    pub target_endpoint_url: Option<String>,

    /// Force path-style access (required for some S3-compatible services).
    #[arg(long, env, default_value_t = DEFAULT_FORCE_PATH_STYLE, help_heading = "AWS")]
    pub target_force_path_style: bool,

    /// Enable S3 Transfer Acceleration.
    #[arg(long, env, default_value_t = DEFAULT_ACCELERATE, help_heading = "AWS")]
    pub target_accelerate: bool,

    /// Enable requester-pays for the target bucket.
    #[arg(long, env, default_value_t = DEFAULT_REQUEST_PAYER, help_heading = "AWS")]
    pub target_request_payer: bool,

    /// Disable stalled stream protection.
    #[arg(long, env, default_value_t = DEFAULT_DISABLE_STALLED_STREAM_PROTECTION, help_heading = "AWS")]
    pub disable_stalled_stream_protection: bool,

    // -----------------------------------------------------------------------
    // Lua options (same as s3sync)
    // -----------------------------------------------------------------------
    /// Path to the Lua script that is executed as event callback.
    #[cfg(feature = "lua_support")]
    #[arg(
        long,
        env,
        value_parser = NonEmptyStringValueParser::new(),
        help_heading = "Lua",
        long_help = r#"Path to the Lua script that is executed as event callback"#
    )]
    pub event_callback_lua_script: Option<String>,

    /// Allow Lua OS and I/O library functions in the Lua script.
    #[cfg(feature = "lua_support")]
    #[arg(
        long,
        env,
        conflicts_with_all = ["allow_lua_unsafe_vm"],
        default_value_t = DEFAULT_ALLOW_LUA_OS_LIBRARY,
        help_heading = "Lua",
        long_help = "Allow Lua OS and I/O library functions in the Lua script."
    )]
    pub allow_lua_os_library: bool,

    /// Allow unsafe Lua VM functions in the Lua script.
    #[cfg(feature = "lua_support")]
    #[arg(
        long,
        env,
        conflicts_with_all = ["allow_lua_os_library"],
        default_value_t = DEFAULT_ALLOW_LUA_UNSAFE_VM,
        help_heading = "Dangerous",
        long_help = r#"Allow unsafe Lua VM functions in the Lua script.
It allows the Lua script to load unsafe standard libraries or C modules."#
    )]
    pub allow_lua_unsafe_vm: bool,

    /// Lua VM memory limit.
    #[cfg(feature = "lua_support")]
    #[arg(
        long,
        env,
        default_value = DEFAULT_LUA_VM_MEMORY_LIMIT,
        value_parser = check_human_bytes,
        help_heading = "Lua",
        long_help = r#"Memory limit for the Lua VM. Allow suffixes: KB, KiB, MB, MiB, GB, GiB.
Zero means no limit.
If the memory limit is exceeded, the whole process will be terminated."#
    )]
    pub lua_vm_memory_limit: String,

    // -----------------------------------------------------------------------
    // Advanced options (same as s3sync)
    // -----------------------------------------------------------------------
    /// Enable If-Match conditional deletion using each object's own ETag.
    #[arg(long, env, default_value_t = DEFAULT_IF_MATCH, help_heading = "Advanced")]
    pub if_match: bool,

    /// Treat warnings as errors (exit code 1 instead of 3).
    #[arg(long, env, default_value_t = DEFAULT_WARN_AS_ERROR, help_heading = "Advanced")]
    pub warn_as_error: bool,

    /// Max keys per listing request. Default: 1000.
    #[arg(long, env, default_value_t = DEFAULT_MAX_KEYS, help_heading = "Advanced")]
    pub max_keys: i32,

    /// Generate shell completions.
    #[arg(long, env, help_heading = "Advanced")]
    pub auto_complete_shell: Option<clap_complete::shells::Shell>,
}

// ---------------------------------------------------------------------------
// parse_from_args (public API)
// ---------------------------------------------------------------------------

/// Parse command-line arguments into a `CLIArgs` struct.
///
/// This is the primary entry point for argument parsing, following s3sync's pattern.
///
/// # Example
///
/// ```
/// use s3rm_rs::config::args::parse_from_args;
///
/// let args = vec!["s3rm", "s3://my-bucket/prefix/", "--dry-run"];
/// let cli_args = parse_from_args(args).unwrap();
/// assert!(cli_args.dry_run);
/// ```
pub fn parse_from_args<I, T>(args: I) -> Result<CLIArgs, clap::Error>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    CLIArgs::try_parse_from(args)
}

/// Parse arguments and build a Config in one step.
///
/// Convenience function that combines `parse_from_args` and `Config::try_from`.
pub fn build_config_from_args<I, T>(args: I) -> Result<Config, String>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let cli_args = CLIArgs::try_parse_from(args).map_err(|e| e.to_string())?;
    Config::try_from(cli_args)
}

// ---------------------------------------------------------------------------
// Validation and Config conversion
// ---------------------------------------------------------------------------

impl CLIArgs {
    fn validate(&self) -> Result<(), String> {
        if self.worker_size == 0 {
            return Err(ERROR_MESSAGE_WORKER_SIZE_ZERO.to_string());
        }
        if self.batch_size == 0 {
            return Err(ERROR_MESSAGE_BATCH_SIZE_ZERO.to_string());
        }
        if self.batch_size > 1000 {
            return Err(ERROR_MESSAGE_BATCH_SIZE_TOO_LARGE.to_string());
        }
        if self.max_parallel_listings == 0 {
            return Err(ERROR_MESSAGE_MAX_PARALLEL_LISTINGS_ZERO.to_string());
        }
        if self.object_listing_queue_size == 0 {
            return Err(ERROR_MESSAGE_OBJECT_LISTING_QUEUE_SIZE_ZERO.to_string());
        }
        if self.max_parallel_listing_max_depth == 0 {
            return Err(ERROR_MESSAGE_MAX_PARALLEL_LISTING_MAX_DEPTH_ZERO.to_string());
        }
        Ok(())
    }

    fn build_filter_config(&self) -> Result<FilterConfig, String> {
        let compile_regex =
            |pattern: &Option<String>, name: &str| -> Result<Option<Regex>, String> {
                match pattern {
                    Some(p) => Regex::new(p)
                        .map(Some)
                        .map_err(|e| format!("{ERROR_MESSAGE_INVALID_REGEX} for {name}: {e}")),
                    None => Ok(None),
                }
            };

        Ok(FilterConfig {
            before_time: self.filter_mtime_before,
            after_time: self.filter_mtime_after,
            include_regex: compile_regex(&self.filter_include_regex, "filter-include-regex")?,
            exclude_regex: compile_regex(&self.filter_exclude_regex, "filter-exclude-regex")?,
            include_content_type_regex: compile_regex(
                &self.filter_include_content_type_regex,
                "filter-include-content-type-regex",
            )?,
            exclude_content_type_regex: compile_regex(
                &self.filter_exclude_content_type_regex,
                "filter-exclude-content-type-regex",
            )?,
            include_metadata_regex: compile_regex(
                &self.filter_include_metadata_regex,
                "filter-include-metadata-regex",
            )?,
            exclude_metadata_regex: compile_regex(
                &self.filter_exclude_metadata_regex,
                "filter-exclude-metadata-regex",
            )?,
            include_tag_regex: compile_regex(
                &self.filter_include_tag_regex,
                "filter-include-tag-regex",
            )?,
            exclude_tag_regex: compile_regex(
                &self.filter_exclude_tag_regex,
                "filter-exclude-tag-regex",
            )?,
            larger_size: self
                .filter_larger_size
                .as_deref()
                .map(|s| parse_human_bytes(s).unwrap() as u64),
            smaller_size: self
                .filter_smaller_size
                .as_deref()
                .map(|s| parse_human_bytes(s).unwrap() as u64),
        })
    }

    fn build_client_config(&self) -> Option<ClientConfig> {
        let credential = if let Some(ref profile) = self.target_profile {
            S3Credentials::Profile(profile.clone())
        } else if let Some(ref access_key) = self.target_access_key {
            let secret_key = self.target_secret_key.clone().unwrap_or_default();
            S3Credentials::Credentials {
                access_keys: AccessKeys {
                    access_key: access_key.clone(),
                    secret_access_key: secret_key,
                    session_token: self.target_session_token.clone(),
                },
            }
        } else {
            S3Credentials::FromEnvironment
        };

        let request_payer = if self.target_request_payer {
            Some(RequestPayer::Requester)
        } else {
            None
        };

        Some(ClientConfig {
            client_config_location: ClientConfigLocation {
                aws_config_file: self.aws_config_file.clone(),
                aws_shared_credentials_file: self.aws_shared_credentials_file.clone(),
            },
            credential,
            region: self.target_region.clone(),
            endpoint_url: self.target_endpoint_url.clone(),
            force_path_style: self.target_force_path_style,
            accelerate: self.target_accelerate,
            request_payer,
            retry_config: RetryConfig {
                aws_max_attempts: self.aws_max_attempts,
                initial_backoff_milliseconds: self.initial_backoff_milliseconds,
            },
            cli_timeout_config: CLITimeoutConfig {
                operation_timeout_milliseconds: self.operation_timeout_milliseconds,
                operation_attempt_timeout_milliseconds: self.operation_attempt_timeout_milliseconds,
                connect_timeout_milliseconds: self.connect_timeout_milliseconds,
                read_timeout_milliseconds: self.read_timeout_milliseconds,
            },
            disable_stalled_stream_protection: self.disable_stalled_stream_protection,
            request_checksum_calculation: RequestChecksumCalculation::WhenRequired,
            parallel_upload_semaphore: Arc::new(Semaphore::new(1)),
        })
    }

    fn build_tracing_config(&self) -> Option<TracingConfig> {
        let log_level = self.verbosity.log_level()?;

        Some(TracingConfig {
            tracing_level: log_level,
            json_tracing: self.json_tracing,
            aws_sdk_tracing: self.aws_sdk_tracing,
            span_events_tracing: self.span_events_tracing,
            disable_color_tracing: self.disable_color_tracing,
        })
    }

    fn parse_target(&self) -> Result<StoragePath, String> {
        let uri = &self.target;
        // Remove "s3://" prefix
        let without_scheme = &uri[5..];

        let (bucket, prefix) = match without_scheme.find('/') {
            Some(idx) => {
                let bucket = &without_scheme[..idx];
                let prefix = &without_scheme[idx + 1..];
                (bucket.to_string(), prefix.to_string())
            }
            None => (without_scheme.to_string(), String::new()),
        };

        if bucket.is_empty() {
            return Err(ERROR_MESSAGE_INVALID_TARGET.to_string());
        }

        Ok(StoragePath::S3 { bucket, prefix })
    }
}

impl TryFrom<CLIArgs> for Config {
    type Error = String;

    #[allow(clippy::needless_late_init)]
    fn try_from(args: CLIArgs) -> Result<Self, Self::Error> {
        args.validate()?;

        let target = args.parse_target()?;
        let filter_config = args.build_filter_config()?;
        let target_client_config = args.build_client_config();
        let tracing_config = args.build_tracing_config();

        // Express One Zone: default batch_size to 1 unless parallel listings allowed
        let mut batch_size = args.batch_size;
        let StoragePath::S3 { ref bucket, .. } = target;
        if is_express_onezone_storage(bucket) && !args.allow_parallel_listings_in_express_one_zone {
            batch_size = 1;
        }

        // Handle Lua script loading
        let filter_callback_lua_script: Option<String>;
        let event_callback_lua_script: Option<String>;
        let allow_lua_os_library: bool;
        let allow_lua_unsafe_vm: bool;
        let lua_vm_memory_limit: usize;

        cfg_if::cfg_if! {
            if #[cfg(feature = "lua_support")] {
                filter_callback_lua_script = if let Some(ref script) = args.filter_callback_lua_script {
                    if !PathBuf::from(script).exists() {
                        return Err(format!("Filter callback Lua script not found: {script}"));
                    }
                    Some(script.clone())
                } else {
                    None
                };
                event_callback_lua_script = if let Some(ref script) = args.event_callback_lua_script {
                    if !PathBuf::from(script).exists() {
                        return Err(format!("Event callback Lua script not found: {script}"));
                    }
                    Some(script.clone())
                } else {
                    None
                };
                allow_lua_os_library = args.allow_lua_os_library;
                allow_lua_unsafe_vm = args.allow_lua_unsafe_vm;
                lua_vm_memory_limit = parse_human_bytes(&args.lua_vm_memory_limit)
                    .map_err(|e| format!("Invalid lua-vm-memory-limit: {e}"))?;
            } else {
                filter_callback_lua_script = None;
                event_callback_lua_script = None;
                allow_lua_os_library = false;
                allow_lua_unsafe_vm = false;
                lua_vm_memory_limit = 64 * 1024 * 1024;
            }
        }

        // Load and validate Lua scripts if provided
        #[cfg(feature = "lua_support")]
        {
            if let Some(ref script_path) = filter_callback_lua_script {
                let engine = if allow_lua_unsafe_vm {
                    crate::lua::engine::LuaScriptCallbackEngine::unsafe_new(lua_vm_memory_limit)
                } else if allow_lua_os_library {
                    crate::lua::engine::LuaScriptCallbackEngine::new(lua_vm_memory_limit)
                } else {
                    crate::lua::engine::LuaScriptCallbackEngine::new_without_os_io_libs(
                        lua_vm_memory_limit,
                    )
                };
                let script_content = std::fs::read_to_string(script_path)
                    .map_err(|e| format!("Failed to read filter Lua script: {e}"))?;
                engine
                    .load_and_compile(&script_content)
                    .map_err(|e| format!("Failed to load filter Lua script: {e}"))?;
            }
            if let Some(ref script_path) = event_callback_lua_script {
                let engine = if allow_lua_unsafe_vm {
                    crate::lua::engine::LuaScriptCallbackEngine::unsafe_new(lua_vm_memory_limit)
                } else if allow_lua_os_library {
                    crate::lua::engine::LuaScriptCallbackEngine::new(lua_vm_memory_limit)
                } else {
                    crate::lua::engine::LuaScriptCallbackEngine::new_without_os_io_libs(
                        lua_vm_memory_limit,
                    )
                };
                let script_content = std::fs::read_to_string(script_path)
                    .map_err(|e| format!("Failed to read event Lua script: {e}"))?;
                engine
                    .load_and_compile(&script_content)
                    .map_err(|e| format!("Failed to load event Lua script: {e}"))?;
            }
        }

        Ok(Config {
            target,
            show_no_progress: args.show_no_progress,
            target_client_config,
            force_retry_config: ForceRetryConfig {
                force_retry_count: args.force_retry_count,
                force_retry_interval_milliseconds: args.force_retry_interval_milliseconds,
            },
            tracing_config,
            worker_size: args.worker_size,
            warn_as_error: args.warn_as_error,
            dry_run: args.dry_run,
            rate_limit_objects: args.rate_limit_objects,
            max_parallel_listings: args.max_parallel_listings,
            object_listing_queue_size: args.object_listing_queue_size,
            max_parallel_listing_max_depth: args.max_parallel_listing_max_depth,
            allow_parallel_listings_in_express_one_zone: args
                .allow_parallel_listings_in_express_one_zone,
            filter_config,
            max_keys: args.max_keys,
            auto_complete_shell: args.auto_complete_shell,
            event_callback_lua_script,
            filter_callback_lua_script,
            allow_lua_os_library,
            allow_lua_unsafe_vm,
            lua_vm_memory_limit,
            if_match: args.if_match,
            max_delete: args.max_delete,
            filter_manager: FilterManager::new(),
            event_manager: EventManager::new(),
            batch_size,
            delete_all_versions: args.delete_all_versions,
            force: args.force,
        })
    }
}

fn is_express_onezone_storage(bucket: &str) -> bool {
    bucket.ends_with(EXPRESS_ONEZONE_STORAGE_SUFFIX)
}
