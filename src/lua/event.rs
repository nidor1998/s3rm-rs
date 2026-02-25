//! Lua event callback implementation.
//!
//! Adapted from s3sync's `callback/lua_event_callback.rs`.
//! Converts `EventData` to a Lua table and calls the Lua `on_event()` function.

use anyhow::Result;
use async_trait::async_trait;
use tracing::warn;

use crate::lua::engine::LuaScriptCallbackEngine;
use crate::types::event_callback::{EventCallback, EventData};

/// Lua-based event callback.
///
/// Loads a Lua script that defines an `on_event(event_data)` function.
/// For each event, converts `EventData` to a Lua table and calls the function.
pub struct LuaEventCallback {
    pub(crate) lua: LuaScriptCallbackEngine,
}

impl LuaEventCallback {
    /// Create a new Lua event callback with the specified security mode.
    #[allow(clippy::new_without_default)]
    pub fn new(memory_limit: usize, allow_lua_os_library: bool, unsafe_lua: bool) -> Self {
        let lua = if unsafe_lua {
            LuaScriptCallbackEngine::unsafe_new(memory_limit)
        } else if allow_lua_os_library {
            LuaScriptCallbackEngine::new(memory_limit)
        } else {
            LuaScriptCallbackEngine::new_without_os_io_libs(memory_limit)
        };

        Self { lua }
    }

    /// Load and compile a Lua event script from a file path.
    pub fn load_and_compile(&mut self, script_path: &str) -> Result<()> {
        let lua_script = std::fs::read(script_path)?;
        self.lua.load_and_compile(&String::from_utf8(lua_script)?)
    }

    /// Load and compile a Lua event script from a string.
    #[cfg(test)]
    pub fn load_and_compile_from_string(&mut self, script: &str) -> Result<()> {
        self.lua.load_and_compile(script)
    }
}

#[async_trait]
impl EventCallback for LuaEventCallback {
    async fn on_event(&mut self, event_data: EventData) {
        let table = match self.lua.get_engine().create_table() {
            Ok(t) => t,
            Err(e) => {
                warn!("Failed to create Lua table for event: {}", e);
                return;
            }
        };

        // Set all event data fields on the Lua table
        let _ = table.set("event_type", event_data.event_type.bits());
        let _ = table.set("dry_run", event_data.dry_run);
        let _ = table.set("key", event_data.key.clone());
        let _ = table.set("version_id", event_data.version_id.clone());
        let _ = table.set("size", event_data.size);
        let _ = table.set("last_modified", event_data.last_modified.clone());
        let _ = table.set("e_tag", event_data.e_tag.clone());
        let _ = table.set("error_message", event_data.error_message.clone());
        let _ = table.set("message", event_data.message.clone());

        // Statistics fields
        let _ = table.set("stats_deleted_objects", event_data.stats_deleted_objects);
        let _ = table.set("stats_deleted_bytes", event_data.stats_deleted_bytes);
        let _ = table.set("stats_failed_objects", event_data.stats_failed_objects);
        let _ = table.set("stats_skipped_objects", event_data.stats_skipped_objects);
        let _ = table.set("stats_error_count", event_data.stats_error_count);
        let _ = table.set("stats_duration_sec", event_data.stats_duration_sec);
        let _ = table.set("stats_objects_per_sec", event_data.stats_objects_per_sec);

        let func: mlua::Result<mlua::Function> = self.lua.get_engine().globals().get("on_event");
        if let Err(e) = func {
            warn!("Lua function 'on_event' not found: {}", e);
            return;
        }

        let func_result: mlua::Result<()> = func.unwrap().call_async(table).await;
        if let Err(e) = func_result {
            warn!("Error executing Lua event callback: {}", e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::event_callback::EventType;

    #[tokio::test]
    async fn create_all_modes() {
        let _callback = LuaEventCallback::new(8 * 1024 * 1024, false, false);
        let _callback = LuaEventCallback::new(8 * 1024 * 1024, true, false);
        let _callback = LuaEventCallback::new(0, true, true);
    }

    #[tokio::test]
    async fn on_event_with_valid_script() {
        let mut callback = LuaEventCallback::new(8 * 1024 * 1024, false, false);
        callback
            .load_and_compile_from_string(
                r#"
                function on_event(event_data)
                    -- Just accept the event without error
                end
                "#,
            )
            .unwrap();

        let mut event_data = EventData::new(EventType::PIPELINE_START);
        event_data.message = Some("test message".to_string());
        callback.on_event(event_data).await;
    }

    #[tokio::test]
    async fn on_event_missing_function_does_not_panic() {
        let mut callback = LuaEventCallback::new(8 * 1024 * 1024, false, false);
        callback
            .load_and_compile_from_string("-- no on_event function")
            .unwrap();

        let event_data = EventData::new(EventType::PIPELINE_START);
        // Should log a warning but not panic
        callback.on_event(event_data).await;
    }

    #[tokio::test]
    async fn on_event_runtime_error_does_not_panic() {
        let mut callback = LuaEventCallback::new(8 * 1024 * 1024, false, false);
        callback
            .load_and_compile_from_string(
                r#"
                function on_event(event_data)
                    error("intentional error")
                end
                "#,
            )
            .unwrap();

        let event_data = EventData::new(EventType::DELETE_COMPLETE);
        // Should log a warning but not panic
        callback.on_event(event_data).await;
    }

    #[tokio::test]
    async fn on_event_accesses_event_fields() {
        let mut callback = LuaEventCallback::new(8 * 1024 * 1024, false, false);
        callback
            .load_and_compile_from_string(
                r#"
                received_key = nil
                function on_event(event_data)
                    received_key = event_data.key
                end
                "#,
            )
            .unwrap();

        let mut event_data = EventData::new(EventType::DELETE_COMPLETE);
        event_data.key = Some("test/key.txt".to_string());
        callback.on_event(event_data).await;

        // Verify the Lua script received the key
        let key: Option<String> = callback
            .lua
            .get_engine()
            .globals()
            .get("received_key")
            .unwrap();
        assert_eq!(key, Some("test/key.txt".to_string()));
    }

    #[tokio::test]
    async fn on_event_receives_statistics() {
        let mut callback = LuaEventCallback::new(8 * 1024 * 1024, false, false);
        callback
            .load_and_compile_from_string(
                r#"
                received_deleted = nil
                function on_event(event_data)
                    received_deleted = event_data.stats_deleted_objects
                end
                "#,
            )
            .unwrap();

        let mut event_data = EventData::new(EventType::STATS_REPORT);
        event_data.stats_deleted_objects = Some(42);
        callback.on_event(event_data).await;

        let deleted: Option<u64> = callback
            .lua
            .get_engine()
            .globals()
            .get("received_deleted")
            .unwrap();
        assert_eq!(deleted, Some(42));
    }
}
