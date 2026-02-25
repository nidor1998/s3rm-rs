//! Example: Using s3rm-rs as a library with Rust callbacks.
//!
//! This example demonstrates how to:
//! 1. Build a [`Config`] from CLI-style arguments
//! 2. Register a custom Rust filter callback
//! 3. Register a custom Rust event callback
//! 4. Run the [`DeletionPipeline`] and inspect results
//!
//! Run with:
//! ```sh
//! cargo run --example library_usage -- s3://my-bucket/prefix/ --dry-run --force
//! ```

use anyhow::Result;
use async_trait::async_trait;
use s3rm_rs::{
    Config, DeletionPipeline, EventCallback, EventData, EventType, FilterCallback, S3Object,
    build_config_from_args, create_pipeline_cancellation_token,
};

// ---------------------------------------------------------------------------
// Custom filter: only delete objects larger than 1 KB
// ---------------------------------------------------------------------------

struct SizeFilter {
    min_bytes: i64,
}

#[async_trait]
impl FilterCallback for SizeFilter {
    async fn filter(&mut self, object: &S3Object) -> Result<bool> {
        Ok(object.size() >= self.min_bytes)
    }
}

// ---------------------------------------------------------------------------
// Custom event handler: print deletion events to stdout
// ---------------------------------------------------------------------------

struct LoggingEventHandler;

#[async_trait]
impl EventCallback for LoggingEventHandler {
    async fn on_event(&mut self, event: EventData) {
        if event.event_type.contains(EventType::DELETE_COMPLETE) {
            println!(
                "  Deleted: {} ({} bytes)",
                event.key.as_deref().unwrap_or("?"),
                event.size.unwrap_or(0),
            );
        }
        if event.event_type.contains(EventType::DELETE_FAILED) {
            eprintln!(
                "  FAILED:  {} - {}",
                event.key.as_deref().unwrap_or("?"),
                event.error_message.as_deref().unwrap_or("unknown"),
            );
        }
        if event.event_type.contains(EventType::PIPELINE_END) {
            println!("Pipeline finished.");
        }
    }
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> Result<()> {
    // 1. Build Config from real CLI arguments (same parser as the s3rm binary).
    //    You could also construct Config manually if you prefer.
    let mut config: Config =
        build_config_from_args(std::env::args_os()).map_err(|e| anyhow::anyhow!(e))?;

    // 2. Register a Rust filter callback (objects < 1 KB will be skipped).
    config
        .filter_manager
        .register_callback(SizeFilter { min_bytes: 1024 });

    // 3. Register a Rust event callback for all event types.
    config.event_manager.register_callback(
        EventType::ALL_EVENTS,
        LoggingEventHandler,
        false, // not dry-run-only
    );

    // 4. Create a cancellation token (wire to Ctrl+C if desired).
    let token = create_pipeline_cancellation_token();

    // 5. Build and run the pipeline.
    let mut pipeline = DeletionPipeline::new(config, token).await;

    // Close the stats sender if you don't need the stats receiver channel.
    pipeline.close_stats_sender();

    pipeline.run().await;

    // 6. Check for errors.
    if pipeline.has_error() {
        let errors = pipeline.get_errors_and_consume().unwrap();
        for err in &errors {
            eprintln!("Pipeline error: {err:?}");
        }
    }

    // 7. Print summary statistics.
    let stats = pipeline.get_deletion_stats();
    println!(
        "Summary: {} deleted ({} bytes), {} failed, {:.1}s",
        stats.stats_deleted_objects,
        stats.stats_deleted_bytes,
        stats.stats_failed_objects,
        stats.duration.as_secs_f64(),
    );

    Ok(())
}
