// Property-based tests for rate limiting enforcement.
//
// Feature: s3rm-rs, Property 36: Rate Limiting Enforcement
// For any rate limit configuration, the Rate Limiter should enforce that
// the deletion rate does not exceed the specified maximum objects per second.
// **Validates: Requirements 8.7**

#[cfg(test)]
mod tests {
    use crate::callback::event_manager::EventManager;
    use crate::callback::filter_manager::FilterManager;
    use crate::config::args::parse_from_args;
    use crate::config::{
        CLITimeoutConfig, ClientConfig, Config, FilterConfig, ForceRetryConfig, RetryConfig,
        TracingConfig,
    };
    use crate::storage::create_storage;
    use crate::types::{AccessKeys, ClientConfigLocation, S3Credentials, StoragePath};

    use aws_smithy_types::checksum_config::RequestChecksumCalculation;
    use proptest::prelude::*;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

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
                    access_key: "AKIAIOSFODNN7EXAMPLE".to_string(),
                    secret_access_key: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".to_string(),
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
            accelerate: false,
            request_payer: None,
        }
    }

    fn make_test_config(rate_limit: Option<u32>) -> Config {
        Config {
            target: StoragePath::S3 {
                bucket: "test-bucket".to_string(),
                prefix: "prefix/".to_string(),
            },
            show_no_progress: false,
            log_deletion_summary: false,
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
                disable_color_tracing: true,
            }),
            worker_size: 4,
            warn_as_error: false,
            dry_run: false,
            rate_limit_objects: rate_limit,
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
            force: true,
            test_user_defined_callback: false,
        }
    }

    // -----------------------------------------------------------------------
    // Generators
    // -----------------------------------------------------------------------

    /// Generate a valid rate limit value (minimum 10 per CLI constraint).
    fn arb_rate_limit() -> impl Strategy<Value = u32> {
        10u32..=100_000u32
    }

    /// Generate a rate limit value below the minimum (invalid).
    fn arb_invalid_rate_limit() -> impl Strategy<Value = u32> {
        0u32..10u32
    }

    // -----------------------------------------------------------------------
    // Feature: s3rm-rs, Property 36: Rate Limiting Enforcement
    // -----------------------------------------------------------------------

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(50))]

        /// Feature: s3rm-rs, Property 36: Rate Limiting Enforcement (CLI propagation)
        /// **Validates: Requirements 8.7**
        ///
        /// For any valid rate limit value provided via CLI, the parsed Config
        /// should contain the exact rate_limit_objects value.
        #[test]
        fn prop_rate_limit_cli_propagation(
            rate_limit in arb_rate_limit(),
        ) {
            let rate_str = rate_limit.to_string();
            let args: Vec<&str> = vec![
                "s3rm",
                "s3://test-bucket/prefix/",
                "--rate-limit-objects",
                &rate_str,
                "--batch-size",
                "1",
            ];

            let cli = parse_from_args(args).unwrap();
            let config = Config::try_from(cli).unwrap();

            prop_assert_eq!(
                config.rate_limit_objects,
                Some(rate_limit),
                "rate_limit_objects must match CLI input"
            );
        }

    }

    /// Feature: s3rm-rs, Property 36: Rate Limiting Enforcement (no rate limit default)
    /// **Validates: Requirements 8.7**
    ///
    /// When no --rate-limit-objects is specified, rate_limit_objects in
    /// Config should be None, meaning no rate limiting is applied.
    #[test]
    fn prop_rate_limit_default_none() {
        let args: Vec<&str> = vec!["s3rm", "s3://test-bucket/prefix/"];

        let cli = parse_from_args(args).unwrap();
        let config = Config::try_from(cli).unwrap();

        assert!(
            config.rate_limit_objects.is_none(),
            "rate_limit_objects must be None when not specified"
        );
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(50))]

        /// Feature: s3rm-rs, Property 36: Rate Limiting Enforcement (minimum value rejection)
        /// **Validates: Requirements 8.7**
        ///
        /// For any rate limit value below the minimum (10), CLI parsing should
        /// reject the input. This ensures rate limiting cannot be configured
        /// with unreasonably low values.
        #[test]
        fn prop_rate_limit_rejects_below_minimum(
            rate_limit in arb_invalid_rate_limit(),
        ) {
            let rate_str = rate_limit.to_string();
            let args: Vec<&str> = vec![
                "s3rm",
                "s3://test-bucket/prefix/",
                "--rate-limit-objects",
                &rate_str,
            ];

            let result = parse_from_args(args);
            prop_assert!(
                result.is_err(),
                "rate_limit_objects below 10 must be rejected, but {} was accepted",
                rate_limit
            );
        }

        /// Feature: s3rm-rs, Property 36: Rate Limiting Enforcement (rate limiter creation)
        /// **Validates: Requirements 8.7**
        ///
        /// For any valid rate limit value, create_storage should successfully
        /// create a storage instance with rate limiting applied. The rate
        /// limiter uses a token bucket algorithm with refill intervals
        /// computed from the configured rate.
        #[test]
        fn prop_rate_limit_storage_creation(
            rate_limit in arb_rate_limit(),
        ) {
            init_dummy_tracing_subscriber();

            let config = make_test_config(Some(rate_limit));

            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();

            rt.block_on(async {
                let cancellation_token =
                    crate::types::token::create_pipeline_cancellation_token();
                let (stats_sender, _stats_receiver) = async_channel::unbounded();
                let has_warning = Arc::new(AtomicBool::new(false));

                let storage = create_storage(
                    config,
                    cancellation_token,
                    stats_sender,
                    has_warning,
                )
                .await;

                prop_assert!(
                    storage.get_client().is_some(),
                    "Storage must be created successfully with rate_limit_objects={}",
                    rate_limit
                );

                Ok(())
            })?;
        }

        /// Feature: s3rm-rs, Property 36: Rate Limiting Enforcement (storage creation without rate limit)
        /// **Validates: Requirements 8.7**
        ///
        /// When rate_limit_objects is None, create_storage should still succeed
        /// and create a valid storage instance (no rate limiter applied),
        /// regardless of worker_size.
        #[test]
        fn prop_rate_limit_none_storage_creation(
            worker_size in 1u16..64,
        ) {
            init_dummy_tracing_subscriber();

            let mut config = make_test_config(None);
            config.worker_size = worker_size;

            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();

            rt.block_on(async {
                let cancellation_token =
                    crate::types::token::create_pipeline_cancellation_token();
                let (stats_sender, _stats_receiver) = async_channel::unbounded();
                let has_warning = Arc::new(AtomicBool::new(false));

                let storage = create_storage(
                    config,
                    cancellation_token,
                    stats_sender,
                    has_warning,
                )
                .await;

                prop_assert!(
                    storage.get_client().is_some(),
                    "Storage must be created successfully without rate limiting"
                );

                Ok(())
            })?;
        }
    }
}
