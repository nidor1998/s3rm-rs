//! Callback management for the deletion pipeline.
//!
//! Adapted from s3sync's `callback/` module.
//! Provides filter and event callback managers that wrap trait objects.

pub mod event_manager;
pub mod filter_manager;
pub mod user_defined_event_callback;
pub mod user_defined_filter_callback;

#[cfg(test)]
mod event_callback_properties;
