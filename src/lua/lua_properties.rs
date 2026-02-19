//! Property-based tests for Lua integration.
//!
//! Tests the correctness properties of Lua filter callbacks,
//! callback type support, and sandbox security.

#[cfg(test)]
mod tests {
    use crate::lua::engine::LuaScriptCallbackEngine;
    use crate::lua::event::LuaEventCallback;
    use crate::lua::filter::LuaFilterCallback;
    use crate::types::S3Object;
    use crate::types::event_callback::{EventCallback, EventData, EventType};
    use crate::types::filter_callback::FilterCallback;
    use aws_sdk_s3::primitives::DateTime;
    use aws_sdk_s3::types::Object;
    use proptest::prelude::*;

    // --- Generators ---

    /// Generate arbitrary S3 object keys (non-empty ASCII strings without null bytes).
    fn arb_s3_key() -> impl Strategy<Value = String> {
        "[a-zA-Z0-9/_.-]{1,100}".prop_map(|s| s)
    }

    /// Generate arbitrary S3 objects with configurable keys.
    fn arb_s3_object() -> impl Strategy<Value = S3Object> {
        (arb_s3_key(), 0i64..10_000_000i64, 0i64..2_000_000_000i64).prop_map(
            |(key, size, timestamp)| {
                S3Object::NotVersioning(
                    Object::builder()
                        .key(key)
                        .size(size)
                        .last_modified(DateTime::from_secs(timestamp))
                        .build(),
                )
            },
        )
    }

    // -------------------------------------------------------------------------
    // Property 11: Lua Filter Callback Execution
    // **Validates: Requirements 2.8**
    //
    // *For any* Lua filter script provided, the tool should execute the script
    // for each object and delete only objects where the script returns true.
    // -------------------------------------------------------------------------

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(50))]

        /// **Property 11: Lua Filter Callback Execution**
        /// **Validates: Requirements 2.8**
        ///
        /// For any S3 object, a Lua filter that returns `true` should pass the object,
        /// and a Lua filter that returns `false` should reject it.
        #[test]
        fn lua_filter_always_true_passes_all(key in arb_s3_key(), size in 0i64..10_000_000i64) {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let mut callback = LuaFilterCallback::new(8 * 1024 * 1024, false, false);
                callback
                    .load_and_compile_from_string("function filter(obj) return true end")
                    .unwrap();

                let object = S3Object::NotVersioning(
                    Object::builder()
                        .key(key)
                        .size(size)
                        .last_modified(DateTime::from_secs(1000))
                        .build(),
                );
                let result = callback.filter(&object).await.unwrap();
                prop_assert!(result);
                Ok(())
            })?;
        }

        /// **Property 11: Lua Filter Callback Execution (false case)**
        /// **Validates: Requirements 2.8**
        ///
        /// A Lua filter that always returns `false` should reject every object.
        #[test]
        fn lua_filter_always_false_rejects_all(key in arb_s3_key(), size in 0i64..10_000_000i64) {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let mut callback = LuaFilterCallback::new(8 * 1024 * 1024, false, false);
                callback
                    .load_and_compile_from_string("function filter(obj) return false end")
                    .unwrap();

                let object = S3Object::NotVersioning(
                    Object::builder()
                        .key(key)
                        .size(size)
                        .last_modified(DateTime::from_secs(1000))
                        .build(),
                );
                let result = callback.filter(&object).await.unwrap();
                prop_assert!(!result);
                Ok(())
            })?;
        }

        /// **Property 11: Lua Filter Callback Execution (key-based)**
        /// **Validates: Requirements 2.8**
        ///
        /// A Lua filter that matches on key prefix should correctly distinguish
        /// objects based on their key.
        #[test]
        fn lua_filter_key_prefix_based(
            key in prop_oneof![
                // ~50%: keys that start with "logs/"
                "[a-zA-Z0-9/_.-]{0,94}".prop_map(|suffix| format!("logs/{suffix}")),
                // ~50%: arbitrary keys (unlikely to start with "logs/")
                arb_s3_key(),
            ]
        ) {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let mut callback = LuaFilterCallback::new(8 * 1024 * 1024, false, false);
                callback
                    .load_and_compile_from_string(
                        r#"
                        function filter(obj)
                            return string.find(obj.key, "^logs/") ~= nil
                        end
                        "#,
                    )
                    .unwrap();

                let object = S3Object::NotVersioning(
                    Object::builder()
                        .key(&key)
                        .size(100)
                        .last_modified(DateTime::from_secs(1000))
                        .build(),
                );
                let result = callback.filter(&object).await.unwrap();

                // Result should match whether the key starts with "logs/"
                let expected = key.starts_with("logs/");
                prop_assert_eq!(result, expected);
                Ok(())
            })?;
        }

        /// **Property 11: Lua Filter Callback Execution (size-based)**
        /// **Validates: Requirements 2.8**
        ///
        /// A Lua filter that checks size should correctly filter based on object size.
        #[test]
        fn lua_filter_size_based(size in 0i64..10_000_000i64) {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let threshold = 5_000_000i64;
                let mut callback = LuaFilterCallback::new(8 * 1024 * 1024, false, false);
                callback
                    .load_and_compile_from_string(
                        &format!(
                            "function filter(obj) return obj.size > {} end",
                            threshold
                        ),
                    )
                    .unwrap();

                let object = S3Object::NotVersioning(
                    Object::builder()
                        .key("test-key")
                        .size(size)
                        .last_modified(DateTime::from_secs(1000))
                        .build(),
                );
                let result = callback.filter(&object).await.unwrap();
                let expected = size > threshold;
                prop_assert_eq!(result, expected);
                Ok(())
            })?;
        }

        /// **Property 11: Lua filter receives correct object fields**
        /// **Validates: Requirements 2.8**
        ///
        /// The Lua filter callback must receive all S3Object fields correctly.
        #[test]
        fn lua_filter_receives_all_fields(obj in arb_s3_object()) {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let mut callback = LuaFilterCallback::new(8 * 1024 * 1024, false, false);
                callback
                    .load_and_compile_from_string(
                        r#"
                        function filter(obj)
                            -- Verify all fields are accessible (non-nil for required fields)
                            assert(obj.key ~= nil, "key is nil")
                            assert(obj.last_modified ~= nil, "last_modified is nil")
                            assert(type(obj.size) == "number", "size is not a number")
                            assert(type(obj.is_latest) == "boolean", "is_latest is not boolean")
                            assert(type(obj.is_delete_marker) == "boolean", "is_delete_marker is not boolean")
                            return true
                        end
                        "#,
                    )
                    .unwrap();

                let result = callback.filter(&obj).await.unwrap();
                prop_assert!(result);
                Ok(())
            })?;
        }
    }

    // -------------------------------------------------------------------------
    // Property 14: Lua Callback Type Support
    // **Validates: Requirements 2.12**
    //
    // *For any* Lua script provided, the tool should support both filter callbacks
    // (returning boolean) and event callbacks (receiving events).
    // -------------------------------------------------------------------------

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(30))]

        /// **Property 14: Lua Callback Type Support (filter type)**
        /// **Validates: Requirements 2.12**
        ///
        /// A Lua filter callback that defines `filter()` should be callable and
        /// return a boolean.
        #[test]
        fn lua_filter_callback_returns_boolean(key in arb_s3_key()) {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let mut callback = LuaFilterCallback::new(8 * 1024 * 1024, false, false);
                callback
                    .load_and_compile_from_string(
                        "function filter(obj) return obj.key ~= nil end",
                    )
                    .unwrap();

                let object = S3Object::NotVersioning(
                    Object::builder()
                        .key(&key)
                        .size(0)
                        .last_modified(DateTime::from_secs(0))
                        .build(),
                );
                let result = callback.filter(&object).await;
                // Must be Ok(bool), not an error
                prop_assert!(result.is_ok());
                Ok(())
            })?;
        }

        /// **Property 14: Lua Callback Type Support (event type)**
        /// **Validates: Requirements 2.12**
        ///
        /// A Lua event callback that defines `on_event()` should be callable
        /// and receive structured event data without error.
        #[test]
        fn lua_event_callback_receives_events(
            key in arb_s3_key(),
            event_type_bits in prop::sample::select(vec![
                EventType::PIPELINE_START,
                EventType::PIPELINE_END,
                EventType::DELETE_COMPLETE,
                EventType::DELETE_FAILED,
                EventType::DELETE_FILTERED,
                EventType::PIPELINE_ERROR,
                EventType::STATS_REPORT,
            ]),
        ) {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let mut callback = LuaEventCallback::new(8 * 1024 * 1024, false, false);
                callback
                    .load_and_compile_from_string(
                        r#"
                        last_event_type = nil
                        last_key = nil
                        function on_event(event_data)
                            last_event_type = event_data.event_type
                            last_key = event_data.key
                        end
                        "#,
                    )
                    .unwrap();

                let mut event_data = EventData::new(event_type_bits);
                event_data.key = Some(key.clone());

                // Should not panic
                callback.on_event(event_data).await;

                // Verify the event was received by the Lua script
                let received_type: u64 = callback
                    .lua
                    .get_engine()
                    .globals()
                    .get("last_event_type")
                    .unwrap();
                prop_assert_eq!(received_type, event_type_bits.bits());

                let received_key: Option<String> = callback
                    .lua
                    .get_engine()
                    .globals()
                    .get("last_key")
                    .unwrap();
                prop_assert_eq!(received_key, Some(key));
                Ok(())
            })?;
        }
    }

    // -------------------------------------------------------------------------
    // Property 15: Lua Sandbox Security
    // **Validates: Requirements 2.13, 2.14**
    //
    // *For any* Lua script executed in safe mode (default), attempts to access
    // OS or I/O libraries should fail, and when explicitly enabled, OS/I/O
    // operations should succeed.
    // -------------------------------------------------------------------------

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(20))]

        /// **Property 15: Lua Sandbox Security (safe mode blocks OS)**
        /// **Validates: Requirements 2.13**
        ///
        /// In safe mode, OS library calls should fail for any script that
        /// attempts to use `os.*` functions.
        #[test]
        fn safe_mode_blocks_os_functions(
            os_func in prop::sample::select(vec![
                "os.execute('echo hello')",
                "os.remove('/tmp/test')",
                "os.rename('/tmp/a', '/tmp/b')",
                "os.getenv('HOME')",
            ]),
        ) {
            let engine = LuaScriptCallbackEngine::new_without_os_io_libs(8 * 1024 * 1024);
            let result = engine.load_and_compile(os_func);
            prop_assert!(result.is_err(), "os function should be blocked in safe mode: {}", os_func);
        }

        /// **Property 15: Lua Sandbox Security (safe mode blocks IO)**
        /// **Validates: Requirements 2.13**
        ///
        /// In safe mode, IO library calls should fail for any script that
        /// attempts to use `io.*` functions.
        #[test]
        fn safe_mode_blocks_io_functions(
            io_func in prop::sample::select(vec![
                "io.open('/etc/passwd', 'r')",
                "io.write('hello')",
                "io.lines('/etc/hosts')",
            ]),
        ) {
            let engine = LuaScriptCallbackEngine::new_without_os_io_libs(8 * 1024 * 1024);
            let result = engine.load_and_compile(io_func);
            prop_assert!(result.is_err(), "io function should be blocked in safe mode: {}", io_func);
        }

        /// **Property 15: Lua Sandbox Security (allow_lua_os_library mode)**
        /// **Validates: Requirements 2.14**
        ///
        /// When the OS library is explicitly enabled, OS functions should succeed.
        #[test]
        fn allow_os_library_enables_os_functions(
            os_func in prop::sample::select(vec![
                "local t = os.clock()",
                "local d = os.date()",
                "local t = os.time()",
            ]),
        ) {
            let engine = LuaScriptCallbackEngine::new(8 * 1024 * 1024);
            let result = engine.load_and_compile(os_func);
            prop_assert!(result.is_ok(), "os function should work with allow_lua_os_library: {}", os_func);
        }
    }

    // Non-proptest sandbox tests for completeness

    #[test]
    fn safe_mode_allows_standard_libraries() {
        // Safe mode should still allow standard libraries like string, table, math
        let engine = LuaScriptCallbackEngine::new_without_os_io_libs(8 * 1024 * 1024);

        assert!(
            engine
                .load_and_compile("local x = string.len('hello')")
                .is_ok()
        );
        assert!(engine.load_and_compile("local x = math.max(1, 2)").is_ok());
        assert!(
            engine
                .load_and_compile("local t = {}; table.insert(t, 1)")
                .is_ok()
        );
    }

    #[test]
    fn unsafe_mode_allows_everything() {
        let engine = LuaScriptCallbackEngine::unsafe_new(8 * 1024 * 1024);

        // OS functions should work
        assert!(engine.load_and_compile("local t = os.clock()").is_ok());
        // Debug library should be available in unsafe mode
        assert!(
            engine
                .load_and_compile("local info = debug.getinfo(1)")
                .is_ok()
        );
    }

    #[tokio::test]
    async fn filter_callback_in_safe_mode_cannot_access_filesystem() {
        let mut callback = LuaFilterCallback::new(8 * 1024 * 1024, false, false);
        // Define a filter that tries to use io.open at runtime
        callback
            .load_and_compile_from_string(
                r#"
                function filter(obj)
                    local f = io.open("/etc/passwd", "r")
                    return f ~= nil
                end
                "#,
            )
            .unwrap();

        // The error occurs at runtime when the filter is called, not at compile time
        let object = S3Object::NotVersioning(
            Object::builder()
                .key("test")
                .size(0)
                .last_modified(DateTime::from_secs(0))
                .build(),
        );
        let result = callback.filter(&object).await;
        assert!(
            result.is_err(),
            "io.open should fail in safe mode at runtime"
        );
    }

    #[tokio::test]
    async fn filter_callback_with_os_library_can_use_os_clock() {
        let mut callback = LuaFilterCallback::new(8 * 1024 * 1024, true, false);
        callback
            .load_and_compile_from_string(
                r#"
                function filter(obj)
                    local t = os.clock()
                    return t >= 0
                end
                "#,
            )
            .unwrap();

        let object = S3Object::NotVersioning(
            Object::builder()
                .key("test")
                .size(0)
                .last_modified(DateTime::from_secs(0))
                .build(),
        );
        let result = callback.filter(&object).await.unwrap();
        assert!(result);
    }
}
