//! Shared test utilities for the s3rm library crate.
//!
//! This module provides canonical helper functions used across multiple test
//! modules, eliminating duplication and ensuring consistency.

use aws_sdk_s3::primitives::DateTime;
use aws_sdk_s3::types::{Object, ObjectVersion, ObjectVersionStorageClass};

use crate::callback::event_manager::EventManager;
use crate::callback::filter_manager::FilterManager;
use crate::config::{Config, FilterConfig, ForceRetryConfig};
use crate::types::{S3Object, StoragePath};

/// Initialise a dummy tracing subscriber for tests.
///
/// Uses `try_init` so that only the first call in a process actually
/// installs the subscriber; subsequent calls are silently ignored.
pub(crate) fn init_dummy_tracing_subscriber() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("dummy=trace")
        .try_init();
}

/// Create a default [`Config`] suitable for most unit / property tests.
///
/// Key defaults: `worker_size=4`, `batch_size=1000`,
/// bucket=`"test-bucket"`, prefix=`"prefix/"`.
pub(crate) fn make_test_config() -> Config {
    Config {
        target: StoragePath::S3 {
            bucket: "test-bucket".to_string(),
            prefix: "prefix/".to_string(),
        },
        show_no_progress: false,
        target_client_config: None,
        force_retry_config: ForceRetryConfig {
            force_retry_count: 0,
            force_retry_interval_milliseconds: 0,
        },
        tracing_config: None,
        worker_size: 4,
        warn_as_error: false,
        dry_run: false,
        rate_limit_objects: None,
        max_parallel_listings: 1,
        object_listing_queue_size: 1000,
        max_parallel_listing_max_depth: 0,
        allow_parallel_listings_in_express_one_zone: false,
        filter_config: FilterConfig::default(),
        max_keys: 1000,
        auto_complete_shell: None,
        event_callback_lua_script: None,
        filter_callback_lua_script: None,
        allow_lua_os_library: false,
        allow_lua_unsafe_vm: false,
        lua_vm_memory_limit: 0,
        if_match: false,
        max_delete: None,
        filter_manager: FilterManager::new(),
        event_manager: EventManager::new(),
        batch_size: 1000,
        delete_all_versions: false,
        force: false,
        test_user_defined_callback: false,
    }
}

/// Create a non-versioned [`S3Object`] with the given key and size.
pub(crate) fn make_s3_object(key: &str, size: i64) -> S3Object {
    S3Object::NotVersioning(
        Object::builder()
            .key(key)
            .size(size)
            .last_modified(DateTime::from_secs(1000))
            .build(),
    )
}

/// Create a versioned [`S3Object`] with the given key, version ID and size.
pub(crate) fn make_versioned_s3_object(key: &str, version_id: &str, size: i64) -> S3Object {
    S3Object::Versioning(
        ObjectVersion::builder()
            .key(key)
            .version_id(version_id)
            .size(size)
            .is_latest(true)
            .storage_class(ObjectVersionStorageClass::Standard)
            .last_modified(DateTime::from_secs(1000))
            .build(),
    )
}
