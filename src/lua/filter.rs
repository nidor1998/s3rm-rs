//! Lua filter callback implementation.
//!
//! Adapted from s3sync's `callback/lua_filter_callback.rs`.
//! Converts `S3Object` to a Lua table and calls the Lua `filter()` function.

use anyhow::Result;
use async_trait::async_trait;
use tracing::error;

use crate::lua::engine::LuaScriptCallbackEngine;
use crate::types::S3Object;
use crate::types::filter_callback::FilterCallback;

/// Lua-based filter callback.
///
/// Loads a Lua script that defines a `filter(object)` function.
/// For each object, converts it to a Lua table and calls the function.
/// Returns `true` if the object should be deleted, `false` to skip it.
pub struct LuaFilterCallback {
    lua: LuaScriptCallbackEngine,
}

impl LuaFilterCallback {
    /// Create a new Lua filter callback with the specified security mode.
    ///
    /// - `unsafe_lua`: Full capabilities (overrides other flags)
    /// - `allow_lua_os_library`: Enables OS library (when `unsafe_lua` is false)
    /// - Default: Safe mode (no OS/IO)
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

    /// Load and compile a Lua filter script from a file path.
    pub fn load_and_compile(&mut self, script_path: &str) -> Result<()> {
        let lua_script = std::fs::read(script_path)?;
        self.lua.load_and_compile(&String::from_utf8(lua_script)?)
    }

    /// Load and compile a Lua filter script from a string.
    #[cfg(test)]
    pub fn load_and_compile_from_string(&mut self, script: &str) -> Result<()> {
        self.lua.load_and_compile(script)
    }

    async fn filter_by_lua(&mut self, object: &S3Object) -> Result<bool> {
        let table = self.lua.get_engine().create_table()?;
        table.set("key", object.key())?;
        table.set("last_modified", object.last_modified().to_string())?;
        table.set("version_id", object.version_id())?;
        table.set("e_tag", object.e_tag())?;
        // Note: is_latest() returns true for NotVersioning objects (they are
        // semantically the latest and only version) and defaults to true when the
        // field is absent (None) from the S3 response.
        table.set("is_latest", object.is_latest())?;
        table.set("is_delete_marker", object.is_delete_marker())?;
        table.set("size", object.size())?;

        let func: mlua::Function = self.lua.get_engine().globals().get("filter")?;
        let result: bool = func.call_async(table).await?;

        Ok(result)
    }
}

#[async_trait]
impl FilterCallback for LuaFilterCallback {
    async fn filter(&mut self, object: &S3Object) -> Result<bool> {
        let result = self.filter_by_lua(object).await;
        if let Err(err) = &result {
            error!("Lua script filter callback error: {}", err);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aws_sdk_s3::primitives::DateTime;
    use aws_sdk_s3::types::Object;

    fn make_test_object(key: &str) -> S3Object {
        S3Object::NotVersioning(
            Object::builder()
                .key(key)
                .size(1024)
                .last_modified(DateTime::from_secs(1000))
                .build(),
        )
    }

    #[tokio::test]
    async fn create_all_modes() {
        let _callback = LuaFilterCallback::new(8 * 1024 * 1024, false, false);
        let _callback = LuaFilterCallback::new(8 * 1024 * 1024, true, false);
        let _callback = LuaFilterCallback::new(0, true, true);
    }

    #[tokio::test]
    async fn filter_returns_true() {
        let mut callback = LuaFilterCallback::new(8 * 1024 * 1024, false, false);
        callback
            .load_and_compile_from_string("function filter(obj) return true end")
            .unwrap();

        let object = make_test_object("test-key");
        let result = callback.filter(&object).await.unwrap();
        assert!(result);
    }

    #[tokio::test]
    async fn filter_returns_false() {
        let mut callback = LuaFilterCallback::new(8 * 1024 * 1024, false, false);
        callback
            .load_and_compile_from_string("function filter(obj) return false end")
            .unwrap();

        let object = make_test_object("test-key");
        let result = callback.filter(&object).await.unwrap();
        assert!(!result);
    }

    #[tokio::test]
    async fn filter_accesses_object_key() {
        let mut callback = LuaFilterCallback::new(8 * 1024 * 1024, false, false);
        callback
            .load_and_compile_from_string(
                r#"
                function filter(obj)
                    return obj.key == "include-me"
                end
                "#,
            )
            .unwrap();

        let include = make_test_object("include-me");
        assert!(callback.filter(&include).await.unwrap());

        let exclude = make_test_object("exclude-me");
        assert!(!callback.filter(&exclude).await.unwrap());
    }

    #[tokio::test]
    async fn filter_accesses_object_size() {
        let mut callback = LuaFilterCallback::new(8 * 1024 * 1024, false, false);
        callback
            .load_and_compile_from_string(
                r#"
                function filter(obj)
                    return obj.size > 500
                end
                "#,
            )
            .unwrap();

        // Object with size 1024 should pass
        let object = make_test_object("big-file");
        assert!(callback.filter(&object).await.unwrap());
    }

    #[tokio::test]
    async fn filter_error_on_missing_function() {
        let mut callback = LuaFilterCallback::new(8 * 1024 * 1024, false, false);
        callback
            .load_and_compile_from_string("-- no filter function defined")
            .unwrap();

        let object = make_test_object("test-key");
        let result = callback.filter(&object).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn filter_error_on_runtime_error() {
        let mut callback = LuaFilterCallback::new(8 * 1024 * 1024, false, false);
        callback
            .load_and_compile_from_string(
                r#"
                function filter(obj)
                    error("intentional error")
                end
                "#,
            )
            .unwrap();

        let object = make_test_object("test-key");
        let result = callback.filter(&object).await;
        assert!(result.is_err());
    }
}
