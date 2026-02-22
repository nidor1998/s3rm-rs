//! Property-based tests for safety features.
//!
//! Tests Properties 16-18 from the design document.

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

    use std::sync::Mutex;

    // -----------------------------------------------------------------------
    // Mock PromptHandler for testing
    // -----------------------------------------------------------------------

    /// Mock prompt handler that returns a predetermined response.
    /// Always reports as interactive so that prompt logic is exercised.
    struct MockPromptHandler {
        response: String,
    }

    impl MockPromptHandler {
        fn new(response: &str) -> Self {
            Self {
                response: response.to_string(),
            }
        }
    }

    impl PromptHandler for MockPromptHandler {
        fn read_confirmation(&self, _target_display: &str, _use_color: bool) -> Result<String> {
            Ok(self.response.clone())
        }

        fn is_interactive(&self) -> bool {
            true
        }
    }

    /// Capturing mock that records the `target_display` and `use_color` arguments
    /// passed to `read_confirmation`. Used for Property 19 testing.
    struct CapturingPromptHandler {
        response: String,
        captured_target: Mutex<Option<String>>,
        captured_use_color: Mutex<Option<bool>>,
    }

    impl CapturingPromptHandler {
        fn new(response: &str) -> Self {
            Self {
                response: response.to_string(),
                captured_target: Mutex::new(None),
                captured_use_color: Mutex::new(None),
            }
        }
    }

    impl PromptHandler for CapturingPromptHandler {
        fn read_confirmation(&self, target_display: &str, use_color: bool) -> Result<String> {
            *self.captured_target.lock().unwrap() = Some(target_display.to_string());
            *self.captured_use_color.lock().unwrap() = Some(use_color);
            Ok(self.response.clone())
        }

        fn is_interactive(&self) -> bool {
            true
        }
    }

    /// Mock prompt handler that always reports as non-interactive.
    struct NonInteractivePromptHandler;

    impl PromptHandler for NonInteractivePromptHandler {
        fn read_confirmation(&self, _target_display: &str, _use_color: bool) -> Result<String> {
            unreachable!("should not be called in non-interactive mode")
        }

        fn is_interactive(&self) -> bool {
            false
        }
    }

    // -----------------------------------------------------------------------
    // Test helpers
    // -----------------------------------------------------------------------

    fn make_config_with_target(
        dry_run: bool,
        force: bool,
        json_tracing: bool,
        disable_color: bool,
        bucket: &str,
        prefix: &str,
    ) -> Config {
        Config {
            target: StoragePath::S3 {
                bucket: bucket.to_string(),
                prefix: prefix.to_string(),
            },
            show_no_progress: false,
            report_deletion_status: false,
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

    fn make_config(dry_run: bool, force: bool, json_tracing: bool) -> Config {
        Config {
            target: StoragePath::S3 {
                bucket: "test-bucket".to_string(),
                prefix: String::new(),
            },
            show_no_progress: false,
            report_deletion_status: false,
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
                disable_color_tracing: false,
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
    // Feature: s3rm-rs, Property 16: Dry-Run Mode Safety
    // **Validates: Requirements 3.1**
    //
    // In dry-run mode, the pipeline runs fully (listing, filtering) but
    // the deletion layer simulates deletions. The SafetyChecker skips the
    // confirmation prompt because no destructive operation will occur.
    // -----------------------------------------------------------------------

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn property_16_dry_run_skips_confirmation(
            force in proptest::bool::ANY,
        ) {
            // Feature: s3rm-rs, Property 16: Dry-Run Mode Safety
            // **Validates: Requirements 3.1**
            //
            // Regardless of force flag, if dry_run is true,
            // check_before_deletion MUST return Ok (no confirmation needed —
            // deletions will be simulated).
            let config = make_config(true, force, false);
            // Use "no" response to prove the prompt is never consulted.
            let handler = MockPromptHandler::new("no");
            let checker = SafetyChecker::with_prompt_handler(&config, Box::new(handler));

            let result = checker.check_before_deletion();
            prop_assert!(result.is_ok(), "dry-run should skip confirmation and return Ok");
        }
    }

    // -----------------------------------------------------------------------
    // Feature: s3rm-rs, Property 17: Confirmation Prompt Validation
    // **Validates: Requirements 3.3**
    //
    // For the confirmation prompt, only the exact string "yes" should be
    // accepted. All other inputs should be rejected.
    // -----------------------------------------------------------------------

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn property_17_only_exact_yes_accepted(
            input in "[a-zA-Z0-9 ]{0,20}",
        ) {
            // Feature: s3rm-rs, Property 17: Confirmation Prompt Validation
            // **Validates: Requirements 3.3**
            //
            // Any input that is not exactly "yes" must result in Cancelled error.
            prop_assume!(input.trim() != "yes");

            let config = make_config(false, false, false);
            let handler = MockPromptHandler::new(&input);
            let checker = SafetyChecker::with_prompt_handler(&config, Box::new(handler));

            let result = checker.check_before_deletion();
            prop_assert!(result.is_err());
            let err = result.unwrap_err();
            let s3rm_err = err.downcast_ref::<S3rmError>().unwrap();
            prop_assert_eq!(s3rm_err, &S3rmError::Cancelled);
        }

    }

    #[test]
    fn property_17_exact_yes_is_accepted() {
        // Feature: s3rm-rs, Property 17: Confirmation Prompt Validation
        // **Validates: Requirements 3.3**
        //
        // The exact string "yes" must always be accepted.
        let config = make_config(false, false, false);
        let handler = MockPromptHandler::new("yes");
        let checker = SafetyChecker::with_prompt_handler(&config, Box::new(handler));

        let result = checker.check_before_deletion();
        assert!(result.is_ok());
    }

    // -----------------------------------------------------------------------
    // Feature: s3rm-rs, Property 18: Force Flag Behavior
    // **Validates: Requirements 3.4, 13.2**
    //
    // For any deletion operation with the force flag enabled, confirmation
    // prompts should be skipped and the operation should proceed.
    // -----------------------------------------------------------------------

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn property_18_force_flag_skips_confirmation(
            json_tracing in proptest::bool::ANY,
        ) {
            // Feature: s3rm-rs, Property 18: Force Flag Behavior
            // **Validates: Requirements 3.4, 13.2**
            //
            // When force=true and dry_run=false, check_before_deletion MUST
            // return Ok regardless of json_tracing.
            let config = make_config(false, true, json_tracing);
            // Use a handler that would fail if called — force should prevent it.
            let handler = MockPromptHandler::new("no");
            let checker = SafetyChecker::with_prompt_handler(&config, Box::new(handler));

            let result = checker.check_before_deletion();
            prop_assert!(result.is_ok());
        }
    }

    // -----------------------------------------------------------------------
    // Feature: s3rm-rs, Property 19: Confirmation Prompt Target Display
    // **Validates: Requirements 3.5**
    //
    // For any destructive deletion operation (not dry-run, not force), the
    // confirmation prompt should display the target prefix (e.g.
    // `s3://bucket/prefix`) with colored text so users can verify which
    // objects will be affected.
    // -----------------------------------------------------------------------

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn property_19_prompt_displays_s3_uri_target(
            bucket in "[a-z0-9][a-z0-9.-]{2,20}",
            prefix in "[a-zA-Z0-9/_.-]{0,30}",
            disable_color in proptest::bool::ANY,
        ) {
            // Feature: s3rm-rs, Property 19: Confirmation Prompt Target Display
            // **Validates: Requirements 3.5**
            //
            // When the confirmation prompt is triggered (not dry-run, not force,
            // interactive), the target_display passed to read_confirmation must
            // be in the format `s3://bucket/prefix` (or `s3://bucket/` if prefix
            // is empty), and use_color must reflect the disable_color setting.
            let config = make_config_with_target(
                false,          // dry_run
                false,          // force
                false,          // json_tracing (must be false so prompt fires)
                disable_color,
                &bucket,
                &prefix,
            );

            let handler = std::sync::Arc::new(CapturingPromptHandler::new("yes"));
            // We need to extract captured values after check_before_deletion.
            // Since SafetyChecker takes Box<dyn PromptHandler>, we wrap with a
            // forwarding handler that shares state via Arc.
            let handler_clone = handler.clone();

            // Build a forwarding PromptHandler that delegates to the Arc'd handler.
            struct ForwardingHandler(std::sync::Arc<CapturingPromptHandler>);
            impl PromptHandler for ForwardingHandler {
                fn read_confirmation(&self, target_display: &str, use_color: bool) -> Result<String> {
                    self.0.read_confirmation(target_display, use_color)
                }
                fn is_interactive(&self) -> bool {
                    true
                }
            }

            let checker = SafetyChecker::with_prompt_handler(
                &config,
                Box::new(ForwardingHandler(handler_clone)),
            );

            let result = checker.check_before_deletion();
            prop_assert!(result.is_ok(), "Prompt accepted 'yes' should succeed");

            // Verify target_display format
            let captured = handler.captured_target.lock().unwrap();
            let target = captured.as_ref().expect("prompt should have been called");
            let expected = if prefix.is_empty() {
                format!("s3://{}/", bucket)
            } else {
                format!("s3://{}/{}", bucket, prefix)
            };
            prop_assert_eq!(target, &expected,
                "target_display should be s3://bucket/prefix format");

            // Verify use_color reflects disable_color setting
            let captured_color = handler.captured_use_color.lock().unwrap();
            let use_color = captured_color.expect("use_color should have been captured");
            prop_assert_eq!(use_color, !disable_color,
                "use_color should be the inverse of disable_color");
        }
    }

    // -----------------------------------------------------------------------
    // Unit tests for edge cases and specific scenarios
    // -----------------------------------------------------------------------

    #[test]
    fn dry_run_skips_confirmation_even_without_force() {
        let config = make_config(true, false, false);
        let handler = MockPromptHandler::new("no");
        let checker = SafetyChecker::with_prompt_handler(&config, Box::new(handler));

        let result = checker.check_before_deletion();
        assert!(result.is_ok());
    }

    #[test]
    fn dry_run_with_force_also_skips_confirmation() {
        let config = make_config(true, true, false);
        let handler = MockPromptHandler::new("no");
        let checker = SafetyChecker::with_prompt_handler(&config, Box::new(handler));

        let result = checker.check_before_deletion();
        assert!(result.is_ok());
    }

    #[test]
    fn json_logging_skips_prompt() {
        let config = make_config(false, false, true);
        let handler = MockPromptHandler::new("no");
        let checker = SafetyChecker::with_prompt_handler(&config, Box::new(handler));

        let result = checker.check_before_deletion();
        assert!(result.is_ok());
    }

    #[test]
    fn non_interactive_environment_skips_prompt() {
        let config = make_config(false, false, false);
        let checker =
            SafetyChecker::with_prompt_handler(&config, Box::new(NonInteractivePromptHandler));

        let result = checker.check_before_deletion();
        assert!(result.is_ok());
    }

    #[test]
    fn abbreviated_yes_responses_rejected() {
        let rejected_inputs = vec!["y", "Y", "YES", "Yes", "ye", "yep", "yeah", "ok", ""];
        for input in rejected_inputs {
            let config = make_config(false, false, false);
            let handler = MockPromptHandler::new(input);
            let checker = SafetyChecker::with_prompt_handler(&config, Box::new(handler));

            let result = checker.check_before_deletion();
            assert!(
                result.is_err(),
                "Input '{}' should be rejected but was accepted",
                input
            );
            let err = result.unwrap_err();
            assert_eq!(
                err.downcast_ref::<S3rmError>().unwrap(),
                &S3rmError::Cancelled,
                "Input '{}' should produce Cancelled error",
                input
            );
        }
    }

    #[test]
    fn exact_yes_accepted() {
        let config = make_config(false, false, false);
        let handler = MockPromptHandler::new("yes");
        let checker = SafetyChecker::with_prompt_handler(&config, Box::new(handler));

        let result = checker.check_before_deletion();
        assert!(result.is_ok());
    }

    #[test]
    fn checker_from_config_without_tracing_config() {
        let mut config = make_config(false, false, false);
        config.tracing_config = None;
        let handler = MockPromptHandler::new("yes");
        let checker = SafetyChecker::with_prompt_handler(&config, Box::new(handler));

        let result = checker.check_before_deletion();
        assert!(result.is_ok());
    }
}
