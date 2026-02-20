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

    fn make_config(
        dry_run: bool,
        force: bool,
        max_delete: Option<u64>,
        json_tracing: bool,
    ) -> Config {
        Config {
            target: StoragePath::S3 {
                bucket: "test-bucket".to_string(),
                prefix: String::new(),
            },
            show_no_progress: false,
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
            max_delete,
            filter_manager: FilterManager::new(),
            event_manager: EventManager::new(),
            batch_size: 1000,
            delete_all_versions: false,
            force,
        }
    }

    // -----------------------------------------------------------------------
    // Property 16: Dry-Run Mode Safety
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
            max_delete in proptest::option::of(0u64..1000),
        ) {
            // Feature: s3rm-rs, Property 16: Dry-Run Mode Safety
            // **Validates: Requirements 3.1**
            //
            // Regardless of force flag or max_delete, if dry_run is true,
            // check_before_deletion MUST return Ok (no confirmation needed —
            // deletions will be simulated).
            let config = make_config(true, force, max_delete, false);
            // Use "no" response to prove the prompt is never consulted.
            let handler = MockPromptHandler::new("no");
            let checker = SafetyChecker::with_prompt_handler(&config, Box::new(handler));

            let result = checker.check_before_deletion();
            prop_assert!(result.is_ok(), "dry-run should skip confirmation and return Ok");
        }
    }

    // -----------------------------------------------------------------------
    // Property 17: Confirmation Prompt Validation
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

            let config = make_config(false, false, None, false);
            let handler = MockPromptHandler::new(&input);
            let checker = SafetyChecker::with_prompt_handler(&config, Box::new(handler));

            let result = checker.check_before_deletion();
            prop_assert!(result.is_err());
            let err = result.unwrap_err();
            let s3rm_err = err.downcast_ref::<S3rmError>().unwrap();
            prop_assert_eq!(s3rm_err, &S3rmError::Cancelled);
        }

        #[test]
        fn property_17_exact_yes_is_accepted(
            max_delete in proptest::option::of(1u64..100_000),
        ) {
            // Feature: s3rm-rs, Property 17: Confirmation Prompt Validation
            // **Validates: Requirements 3.3**
            //
            // The exact string "yes" must always be accepted.
            let config = make_config(false, false, max_delete, false);
            let handler = MockPromptHandler::new("yes");
            let checker = SafetyChecker::with_prompt_handler(&config, Box::new(handler));

            let result = checker.check_before_deletion();
            prop_assert!(result.is_ok());
        }
    }

    // -----------------------------------------------------------------------
    // Property 18: Force Flag Behavior
    // **Validates: Requirements 3.4, 13.2**
    //
    // For any deletion operation with the force flag enabled, confirmation
    // prompts should be skipped and the operation should proceed.
    // -----------------------------------------------------------------------

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn property_18_force_flag_skips_confirmation(
            max_delete in proptest::option::of(0u64..1000),
            json_tracing in proptest::bool::ANY,
        ) {
            // Feature: s3rm-rs, Property 18: Force Flag Behavior
            // **Validates: Requirements 3.4, 13.2**
            //
            // When force=true and dry_run=false, check_before_deletion MUST
            // return Ok regardless of max_delete or json_tracing.
            let config = make_config(false, true, max_delete, json_tracing);
            // Use a handler that would fail if called — force should prevent it.
            let handler = MockPromptHandler::new("no");
            let checker = SafetyChecker::with_prompt_handler(&config, Box::new(handler));

            let result = checker.check_before_deletion();
            prop_assert!(result.is_ok());
        }
    }

    // -----------------------------------------------------------------------
    // Unit tests for edge cases and specific scenarios
    // -----------------------------------------------------------------------

    #[test]
    fn dry_run_skips_confirmation_even_without_force() {
        let config = make_config(true, false, None, false);
        let handler = MockPromptHandler::new("no");
        let checker = SafetyChecker::with_prompt_handler(&config, Box::new(handler));

        let result = checker.check_before_deletion();
        assert!(result.is_ok());
    }

    #[test]
    fn dry_run_with_force_also_skips_confirmation() {
        let config = make_config(true, true, None, false);
        let handler = MockPromptHandler::new("no");
        let checker = SafetyChecker::with_prompt_handler(&config, Box::new(handler));

        let result = checker.check_before_deletion();
        assert!(result.is_ok());
    }

    #[test]
    fn json_logging_skips_prompt() {
        let config = make_config(false, false, None, true);
        let handler = MockPromptHandler::new("no");
        let checker = SafetyChecker::with_prompt_handler(&config, Box::new(handler));

        let result = checker.check_before_deletion();
        assert!(result.is_ok());
    }

    #[test]
    fn non_interactive_environment_skips_prompt() {
        let config = make_config(false, false, None, false);
        let checker =
            SafetyChecker::with_prompt_handler(&config, Box::new(NonInteractivePromptHandler));

        let result = checker.check_before_deletion();
        assert!(result.is_ok());
    }

    #[test]
    fn abbreviated_yes_responses_rejected() {
        let rejected_inputs = vec!["y", "Y", "YES", "Yes", "ye", "yep", "yeah", "ok", ""];
        for input in rejected_inputs {
            let config = make_config(false, false, None, false);
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
        let config = make_config(false, false, None, false);
        let handler = MockPromptHandler::new("yes");
        let checker = SafetyChecker::with_prompt_handler(&config, Box::new(handler));

        let result = checker.check_before_deletion();
        assert!(result.is_ok());
    }

    #[test]
    fn checker_from_config_without_tracing_config() {
        let mut config = make_config(false, false, None, false);
        config.tracing_config = None;
        let handler = MockPromptHandler::new("yes");
        let checker = SafetyChecker::with_prompt_handler(&config, Box::new(handler));

        let result = checker.check_before_deletion();
        assert!(result.is_ok());
    }
}
