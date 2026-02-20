//! Property-based tests for safety features.
//!
//! Tests Properties 16-20 from the design document.

#[cfg(test)]
mod tests {
    use crate::callback::event_manager::EventManager;
    use crate::callback::filter_manager::FilterManager;
    use crate::config::{Config, FilterConfig, ForceRetryConfig, TracingConfig};
    use crate::safety::{DeletionSummary, PromptHandler, SafetyChecker};
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
        fn read_confirmation(
            &self,
            _summary: &DeletionSummary,
            _threshold_exceeded: bool,
        ) -> Result<String> {
            Ok(self.response.clone())
        }

        fn is_interactive(&self) -> bool {
            true
        }
    }

    /// Mock prompt handler that always reports as non-interactive.
    struct NonInteractivePromptHandler;

    impl PromptHandler for NonInteractivePromptHandler {
        fn read_confirmation(
            &self,
            _summary: &DeletionSummary,
            _threshold_exceeded: bool,
        ) -> Result<String> {
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
    // For any deletion operation with dry-run enabled, no actual deletions
    // should occur, and the tool should return a DryRun error.
    // -----------------------------------------------------------------------

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn property_16_dry_run_returns_dry_run_error(
            total_objects in 0u64..100_000,
            total_bytes in 0u64..1_000_000_000,
            force in proptest::bool::ANY,
            max_delete in proptest::option::of(0u64..1000),
        ) {
            // Feature: s3rm-rs, Property 16: Dry-Run Mode Safety
            // **Validates: Requirements 3.1**
            //
            // Regardless of force flag, max_delete, or summary values,
            // if dry_run is true, check_before_deletion MUST return DryRun.
            let config = make_config(true, force, max_delete, false);
            let handler = MockPromptHandler::new("yes");
            let checker = SafetyChecker::with_prompt_handler(&config, Box::new(handler));
            let summary = DeletionSummary::new(total_objects, total_bytes);

            let result = checker.check_before_deletion(&summary);
            prop_assert!(result.is_err());
            let err = result.unwrap_err();
            let s3rm_err = err.downcast_ref::<S3rmError>().unwrap();
            prop_assert_eq!(s3rm_err, &S3rmError::DryRun);
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
            let summary = DeletionSummary::new(10, 1024);

            let result = checker.check_before_deletion(&summary);
            prop_assert!(result.is_err());
            let err = result.unwrap_err();
            let s3rm_err = err.downcast_ref::<S3rmError>().unwrap();
            prop_assert_eq!(s3rm_err, &S3rmError::Cancelled);
        }

        #[test]
        fn property_17_exact_yes_is_accepted(
            total_objects in 1u64..100_000,
            total_bytes in 0u64..1_000_000_000,
        ) {
            // Feature: s3rm-rs, Property 17: Confirmation Prompt Validation
            // **Validates: Requirements 3.3**
            //
            // The exact string "yes" must always be accepted.
            let config = make_config(false, false, None, false);
            let handler = MockPromptHandler::new("yes");
            let checker = SafetyChecker::with_prompt_handler(&config, Box::new(handler));
            let summary = DeletionSummary::new(total_objects, total_bytes);

            let result = checker.check_before_deletion(&summary);
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
            total_objects in 0u64..100_000,
            total_bytes in 0u64..1_000_000_000,
            max_delete in proptest::option::of(0u64..1000),
            json_tracing in proptest::bool::ANY,
        ) {
            // Feature: s3rm-rs, Property 18: Force Flag Behavior
            // **Validates: Requirements 3.4, 13.2**
            //
            // When force=true and dry_run=false, check_before_deletion MUST
            // return Ok regardless of summary, max_delete, or json_tracing.
            let config = make_config(false, true, max_delete, json_tracing);
            // Use a handler that would fail if called — force should prevent it.
            let handler = MockPromptHandler::new("no");
            let checker = SafetyChecker::with_prompt_handler(&config, Box::new(handler));
            let summary = DeletionSummary::new(total_objects, total_bytes);

            let result = checker.check_before_deletion(&summary);
            prop_assert!(result.is_ok());
        }
    }

    // -----------------------------------------------------------------------
    // Property 19: Deletion Summary Display
    // **Validates: Requirements 3.5**
    //
    // For any deletion operation, the summary should include total object
    // count and estimated storage size.
    // -----------------------------------------------------------------------

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn property_19_summary_contains_count_and_size(
            total_objects in 0u64..10_000_000,
            total_bytes in 0u64..10_000_000_000u64,
        ) {
            // Feature: s3rm-rs, Property 19: Deletion Summary Display
            // **Validates: Requirements 3.5**
            //
            // The display output of DeletionSummary must contain both
            // the object count and a human-readable size representation.
            let summary = DeletionSummary::new(total_objects, total_bytes);
            let display = format!("{}", summary);

            // Must contain the object count
            prop_assert!(
                display.contains(&total_objects.to_string()),
                "Display '{}' must contain object count '{}'",
                display,
                total_objects
            );

            // Must contain "Estimated size:" label
            prop_assert!(
                display.contains("Estimated size:"),
                "Display '{}' must contain 'Estimated size:'",
                display
            );

            // Must contain "Objects to delete:" label
            prop_assert!(
                display.contains("Objects to delete:"),
                "Display '{}' must contain 'Objects to delete:'",
                display
            );
        }
    }

    // -----------------------------------------------------------------------
    // Property 20: Threshold-Based Additional Confirmation
    // **Validates: Requirements 3.6**
    //
    // When object count exceeds --max-delete threshold, the tool should
    // stop and require confirmation unless force flag is set.
    // -----------------------------------------------------------------------

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn property_20_threshold_exceeded_detection(
            max_delete in 1u64..10_000,
            extra in 1u64..10_000,
        ) {
            // Feature: s3rm-rs, Property 20: Threshold-Based Additional Confirmation
            // **Validates: Requirements 3.6**
            //
            // When object_count > max_delete, is_threshold_exceeded returns true.
            let object_count = max_delete + extra;
            let config = make_config(false, false, Some(max_delete), false);
            let handler = MockPromptHandler::new("yes");
            let checker = SafetyChecker::with_prompt_handler(&config, Box::new(handler));

            prop_assert!(checker.is_threshold_exceeded(object_count));
        }

        #[test]
        fn property_20_threshold_not_exceeded_when_within_limit(
            max_delete in 1u64..10_000,
            object_count in 0u64..10_000,
        ) {
            // Feature: s3rm-rs, Property 20: Threshold-Based Additional Confirmation
            // **Validates: Requirements 3.6**
            //
            // When object_count <= max_delete, is_threshold_exceeded returns false.
            prop_assume!(object_count <= max_delete);
            let config = make_config(false, false, Some(max_delete), false);
            let handler = MockPromptHandler::new("yes");
            let checker = SafetyChecker::with_prompt_handler(&config, Box::new(handler));

            prop_assert!(!checker.is_threshold_exceeded(object_count));
        }

        #[test]
        fn property_20_no_threshold_when_not_configured(
            object_count in 0u64..1_000_000,
        ) {
            // Feature: s3rm-rs, Property 20: Threshold-Based Additional Confirmation
            // **Validates: Requirements 3.6**
            //
            // When max_delete is None, is_threshold_exceeded always returns false.
            let config = make_config(false, false, None, false);
            let handler = MockPromptHandler::new("yes");
            let checker = SafetyChecker::with_prompt_handler(&config, Box::new(handler));

            prop_assert!(!checker.is_threshold_exceeded(object_count));
        }

        #[test]
        fn property_20_force_bypasses_threshold(
            max_delete in 1u64..1_000,
            extra in 1u64..1_000,
            total_bytes in 0u64..1_000_000_000,
        ) {
            // Feature: s3rm-rs, Property 20: Threshold-Based Additional Confirmation
            // **Validates: Requirements 3.6**
            //
            // Even when threshold is exceeded, force=true should bypass.
            let object_count = max_delete + extra;
            let config = make_config(false, true, Some(max_delete), false);
            let handler = MockPromptHandler::new("no");
            let checker = SafetyChecker::with_prompt_handler(&config, Box::new(handler));
            let summary = DeletionSummary::new(object_count, total_bytes);

            let result = checker.check_before_deletion(&summary);
            prop_assert!(result.is_ok());
        }
    }

    // -----------------------------------------------------------------------
    // Unit tests for edge cases and specific scenarios
    // -----------------------------------------------------------------------

    #[test]
    fn dry_run_takes_precedence_over_force() {
        let config = make_config(true, true, None, false);
        let handler = MockPromptHandler::new("yes");
        let checker = SafetyChecker::with_prompt_handler(&config, Box::new(handler));
        let summary = DeletionSummary::new(100, 1024);

        let result = checker.check_before_deletion(&summary);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.downcast_ref::<S3rmError>().unwrap(), &S3rmError::DryRun);
    }

    #[test]
    fn json_logging_skips_prompt() {
        // Even with dry_run=false and force=false, JSON logging should skip prompts
        let config = make_config(false, false, None, true);
        // Use NonInteractive handler but it won't matter since JSON check comes first
        let handler = MockPromptHandler::new("no");
        let checker = SafetyChecker::with_prompt_handler(&config, Box::new(handler));
        let summary = DeletionSummary::new(100, 1024);

        // JSON logging causes should_skip_prompt to return true → Ok(())
        let result = checker.check_before_deletion(&summary);
        assert!(result.is_ok());
    }

    #[test]
    fn non_interactive_environment_skips_prompt() {
        let config = make_config(false, false, None, false);
        let checker =
            SafetyChecker::with_prompt_handler(&config, Box::new(NonInteractivePromptHandler));
        let summary = DeletionSummary::new(100, 1024);

        let result = checker.check_before_deletion(&summary);
        assert!(result.is_ok());
    }

    #[test]
    fn abbreviated_yes_responses_rejected() {
        let rejected_inputs = vec!["y", "Y", "YES", "Yes", "ye", "yep", "yeah", "ok", ""];
        for input in rejected_inputs {
            let config = make_config(false, false, None, false);
            let handler = MockPromptHandler::new(input);
            let checker = SafetyChecker::with_prompt_handler(&config, Box::new(handler));
            let summary = DeletionSummary::new(100, 1024);

            let result = checker.check_before_deletion(&summary);
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
        let summary = DeletionSummary::new(100, 1024);

        let result = checker.check_before_deletion(&summary);
        assert!(result.is_ok());
    }

    #[test]
    fn threshold_exceeded_with_confirmation_yes() {
        let config = make_config(false, false, Some(50), false);
        let handler = MockPromptHandler::new("yes");
        let checker = SafetyChecker::with_prompt_handler(&config, Box::new(handler));
        let summary = DeletionSummary::new(100, 1024); // 100 > 50

        assert!(checker.is_threshold_exceeded(100));
        let result = checker.check_before_deletion(&summary);
        assert!(result.is_ok());
    }

    #[test]
    fn threshold_exceeded_with_confirmation_no() {
        let config = make_config(false, false, Some(50), false);
        let handler = MockPromptHandler::new("no");
        let checker = SafetyChecker::with_prompt_handler(&config, Box::new(handler));
        let summary = DeletionSummary::new(100, 1024); // 100 > 50

        let result = checker.check_before_deletion(&summary);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(
            err.downcast_ref::<S3rmError>().unwrap(),
            &S3rmError::Cancelled
        );
    }

    #[test]
    fn summary_display_zero_objects() {
        let summary = DeletionSummary::new(0, 0);
        let display = format!("{}", summary);
        assert!(display.contains("Objects to delete: 0"));
        assert!(display.contains("Estimated size:"));
    }

    #[test]
    fn summary_display_large_values() {
        let summary = DeletionSummary::new(1_000_000, 1_073_741_824); // 1 GiB
        let display = format!("{}", summary);
        assert!(display.contains("Objects to delete: 1000000"));
        assert!(display.contains("Estimated size:"));
        // byte-unit should format 1 GiB appropriately
        assert!(display.contains("GiB") || display.contains("GB"));
    }

    #[test]
    fn deletion_summary_equality() {
        let s1 = DeletionSummary::new(100, 2048);
        let s2 = DeletionSummary::new(100, 2048);
        let s3 = DeletionSummary::new(200, 2048);
        assert_eq!(s1, s2);
        assert_ne!(s1, s3);
    }

    #[test]
    fn checker_from_config_without_tracing_config() {
        // When tracing_config is None, json_logging defaults to false
        let mut config = make_config(false, false, None, false);
        config.tracing_config = None;
        let handler = MockPromptHandler::new("yes");
        let checker = SafetyChecker::with_prompt_handler(&config, Box::new(handler));
        let summary = DeletionSummary::new(10, 100);

        let result = checker.check_before_deletion(&summary);
        assert!(result.is_ok());
    }

    #[test]
    fn threshold_boundary_value() {
        // Exactly at the limit should NOT trigger threshold exceeded
        let config = make_config(false, false, Some(100), false);
        let handler = MockPromptHandler::new("yes");
        let checker = SafetyChecker::with_prompt_handler(&config, Box::new(handler));

        assert!(!checker.is_threshold_exceeded(100)); // equal → not exceeded
        assert!(checker.is_threshold_exceeded(101)); // one over → exceeded
        assert!(!checker.is_threshold_exceeded(99)); // one under → not exceeded
    }
}
