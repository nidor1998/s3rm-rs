/*!
# s3rm-rs

s3rm-rs is an extremely fast Amazon S3 object deletion tool.
It can be used to delete objects from S3 buckets with powerful filtering,
safety features, and versioning support.

## Features

- **High Performance**: Parallel deletion using S3 batch API (up to 1000 objects per request)
  with configurable worker pools (1–65,535 concurrent workers).
- **Flexible Filtering**: Regex patterns on keys/content-type/metadata/tags, size ranges,
  time ranges, and Lua script-based custom filtering.
- **Safety First**: Dry-run mode, confirmation prompts, force flag, max-delete threshold.
- **Versioning Support**: Delete markers, all-versions deletion for versioned buckets.
- **Library-First**: All CLI features available as a Rust library for programmatic use.
- **s3sync Compatible**: Reuses s3sync's proven infrastructure (~90% code reuse).

## Architecture

s3rm-rs uses a streaming pipeline architecture:

```text
ObjectLister → [Filter Stages] → ObjectDeleter Workers (MPMC) → Terminator
```

All core functionality resides in this library crate. The CLI binary is a thin
wrapper that parses arguments, builds a [`Config`], and runs a [`DeletionPipeline`].

## Quick Start (Library Usage)

```toml
[dependencies]
s3rm-rs = "0.1"
tokio = { version = "1", features = ["full"] }
```

To use s3rm-rs as a library, construct a [`Config`], create a
[`DeletionPipeline`], and call [`run()`](DeletionPipeline::run):

```no_run
use s3rm_rs::Config;
use s3rm_rs::DeletionPipeline;
use s3rm_rs::create_pipeline_cancellation_token;

# async fn example() {
# let config: Config = todo!();
let cancellation_token = create_pipeline_cancellation_token();
let mut pipeline = DeletionPipeline::new(config, cancellation_token).await;

// Close the stats sender if you don't need real-time progress reporting.
pipeline.close_stats_sender();

// Run the pipeline (blocks until all deletions are complete).
pipeline.run().await;

if pipeline.has_error() {
    eprintln!("{:?}", pipeline.get_errors_and_consume().unwrap()[0]);
}

let stats = pipeline.get_deletion_stats();
println!("Deleted {} objects ({} bytes)",
    stats.stats_deleted_objects, stats.stats_deleted_bytes);
# }
```

## Callback Registration

Register Rust callbacks for custom filtering or event handling:

```no_run
use s3rm_rs::{FilterCallback, EventCallback, EventData, S3Object};
use async_trait::async_trait;
use anyhow::Result;

struct MyFilter;

#[async_trait]
impl FilterCallback for MyFilter {
    async fn filter(&mut self, object: &S3Object) -> Result<bool> {
        // Return true to delete, false to skip
        Ok(object.size() > 1024)
    }
}

struct MyEventHandler;

#[async_trait]
impl EventCallback for MyEventHandler {
    async fn on_event(&mut self, event: EventData) {
        println!("Event: {:?}", event.event_type);
    }
}
```

Register callbacks via [`FilterManager`] and [`EventManager`] on the [`Config`]:

```ignore
use s3rm_rs::{Config, FilterManager, EventManager, EventType};

let mut config: Config = /* ... */;
config.filter_manager.register_callback(my_filter);
config.event_manager.register_callback(EventType::ALL_EVENTS, my_handler, false);
```

## Lua Scripting

Lua filter and event callbacks can be registered via script paths in the [`Config`]:

```no_run
# let mut config: s3rm_rs::Config = todo!();
config.filter_callback_lua_script = Some("path/to/filter.lua".to_string());
config.event_callback_lua_script = Some("path/to/event.lua".to_string());
```

Lua scripts run in a sandboxed VM by default (no OS or I/O library access).
Use `allow_lua_os_library` and `allow_lua_unsafe_vm` on [`Config`] to relax restrictions.

For more information, see the [s3sync documentation](https://github.com/nidor1998/s3sync)
as s3rm-rs shares the same Lua integration.
*/

#![allow(clippy::collapsible_if)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::unnecessary_unwrap)]

// ---------------------------------------------------------------------------
// Module declarations
// ---------------------------------------------------------------------------

pub mod callback;
pub mod config;
pub mod deleter;
pub mod filters;
pub mod lister;
#[cfg(feature = "lua_support")]
pub mod lua;
pub mod pipeline;
pub mod safety;
pub mod stage;
pub mod storage;
pub mod terminator;
pub mod types;

// ---------------------------------------------------------------------------
// Root-level re-exports for convenient access
// ---------------------------------------------------------------------------

// Core pipeline
pub use pipeline::DeletionPipeline;

// Configuration
pub use config::Config;
pub use config::args::{CLIArgs, build_config_from_args, parse_from_args};

// Statistics
pub use types::{DeletionStatistics, DeletionStats, DeletionStatsReport};

// Object types
pub use types::{S3Object, S3Target};

// Error types
pub use types::error::{S3rmError, exit_code_from_error, is_cancelled_error};

// Deletion outcome and error types
pub use types::{DeletionError, DeletionEvent, DeletionOutcome};

// Cancellation token
pub use types::token::{PipelineCancellationToken, create_pipeline_cancellation_token};

// Callback traits
pub use types::event_callback::{EventCallback, EventData, EventType};
pub use types::filter_callback::FilterCallback;

// Callback managers
pub use callback::event_manager::EventManager;
pub use callback::filter_manager::FilterManager;

#[cfg(test)]
mod lib_properties;

#[cfg(test)]
mod versioning_properties;

#[cfg(test)]
mod retry_properties;

#[cfg(test)]
mod optimistic_locking_properties;

#[cfg(test)]
mod logging_properties;

#[cfg(test)]
mod aws_config_properties;

#[cfg(test)]
mod rate_limiting_properties;

#[cfg(test)]
mod cross_platform_properties;

#[cfg(test)]
mod cicd_properties;

#[cfg(test)]
mod additional_properties;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn library_crate_loads() {
        // Basic sanity check that the library crate compiles and loads
        assert!(true);
    }

    #[test]
    fn root_re_exports_accessible() {
        // Verify that all root-level re-exports are accessible.
        // This test ensures that the public API surface is correct.
        let _ = std::any::type_name::<DeletionPipeline>();
        let _ = std::any::type_name::<Config>();
        let _ = std::any::type_name::<DeletionStats>();
        let _ = std::any::type_name::<DeletionStatsReport>();
        let _ = std::any::type_name::<DeletionStatistics>();
        let _ = std::any::type_name::<S3Object>();
        let _ = std::any::type_name::<S3Target>();
        let _ = std::any::type_name::<S3rmError>();
        let _ = std::any::type_name::<DeletionError>();
        let _ = std::any::type_name::<DeletionEvent>();
        let _ = std::any::type_name::<DeletionOutcome>();
        let _ = std::any::type_name::<PipelineCancellationToken>();
        let _ = std::any::type_name::<EventData>();
        let _ = std::any::type_name::<EventType>();
        let _ = std::any::type_name::<FilterManager>();
        let _ = std::any::type_name::<EventManager>();
    }

    #[test]
    fn create_cancellation_token_from_root() {
        let token = create_pipeline_cancellation_token();
        assert!(!token.is_cancelled());
    }

    #[test]
    fn error_helpers_accessible() {
        let err = anyhow::anyhow!(S3rmError::Cancelled);
        assert!(is_cancelled_error(&err));
        assert_eq!(exit_code_from_error(&err), 0);
    }
}
