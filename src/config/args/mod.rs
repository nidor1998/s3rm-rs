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

mod value_parser;

#[cfg(test)]
mod tests;

// ---------------------------------------------------------------------------
// Default constants (aligned with s3sync)
// ---------------------------------------------------------------------------

const EXPRESS_ONEZONE_STORAGE_SUFFIX: &str = "--x-s3";

const DEFAULT_WORKER_SIZE: u16 = 24;
const DEFAULT_BATCH_SIZE: u16 = 200;
const DEFAULT_AWS_MAX_ATTEMPTS: u32 = 10;
const DEFAULT_FORCE_RETRY_COUNT: u32 = 0;
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

const ERROR_MESSAGE_INVALID_TARGET: &str = "target must be an S3 path (e.g. s3://bucket/prefix)";

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
    let v = value_parser::human_bytes::parse_human_bytes(s)?;
    usize::try_from(v).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// CLIArgs (clap-derived argument struct)
// ---------------------------------------------------------------------------

/// s3rm - Fast Amazon S3 object deletion tool.
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
    /// List objects that would be deleted without actually deleting them
    #[arg(short = 'd', long, env, default_value_t = DEFAULT_DRY_RUN, help_heading = "General")]
    pub dry_run: bool,

    /// Skip the confirmation prompt before deleting
    #[arg(short = 'f', long, env, default_value_t = DEFAULT_FORCE, help_heading = "General")]
    pub force: bool,

    /// Hide the progress bar
    #[arg(long, env, default_value_t = DEFAULT_SHOW_NO_PROGRESS, help_heading = "General")]
    pub show_no_progress: bool,

    /// Delete all versions of matching objects, including delete markers
    #[arg(long, env, default_value_t = DEFAULT_DELETE_ALL_VERSIONS, help_heading = "General")]
    pub delete_all_versions: bool,

    /// Objects per batch deletion request (1-1000; 1 uses single-object deletion)
    #[arg(long, env, default_value_t = DEFAULT_BATCH_SIZE, value_parser = clap::value_parser!(u16).range(1..=1000), help_heading = "General")]
    pub batch_size: u16,

    /// Stop deleting after this many objects have been deleted
    #[arg(long, env, value_parser = clap::value_parser!(u64).range(1..), help_heading = "General")]
    pub max_delete: Option<u64>,

    // -----------------------------------------------------------------------
    // Filter options (same as s3sync)
    // -----------------------------------------------------------------------
    /// Delete only objects whose key matches this regex
    #[arg(long, env, value_parser = value_parser::regex::parse_regex, help_heading = "Filtering")]
    pub filter_include_regex: Option<String>,

    /// Skip objects whose key matches this regex
    #[arg(long, env, value_parser = value_parser::regex::parse_regex, help_heading = "Filtering")]
    pub filter_exclude_regex: Option<String>,

    /// Delete only objects whose content type matches this regex
    #[arg(long, env, value_parser = value_parser::regex::parse_regex, help_heading = "Filtering")]
    pub filter_include_content_type_regex: Option<String>,

    /// Skip objects whose content type matches this regex
    #[arg(long, env, value_parser = value_parser::regex::parse_regex, help_heading = "Filtering")]
    pub filter_exclude_content_type_regex: Option<String>,

    /// Delete only objects whose user-defined metadata matches this regex
    #[arg(long, env, value_parser = value_parser::regex::parse_regex, help_heading = "Filtering",
        long_help = r#"Delete only objects whose user-defined metadata matches this regular expression.
Keys (lowercase) must be sorted alphabetically and separated by commas.
This filter is applied after all other filters except tag filters.
May require an extra API call per object to retrieve metadata.

Example: "key1=(value1|value2),key2=value2""#)]
    pub filter_include_metadata_regex: Option<String>,

    /// Skip objects whose user-defined metadata matches this regex
    #[arg(long, env, value_parser = value_parser::regex::parse_regex, help_heading = "Filtering",
        long_help = r#"Skip objects whose user-defined metadata matches this regular expression.
Keys (lowercase) must be sorted alphabetically and separated by commas.
This filter is applied after all other filters except tag filters.
May require an extra API call per object to retrieve metadata.

Example: "key1=(value1|value2),key2=value2""#)]
    pub filter_exclude_metadata_regex: Option<String>,

    /// Delete only objects whose tags match this regex
    #[arg(long, env, value_parser = value_parser::regex::parse_regex, help_heading = "Filtering",
        long_help = r#"Delete only objects whose tags match this regular expression.
Keys must be sorted alphabetically and separated by '&'.
This filter is applied after all other filters.
Requires an extra API call per object to retrieve tags.

Example: "key1=(value1|value2)&key2=value2""#)]
    pub filter_include_tag_regex: Option<String>,

    /// Skip objects whose tags match this regex
    #[arg(long, env, value_parser = value_parser::regex::parse_regex, help_heading = "Filtering",
        long_help = r#"Skip objects whose tags match this regular expression.
Keys must be sorted alphabetically and separated by '&'.
This filter is applied after all other filters.
Requires an extra API call per object to retrieve tags.

Example: "key1=(value1|value2)&key2=value2""#)]
    pub filter_exclude_tag_regex: Option<String>,

    /// Delete only objects modified before this time
    #[arg(
        long,
        env,
        help_heading = "Filtering",
        long_help = r#"Delete only objects older than the given time (RFC 3339 format).
Example: 2023-02-19T12:00:00Z"#
    )]
    pub filter_mtime_before: Option<DateTime<Utc>>,

    /// Delete only objects modified at or after this time
    #[arg(
        long,
        env,
        help_heading = "Filtering",
        long_help = r#"Delete only objects newer than or equal to the given time (RFC 3339 format).
Example: 2023-02-19T12:00:00Z"#
    )]
    pub filter_mtime_after: Option<DateTime<Utc>>,

    /// Delete only objects smaller than this size
    #[arg(
        long,
        env,
        value_parser = value_parser::human_bytes::check_human_bytes,
        help_heading = "Filtering",
        long_help = r#"Delete only objects smaller than the given size.
Supported suffixes: KB, KiB, MB, MiB, GB, GiB, TB, TiB"#
    )]
    pub filter_smaller_size: Option<String>,

    /// Delete only objects larger than or equal to this size
    #[arg(
        long,
        env,
        value_parser = value_parser::human_bytes::check_human_bytes,
        help_heading = "Filtering",
        long_help = r#"Delete only objects larger than or equal to the given size.
Supported suffixes: KB, KiB, MB, MiB, GB, GiB, TB, TiB"#
    )]
    pub filter_larger_size: Option<String>,

    // -----------------------------------------------------------------------
    // Tracing/Logging options (same as s3sync)
    // -----------------------------------------------------------------------
    /// Verbosity level (-q quiet, default normal, -v, -vv, -vvv)
    #[command(flatten)]
    pub verbosity: Verbosity<WarnLevel>,

    /// Output structured logs in JSON format (requires --force)
    #[arg(long, env, default_value_t = DEFAULT_JSON_TRACING, requires = "force", help_heading = "Tracing/Logging")]
    pub json_tracing: bool,

    /// Include AWS SDK internal traces in log output
    #[arg(long, env, default_value_t = DEFAULT_AWS_SDK_TRACING, help_heading = "Tracing/Logging")]
    pub aws_sdk_tracing: bool,

    /// Include span open/close events in log output
    #[arg(long, env, default_value_t = DEFAULT_SPAN_EVENTS_TRACING, help_heading = "Tracing/Logging")]
    pub span_events_tracing: bool,

    /// Disable colored output in logs
    #[arg(long, env, default_value_t = DEFAULT_DISABLE_COLOR_TRACING, help_heading = "Tracing/Logging")]
    pub disable_color_tracing: bool,

    // -----------------------------------------------------------------------
    // AWS configuration (target-only, adapted from s3sync)
    // -----------------------------------------------------------------------
    /// Path to the AWS config file
    #[arg(long, env, help_heading = "AWS Configuration")]
    pub aws_config_file: Option<PathBuf>,

    /// Path to the AWS shared credentials file
    #[arg(long, env, help_heading = "AWS Configuration")]
    pub aws_shared_credentials_file: Option<PathBuf>,

    /// Target AWS CLI profile
    #[arg(long, env, conflicts_with_all = ["target_access_key", "target_secret_access_key", "target_session_token"], value_parser = NonEmptyStringValueParser::new(), help_heading = "AWS Configuration")]
    pub target_profile: Option<String>,

    /// Target access key
    #[arg(long, env, conflicts_with_all = ["target_profile"], requires = "target_secret_access_key", value_parser = NonEmptyStringValueParser::new(), help_heading = "AWS Configuration")]
    pub target_access_key: Option<String>,

    /// Target secret access key
    #[arg(long, env, conflicts_with_all = ["target_profile"], requires = "target_access_key", value_parser = NonEmptyStringValueParser::new(), help_heading = "AWS Configuration")]
    pub target_secret_access_key: Option<String>,

    /// Target session token
    #[arg(long, env, conflicts_with_all = ["target_profile"], requires = "target_access_key", value_parser = NonEmptyStringValueParser::new(), help_heading = "AWS Configuration")]
    pub target_session_token: Option<String>,

    /// AWS region for the target
    #[arg(long, env, value_parser = NonEmptyStringValueParser::new(), help_heading = "AWS Configuration")]
    pub target_region: Option<String>,

    /// Custom S3-compatible endpoint URL (e.g. MinIO, Wasabi)
    #[arg(long, env, value_parser = value_parser::url::check_scheme, help_heading = "AWS Configuration")]
    pub target_endpoint_url: Option<String>,

    /// Use path-style access (required by some S3-compatible services)
    #[arg(long, env, default_value_t = DEFAULT_FORCE_PATH_STYLE, help_heading = "AWS Configuration")]
    pub target_force_path_style: bool,

    /// Enable S3 Transfer Acceleration
    #[arg(long, env, default_value_t = DEFAULT_ACCELERATE, help_heading = "AWS Configuration")]
    pub target_accelerate: bool,

    /// Enable requester-pays for the target bucket
    #[arg(long, env, default_value_t = DEFAULT_REQUEST_PAYER, help_heading = "AWS Configuration")]
    pub target_request_payer: bool,

    /// Disable stalled stream protection
    #[arg(long, env, default_value_t = DEFAULT_DISABLE_STALLED_STREAM_PROTECTION, help_heading = "AWS Configuration")]
    pub disable_stalled_stream_protection: bool,

    // -----------------------------------------------------------------------
    // Performance options (same as s3sync)
    // -----------------------------------------------------------------------
    /// Number of concurrent deletion workers (1-65535)
    #[arg(long, env, default_value_t = DEFAULT_WORKER_SIZE, value_parser = clap::value_parser!(u16).range(1..), help_heading = "Performance")]
    pub worker_size: u16,

    /// Number of concurrent listing operations
    #[arg(long, env, default_value_t = DEFAULT_MAX_PARALLEL_LISTINGS, value_parser = clap::value_parser!(u16).range(1..), help_heading = "Performance")]
    pub max_parallel_listings: u16,

    /// Maximum depth for parallel listing operations
    #[arg(long, env, default_value_t = DEFAULT_PARALLEL_LISTING_MAX_DEPTH, value_parser = clap::value_parser!(u16).range(1..), help_heading = "Performance")]
    pub max_parallel_listing_max_depth: u16,

    /// Maximum objects per second (rate limiting)
    #[arg(long, env, value_parser = clap::value_parser!(u32).range(10..), help_heading = "Performance")]
    pub rate_limit_objects: Option<u32>,

    /// Internal queue size for object listing
    #[arg(long, env, default_value_t = DEFAULT_OBJECT_LISTING_QUEUE_SIZE, value_parser = clap::value_parser!(u32).range(1..), help_heading = "Performance")]
    pub object_listing_queue_size: u32,

    /// Allow parallel listings in Express One Zone storage
    #[arg(long, env, default_value_t = DEFAULT_ALLOW_PARALLEL_LISTINGS_IN_EXPRESS_ONE_ZONE, help_heading = "Performance")]
    pub allow_parallel_listings_in_express_one_zone: bool,

    // -----------------------------------------------------------------------
    // Retry options (same as s3sync)
    // -----------------------------------------------------------------------
    /// Maximum retry attempts for AWS SDK operations
    #[arg(long, env, default_value_t = DEFAULT_AWS_MAX_ATTEMPTS, help_heading = "Retry Options")]
    pub aws_max_attempts: u32,

    /// Initial backoff in milliseconds for retries
    #[arg(long, env, default_value_t = DEFAULT_INITIAL_BACKOFF_MILLISECONDS, help_heading = "Retry Options")]
    pub initial_backoff_milliseconds: u64,

    /// Number of application-level retries after SDK retries are exhausted
    #[arg(long, env, default_value_t = DEFAULT_FORCE_RETRY_COUNT, help_heading = "Retry Options")]
    pub force_retry_count: u32,

    /// Interval in milliseconds between application-level retries
    #[arg(long, env, default_value_t = DEFAULT_FORCE_RETRY_INTERVAL_MILLISECONDS, help_heading = "Retry Options")]
    pub force_retry_interval_milliseconds: u64,

    // -----------------------------------------------------------------------
    // Timeout options (same as s3sync)
    // -----------------------------------------------------------------------
    /// Overall operation timeout in milliseconds
    #[arg(long, env, help_heading = "Timeout Options")]
    pub operation_timeout_milliseconds: Option<u64>,

    /// Per-attempt operation timeout in milliseconds
    #[arg(long, env, help_heading = "Timeout Options")]
    pub operation_attempt_timeout_milliseconds: Option<u64>,

    /// Connection timeout in milliseconds
    #[arg(long, env, help_heading = "Timeout Options")]
    pub connect_timeout_milliseconds: Option<u64>,

    /// Read timeout in milliseconds
    #[arg(long, env, help_heading = "Timeout Options")]
    pub read_timeout_milliseconds: Option<u64>,

    // -----------------------------------------------------------------------
    // Lua scripting support (same as s3sync)
    // -----------------------------------------------------------------------
    /// Path to a Lua filter callback script
    #[cfg(feature = "lua_support")]
    #[arg(
        long,
        env,
        value_parser = value_parser::file_exist::is_file_exist,
        help_heading = "Lua scripting support",
        long_help = "Path to a Lua script used as a filter callback.\nThe script is called for each object and must return true to delete the object."
    )]
    pub filter_callback_lua_script: Option<String>,

    /// Path to a Lua event callback script
    #[cfg(feature = "lua_support")]
    #[arg(
        long,
        env,
        value_parser = value_parser::file_exist::is_file_exist,
        help_heading = "Lua scripting support",
        long_help = "Path to a Lua script used as an event callback.\nThe script receives deletion events such as progress, errors, and completion."
    )]
    pub event_callback_lua_script: Option<String>,

    /// Allow Lua OS and I/O library access in the Lua script
    #[cfg(feature = "lua_support")]
    #[arg(
        long,
        env,
        conflicts_with_all = ["allow_lua_unsafe_vm"],
        default_value_t = DEFAULT_ALLOW_LUA_OS_LIBRARY,
        help_heading = "Lua scripting support",
        long_help = "Allow Lua OS and I/O library access in the Lua script"
    )]
    pub allow_lua_os_library: bool,

    /// Memory limit for the Lua VM
    #[cfg(feature = "lua_support")]
    #[arg(
        long,
        env,
        default_value = DEFAULT_LUA_VM_MEMORY_LIMIT,
        value_parser = value_parser::human_bytes::check_human_bytes,
        help_heading = "Lua scripting support",
        long_help = "Memory limit for the Lua VM.\nSupported suffixes: KB, KiB, MB, MiB, GB, GiB.\nSet to 0 for no limit. Exceeding this limit terminates the process."
    )]
    pub lua_vm_memory_limit: String,

    // -----------------------------------------------------------------------
    // Advanced options (same as s3sync)
    // -----------------------------------------------------------------------
    /// Use ETag-based conditional deletion to prevent race conditions
    #[arg(long, env, default_value_t = DEFAULT_IF_MATCH, help_heading = "Advanced")]
    pub if_match: bool,

    /// Treat warnings as errors (exit code 1 instead of 3)
    #[arg(long, env, default_value_t = DEFAULT_WARN_AS_ERROR, help_heading = "Advanced")]
    pub warn_as_error: bool,

    /// Maximum number of objects returned in a single list object request
    #[arg(long, env, default_value_t = DEFAULT_MAX_KEYS, value_parser = clap::value_parser!(i32).range(1..=32767), help_heading = "Advanced")]
    pub max_keys: i32,

    /// Generate shell completions for the given shell
    #[arg(long, env, help_heading = "Advanced")]
    pub auto_complete_shell: Option<clap_complete::shells::Shell>,

    // -----------------------------------------------------------------------
    // Dangerous options
    // -----------------------------------------------------------------------
    /// Allow loading unsafe Lua standard libraries and C modules
    #[cfg(feature = "lua_support")]
    #[arg(
        long,
        env,
        conflicts_with_all = ["allow_lua_os_library"],
        default_value_t = DEFAULT_ALLOW_LUA_UNSAFE_VM,
        help_heading = "Dangerous",
        long_help = "Allow loading unsafe Lua standard libraries and C modules.\nThis removes all sandbox restrictions from the Lua VM."
    )]
    pub allow_lua_unsafe_vm: bool,
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
    fn build_filter_config(&self) -> FilterConfig {
        // value_parser already validated regexes at parse time, so unwrap is safe
        let compile_regex = |pattern: &Option<String>| -> Option<Regex> {
            pattern.as_ref().map(|p| Regex::new(p).unwrap())
        };

        FilterConfig {
            before_time: self.filter_mtime_before,
            after_time: self.filter_mtime_after,
            include_regex: compile_regex(&self.filter_include_regex),
            exclude_regex: compile_regex(&self.filter_exclude_regex),
            include_content_type_regex: compile_regex(&self.filter_include_content_type_regex),
            exclude_content_type_regex: compile_regex(&self.filter_exclude_content_type_regex),
            include_metadata_regex: compile_regex(&self.filter_include_metadata_regex),
            exclude_metadata_regex: compile_regex(&self.filter_exclude_metadata_regex),
            include_tag_regex: compile_regex(&self.filter_include_tag_regex),
            exclude_tag_regex: compile_regex(&self.filter_exclude_tag_regex),
            larger_size: self
                .filter_larger_size
                .as_deref()
                .map(|s| parse_human_bytes(s).unwrap() as u64),
            smaller_size: self
                .filter_smaller_size
                .as_deref()
                .map(|s| parse_human_bytes(s).unwrap() as u64),
        }
    }

    fn build_client_config(&self) -> Option<ClientConfig> {
        let credential = if let Some(ref profile) = self.target_profile {
            S3Credentials::Profile(profile.clone())
        } else if let Some(ref access_key) = self.target_access_key {
            let secret_key = self.target_secret_access_key.clone().unwrap_or_default();
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
        })
    }

    fn build_tracing_config(&self, dry_run: bool) -> Option<TracingConfig> {
        let mut tracing_config = self.verbosity.log_level().map(|log_level| TracingConfig {
            tracing_level: log_level,
            json_tracing: self.json_tracing,
            aws_sdk_tracing: self.aws_sdk_tracing,
            span_events_tracing: self.span_events_tracing,
            disable_color_tracing: self.disable_color_tracing,
        });

        if dry_run {
            if tracing_config.is_none() {
                tracing_config = Some(TracingConfig {
                    tracing_level: log::Level::Info,
                    json_tracing: DEFAULT_JSON_TRACING,
                    aws_sdk_tracing: DEFAULT_AWS_SDK_TRACING,
                    span_events_tracing: DEFAULT_SPAN_EVENTS_TRACING,
                    disable_color_tracing: DEFAULT_DISABLE_COLOR_TRACING,
                });
            } else if tracing_config.unwrap().tracing_level < log::Level::Info {
                tracing_config = Some(TracingConfig {
                    tracing_level: log::Level::Info,
                    json_tracing: tracing_config.unwrap().json_tracing,
                    aws_sdk_tracing: tracing_config.unwrap().aws_sdk_tracing,
                    span_events_tracing: tracing_config.unwrap().span_events_tracing,
                    disable_color_tracing: tracing_config.unwrap().disable_color_tracing,
                });
            }
        }

        tracing_config
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
        let target = args.parse_target()?;
        let filter_config = args.build_filter_config();
        let target_client_config = args.build_client_config();
        let tracing_config = args.build_tracing_config(args.dry_run);

        // Express One Zone: override batch_size to 1 unless parallel listings allowed
        let mut batch_size = args.batch_size;
        let StoragePath::S3 { ref bucket, .. } = target;
        if is_express_onezone_storage(bucket) && !args.allow_parallel_listings_in_express_one_zone {
            if batch_size != DEFAULT_BATCH_SIZE {
                tracing::warn!(
                    "--batch-size={} is overridden to 1 for Express One Zone storage. \
                     Use --allow-parallel-listings-in-express-one-zone to keep the specified value.",
                    batch_size,
                );
            }
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
                filter_callback_lua_script = args.filter_callback_lua_script.clone();
                event_callback_lua_script = args.event_callback_lua_script.clone();
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

        // Build callback managers and register Lua callbacks (like s3sync)
        #[allow(unused_mut)]
        let mut filter_manager = FilterManager::new();
        cfg_if::cfg_if! {
            if #[cfg(feature = "lua_support")] {
                if let Some(ref script_path) = filter_callback_lua_script {
                    let mut lua_filter_callback =
                        crate::lua::filter::LuaFilterCallback::new(
                            lua_vm_memory_limit,
                            allow_lua_os_library,
                            allow_lua_unsafe_vm,
                        );
                    lua_filter_callback
                        .load_and_compile(script_path.as_str())
                        .map_err(|e| format!("Failed to load filter Lua script: {e}"))?;
                    filter_manager.register_callback(lua_filter_callback);
                }
            }
        }

        #[allow(unused_mut)]
        let mut event_manager = EventManager::new();
        cfg_if::cfg_if! {
            if #[cfg(feature = "lua_support")] {
                if let Some(ref script_path) = event_callback_lua_script {
                    let mut lua_event_callback =
                        crate::lua::event::LuaEventCallback::new(
                            lua_vm_memory_limit,
                            allow_lua_os_library,
                            allow_lua_unsafe_vm,
                        );
                    lua_event_callback
                        .load_and_compile(script_path.as_str())
                        .map_err(|e| format!("Failed to load event Lua script: {e}"))?;
                    event_manager.register_callback(
                        crate::types::event_callback::EventType::ALL_EVENTS,
                        lua_event_callback,
                        args.dry_run,
                    );
                }
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
            filter_manager,
            event_manager,
            batch_size,
            delete_all_versions: args.delete_all_versions,
            force: args.force,
            test_user_defined_callback: false,
        })
    }
}

fn is_express_onezone_storage(bucket: &str) -> bool {
    bucket.ends_with(EXPRESS_ONEZONE_STORAGE_SUFFIX)
}
