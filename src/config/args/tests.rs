use super::*;
use crate::config::Config;
use proptest::prelude::*;

fn init_dummy_tracing_subscriber() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("dummy=trace")
        .try_init();
}

// ---------------------------------------------------------------------------
// Basic parsing tests
// ---------------------------------------------------------------------------

#[test]
fn parse_minimal_args() {
    init_dummy_tracing_subscriber();

    let args = vec!["s3rm", "s3://my-bucket/prefix/"];
    let cli = parse_from_args(args).unwrap();
    assert_eq!(cli.target, "s3://my-bucket/prefix/");
    assert!(!cli.dry_run);
    assert!(!cli.force);
}

#[test]
fn parse_dry_run_long() {
    let args = vec!["s3rm", "s3://bucket/", "--dry-run"];
    let cli = parse_from_args(args).unwrap();
    assert!(cli.dry_run);
}

#[test]
fn parse_dry_run_short() {
    let args = vec!["s3rm", "s3://bucket/", "-d"];
    let cli = parse_from_args(args).unwrap();
    assert!(cli.dry_run);
}

#[test]
fn parse_force_long() {
    let args = vec!["s3rm", "s3://bucket/", "--force"];
    let cli = parse_from_args(args).unwrap();
    assert!(cli.force);
}

#[test]
fn parse_force_short() {
    let args = vec!["s3rm", "s3://bucket/", "-f"];
    let cli = parse_from_args(args).unwrap();
    assert!(cli.force);
}

#[test]
fn parse_delete_all_versions() {
    let args = vec!["s3rm", "s3://bucket/", "--delete-all-versions"];
    let cli = parse_from_args(args).unwrap();
    assert!(cli.delete_all_versions);
}

#[test]
fn parse_max_delete() {
    let args = vec!["s3rm", "s3://bucket/", "--max-delete", "5000"];
    let cli = parse_from_args(args).unwrap();
    assert_eq!(cli.max_delete, Some(5000));
}

#[test]
fn parse_batch_size() {
    let args = vec!["s3rm", "s3://bucket/", "--batch-size", "500"];
    let cli = parse_from_args(args).unwrap();
    assert_eq!(cli.batch_size, 500);
}

#[test]
fn parse_worker_size() {
    let args = vec!["s3rm", "s3://bucket/", "--worker-size", "100"];
    let cli = parse_from_args(args).unwrap();
    assert_eq!(cli.worker_size, 100);
}

#[test]
fn parse_filter_options() {
    let args = vec![
        "s3rm",
        "s3://bucket/",
        "--filter-include-regex",
        ".*\\.log$",
        "--filter-exclude-regex",
        "^temp/",
        "--filter-smaller-size",
        "1024",
        "--filter-larger-size",
        "100",
    ];
    let cli = parse_from_args(args).unwrap();
    assert_eq!(cli.filter_include_regex.as_deref(), Some(".*\\.log$"));
    assert_eq!(cli.filter_exclude_regex.as_deref(), Some("^temp/"));
    assert_eq!(cli.filter_smaller_size.as_deref(), Some("1024"));
    assert_eq!(cli.filter_larger_size.as_deref(), Some("100"));
}

#[test]
fn parse_filter_size_human_readable() {
    let args = vec![
        "s3rm",
        "s3://bucket/",
        "--filter-smaller-size",
        "64MiB",
        "--filter-larger-size",
        "1GiB",
    ];
    let cli = parse_from_args(args).unwrap();
    let config = Config::try_from(cli).unwrap();
    assert_eq!(config.filter_config.smaller_size, Some(64 * 1024 * 1024));
    assert_eq!(config.filter_config.larger_size, Some(1024 * 1024 * 1024));
}

#[test]
fn parse_aws_config_options() {
    let args = vec![
        "s3rm",
        "s3://bucket/",
        "--target-profile",
        "prod",
        "--target-region",
        "us-west-2",
        "--target-endpoint-url",
        "https://minio.local:9000",
        "--target-force-path-style",
    ];
    let cli = parse_from_args(args).unwrap();
    assert_eq!(cli.target_profile.as_deref(), Some("prod"));
    assert_eq!(cli.target_region.as_deref(), Some("us-west-2"));
    assert_eq!(
        cli.target_endpoint_url.as_deref(),
        Some("https://minio.local:9000")
    );
    assert!(cli.target_force_path_style);
}

#[test]
fn parse_logging_options() {
    let args = vec![
        "s3rm",
        "s3://bucket/",
        "-vvv",
        "--json-tracing",
        "--disable-color-tracing",
    ];
    let cli = parse_from_args(args).unwrap();
    assert!(cli.json_tracing);
    assert!(cli.disable_color_tracing);
}

#[test]
fn parse_retry_options() {
    let args = vec![
        "s3rm",
        "s3://bucket/",
        "--aws-max-attempts",
        "5",
        "--force-retry-count",
        "3",
        "--initial-backoff-milliseconds",
        "200",
    ];
    let cli = parse_from_args(args).unwrap();
    assert_eq!(cli.aws_max_attempts, 5);
    assert_eq!(cli.force_retry_count, 3);
    assert_eq!(cli.initial_backoff_milliseconds, 200);
}

#[test]
fn parse_timeout_options() {
    let args = vec![
        "s3rm",
        "s3://bucket/",
        "--operation-timeout-milliseconds",
        "30000",
        "--connect-timeout-milliseconds",
        "5000",
    ];
    let cli = parse_from_args(args).unwrap();
    assert_eq!(cli.operation_timeout_milliseconds, Some(30000));
    assert_eq!(cli.connect_timeout_milliseconds, Some(5000));
}

#[test]
fn parse_advanced_options() {
    let args = vec![
        "s3rm",
        "s3://bucket/",
        "--if-match",
        "--warn-as-error",
        "--max-keys",
        "500",
    ];
    let cli = parse_from_args(args).unwrap();
    assert!(cli.if_match);
    assert!(cli.warn_as_error);
    assert_eq!(cli.max_keys, 500);
}

#[test]
fn parse_invalid_target_no_s3_prefix() {
    let args = vec!["s3rm", "my-bucket/prefix"];
    let result = parse_from_args(args);
    assert!(result.is_err());
}

#[test]
fn parse_missing_target() {
    let args = vec!["s3rm"];
    let result = parse_from_args(args);
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// Config::try_from tests
// ---------------------------------------------------------------------------

#[test]
fn config_from_minimal_args() {
    init_dummy_tracing_subscriber();

    let args = vec!["s3rm", "s3://my-bucket/prefix/"];
    let cli = parse_from_args(args).unwrap();
    let config = Config::try_from(cli).unwrap();

    let StoragePath::S3 { bucket, prefix } = &config.target;
    assert_eq!(bucket, "my-bucket");
    assert_eq!(prefix, "prefix/");
    assert!(!config.dry_run);
    assert!(!config.force);
    assert_eq!(config.batch_size, 1000);
    assert_eq!(config.worker_size, 16);
}

#[test]
fn config_from_full_args() {
    init_dummy_tracing_subscriber();

    let args = vec![
        "s3rm",
        "s3://bucket/logs/",
        "--dry-run",
        "--force",
        "--batch-size",
        "500",
        "--worker-size",
        "50",
        "--delete-all-versions",
        "--max-delete",
        "10000",
        "--if-match",
        "--filter-include-regex",
        ".*\\.log$",
    ];
    let cli = parse_from_args(args).unwrap();
    let config = Config::try_from(cli).unwrap();

    assert!(config.dry_run);
    assert!(config.force);
    assert_eq!(config.batch_size, 500);
    assert_eq!(config.worker_size, 50);
    assert!(config.delete_all_versions);
    assert_eq!(config.max_delete, Some(10000));
    assert!(config.if_match);
    assert!(config.filter_config.include_regex.is_some());
}

#[test]
fn config_validates_batch_size_zero() {
    let args = vec!["s3rm", "s3://bucket/", "--batch-size", "0"];
    let cli = parse_from_args(args).unwrap();
    let result = Config::try_from(cli);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .contains("Batch size must be at least 1")
    );
}

#[test]
fn config_validates_batch_size_too_large() {
    let args = vec!["s3rm", "s3://bucket/", "--batch-size", "1001"];
    let cli = parse_from_args(args).unwrap();
    let result = Config::try_from(cli);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .contains("Batch size must be at most 1000")
    );
}

#[test]
fn config_validates_worker_size_zero() {
    let args = vec!["s3rm", "s3://bucket/", "--worker-size", "0"];
    let cli = parse_from_args(args).unwrap();
    let result = Config::try_from(cli);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .contains("Worker size must be at least 1")
    );
}

#[test]
fn config_validates_invalid_regex() {
    let args = vec!["s3rm", "s3://bucket/", "--filter-include-regex", "[invalid"];
    let cli = parse_from_args(args).unwrap();
    let result = Config::try_from(cli);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Invalid regular expression"));
}

#[test]
fn config_express_onezone_defaults_batch_to_1() {
    let args = vec!["s3rm", "s3://my-bucket--x-s3/prefix/"];
    let cli = parse_from_args(args).unwrap();
    let config = Config::try_from(cli).unwrap();
    assert_eq!(config.batch_size, 1);
}

#[test]
fn config_express_onezone_with_parallel_listings_keeps_batch_size() {
    let args = vec![
        "s3rm",
        "s3://my-bucket--x-s3/prefix/",
        "--allow-parallel-listings-in-express-one-zone",
    ];
    let cli = parse_from_args(args).unwrap();
    let config = Config::try_from(cli).unwrap();
    assert_eq!(config.batch_size, 1000);
}

#[test]
fn config_tracing_config_none_when_silent() {
    let args = vec!["s3rm", "s3://bucket/", "-qq"];
    let cli = parse_from_args(args).unwrap();
    let config = Config::try_from(cli).unwrap();
    assert!(config.tracing_config.is_none());
}

#[test]
fn config_tracing_config_info_with_verbose() {
    // WarnLevel default: no flag → Warn, -v → Info, -vv → Debug, -vvv → Trace
    let args = vec!["s3rm", "s3://bucket/", "-v"];
    let cli = parse_from_args(args).unwrap();
    let config = Config::try_from(cli).unwrap();
    assert!(config.tracing_config.is_some());
    assert_eq!(
        config.tracing_config.unwrap().tracing_level,
        log::Level::Info
    );
}

#[test]
fn config_tracing_config_trace_with_very_verbose() {
    let args = vec!["s3rm", "s3://bucket/", "-vvv"];
    let cli = parse_from_args(args).unwrap();
    let config = Config::try_from(cli).unwrap();
    assert!(config.tracing_config.is_some());
    assert_eq!(
        config.tracing_config.unwrap().tracing_level,
        log::Level::Trace
    );
}

#[test]
fn config_target_client_config_from_profile() {
    let args = vec!["s3rm", "s3://bucket/", "--target-profile", "myprofile"];
    let cli = parse_from_args(args).unwrap();
    let config = Config::try_from(cli).unwrap();
    let client_config = config.target_client_config.unwrap();
    assert!(matches!(client_config.credential, S3Credentials::Profile(ref p) if p == "myprofile"));
}

#[test]
fn config_target_client_config_from_access_keys() {
    let args = vec![
        "s3rm",
        "s3://bucket/",
        "--target-access-key",
        "AKIA...",
        "--target-secret-key",
        "secret123",
    ];
    let cli = parse_from_args(args).unwrap();
    let config = Config::try_from(cli).unwrap();
    let client_config = config.target_client_config.unwrap();
    assert!(matches!(
        client_config.credential,
        S3Credentials::Credentials { .. }
    ));
}

#[test]
fn config_target_client_config_from_environment() {
    let args = vec!["s3rm", "s3://bucket/"];
    let cli = parse_from_args(args).unwrap();
    let config = Config::try_from(cli).unwrap();
    let client_config = config.target_client_config.unwrap();
    assert!(matches!(
        client_config.credential,
        S3Credentials::FromEnvironment
    ));
}

#[test]
fn build_config_from_args_convenience() {
    init_dummy_tracing_subscriber();

    let args = vec!["s3rm", "s3://bucket/prefix/", "--dry-run", "--force"];
    let config = build_config_from_args(args).unwrap();
    assert!(config.dry_run);
    assert!(config.force);
}

#[test]
fn build_config_from_args_error() {
    let args = vec!["s3rm"];
    let result = build_config_from_args(args);
    assert!(result.is_err());
}

#[test]
fn parse_human_bytes_mib() {
    assert_eq!(parse_human_bytes("64MiB").unwrap(), 64 * 1024 * 1024);
}

#[test]
fn parse_human_bytes_gib() {
    assert_eq!(parse_human_bytes("1GiB").unwrap(), 1024 * 1024 * 1024);
}

#[test]
fn parse_human_bytes_kib() {
    assert_eq!(parse_human_bytes("512KiB").unwrap(), 512 * 1024);
}

#[test]
fn parse_human_bytes_plain() {
    assert_eq!(parse_human_bytes("1000").unwrap(), 1000);
}

#[test]
fn parse_human_bytes_invalid() {
    assert!(parse_human_bytes("abc").is_err());
}

// ---------------------------------------------------------------------------
// Property tests
// ---------------------------------------------------------------------------

// Feature: s3rm-rs, Property 33: Configuration Precedence
// **Validates: Requirements 8.1, 8.2, 8.3, 8.5**
proptest! {
    #![proptest_config(ProptestConfig {
        cases: 100,
        .. ProptestConfig::default()
    })]

    #[test]
    fn test_configuration_precedence(
        worker_size in 1u16..=100,
        batch_size in 1u16..=1000,
    ) {
        // CLI args should always take precedence.
        let args = vec![
            "s3rm".to_string(),
            "s3://bucket/".to_string(),
            "--worker-size".to_string(),
            worker_size.to_string(),
            "--batch-size".to_string(),
            batch_size.to_string(),
        ];
        let cli = parse_from_args(args).unwrap();
        let config = Config::try_from(cli).unwrap();
        prop_assert_eq!(config.worker_size, worker_size);
        prop_assert_eq!(config.batch_size, batch_size);
    }
}

// Feature: s3rm-rs, Property 38: Input Validation and Error Messages
// **Validates: Requirements 10.2**
proptest! {
    #![proptest_config(ProptestConfig {
        cases: 100,
        .. ProptestConfig::default()
    })]

    #[test]
    fn test_input_validation_rejects_invalid_targets(
        bad_prefix in "[a-z]{1,10}",
    ) {
        // Non-s3:// targets should be rejected
        let args = vec!["s3rm".to_string(), bad_prefix.clone()];
        let result = parse_from_args(args);
        prop_assert!(result.is_err());
    }

    #[test]
    fn test_input_validation_accepts_valid_targets(
        bucket in "[a-z][a-z0-9\\-]{2,10}",
        prefix in "[a-z0-9/]{0,20}",
    ) {
        let target = format!("s3://{bucket}/{prefix}");
        let args = vec!["s3rm".to_string(), target];
        let result = parse_from_args(args);
        prop_assert!(result.is_ok());
    }
}

// Feature: s3rm-rs, Property 39: Flag Alias Support
// **Validates: Requirements 10.4**
proptest! {
    #![proptest_config(ProptestConfig {
        cases: 50,
        .. ProptestConfig::default()
    })]

    #[test]
    fn test_flag_alias_dry_run(use_short in proptest::bool::ANY) {
        let flag = if use_short { "-d" } else { "--dry-run" };
        let args = vec!["s3rm", "s3://bucket/", flag];
        let cli = parse_from_args(args).unwrap();
        prop_assert!(cli.dry_run);
    }

    #[test]
    fn test_flag_alias_force(use_short in proptest::bool::ANY) {
        let flag = if use_short { "-f" } else { "--force" };
        let args = vec!["s3rm", "s3://bucket/", flag];
        let cli = parse_from_args(args).unwrap();
        prop_assert!(cli.force);
    }
}

// Feature: s3rm-rs, Property 40: Exit Code Mapping
// **Validates: Requirements 10.5, 13.4**
proptest! {
    #![proptest_config(ProptestConfig {
        cases: 50,
        .. ProptestConfig::default()
    })]

    #[test]
    fn test_exit_code_mapping_cancelled(_dummy in 0..1i32) {
        use crate::types::error::S3rmError;
        let err = anyhow::anyhow!(S3rmError::Cancelled);
        prop_assert_eq!(crate::types::error::exit_code_from_error(&err), 0);
    }

    #[test]
    fn test_exit_code_mapping_invalid_config(msg in "[a-z ]{1,20}") {
        use crate::types::error::S3rmError;
        let err = anyhow::anyhow!(S3rmError::InvalidConfig(msg));
        prop_assert_eq!(crate::types::error::exit_code_from_error(&err), 2);
    }

    #[test]
    fn test_exit_code_mapping_partial_failure(
        deleted in 0u64..1000,
        failed in 1u64..1000,
    ) {
        use crate::types::error::S3rmError;
        let err = anyhow::anyhow!(S3rmError::PartialFailure { deleted, failed });
        prop_assert_eq!(crate::types::error::exit_code_from_error(&err), 3);
    }
}
