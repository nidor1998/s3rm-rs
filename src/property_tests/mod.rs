//! Property-based tests, consolidated from across `src/`.
//!
//! Each sub-module validates specific properties from the requirements
//! document using proptest-based generative testing.
//!
//! Note: `indicator_properties` remains in `bin/s3rm/` because it belongs
//! to the binary crate and imports binary-local modules.

mod additional_properties;
mod aws_config_properties;
mod cicd_properties;
mod cross_platform_properties;
mod event_callback_properties;
mod filter_properties;
mod lib_properties;
mod logging_properties;
#[cfg(feature = "lua_support")]
mod lua_properties;
mod optimistic_locking_properties;
mod rate_limiting_properties;
mod retry_properties;
mod safety_properties;
mod versioning_properties;
