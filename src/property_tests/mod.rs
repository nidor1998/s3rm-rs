//! Root-level property-based tests, consolidated from `src/`.
//!
//! Each sub-module validates specific properties from the requirements
//! document using proptest-based generative testing.

mod additional_properties;
mod aws_config_properties;
mod cicd_properties;
mod cross_platform_properties;
mod lib_properties;
mod logging_properties;
mod optimistic_locking_properties;
mod rate_limiting_properties;
mod retry_properties;
mod versioning_properties;
