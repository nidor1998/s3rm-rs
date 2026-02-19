//! Lua VM integration for s3rm-rs.
//!
//! Reused from s3sync's `lua/` module. Provides:
//! - `LuaScriptCallbackEngine`: Lua VM with configurable security modes
//! - `LuaFilterCallback`: Lua-based object filter callback
//! - `LuaEventCallback`: Lua-based event callback
//!
//! The Lua integration supports three security modes:
//! - **Safe mode** (default): No OS or I/O library access
//! - **Allow OS library**: Enables Lua OS library functions
//! - **Unsafe mode**: Full Lua VM capabilities

pub mod engine;
pub mod event;
pub mod filter;
mod lua_properties;
