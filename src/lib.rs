/*!
# s3rm-rs

s3rm-rs is a fast Amazon S3 object deletion tool.
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
s3rm-rs = "0.2"
tokio = { version = "1", features = ["full"] }
```

The easiest way is [`build_config_from_args`] — pass CLI-style arguments
and get a ready-to-run [`Config`]:

```no_run
use s3rm_rs::{build_config_from_args, DeletionPipeline, create_pipeline_cancellation_token};

#[tokio::main]
async fn main() {
    // Same arguments you would pass to the s3rm CLI.
    let config = build_config_from_args([
        "s3rm",
        "s3://my-bucket/logs/2024/",
        "--dry-run",
        "--force",
    ]).expect("invalid arguments");

    let token = create_pipeline_cancellation_token();
    let mut pipeline = DeletionPipeline::new(config, token).await;
    // The pipeline sends real-time stats to a channel for progress reporting.
    // Close the sender if you aren't reading from get_stats_receiver(),
    // otherwise the channel fills up and the pipeline stalls.
    pipeline.close_stats_sender();

    pipeline.run().await;

    let stats = pipeline.get_deletion_stats();
    println!("Deleted {} objects ({} bytes)",
        stats.stats_deleted_objects, stats.stats_deleted_bytes);
}
```

Or build a [`Config`] programmatically with [`Config::for_target`]:

```no_run
# use s3rm_rs::Config;
let mut config = Config::for_target("my-bucket", "logs/2024/");
config.dry_run = true;          // preview without deleting
config.worker_size = 100;       // more concurrent workers
config.max_delete = Some(5000); // stop after 5 000 deletions
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

// ---------------------------------------------------------------------------
// Module declarations
// ---------------------------------------------------------------------------

pub mod callback;
pub mod config;
pub(crate) mod deleter;
pub(crate) mod filters;
pub(crate) mod lister;
#[cfg(feature = "lua_support")]
pub(crate) mod lua;
pub(crate) mod pipeline;
pub(crate) mod safety;
pub(crate) mod stage;
pub(crate) mod storage;
pub(crate) mod terminator;
pub mod types;

#[cfg(test)]
pub(crate) mod test_utils;

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

/// Per-object deletion error returned when an individual object fails to delete.
pub use types::DeletionError;

/// Per-object deletion event emitted for each processed object (success or failure).
pub use types::DeletionEvent;

/// Outcome of a single object deletion attempt (success with metadata, or error).
pub use types::DeletionOutcome;

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
