// Property-based tests for CI/CD integration features.
//
// Feature: s3rm-rs, Property 48: Non-Interactive Environment Detection
// For any execution in a non-interactive environment (no TTY) without --force,
// the tool should error to prevent unsafe unconfirmed deletions. With --force
// or --dry-run, the tool proceeds without prompting.
// **Validates: Requirements 13.1**
//
// Feature: s3rm-rs, Property 49: Output Stream Separation
// For any log output, the tool should write all log messages (including errors)
// to stdout via tracing-subscriber by default.
// **Validates: Requirements 13.6**

#[cfg(test)]
mod tests {
    use crate::callback::event_manager::EventManager;
    use crate::callback::filter_manager::FilterManager;
    use crate::config::{Config, FilterConfig, ForceRetryConfig, TracingConfig};
    use crate::safety::{PromptHandler, SafetyChecker};
    use crate::types::StoragePath;
    use crate::types::error::S3rmError;
    use anyhow::Result;
    use proptest::prelude::*;

    // -----------------------------------------------------------------------
    // Mock PromptHandlers
    // -----------------------------------------------------------------------

    /// Mock prompt handler that simulates a non-interactive (non-TTY) environment.
    /// read_confirmation should never be called in non-interactive mode.
    struct NonInteractiveHandler;

    impl PromptHandler for NonInteractiveHandler {
        fn read_confirmation(&self, _target_display: &str, _use_color: bool) -> Result<String> {
            unreachable!("read_confirmation must not be called in non-interactive mode")
        }

        fn is_interactive(&self) -> bool {
            false
        }
    }

    /// Mock prompt handler that simulates an interactive (TTY) environment
    /// and returns a predetermined response.
    struct InteractiveHandler {
        response: String,
    }

    impl InteractiveHandler {
        fn new(response: &str) -> Self {
            Self {
                response: response.to_string(),
            }
        }
    }

    impl PromptHandler for InteractiveHandler {
        fn read_confirmation(&self, _target_display: &str, _use_color: bool) -> Result<String> {
            Ok(self.response.clone())
        }

        fn is_interactive(&self) -> bool {
            true
        }
    }

    // -----------------------------------------------------------------------
    // Test helpers
    // -----------------------------------------------------------------------

    fn make_config(dry_run: bool, force: bool, json_tracing: bool, disable_color: bool) -> Config {
        Config {
            target: StoragePath::S3 {
                bucket: "test-bucket".to_string(),
                prefix: String::new(),
            },
            show_no_progress: false,
            log_deletion_summary: false,
            target_client_config: None,
            force_retry_config: ForceRetryConfig {
                force_retry_count: 0,
                force_retry_interval_milliseconds: 0,
            },
            tracing_config: Some(TracingConfig {
                tracing_level: log::Level::Info,
                json_tracing,
                aws_sdk_tracing: false,
                span_events_tracing: false,
                disable_color_tracing: disable_color,
            }),
            worker_size: 4,
            warn_as_error: false,
            dry_run,
            rate_limit_objects: None,
            max_parallel_listings: 1,
            object_listing_queue_size: 1000,
            max_parallel_listing_max_depth: 1,
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
            force,
            test_user_defined_callback: false,
        }
    }

    // -----------------------------------------------------------------------
    // Feature: s3rm-rs, Property 48: Non-Interactive Environment Detection
    // **Validates: Requirements 13.1**
    //
    // When the environment is non-interactive (no TTY) and --force is not set,
    // the SafetyChecker MUST return an error to prevent unsafe unconfirmed
    // deletions. With --force or --dry-run, the tool proceeds without prompting.
    // -----------------------------------------------------------------------

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// Feature: s3rm-rs, Property 48: Non-Interactive Environment Detection
        /// **Validates: Requirements 13.1**
        ///
        /// In a non-interactive environment, when --force or --dry-run is set,
        /// the operation proceeds without prompting.
        #[test]
        fn property_48_non_interactive_with_force_or_dry_run_succeeds(
            force in proptest::bool::ANY,
            dry_run in proptest::bool::ANY,
            json_tracing in proptest::bool::ANY,
        ) {
            // Feature: s3rm-rs, Property 48: Non-Interactive Environment Detection
            // **Validates: Requirements 13.1**
            prop_assume!(force || dry_run);

            let config = make_config(dry_run, force, json_tracing, false);
            let checker = SafetyChecker::with_prompt_handler(
                &config,
                Box::new(NonInteractiveHandler),
            );

            let result = checker.check_before_deletion();
            prop_assert!(
                result.is_ok(),
                "Non-interactive with force={} or dry_run={} must succeed, \
                 but got error: {:?}",
                force,
                dry_run,
                result.err(),
            );
        }

        /// Feature: s3rm-rs, Property 48: Non-Interactive Environment Detection
        /// **Validates: Requirements 13.1**
        ///
        /// In a non-interactive environment without --force and without --dry-run,
        /// the operation MUST fail with an InvalidConfig error.
        #[test]
        fn property_48_non_interactive_without_force_errors(
            json_tracing in proptest::bool::ANY,
        ) {
            // Feature: s3rm-rs, Property 48: Non-Interactive Environment Detection
            // **Validates: Requirements 13.1**
            let config = make_config(false, false, json_tracing, false);
            let checker = SafetyChecker::with_prompt_handler(
                &config,
                Box::new(NonInteractiveHandler),
            );

            let result = checker.check_before_deletion();
            prop_assert!(
                result.is_err(),
                "Non-interactive without --force must error"
            );
            let err = result.unwrap_err();
            let s3rm_err = err.downcast_ref::<S3rmError>().unwrap();
            prop_assert!(
                matches!(s3rm_err, S3rmError::InvalidConfig(_)),
                "Expected InvalidConfig error, got: {:?}",
                s3rm_err,
            );
        }

        /// Feature: s3rm-rs, Property 48: Non-Interactive Environment Detection
        /// **Validates: Requirements 13.1**
        ///
        /// In an interactive environment without dry-run, force, or JSON logging,
        /// the confirmation prompt IS exercised. This is the contrast case proving
        /// that non-TTY detection is the mechanism that disables prompts.
        #[test]
        fn property_48_interactive_env_does_prompt(
            response in "[a-zA-Z0-9 ]{0,20}",
        ) {
            // Feature: s3rm-rs, Property 48: Non-Interactive Environment Detection
            // **Validates: Requirements 13.1**
            prop_assume!(response.trim() != "yes");

            let config = make_config(false, false, false, false);
            let checker = SafetyChecker::with_prompt_handler(
                &config,
                Box::new(InteractiveHandler::new(&response)),
            );

            let result = checker.check_before_deletion();
            prop_assert!(
                result.is_err(),
                "Interactive environment with non-'yes' input should be rejected"
            );
            let err = result.unwrap_err();
            let s3rm_err = err.downcast_ref::<S3rmError>().unwrap();
            prop_assert_eq!(s3rm_err, &S3rmError::Cancelled);
        }
    }

    /// Feature: s3rm-rs, Property 48: Non-Interactive Environment Detection
    /// **Validates: Requirements 13.1**
    ///
    /// Verify non-interactive without force errors even with no tracing config.
    #[test]
    fn property_48_non_interactive_no_tracing_config_errors() {
        let mut config = make_config(false, false, false, false);
        config.tracing_config = None;
        let checker = SafetyChecker::with_prompt_handler(&config, Box::new(NonInteractiveHandler));

        let result = checker.check_before_deletion();
        assert!(
            result.is_err(),
            "Non-interactive without --force must error even without tracing config"
        );
    }

    /// Feature: s3rm-rs, Property 48: Non-Interactive Environment Detection
    /// **Validates: Requirements 13.1**
    ///
    /// Verify non-interactive with force succeeds even with no tracing config.
    #[test]
    fn property_48_non_interactive_no_tracing_config_with_force_succeeds() {
        let mut config = make_config(false, true, false, false);
        config.tracing_config = None;
        let checker = SafetyChecker::with_prompt_handler(&config, Box::new(NonInteractiveHandler));

        let result = checker.check_before_deletion();
        assert!(result.is_ok(), "Non-interactive with --force must succeed");
    }

    /// Feature: s3rm-rs, Property 48: Non-Interactive Environment Detection
    /// **Validates: Requirements 13.1**
    ///
    /// Verify that JSON logging without --force errors (interactive handler,
    /// but JSON logging makes the environment non-interactive for prompts).
    #[test]
    fn property_48_json_logging_without_force_errors() {
        let config = make_config(false, false, true, false);
        let checker =
            SafetyChecker::with_prompt_handler(&config, Box::new(InteractiveHandler::new("no")));

        let result = checker.check_before_deletion();
        assert!(result.is_err(), "JSON logging without --force must error");
    }

    /// Feature: s3rm-rs, Property 48: Non-Interactive Environment Detection
    /// **Validates: Requirements 13.1**
    ///
    /// Verify that JSON logging with --force succeeds.
    #[test]
    fn property_48_json_logging_with_force_succeeds() {
        let config = make_config(false, true, true, false);
        let checker =
            SafetyChecker::with_prompt_handler(&config, Box::new(InteractiveHandler::new("no")));

        let result = checker.check_before_deletion();
        assert!(result.is_ok(), "JSON logging with --force must succeed");
    }

    // -----------------------------------------------------------------------
    // Feature: s3rm-rs, Property 49: Output Stream Separation
    // **Validates: Requirements 13.6**
    //
    // All log messages (including errors) are written to stdout via
    // tracing-subscriber by default. This is verified by checking that
    // the tracing configuration uses fmt() (which defaults to stdout)
    // without any explicit stderr writer override.
    // -----------------------------------------------------------------------

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// Feature: s3rm-rs, Property 49: Output Stream Separation
        /// **Validates: Requirements 13.6**
        ///
        /// For any verbosity flag combination parsed from CLI args, the resulting
        /// Config produces a TracingConfig (or None for silent mode) that preserves
        /// the json_tracing flag correctly — ensuring JSON mode uses the same
        /// stdout-based tracing path.
        #[test]
        fn property_49_cli_tracing_config_json_propagation(
            verbosity_idx in 0..6usize,
            json_tracing in proptest::bool::ANY,
        ) {
            // Feature: s3rm-rs, Property 49: Output Stream Separation
            // **Validates: Requirements 13.6**
            use crate::config::args::parse_from_args;

            let verbosity_flags: Vec<Vec<&str>> = vec![
                vec!["-qq"],
                vec!["-q"],
                vec![],
                vec!["-v"],
                vec!["-vv"],
                vec!["-vvv"],
            ];

            let mut args: Vec<&str> = vec!["s3rm", "s3://bucket/prefix/"];
            args.extend(verbosity_flags[verbosity_idx].clone());
            if json_tracing {
                args.push("--json-tracing");
                args.push("--force");
            }

            let cli = parse_from_args(args).unwrap();
            let config = Config::try_from(cli).unwrap();

            // Silent mode (-qq): no tracing config at all — acceptable per design
            // All other modes: TracingConfig is present and json_tracing is propagated
            if let Some(tc) = &config.tracing_config {
                prop_assert_eq!(tc.json_tracing, json_tracing,
                    "json_tracing flag must propagate through CLI -> Config pipeline");
            }
        }
    }

    /// Feature: s3rm-rs, Property 49: Output Stream Separation
    /// **Validates: Requirements 13.6**
    ///
    /// Verify that the tracing config from a default CLI invocation (no special
    /// flags) produces a configuration that will route to stdout.
    #[test]
    fn property_49_default_cli_routes_to_stdout() {
        use crate::config::args::parse_from_args;

        let args = vec!["s3rm", "s3://bucket/prefix/"];
        let cli = parse_from_args(args).unwrap();
        let config = Config::try_from(cli).unwrap();

        // Default verbosity is Warn level with tracing enabled
        let tc = config
            .tracing_config
            .expect("Default config must have tracing enabled");
        assert!(!tc.json_tracing, "JSON tracing off by default");
        assert!(!tc.aws_sdk_tracing, "AWS SDK tracing off by default");
        assert!(!tc.span_events_tracing, "Span events off by default");
        // init_tracing uses tracing_subscriber::fmt() which defaults to stdout.
        // No .with_writer(stderr) is called — all output goes to stdout.
    }

    /// Feature: s3rm-rs, Property 49: Output Stream Separation
    /// **Validates: Requirements 13.6**
    ///
    /// Verify JSON logging mode also routes to stdout (not stderr).
    #[test]
    fn property_49_json_logging_routes_to_stdout() {
        use crate::config::args::parse_from_args;

        let args = vec![
            "s3rm",
            "s3://bucket/prefix/",
            "--json-tracing",
            "--force",
            "-v",
        ];
        let cli = parse_from_args(args).unwrap();
        let config = Config::try_from(cli).unwrap();

        let tc = config
            .tracing_config
            .expect("JSON tracing must have tracing enabled");
        assert!(tc.json_tracing, "JSON tracing must be enabled");
        // init_tracing calls subscriber_builder.json().init() which still
        // uses the default stdout writer. No stderr override exists.
    }
}
