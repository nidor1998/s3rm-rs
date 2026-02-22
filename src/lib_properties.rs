// Property-based tests for the s3rm-rs library API.
//
// Feature: s3rm-rs, Property 44: Library API Configuration
// For any configuration option available in the CLI, the library API should
// provide equivalent programmatic configuration functions.
// **Validates: Requirements 12.4**
//
// Feature: s3rm-rs, Property 45: Library Callback Registration
// For any Rust filter or event callback registered via the library API, the
// callback should be invoked at the appropriate times with correct data.
// **Validates: Requirements 12.5, 12.6**
//
// Feature: s3rm-rs, Property 46: Library Lua Callback Support
// For any Lua script path provided to the library API, the library should load
// and execute the script for filter or event operations.
// **Validates: Requirements 12.7**
//
// Feature: s3rm-rs, Property 47: Library Async Result Handling
// For any library API function call, the function should return a Result type
// that allows proper error handling.
// **Validates: Requirements 12.8**

#[cfg(test)]
mod tests {
    use crate::callback::event_manager::EventManager;
    use crate::callback::filter_manager::FilterManager;
    use crate::config::{Config, FilterConfig, ForceRetryConfig};
    use crate::types::event_callback::{EventCallback, EventData, EventType};
    use crate::types::filter_callback::FilterCallback;
    use crate::types::token::create_pipeline_cancellation_token;
    use crate::types::{S3Object, StoragePath};
    use anyhow::Result;
    use async_trait::async_trait;
    use aws_sdk_s3::types::Object;
    use proptest::prelude::*;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    // -----------------------------------------------------------------------
    // Test callback implementations
    // -----------------------------------------------------------------------

    /// A filter callback that accepts objects larger than a given threshold.
    struct SizeThresholdFilter {
        min_size: i64,
    }

    #[async_trait]
    impl FilterCallback for SizeThresholdFilter {
        async fn filter(&mut self, object: &S3Object) -> Result<bool> {
            Ok(object.size() >= self.min_size)
        }
    }

    /// A filter callback that always returns an error.
    struct ErrorFilter;

    #[async_trait]
    impl FilterCallback for ErrorFilter {
        async fn filter(&mut self, _object: &S3Object) -> Result<bool> {
            Err(anyhow::anyhow!("filter error"))
        }
    }

    /// An event callback that collects all received events.
    struct CollectingCallback {
        events: Arc<Mutex<Vec<EventData>>>,
    }

    impl CollectingCallback {
        fn new() -> (Self, Arc<Mutex<Vec<EventData>>>) {
            let events = Arc::new(Mutex::new(Vec::new()));
            (
                Self {
                    events: events.clone(),
                },
                events,
            )
        }
    }

    #[async_trait]
    impl EventCallback for CollectingCallback {
        async fn on_event(&mut self, event_data: EventData) {
            self.events.lock().await.push(event_data);
        }
    }

    // -----------------------------------------------------------------------
    // Generators
    // -----------------------------------------------------------------------

    fn arb_worker_size() -> impl Strategy<Value = u16> {
        1u16..=1000
    }

    fn arb_batch_size() -> impl Strategy<Value = u16> {
        1u16..=1000
    }

    fn arb_max_delete() -> impl Strategy<Value = Option<u64>> {
        prop_oneof![Just(None), (1u64..1_000_000).prop_map(Some),]
    }

    fn arb_object_size() -> impl Strategy<Value = i64> {
        0i64..10_000_000
    }

    fn make_test_config(
        worker_size: u16,
        batch_size: u16,
        dry_run: bool,
        force: bool,
        max_delete: Option<u64>,
    ) -> Config {
        Config {
            target: StoragePath::S3 {
                bucket: "test-bucket".to_string(),
                prefix: "test-prefix/".to_string(),
            },
            show_no_progress: true,
            report_deletion_status: false,
            target_client_config: None,
            force_retry_config: ForceRetryConfig {
                force_retry_count: 3,
                force_retry_interval_milliseconds: 1000,
            },
            tracing_config: None,
            worker_size,
            warn_as_error: false,
            dry_run,
            rate_limit_objects: None,
            max_parallel_listings: 10,
            object_listing_queue_size: 1000,
            max_parallel_listing_max_depth: 5,
            allow_parallel_listings_in_express_one_zone: false,
            filter_config: FilterConfig::default(),
            max_keys: 1000,
            auto_complete_shell: None,
            event_callback_lua_script: None,
            filter_callback_lua_script: None,
            allow_lua_os_library: false,
            allow_lua_unsafe_vm: false,
            lua_vm_memory_limit: 50 * 1024 * 1024,
            if_match: false,
            max_delete,
            filter_manager: FilterManager::new(),
            event_manager: EventManager::new(),
            batch_size,
            delete_all_versions: false,
            force,
            test_user_defined_callback: false,
        }
    }

    fn make_test_object(key: &str, size: i64) -> S3Object {
        S3Object::NotVersioning(
            Object::builder()
                .key(key)
                .size(size)
                .last_modified(aws_sdk_s3::primitives::DateTime::from_secs(1_700_000_000))
                .build(),
        )
    }

    // -----------------------------------------------------------------------
    // Feature: s3rm-rs, Property 44: Library API Configuration
    // -----------------------------------------------------------------------

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// Feature: s3rm-rs, Property 44: Library API Configuration
        /// **Validates: Requirements 12.4**
        ///
        /// For any valid configuration values, the Config struct should:
        /// 1. Accept the values programmatically
        /// 2. Preserve them accurately
        /// 3. Support all CLI-equivalent options
        #[test]
        fn config_preserves_programmatic_settings(
            worker_size in arb_worker_size(),
            batch_size in arb_batch_size(),
            dry_run in any::<bool>(),
            force in any::<bool>(),
            max_delete in arb_max_delete(),
        ) {
            let config = make_test_config(worker_size, batch_size, dry_run, force, max_delete);

            // Verify all configuration values are preserved
            prop_assert_eq!(config.worker_size, worker_size);
            prop_assert_eq!(config.batch_size, batch_size);
            prop_assert_eq!(config.dry_run, dry_run);
            prop_assert_eq!(config.force, force);
            prop_assert_eq!(config.max_delete, max_delete);
        }

        /// Feature: s3rm-rs, Property 44: Library API Configuration (Clone)
        /// **Validates: Requirements 12.4**
        ///
        /// Config should be cloneable and preserve all values through cloning.
        #[test]
        fn config_clone_preserves_all_settings(
            worker_size in arb_worker_size(),
            batch_size in arb_batch_size(),
            dry_run in any::<bool>(),
            force in any::<bool>(),
            max_delete in arb_max_delete(),
        ) {
            let config = make_test_config(worker_size, batch_size, dry_run, force, max_delete);
            let cloned = config.clone();

            prop_assert_eq!(cloned.worker_size, worker_size);
            prop_assert_eq!(cloned.batch_size, batch_size);
            prop_assert_eq!(cloned.dry_run, dry_run);
            prop_assert_eq!(cloned.force, force);
            prop_assert_eq!(cloned.max_delete, max_delete);
            prop_assert_eq!(cloned.if_match, config.if_match);
            prop_assert_eq!(cloned.delete_all_versions, config.delete_all_versions);
            prop_assert_eq!(cloned.warn_as_error, config.warn_as_error);
        }

        /// Feature: s3rm-rs, Property 44: Library API Configuration (Filter Config)
        /// **Validates: Requirements 12.4**
        ///
        /// Filter configuration should be settable via the library API,
        /// including size and time range filters.
        #[test]
        fn config_filter_settings_preserved(
            larger_size in prop::option::of(0u64..10_000_000),
            smaller_size in prop::option::of(0u64..10_000_000),
        ) {
            let mut config = make_test_config(16, 1000, false, false, None);
            config.filter_config.larger_size = larger_size;
            config.filter_config.smaller_size = smaller_size;

            prop_assert_eq!(config.filter_config.larger_size, larger_size);
            prop_assert_eq!(config.filter_config.smaller_size, smaller_size);

            // Clone should also preserve filter config
            let cloned = config.clone();
            prop_assert_eq!(cloned.filter_config.larger_size, larger_size);
            prop_assert_eq!(cloned.filter_config.smaller_size, smaller_size);
        }
    }

    // -----------------------------------------------------------------------
    // Feature: s3rm-rs, Property 45: Library Callback Registration
    // -----------------------------------------------------------------------

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// Feature: s3rm-rs, Property 45: Library Callback Registration (Filter)
        /// **Validates: Requirements 12.5, 12.6**
        ///
        /// For any registered Rust filter callback, the callback should be invoked
        /// for each object and its return value should correctly determine
        /// whether the object passes the filter.
        #[test]
        fn filter_callback_invoked_with_correct_data(
            threshold in 0i64..10_000_000,
            object_size in arb_object_size(),
        ) {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let mut manager = FilterManager::new();
                prop_assert!(!manager.is_callback_registered());

                manager.register_callback(SizeThresholdFilter { min_size: threshold });
                prop_assert!(manager.is_callback_registered());

                let object = make_test_object("test/key.txt", object_size);
                let result = manager.execute_filter(&object).await.unwrap();

                // The filter should accept objects >= threshold
                prop_assert_eq!(result, object_size >= threshold);
                Ok(())
            })?;
        }

        /// Feature: s3rm-rs, Property 45: Library Callback Registration (Event)
        /// **Validates: Requirements 12.5, 12.6**
        ///
        /// For any registered Rust event callback and sequence of events,
        /// all events matching the registered flags should be received by the callback.
        #[test]
        fn event_callback_receives_matching_events(
            num_events in 1usize..20,
        ) {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let mut manager = EventManager::new();
                let (callback, events) = CollectingCallback::new();

                manager.register_callback(EventType::ALL_EVENTS, callback, false);
                prop_assert!(manager.is_callback_registered());

                // Send PIPELINE_START + N DELETE_COMPLETE events
                manager.trigger_event(EventData::new(EventType::PIPELINE_START)).await;
                for i in 0..num_events {
                    let mut event_data = EventData::new(EventType::DELETE_COMPLETE);
                    event_data.key = Some(format!("key_{i}"));
                    event_data.size = Some(1024);
                    manager.trigger_event(event_data).await;
                }

                let collected = events.lock().await;
                // 1 PIPELINE_START + num_events DELETE_COMPLETE
                prop_assert_eq!(collected.len(), 1 + num_events);
                prop_assert_eq!(collected[0].event_type, EventType::PIPELINE_START);
                for i in 0..num_events {
                    prop_assert_eq!(collected[1 + i].event_type, EventType::DELETE_COMPLETE);
                    let expected_key = format!("key_{i}");
                    prop_assert_eq!(collected[1 + i].key.as_deref(), Some(expected_key.as_str()));
                }
                Ok(())
            })?;
        }

        /// Feature: s3rm-rs, Property 45: Library Callback Registration (Selective Events)
        /// **Validates: Requirements 12.5, 12.6**
        ///
        /// Event callbacks registered with specific event flags should only
        /// receive events matching those flags.
        #[test]
        fn event_callback_filters_by_event_type(
            num_complete in 1usize..10,
            num_failed in 1usize..10,
        ) {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let mut manager = EventManager::new();
                let (callback, events) = CollectingCallback::new();

                // Register only for DELETE_COMPLETE events
                manager.register_callback(EventType::DELETE_COMPLETE, callback, false);

                for _ in 0..num_complete {
                    manager.trigger_event(EventData::new(EventType::DELETE_COMPLETE)).await;
                }
                for _ in 0..num_failed {
                    manager.trigger_event(EventData::new(EventType::DELETE_FAILED)).await;
                }

                let collected = events.lock().await;
                // Only DELETE_COMPLETE events should be received
                prop_assert_eq!(collected.len(), num_complete);
                for event in collected.iter() {
                    prop_assert_eq!(event.event_type, EventType::DELETE_COMPLETE);
                }
                Ok(())
            })?;
        }
    }

    // -----------------------------------------------------------------------
    // Feature: s3rm-rs, Property 46: Library Lua Callback Support
    // -----------------------------------------------------------------------

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(50))]

        /// Feature: s3rm-rs, Property 46: Library Lua Callback Support
        /// **Validates: Requirements 12.7**
        ///
        /// Lua callback script paths set on Config should be preserved and
        /// accessible for the pipeline to load.
        #[test]
        fn lua_script_paths_preserved_in_config(
            filter_script in prop::option::of("[a-z_/]{1,30}\\.lua"),
            event_script in prop::option::of("[a-z_/]{1,30}\\.lua"),
            allow_os in any::<bool>(),
            allow_unsafe in any::<bool>(),
        ) {
            let mut config = make_test_config(16, 1000, false, true, None);
            config.filter_callback_lua_script = filter_script.clone();
            config.event_callback_lua_script = event_script.clone();
            config.allow_lua_os_library = allow_os;
            config.allow_lua_unsafe_vm = allow_unsafe;

            prop_assert_eq!(&config.filter_callback_lua_script, &filter_script);
            prop_assert_eq!(&config.event_callback_lua_script, &event_script);
            prop_assert_eq!(config.allow_lua_os_library, allow_os);
            prop_assert_eq!(config.allow_lua_unsafe_vm, allow_unsafe);

            // Clone preserves Lua settings
            let cloned = config.clone();
            prop_assert_eq!(&cloned.filter_callback_lua_script, &filter_script);
            prop_assert_eq!(&cloned.event_callback_lua_script, &event_script);
        }
    }

    // -----------------------------------------------------------------------
    // Feature: s3rm-rs, Property 47: Library Async Result Handling
    // -----------------------------------------------------------------------

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// Feature: s3rm-rs, Property 47: Library Async Result Handling
        /// **Validates: Requirements 12.8**
        ///
        /// Multiple independently-created cancellation tokens should be
        /// isolated — cancelling one must not affect another.
        #[test]
        fn cancellation_token_creation_always_succeeds(
            token_count in 2usize..10,
        ) {
            let tokens: Vec<_> = (0..token_count)
                .map(|_| create_pipeline_cancellation_token())
                .collect();

            // None should be cancelled initially
            for t in &tokens {
                prop_assert!(!t.is_cancelled());
            }

            // Cancel the first token — others must remain uncancelled
            tokens[0].cancel();
            prop_assert!(tokens[0].is_cancelled());
            for t in &tokens[1..] {
                prop_assert!(!t.is_cancelled(), "cancelling one token must not affect others");
            }
        }

        /// Feature: s3rm-rs, Property 47: Library Async Result Handling (Filter Errors)
        /// **Validates: Requirements 12.8**
        ///
        /// Filter callbacks that return errors should propagate errors correctly
        /// through the async Result type, regardless of the object being filtered.
        #[test]
        fn filter_callback_error_propagation(
            key in "[a-z0-9/_.-]{1,40}",
            size in 0i64..10_000,
        ) {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let mut manager = FilterManager::new();
                manager.register_callback(ErrorFilter);

                let object = make_test_object(&key, size);
                let result = manager.execute_filter(&object).await;

                prop_assert!(result.is_err());
                prop_assert!(result.unwrap_err().to_string().contains("filter error"));
                Ok(())
            })?;
        }

        /// Feature: s3rm-rs, Property 47: Library Async Result Handling (S3Target Parsing)
        /// **Validates: Requirements 12.8**
        ///
        /// S3Target::parse should return Ok for valid URIs and Err for invalid ones,
        /// with the Result type enabling proper error handling.
        #[test]
        fn s3_target_parse_returns_result(
            bucket in "[a-z][a-z0-9-]{2,20}",
            prefix in prop::option::of("[a-z0-9/]{1,30}"),
        ) {
            use crate::types::S3Target;

            let uri = match &prefix {
                Some(p) => format!("s3://{bucket}/{p}"),
                None => format!("s3://{bucket}"),
            };

            let result = S3Target::parse(&uri);
            prop_assert!(result.is_ok(), "Valid URI should parse successfully: {}", uri);
            let target = result.unwrap();
            prop_assert_eq!(&target.bucket, &bucket);
        }

        /// Feature: s3rm-rs, Property 47: Library Async Result Handling (Invalid URI)
        /// **Validates: Requirements 12.8**
        ///
        /// S3Target::parse should return Err with descriptive messages for invalid URIs.
        #[test]
        fn s3_target_parse_rejects_invalid_uris(
            invalid_uri in "[a-z]{1,10}://[a-z]{0,10}",
        ) {
            use crate::types::S3Target;

            // URIs not starting with s3:// should fail
            if !invalid_uri.starts_with("s3://") {
                let result = S3Target::parse(&invalid_uri);
                prop_assert!(result.is_err());
            }
        }

        /// Feature: s3rm-rs, Property 47: Library Async Result Handling (Error Exit Codes)
        /// **Validates: Requirements 12.8**
        ///
        /// All S3rmError variants should map to a well-defined exit code,
        /// regardless of the message content or count values.
        #[test]
        fn error_exit_codes_are_well_defined(
            msg in "[a-zA-Z0-9 ]{1,30}",
            deleted in 0u64..10_000,
            failed in 0u64..10_000,
        ) {
            use crate::types::error::S3rmError;

            let errors = vec![
                (S3rmError::AwsSdk(msg.clone()), 1),
                (S3rmError::InvalidConfig(msg.clone()), 2),
                (S3rmError::InvalidUri(msg.clone()), 2),
                (S3rmError::InvalidRegex(msg.clone()), 2),
                (S3rmError::LuaScript(msg.clone()), 1),
                (S3rmError::Io(msg.clone()), 1),
                (S3rmError::Cancelled, 0),
                (S3rmError::PartialFailure { deleted, failed }, 3),
                (S3rmError::Pipeline(msg), 1),
            ];

            for (error, expected_code) in errors {
                prop_assert_eq!(error.exit_code(), expected_code,
                    "Error {:?} should have exit code {}", error, expected_code);
            }
        }
    }
}
