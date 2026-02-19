//! Lua script callback engine.
//!
//! Reused from s3sync's `lua/lua_script_callback_engine.rs`.
//! Provides a Lua VM with configurable security modes and memory limits.

#[allow(unused_imports)]
use anyhow::Result;

#[cfg(feature = "lua_support")]
use mlua::{Lua, LuaOptions, StdLib};

#[allow(unused_imports)]
use tracing::debug;

/// Lua VM engine wrapping `mlua::Lua` with security mode configuration.
///
/// Three construction modes are available:
/// - `new()`: All standard libraries enabled (including OS and I/O)
/// - `new_without_os_io_libs()`: Safe mode — OS and I/O libraries removed
/// - `unsafe_new()`: Unsafe mode — full capabilities including debug library
///
/// Reused from s3sync with no modifications.
#[cfg(feature = "lua_support")]
pub struct LuaScriptCallbackEngine {
    engine: Lua,
}

#[cfg(feature = "lua_support")]
impl LuaScriptCallbackEngine {
    /// Create a Lua engine with all standard libraries enabled.
    ///
    /// Includes OS and I/O libraries. Use when `--allow-lua-os-library` is set.
    pub fn new(memory_limit: usize) -> Self {
        debug!(
            "Creating Lua engine with all libraries enabled, memory limit: {} bytes",
            memory_limit
        );

        let engine = Lua::new();
        engine.set_memory_limit(memory_limit).unwrap();
        LuaScriptCallbackEngine { engine }
    }

    /// Create a Lua engine in safe mode (no OS or I/O libraries).
    ///
    /// This is the default mode. Prevents Lua scripts from accessing
    /// the file system, executing processes, or performing I/O operations.
    pub fn new_without_os_io_libs(memory_limit: usize) -> Self {
        debug!(
            "Creating Lua engine without OS library, memory limit: {} bytes",
            memory_limit
        );

        let engine = Lua::new_with(
            StdLib::ALL_SAFE ^ (StdLib::OS | StdLib::IO),
            LuaOptions::default(),
        )
        .unwrap();
        engine.set_memory_limit(memory_limit).unwrap();
        LuaScriptCallbackEngine { engine }
    }

    /// Create an unsafe Lua engine with full capabilities.
    ///
    /// # Safety
    /// This enables the debug library and other unsafe features.
    /// Use only when `--allow-lua-unsafe-vm` is explicitly set.
    pub fn unsafe_new(memory_limit: usize) -> Self {
        debug!("Creating Lua engine with unsafe mode enabled");

        let engine;
        unsafe { engine = Lua::unsafe_new() };
        engine.set_memory_limit(memory_limit).unwrap();
        LuaScriptCallbackEngine { engine }
    }

    /// Get a reference to the underlying `mlua::Lua` engine.
    pub fn get_engine(&self) -> &Lua {
        &self.engine
    }

    /// Load and compile a Lua script string.
    ///
    /// Executes the script to define global functions (e.g., `filter`, `on_event`)
    /// that can be called later by the filter or event callbacks.
    pub fn load_and_compile(&self, script: &str) -> Result<()> {
        self.engine
            .load(script)
            .exec()
            .map_err(|e| anyhow::anyhow!("Failed to load and compile Lua script: {}", e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "lua_support")]
    #[tokio::test]
    async fn create_all_modes() {
        let _lua_engine = LuaScriptCallbackEngine::new(8 * 1024 * 1024);
        let _lua_engine_without_os =
            LuaScriptCallbackEngine::new_without_os_io_libs(8 * 1024 * 1024);
        let _lua_engine_unsafe = LuaScriptCallbackEngine::unsafe_new(8 * 1024 * 1024);
    }

    #[cfg(feature = "lua_support")]
    #[tokio::test]
    async fn load_and_compile_valid_script() {
        let engine = LuaScriptCallbackEngine::new(8 * 1024 * 1024);
        let result = engine.load_and_compile("function filter(obj) return true end");
        assert!(result.is_ok());
    }

    #[cfg(feature = "lua_support")]
    #[tokio::test]
    async fn load_and_compile_invalid_script() {
        let engine = LuaScriptCallbackEngine::new(8 * 1024 * 1024);
        let result = engine.load_and_compile("this is not valid lua %%%");
        assert!(result.is_err());
    }

    #[cfg(feature = "lua_support")]
    #[tokio::test]
    async fn safe_mode_blocks_os_library() {
        let engine = LuaScriptCallbackEngine::new_without_os_io_libs(8 * 1024 * 1024);
        let result = engine.load_and_compile("os.execute('echo hello')");
        assert!(result.is_err());
    }

    #[cfg(feature = "lua_support")]
    #[tokio::test]
    async fn safe_mode_blocks_io_library() {
        let engine = LuaScriptCallbackEngine::new_without_os_io_libs(8 * 1024 * 1024);
        let result = engine.load_and_compile("io.open('/etc/passwd', 'r')");
        assert!(result.is_err());
    }

    #[cfg(feature = "lua_support")]
    #[tokio::test]
    async fn full_mode_allows_os_library() {
        let engine = LuaScriptCallbackEngine::new(8 * 1024 * 1024);
        // os.clock() is a safe OS function that returns CPU time
        let result = engine.load_and_compile("local t = os.clock()");
        assert!(result.is_ok());
    }
}
