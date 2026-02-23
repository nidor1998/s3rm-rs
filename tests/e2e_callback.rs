//! E2E tests for callback functionality (Tests 29.24 - 29.30).
//!
//! Tests Rust filter/event callbacks, Lua filter/event callbacks,
//! Lua sandbox enforcement, Lua memory limits, and combined callbacks.

#![cfg(e2e_test)]

mod common;

use common::{CollectingEventCallback, TestHelper};
use s3rm_rs::{EventData, EventType, FilterCallback, S3Object};
use std::io::Write as IoWrite;
use std::sync::{Arc, Mutex};

// ---------------------------------------------------------------------------
// Helper: Key-prefix filter callback
// ---------------------------------------------------------------------------

/// Rust filter callback that passes only objects whose key starts with a prefix.
struct PrefixFilterCallback {
    prefix: String,
}

#[async_trait::async_trait]
impl FilterCallback for PrefixFilterCallback {
    async fn filter(&mut self, object: &S3Object) -> anyhow::Result<bool> {
        Ok(object.key().starts_with(&self.prefix))
    }
}

// ---------------------------------------------------------------------------
// Helper: Size-based filter callback
// ---------------------------------------------------------------------------

/// Rust filter callback that passes only objects larger than a threshold.
struct SizeFilterCallback {
    min_size: i64,
}

#[async_trait::async_trait]
impl FilterCallback for SizeFilterCallback {
    async fn filter(&mut self, object: &S3Object) -> anyhow::Result<bool> {
        Ok(object.size() >= self.min_size)
    }
}

// ---------------------------------------------------------------------------
// 29.24 Rust Filter Callback
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_rust_filter_callback() {
    e2e_timeout!(async {
        // Purpose: Verify that a Rust FilterCallback registered via
        //          config.filter_manager.register_callback() correctly filters
        //          objects during deletion. Only objects passing the filter should
        //          be deleted.
        // Setup:   Upload 20 objects: 10 with keys starting with "delete-",
        //          10 with keys starting with "keep-".
        // Expected: 10 "delete-" objects removed; 10 "keep-" objects remain.
        //
        // Validates: Requirements 2.9, 12.5

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let _guard = helper.bucket_guard(&bucket);

        for i in 0..10 {
            helper
                .put_object(&bucket, &format!("delete-file{i}.dat"), vec![b'd'; 100])
                .await;
        }
        for i in 0..10 {
            helper
                .put_object(&bucket, &format!("keep-file{i}.dat"), vec![b'k'; 100])
                .await;
        }

        let mut config = TestHelper::build_config(vec![&format!("s3://{bucket}/"), "--force"]);

        // Register the Rust filter callback
        config
            .filter_manager
            .register_callback(PrefixFilterCallback {
                prefix: "delete-".to_string(),
            });

        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(
            result.stats.stats_deleted_objects, 10,
            "Should delete exactly 10 delete- objects"
        );

        let remaining = helper.list_objects(&bucket, "keep-").await;
        assert_eq!(remaining.len(), 10, "All keep- objects should remain");

        let deleted = helper.list_objects(&bucket, "delete-").await;
        assert_eq!(deleted.len(), 0, "All delete- objects should be removed");
    });
}

// ---------------------------------------------------------------------------
// 29.25 Rust Event Callback
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_rust_event_callback() {
    e2e_timeout!(async {
        // Purpose: Verify that a Rust EventCallback registered via
        //          config.event_manager.register_callback() receives pipeline
        //          events including PIPELINE_START, DELETE_COMPLETE, and PIPELINE_END.
        // Setup:   Upload 10 objects.
        // Expected: Callback receives PIPELINE_START, 10 DELETE_COMPLETE events,
        //           and PIPELINE_END. Event data includes correct keys and sizes.
        //
        // Validates: Requirements 7.6, 7.7, 12.6

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let _guard = helper.bucket_guard(&bucket);

        for i in 0..10 {
            helper
                .put_object(&bucket, &format!("event/file{i}.dat"), vec![b'e'; 256])
                .await;
        }

        let collected_events = Arc::new(Mutex::new(Vec::new()));
        let callback = CollectingEventCallback {
            events: Arc::clone(&collected_events),
        };

        let mut config =
            TestHelper::build_config(vec![&format!("s3://{bucket}/event/"), "--force"]);
        config
            .event_manager
            .register_callback(EventType::ALL_EVENTS, callback, false);

        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(result.stats.stats_deleted_objects, 10);

        let events = collected_events.lock().unwrap();

        // Check PIPELINE_START
        let starts: Vec<_> = events
            .iter()
            .filter(|e| e.event_type == EventType::PIPELINE_START)
            .collect();
        assert_eq!(starts.len(), 1, "Should receive exactly 1 PIPELINE_START");

        // Check DELETE_COMPLETE events
        let completes: Vec<_> = events
            .iter()
            .filter(|e| e.event_type == EventType::DELETE_COMPLETE)
            .collect();
        assert_eq!(
            completes.len(),
            10,
            "Should receive 10 DELETE_COMPLETE events"
        );

        // Check PIPELINE_END
        let ends: Vec<_> = events
            .iter()
            .filter(|e| e.event_type == EventType::PIPELINE_END)
            .collect();
        assert_eq!(ends.len(), 1, "Should receive exactly 1 PIPELINE_END");
    });
}

// ---------------------------------------------------------------------------
// 29.26 Lua Filter Callback
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_lua_filter_callback() {
    e2e_timeout!(async {
        // Purpose: Verify --filter-callback-lua-script with a Lua filter script.
        //          The Lua script receives object metadata and returns true/false
        //          to decide which objects to delete.
        // Setup:   Upload 20 objects: 10 with .tmp extension, 10 with .dat extension.
        //          Write a Lua filter script that returns true for .tmp files.
        // Expected: 10 .tmp objects deleted; 10 .dat objects remain.
        //
        // Validates: Requirements 2.8, 2.12

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let _guard = helper.bucket_guard(&bucket);

        for i in 0..10 {
            helper
                .put_object(&bucket, &format!("lua/file{i}.tmp"), vec![b't'; 100])
                .await;
        }
        for i in 0..10 {
            helper
                .put_object(&bucket, &format!("lua/file{i}.dat"), vec![b'd'; 100])
                .await;
        }

        // Write a temporary Lua filter script
        let lua_script = tempfile::NamedTempFile::new().unwrap();
        writeln!(
            lua_script.as_file(),
            r#"
    function filter(object)
        local key = object["key"]
        if string.match(key, "%.tmp$") then
            return true
        end
        return false
    end
    "#
        )
        .unwrap();

        let lua_path = lua_script.path().to_str().unwrap();
        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/lua/"),
            "--filter-callback-lua-script",
            lua_path,
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(
            result.stats.stats_deleted_objects, 10,
            "Should delete exactly 10 .tmp objects"
        );

        let remaining_dat = helper.list_objects(&bucket, "lua/").await;
        let dat_count = remaining_dat.iter().filter(|k| k.ends_with(".dat")).count();
        assert_eq!(dat_count, 10, "All .dat objects should remain");
    });
}

// ---------------------------------------------------------------------------
// 29.27 Lua Event Callback
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_lua_event_callback() {
    e2e_timeout!(async {
        // Purpose: Verify --event-callback-lua-script with a Lua event script.
        //          The Lua script receives events during pipeline execution and
        //          can write output to verify it ran. Requires --allow-lua-os-library
        //          for file I/O.
        // Setup:   Upload 10 objects. Write a Lua event script that writes event
        //          counts to a temp file.
        // Expected: All 10 objects deleted; the Lua output file exists with content.
        //
        // Validates: Requirements 2.12, 7.6

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let _guard = helper.bucket_guard(&bucket);

        for i in 0..10 {
            helper
                .put_object(&bucket, &format!("lua-event/file{i}.dat"), vec![b'e'; 100])
                .await;
        }

        // Create output file for Lua to write to
        let output_file = tempfile::NamedTempFile::new().unwrap();
        let output_path = output_file.path().to_str().unwrap().to_string();

        // Write a Lua event script that writes event count to the output file.
        // The Lua event callback API uses `on_event(event_data)`, not `complete()`.
        // We write to the file on each event call (overwriting the previous count).
        let lua_script = tempfile::NamedTempFile::new().unwrap();
        writeln!(
            lua_script.as_file(),
            r#"
    local count = 0
    function on_event(event_data)
        count = count + 1
        local f = io.open("{output_path}", "w")
        if f then
            f:write("events=" .. tostring(count))
            f:close()
        end
    end
    "#,
            output_path = output_path
        )
        .unwrap();

        let lua_path = lua_script.path().to_str().unwrap();
        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/lua-event/"),
            "--event-callback-lua-script",
            lua_path,
            "--allow-lua-os-library",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(result.stats.stats_deleted_objects, 10);

        // Verify the Lua event script wrote output to the file.
        // The script increments a counter on each event and writes "events=N".
        let content = std::fs::read_to_string(&output_path)
            .expect("Lua event output file should exist and be readable");
        assert!(
            !content.is_empty(),
            "Lua event output file should be non-empty (event script should have written to it)"
        );
        assert!(
            content.starts_with("events="),
            "Lua event output should contain 'events=<count>', got: {content}"
        );
        // Parse the count and verify we received at least one event
        let count_str = content.trim_start_matches("events=");
        let count: u64 = count_str
            .parse()
            .unwrap_or_else(|_| panic!("Failed to parse event count from: {content}"));
        assert!(
            count > 0,
            "Lua event script should have received at least 1 event, got {count}"
        );
    });
}

// ---------------------------------------------------------------------------
// 29.28 Lua Sandbox Blocks OS Access
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_lua_sandbox_blocks_os_access() {
    e2e_timeout!(async {
        // Purpose: Verify that the default Lua sandbox blocks OS library access.
        //          A Lua script that tries to call os.execute() should fail.
        // Setup:   Upload 5 objects. Write a Lua filter script that calls
        //          os.execute("echo test").
        // Expected: Pipeline reports error (Lua sandbox violation).
        //
        // Validates: Requirement 2.13

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let _guard = helper.bucket_guard(&bucket);

        for i in 0..5 {
            helper
                .put_object(&bucket, &format!("sandbox/file{i}.dat"), vec![b's'; 100])
                .await;
        }

        // Write a Lua filter script that tries to use os.execute (should be blocked)
        let lua_script = tempfile::NamedTempFile::new().unwrap();
        writeln!(
            lua_script.as_file(),
            r#"
    function filter(object)
        os.execute("echo sandbox_escape")
        return true
    end
    "#
        )
        .unwrap();

        let lua_path = lua_script.path().to_str().unwrap();
        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/sandbox/"),
            "--filter-callback-lua-script",
            lua_path,
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        // The pipeline should report an error due to sandbox violation
        assert!(
            result.has_error,
            "Pipeline should report error for Lua sandbox violation"
        );
    });
}

// ---------------------------------------------------------------------------
// 29.29 Lua VM Memory Limit
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_lua_vm_memory_limit() {
    e2e_timeout!(async {
        // Purpose: Verify --lua-vm-memory-limit enforces a memory cap on the Lua VM.
        //          A script that allocates excessive memory should be terminated.
        // Setup:   Upload 5 objects. Write a Lua filter script that allocates a
        //          large table exceeding 1MB.
        // Expected: Pipeline reports error due to Lua memory limit exceeded.
        //
        // Validates: Requirement 2.14

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let _guard = helper.bucket_guard(&bucket);

        for i in 0..5 {
            helper
                .put_object(&bucket, &format!("memlimit/file{i}.dat"), vec![b'm'; 100])
                .await;
        }

        // Write a Lua script that allocates excessive memory
        let lua_script = tempfile::NamedTempFile::new().unwrap();
        writeln!(
            lua_script.as_file(),
            r#"
    function filter(object)
        local big = {{}}
        for i = 1, 1000000 do
            big[i] = string.rep("x", 100)
        end
        return true
    end
    "#
        )
        .unwrap();

        let lua_path = lua_script.path().to_str().unwrap();
        let config = TestHelper::build_config(vec![
            &format!("s3://{bucket}/memlimit/"),
            "--filter-callback-lua-script",
            lua_path,
            "--lua-vm-memory-limit",
            "1MB",
            "--force",
        ]);
        let result = TestHelper::run_pipeline(config).await;

        assert!(
            result.has_error,
            "Pipeline should report error for Lua memory limit exceeded"
        );
    });
}

// ---------------------------------------------------------------------------
// 29.30 Rust Filter and Event Callbacks Combined
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_rust_filter_and_event_callbacks_combined() {
    e2e_timeout!(async {
        // Purpose: Verify that Rust filter and event callbacks work together.
        //          The filter callback controls which objects are deleted, and
        //          the event callback observes the results.
        // Setup:   Upload 20 objects: 10 large (5KB), 10 small (100B).
        // Expected: 10 large objects deleted; 10 small remain; event callback
        //           receives DELETE_COMPLETE for each deleted object.
        //
        // Validates: Requirements 2.9, 7.6, 12.5, 12.6

        let helper = TestHelper::new().await;
        let bucket = helper.generate_bucket_name();
        helper.create_bucket(&bucket).await;

        let _guard = helper.bucket_guard(&bucket);

        for i in 0..10 {
            helper
                .put_object(
                    &bucket,
                    &format!("combined/large{i}.dat"),
                    vec![b'L'; 5 * 1024],
                )
                .await;
        }
        for i in 0..10 {
            helper
                .put_object(&bucket, &format!("combined/small{i}.dat"), vec![b's'; 100])
                .await;
        }

        let collected_events = Arc::new(Mutex::new(Vec::new()));
        let event_callback = CollectingEventCallback {
            events: Arc::clone(&collected_events),
        };
        let filter_callback = SizeFilterCallback { min_size: 1024 };

        let mut config =
            TestHelper::build_config(vec![&format!("s3://{bucket}/combined/"), "--force"]);
        config.filter_manager.register_callback(filter_callback);
        config
            .event_manager
            .register_callback(EventType::ALL_EVENTS, event_callback, false);

        let result = TestHelper::run_pipeline(config).await;

        assert!(!result.has_error, "Pipeline should complete without errors");
        assert_eq!(
            result.stats.stats_deleted_objects, 10,
            "Should delete exactly 10 large objects"
        );

        // Verify small objects remain
        let remaining = helper.list_objects(&bucket, "combined/small").await;
        assert_eq!(remaining.len(), 10, "All small objects should remain");

        // Verify event callback received DELETE_COMPLETE events
        let events = collected_events.lock().unwrap();
        let delete_completes: Vec<_> = events
            .iter()
            .filter(|e| e.event_type == EventType::DELETE_COMPLETE)
            .collect();
        assert_eq!(
            delete_completes.len(),
            10,
            "Should receive 10 DELETE_COMPLETE events for the large objects"
        );

        // Verify event callback received DELETE_FILTERED events for skipped objects.
        // The SizeFilterCallback is a user-defined filter, so filtered-out objects
        // trigger DELETE_FILTERED events.
        let delete_filtered: Vec<_> = events
            .iter()
            .filter(|e| e.event_type == EventType::DELETE_FILTERED)
            .collect();
        assert_eq!(
            delete_filtered.len(),
            10,
            "Should receive 10 DELETE_FILTERED events for the small objects"
        );
    });
}
