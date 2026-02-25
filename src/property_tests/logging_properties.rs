// Property-based tests for the logging and verbosity system.
//
// Feature: s3rm-rs, Property 21: Verbosity Level Configuration
// For any verbosity flag (-v, -vv, -vvv, -q), the tool should accept the flag
// and configure the appropriate logging level.
// **Validates: Requirements 4.1**
//
// Feature: s3rm-rs, Property 22: JSON Logging Format
// For any log output with JSON logging enabled, all logs should be valid JSON objects.
// **Validates: Requirements 4.7, 13.3**
//
// Feature: s3rm-rs, Property 23: Color Output Control
// For any execution environment, color output should be enabled by default in TTY
// environments, disabled in non-TTY environments unless explicitly enabled, and
// disabled when the disable-color flag is set.
// **Validates: Requirements 4.8, 4.9, 7.5, 13.7**
//
// Feature: s3rm-rs, Property 24: Error Logging
// For any deletion failure, the tool should log the error message and error code
// at the current verbosity level.
// **Validates: Requirements 4.10**

#[cfg(test)]
mod tests {
    use crate::config::Config;
    use crate::config::args::parse_from_args;
    use crate::types::DeletionError;
    use proptest::prelude::*;

    // -----------------------------------------------------------------------
    // Generators
    // -----------------------------------------------------------------------

    /// Generate a random verbosity flag set for the CLI.
    ///
    /// clap_verbosity_flag with WarnLevel default maps as follows:
    ///   -qq     → log_level() = None  (two below Warn → off)
    ///   -q      → log_level() = Error (one below Warn)
    ///   (none)  → log_level() = Warn  (default)
    ///   -v      → log_level() = Info
    ///   -vv     → log_level() = Debug
    ///   -vvv    → log_level() = Trace
    fn arb_verbosity_flags() -> impl Strategy<Value = (Vec<&'static str>, Option<log::Level>)> {
        prop_oneof![
            Just((vec!["-qq"], None)),
            Just((vec!["-q"], Some(log::Level::Error))),
            Just((vec![], Some(log::Level::Warn))),
            Just((vec!["-v"], Some(log::Level::Info))),
            Just((vec!["-vv"], Some(log::Level::Debug))),
            Just((vec!["-vvv"], Some(log::Level::Trace))),
        ]
    }

    /// Generate a random DeletionError variant.
    fn arb_deletion_error() -> impl Strategy<Value = DeletionError> {
        prop_oneof![
            Just(DeletionError::NotFound),
            Just(DeletionError::AccessDenied),
            Just(DeletionError::PreconditionFailed),
            Just(DeletionError::Throttled),
            "[a-z]{1,20}".prop_map(DeletionError::NetworkError),
            "[a-z]{1,20}".prop_map(DeletionError::ServiceError),
        ]
    }

    // -----------------------------------------------------------------------
    // Feature: s3rm-rs, Property 21: Verbosity Level Configuration
    // -----------------------------------------------------------------------

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// Feature: s3rm-rs, Property 21: Verbosity Level Configuration
        /// **Validates: Requirements 4.1**
        ///
        /// For any verbosity flag combination, the parsed Config should have the
        /// corresponding tracing level set. Silent mode (-qq) disables tracing
        /// entirely (tracing_config = None).
        #[test]
        fn prop_verbosity_level_configuration(
            (flags, expected_level) in arb_verbosity_flags(),
        ) {
            let mut args: Vec<&str> = vec!["s3rm", "s3://bucket/prefix/"];
            args.extend(flags);

            let cli = parse_from_args(args).unwrap();
            let config = Config::try_from(cli).unwrap();

            match expected_level {
                None => {
                    prop_assert!(
                        config.tracing_config.is_none(),
                        "Expected no tracing config for silent mode"
                    );
                }
                Some(level) => {
                    prop_assert!(
                        config.tracing_config.is_some(),
                        "Expected tracing config to be present"
                    );
                    let tracing_config = config.tracing_config.unwrap();
                    prop_assert_eq!(tracing_config.tracing_level, level);
                }
            }
        }

        /// Feature: s3rm-rs, Property 21: Verbosity Level Configuration (dry-run boost)
        /// **Validates: Requirements 4.1, 3.1**
        ///
        /// When dry-run mode is enabled, the default level (Warn) is boosted to Info
        /// so dry-run output is visible. But explicit -q flags are respected — the
        /// user's quiet preference takes priority over dry-run convenience.
        #[test]
        fn prop_verbosity_dry_run_boost(
            (flags, expected_level) in arb_verbosity_flags(),
        ) {
            let mut args: Vec<&str> = vec!["s3rm", "s3://bucket/prefix/", "--dry-run"];
            args.extend(flags);

            let cli = parse_from_args(args).unwrap();
            let config = Config::try_from(cli).unwrap();

            match expected_level {
                // Silent (-qq): user explicitly requested no output, dry-run respects that
                None => {
                    prop_assert!(
                        config.tracing_config.is_none(),
                        "Dry-run must respect explicit silent mode (-qq)"
                    );
                }
                // Error (-q): user explicitly asked for errors only, dry-run respects that
                Some(log::Level::Error) => {
                    prop_assert!(config.tracing_config.is_some());
                    prop_assert_eq!(config.tracing_config.unwrap().tracing_level, log::Level::Error);
                }
                // Warn (default, no flags): dry-run boosts to Info for visibility
                Some(log::Level::Warn) => {
                    prop_assert!(config.tracing_config.is_some());
                    prop_assert_eq!(config.tracing_config.unwrap().tracing_level, log::Level::Info);
                }
                // Info, Debug, Trace (-v/-vv/-vvv): already >= Info, kept as-is
                Some(level) => {
                    prop_assert!(config.tracing_config.is_some());
                    prop_assert_eq!(config.tracing_config.unwrap().tracing_level, level);
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Feature: s3rm-rs, Property 22: JSON Logging Format
    // -----------------------------------------------------------------------

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// Feature: s3rm-rs, Property 22: JSON Logging Format
        /// **Validates: Requirements 4.7, 13.3**
        ///
        /// When --json-tracing is enabled, the TracingConfig must have
        /// json_tracing=true regardless of other flags.
        #[test]
        fn prop_json_logging_format(
            (flags, _) in arb_verbosity_flags(),
            disable_color in proptest::bool::ANY,
            aws_sdk_tracing in proptest::bool::ANY,
        ) {
            let mut args: Vec<&str> = vec!["s3rm", "s3://bucket/prefix/", "--json-tracing", "--force"];
            args.extend(flags.clone());
            if disable_color {
                args.push("--disable-color-tracing");
            }
            if aws_sdk_tracing {
                args.push("--aws-sdk-tracing");
            }

            let cli = parse_from_args(args).unwrap();
            let config = Config::try_from(cli).unwrap();

            if config.tracing_config.is_some() {
                let tracing_config = config.tracing_config.unwrap();
                prop_assert!(
                    tracing_config.json_tracing,
                    "json_tracing must be true when --json-tracing is passed"
                );
            }
            // If tracing_config is None (silent mode -qq), JSON flag has no
            // effect since there's no tracing at all — this is acceptable.
        }

        /// Feature: s3rm-rs, Property 22: JSON Logging Format (default off)
        /// **Validates: Requirements 4.7**
        ///
        /// Without --json-tracing, json_tracing must be false.
        #[test]
        fn prop_json_logging_default_off(
            (flags, _) in arb_verbosity_flags(),
        ) {
            let mut args: Vec<&str> = vec!["s3rm", "s3://bucket/prefix/"];
            args.extend(flags);

            let cli = parse_from_args(args).unwrap();
            let config = Config::try_from(cli).unwrap();

            if let Some(tracing_config) = config.tracing_config {
                prop_assert!(
                    !tracing_config.json_tracing,
                    "json_tracing must default to false"
                );
            }
        }
    }

    // -----------------------------------------------------------------------
    // Feature: s3rm-rs, Property 23: Color Output Control
    // -----------------------------------------------------------------------

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// Feature: s3rm-rs, Property 23: Color Output Control
        /// **Validates: Requirements 4.8, 4.9, 7.5, 13.7**
        ///
        /// When --disable-color-tracing is set, the TracingConfig must have
        /// disable_color_tracing=true. When not set, it must be false (default).
        #[test]
        fn prop_color_output_control(
            (flags, _) in arb_verbosity_flags(),
            disable_color in proptest::bool::ANY,
        ) {
            let mut args: Vec<&str> = vec!["s3rm", "s3://bucket/prefix/"];
            args.extend(flags);
            if disable_color {
                args.push("--disable-color-tracing");
            }

            let cli = parse_from_args(args).unwrap();
            let config = Config::try_from(cli).unwrap();

            if let Some(tracing_config) = config.tracing_config {
                prop_assert_eq!(
                    tracing_config.disable_color_tracing,
                    disable_color,
                    "disable_color_tracing should match CLI flag"
                );
            }
        }

        /// Feature: s3rm-rs, Property 23: Color Output Control (disable-color flag with verbosity)
        /// **Validates: Requirements 4.8, 4.9**
        ///
        /// The --disable-color-tracing flag must propagate correctly through the
        /// CLI parser for all verbosity levels.
        #[test]
        fn prop_color_config_propagation_with_verbosity(
            (flags, _) in arb_verbosity_flags(),
            json_tracing in proptest::bool::ANY,
        ) {
            // With color disabled
            let mut args_disabled: Vec<&str> = vec![
                "s3rm", "s3://bucket/prefix/",
                "--disable-color-tracing",
            ];
            args_disabled.extend(flags.clone());
            if json_tracing {
                args_disabled.push("--json-tracing");
                args_disabled.push("--force");
            }

            let cli_disabled = parse_from_args(args_disabled).unwrap();
            let config_disabled = Config::try_from(cli_disabled).unwrap();

            // With color enabled (default)
            let mut args_enabled: Vec<&str> = vec!["s3rm", "s3://bucket/prefix/"];
            args_enabled.extend(flags);
            if json_tracing {
                args_enabled.push("--json-tracing");
                args_enabled.push("--force");
            }

            let cli_enabled = parse_from_args(args_enabled).unwrap();
            let config_enabled = Config::try_from(cli_enabled).unwrap();

            // Color disabled: disable_color_tracing must be true
            if let Some(tc) = config_disabled.tracing_config {
                prop_assert!(tc.disable_color_tracing, "disable_color_tracing must be true when --disable-color-tracing is set");
            }
            // Color enabled (default): disable_color_tracing must be false
            if let Some(tc) = config_enabled.tracing_config {
                prop_assert!(!tc.disable_color_tracing, "disable_color_tracing must be false by default");
            }
        }
    }

    // -----------------------------------------------------------------------
    // Feature: s3rm-rs, Property 24: Error Logging
    // -----------------------------------------------------------------------

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// Feature: s3rm-rs, Property 24: Error Logging
        /// **Validates: Requirements 4.10**
        ///
        /// For any DeletionError variant, the Display impl produces a non-empty
        /// error message. This message is what the tracing macros (error!, warn!)
        /// emit when logging deletion failures.
        #[test]
        fn prop_deletion_error_has_display_message(
            error in arb_deletion_error(),
        ) {
            let message = error.to_string();
            prop_assert!(!message.is_empty(), "Error message must not be empty");
        }

        /// Feature: s3rm-rs, Property 24: Error Logging (retryable classification)
        /// **Validates: Requirements 4.10, 6.1**
        ///
        /// For any DeletionError, the is_retryable() method returns consistent
        /// results: only Throttled, NetworkError, and ServiceError are retryable.
        #[test]
        fn prop_deletion_error_retryable_consistency(
            error in arb_deletion_error(),
        ) {
            let retryable = error.is_retryable();
            match &error {
                DeletionError::Throttled
                | DeletionError::NetworkError(_)
                | DeletionError::ServiceError(_) => {
                    prop_assert!(retryable, "Transient errors must be retryable");
                }
                DeletionError::NotFound
                | DeletionError::AccessDenied
                | DeletionError::PreconditionFailed => {
                    prop_assert!(!retryable, "Permanent errors must not be retryable");
                }
            }
        }

        /// Feature: s3rm-rs, Property 24: Error Logging (error code in display)
        /// **Validates: Requirements 4.10**
        ///
        /// For DeletionError variants that wrap a message (NetworkError,
        /// ServiceError), the Display output includes the original error message.
        /// For fixed variants, the Display output contains a descriptive string.
        #[test]
        fn prop_deletion_error_includes_context(
            error in arb_deletion_error(),
        ) {
            let message = error.to_string();
            match &error {
                DeletionError::NotFound => {
                    prop_assert!(message.contains("not found") || message.contains("Not found"));
                }
                DeletionError::AccessDenied => {
                    prop_assert!(message.to_lowercase().contains("denied"));
                }
                DeletionError::PreconditionFailed => {
                    prop_assert!(message.to_lowercase().contains("precondition") || message.to_lowercase().contains("etag"));
                }
                DeletionError::Throttled => {
                    prop_assert!(message.to_lowercase().contains("throttl"));
                }
                DeletionError::NetworkError(msg) => {
                    prop_assert!(
                        message.contains(msg),
                        "NetworkError display should include the original message"
                    );
                }
                DeletionError::ServiceError(msg) => {
                    prop_assert!(
                        message.contains(msg),
                        "ServiceError display should include the original message"
                    );
                }
            }
        }
    }
}
