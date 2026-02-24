//! Deletion components for the s3rm-rs pipeline.
//!
//! This module contains the ObjectDeleter worker and the Deleter trait
//! with its two implementations: BatchDeleter and SingleDeleter.
//!
//! **Adapted from s3sync**: ObjectDeleter is adapted from s3sync's ObjectSyncer
//! and ObjectDeleter patterns. Content-type, metadata, and tag filtering are
//! performed within the ObjectDeleter because they require API calls
//! (HeadObject / GetObjectTagging).

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Result, anyhow};
use async_trait::async_trait;
#[cfg(test)]
use aws_sdk_s3::operation::get_object_tagging::GetObjectTaggingOutput;
use aws_sdk_s3::primitives::DateTimeFormat;
use aws_sdk_s3::types::Tag;
use aws_smithy_runtime_api::client::result::SdkError;
use aws_smithy_runtime_api::http::Response;
use aws_smithy_types::body::SdkBody;
use tracing::{debug, error, info, trace, warn};
use urlencoding::encode;

use crate::config::Config;
use crate::stage::{SendResult, Stage};
use crate::types::event_callback::{EventData, EventType};
use crate::types::{DeletionStatistics, DeletionStatsReport, S3Object};

pub mod batch;
pub mod single;

pub use batch::BatchDeleter;
pub use single::SingleDeleter;

// ---------------------------------------------------------------------------
// Deleter trait
// ---------------------------------------------------------------------------

/// Result of a deletion operation, reporting which keys succeeded and which failed.
#[derive(Debug, Clone, Default)]
pub struct DeleteResult {
    /// Keys (with optional version IDs) that were successfully deleted.
    pub deleted: Vec<DeletedKey>,
    /// Keys that failed with error details.
    pub failed: Vec<FailedKey>,
}

/// A successfully deleted key.
#[derive(Debug, Clone)]
pub struct DeletedKey {
    pub key: String,
    pub version_id: Option<String>,
}

/// A key that failed to delete.
#[derive(Debug, Clone)]
pub struct FailedKey {
    pub key: String,
    pub version_id: Option<String>,
    pub error_code: String,
    pub error_message: String,
}

/// Trait for deletion backends (batch or single mode).
///
/// Both BatchDeleter and SingleDeleter implement this trait.
#[async_trait]
pub trait Deleter: Send + Sync {
    /// Delete the given objects from S3.
    ///
    /// Returns detailed results indicating which objects were deleted and which failed.
    async fn delete(&self, objects: &[S3Object], config: &Config) -> Result<DeleteResult>;
}

// ---------------------------------------------------------------------------
// Filter name constants (matching s3sync's naming)
// ---------------------------------------------------------------------------

const INCLUDE_CONTENT_TYPE_REGEX_FILTER_NAME: &str = "include_content_type_regex_filter";
const EXCLUDE_CONTENT_TYPE_REGEX_FILTER_NAME: &str = "exclude_content_type_regex_filter";
const INCLUDE_METADATA_REGEX_FILTER_NAME: &str = "include_metadata_regex_filter";
const EXCLUDE_METADATA_REGEX_FILTER_NAME: &str = "exclude_metadata_regex_filter";
const INCLUDE_TAG_REGEX_FILTER_NAME: &str = "include_tag_regex_filter";
const EXCLUDE_TAG_REGEX_FILTER_NAME: &str = "exclude_tag_regex_filter";

// ---------------------------------------------------------------------------
// ObjectDeleter worker
// ---------------------------------------------------------------------------

/// Pipeline worker that reads objects from the input channel, applies
/// content-type / metadata / tag filters, and deletes matching objects.
///
/// Adapted from s3sync's ObjectSyncer and ObjectDeleter patterns.
pub struct ObjectDeleter {
    worker_index: u16,
    base: Stage,
    deletion_stats_report: Arc<DeletionStatsReport>,
    delete_counter: Arc<AtomicU64>,
    /// Deletion backend: BatchDeleter (batch_size > 1) or SingleDeleter (batch_size == 1).
    deleter: Box<dyn Deleter>,
    /// Buffer of objects that passed filtering, flushed to the deleter at batch boundaries.
    buffer: Vec<S3Object>,
    /// Effective batch size (min of config.batch_size and MAX_BATCH_SIZE).
    effective_batch_size: usize,
}

impl ObjectDeleter {
    pub fn new(
        base: Stage,
        worker_index: u16,
        deletion_stats_report: Arc<DeletionStatsReport>,
        delete_counter: Arc<AtomicU64>,
    ) -> Self {
        // Clone the target storage for the deleter backend.
        let target_storage = base.target.clone();

        let effective_batch_size = if base.config.batch_size <= 1 {
            1
        } else {
            (base.config.batch_size as usize).min(batch::MAX_BATCH_SIZE)
        };

        let deleter: Box<dyn Deleter> = if base.config.batch_size <= 1 {
            Box::new(SingleDeleter::new(target_storage))
        } else {
            Box::new(BatchDeleter::new(target_storage))
        };

        Self {
            worker_index,
            base,
            deletion_stats_report,
            delete_counter,
            deleter,
            buffer: Vec::with_capacity(effective_batch_size),
            effective_batch_size,
        }
    }

    /// Main entry point: read objects from channel, filter, and delete.
    pub async fn delete(&mut self) -> Result<()> {
        debug!(worker_index = self.worker_index, "delete worker started.");
        self.receive_and_delete().await
    }

    async fn receive_and_delete(&mut self) -> Result<()> {
        loop {
            tokio::select! {
                recv_result = self.base.receiver.as_ref().unwrap().recv() => {
                    match recv_result {
                        Ok(object) => {
                            self.process_object(object).await?;
                            // Check if process_object triggered cancellation
                            // (e.g. max_delete threshold). Exit immediately without
                            // flushing the buffer.
                            if self.base.cancellation_token.is_cancelled() {
                                info!(worker_index = self.worker_index, "delete worker has been cancelled.");
                                return Ok(());
                            }
                        },
                        Err(_) if self.base.receiver.as_ref().unwrap().is_closed() => {
                            // Channel closed: all senders dropped. Flush remaining buffer.
                            self.delete_buffered_objects().await?;
                            debug!(worker_index = self.worker_index, "delete worker has been completed.");
                            break;
                        }
                        Err(e) => {
                            error!(worker_index = self.worker_index, error = %e, "unexpected channel error.");
                            break;
                        }
                    }
                },
                _ = self.base.cancellation_token.cancelled() => {
                    info!(worker_index = self.worker_index, "delete worker has been cancelled.");
                    return Ok(());
                }
            }
        }

        Ok(())
    }

    async fn process_object(&mut self, object: S3Object) -> Result<()> {
        // Check max_delete threshold
        let deleted_count = self.delete_counter.fetch_add(1, Ordering::SeqCst) + 1;
        if let Some(max_delete) = self.base.config.max_delete {
            if deleted_count > max_delete {
                self.base
                    .send_stats(DeletionStatistics::DeleteWarning {
                        key: object.key().to_string(),
                    })
                    .await;
                self.base.set_warning();

                let message = "--max-delete has been reached. delete operation has been cancelled.";
                warn!(key = object.key(), message);

                let mut event_data = EventData::new(EventType::DELETE_WARNING);
                event_data.key = Some(object.key().to_string());
                event_data.message = Some(message.to_string());
                self.base
                    .config
                    .event_manager
                    .trigger_event(event_data)
                    .await;

                self.base.cancellation_token.cancel();
                return Ok(());
            }
        }

        let key = object.key();

        // --- Content-type / metadata filtering (requires HeadObject) ---
        let needs_head_object = self
            .base
            .config
            .filter_config
            .include_content_type_regex
            .is_some()
            || self
                .base
                .config
                .filter_config
                .exclude_content_type_regex
                .is_some()
            || self
                .base
                .config
                .filter_config
                .include_metadata_regex
                .is_some()
            || self
                .base
                .config
                .filter_config
                .exclude_metadata_regex
                .is_some();

        if needs_head_object {
            let head_result = self
                .base
                .target
                .head_object(key, object.version_id().map(|v| v.to_string()))
                .await;

            match head_result {
                Ok(head_output) => {
                    // Content-type filters
                    if !self.decide_delete_by_include_content_type_regex(
                        key,
                        head_output.content_type(),
                    ) {
                        self.emit_filter_skip(&object, INCLUDE_CONTENT_TYPE_REGEX_FILTER_NAME)
                            .await;
                        return Ok(());
                    }
                    if !self.decide_delete_by_exclude_content_type_regex(
                        key,
                        head_output.content_type(),
                    ) {
                        self.emit_filter_skip(&object, EXCLUDE_CONTENT_TYPE_REGEX_FILTER_NAME)
                            .await;
                        return Ok(());
                    }

                    // Metadata filters
                    if !self.decide_delete_by_include_metadata_regex(key, head_output.metadata()) {
                        self.emit_filter_skip(&object, INCLUDE_METADATA_REGEX_FILTER_NAME)
                            .await;
                        return Ok(());
                    }
                    if !self.decide_delete_by_exclude_metadata_regex(key, head_output.metadata()) {
                        self.emit_filter_skip(&object, EXCLUDE_METADATA_REGEX_FILTER_NAME)
                            .await;
                        return Ok(());
                    }
                }
                Err(e) => {
                    if is_not_found_error(&e) {
                        debug!(
                            worker_index = self.worker_index,
                            key = key,
                            "object not found during head_object, skipping."
                        );
                        self.base
                            .send_stats(DeletionStatistics::DeleteSkip {
                                key: key.to_string(),
                            })
                            .await;
                        return Ok(());
                    }

                    self.base.cancellation_token.cancel();
                    error!(
                        worker_index = self.worker_index,
                        key = key,
                        error = e.to_string(),
                        "head_object failed."
                    );
                    return Err(anyhow!("head_object failed for key: {}", key));
                }
            }
        }

        // --- Tag filtering (requires GetObjectTagging) ---
        let needs_tagging = self.base.config.filter_config.include_tag_regex.is_some()
            || self.base.config.filter_config.exclude_tag_regex.is_some();

        if needs_tagging {
            let tagging_result = self
                .base
                .target
                .get_object_tagging(key, object.version_id().map(|v| v.to_string()))
                .await;

            match tagging_result {
                Ok(tagging_output) => {
                    let tags = tagging_output.tag_set();
                    if !self.decide_delete_by_include_tag_regex(key, Some(tags)) {
                        self.emit_filter_skip(&object, INCLUDE_TAG_REGEX_FILTER_NAME)
                            .await;
                        return Ok(());
                    }
                    if !self.decide_delete_by_exclude_tag_regex(key, Some(tags)) {
                        self.emit_filter_skip(&object, EXCLUDE_TAG_REGEX_FILTER_NAME)
                            .await;
                        return Ok(());
                    }
                }
                Err(e) => {
                    if is_not_found_error(&e) {
                        debug!(
                            worker_index = self.worker_index,
                            key = key,
                            "object not found during get_object_tagging, skipping."
                        );
                        self.base
                            .send_stats(DeletionStatistics::DeleteSkip {
                                key: key.to_string(),
                            })
                            .await;
                        return Ok(());
                    }

                    self.base.cancellation_token.cancel();
                    error!(
                        worker_index = self.worker_index,
                        key = key,
                        error = e.to_string(),
                        "get_object_tagging failed."
                    );
                    return Err(anyhow!("get_object_tagging failed for key: {}", key));
                }
            }
        }

        // --- Delegate deletion ---
        // Buffer the object for batch/single deletion via the deleter backend.
        // When if_match is enabled, BatchDeleter includes per-object ETags in the
        // DeleteObjects request for conditional deletion.
        self.buffer.push(object);
        if self.buffer.len() >= self.effective_batch_size {
            self.delete_buffered_objects().await?;
        }

        Ok(())
    }

    /// Delete buffered objects by delegating to the Deleter backend
    /// (BatchDeleter or SingleDeleter).
    ///
    /// In dry-run mode, actual S3 API calls are skipped. All objects are
    /// treated as successfully deleted and statistics/events are emitted
    /// with `dry_run = true`.
    async fn delete_buffered_objects(&mut self) -> Result<()> {
        if self.buffer.is_empty() {
            return Ok(());
        }

        // Exit immediately if pipeline was cancelled (e.g. max_delete threshold).
        if self.base.cancellation_token.is_cancelled() {
            return Ok(());
        }

        let batch = std::mem::take(&mut self.buffer);
        let is_dry_run = self.base.config.dry_run;

        // Build a lookup from (key, version_id) to object for resolving sizes after deletion.
        // Using composite key avoids collisions when multiple versions of the same key
        // are in the same batch.
        let obj_by_key: HashMap<(String, Option<String>), &S3Object> = batch
            .iter()
            .map(|o| {
                let key = o.key().to_string();
                let version_id = o.version_id().map(|v| v.to_string());
                ((key, version_id), o)
            })
            .collect();

        // In dry-run mode, simulate successful deletion for all objects
        // without making any S3 API calls.
        let delete_result = if is_dry_run {
            DeleteResult {
                deleted: batch
                    .iter()
                    .map(|o| DeletedKey {
                        key: o.key().to_string(),
                        version_id: o.version_id().map(|v| v.to_string()),
                    })
                    .collect(),
                failed: vec![],
            }
        } else {
            match self.deleter.delete(&batch, &self.base.config).await {
                Ok(result) => result,
                Err(e) => {
                    // Entire operation failed — cancel pipeline
                    self.base.cancellation_token.cancel();
                    error!(
                        worker_index = self.worker_index,
                        error = e.to_string(),
                        "delete worker has been cancelled with error."
                    );
                    return Err(anyhow!("delete worker has been cancelled with error."));
                }
            }
        };

        // Emit per-object stats and events for successes.
        for dk in &delete_result.deleted {
            let lookup_key = (dk.key.clone(), dk.version_id.clone());
            let size = obj_by_key
                .get(&lookup_key)
                .map(|o| o.size() as u64)
                .unwrap_or(0);

            // Per-object info logging (matching s3sync pattern).
            // Dry-run uses "[dry-run]" prefix like s3sync's storage layer.
            let version_id_str = dk.version_id.as_deref().unwrap_or("");
            let last_modified_str = obj_by_key
                .get(&lookup_key)
                .map(|o| {
                    o.last_modified()
                        .fmt(DateTimeFormat::DateTime)
                        .unwrap_or_default()
                })
                .unwrap_or_default();
            if is_dry_run {
                info!(
                    key = dk.key.as_str(),
                    version_id = version_id_str,
                    size = size,
                    last_modified = last_modified_str.as_str(),
                    "[dry-run] delete completed.",
                );
            } else {
                info!(
                    key = dk.key.as_str(),
                    version_id = version_id_str,
                    size = size,
                    last_modified = last_modified_str.as_str(),
                    "delete completed.",
                );
            }

            self.deletion_stats_report.increment_deleted(size);

            self.base
                .send_stats(DeletionStatistics::DeleteComplete {
                    key: dk.key.clone(),
                })
                .await;
            self.base
                .send_stats(DeletionStatistics::DeleteBytes(size))
                .await;

            let mut event_data = EventData::new(EventType::DELETE_COMPLETE);
            event_data.dry_run = is_dry_run;
            event_data.key = Some(dk.key.clone());
            event_data.version_id = dk.version_id.clone();
            event_data.size = Some(size);
            event_data.last_modified = Some(last_modified_str.clone());
            self.base
                .config
                .event_manager
                .trigger_event(event_data)
                .await;
        }

        // Emit per-object stats and events for failures.
        for fk in &delete_result.failed {
            self.deletion_stats_report.increment_failed();

            self.base
                .send_stats(DeletionStatistics::DeleteWarning {
                    key: fk.key.clone(),
                })
                .await;

            let mut event_data = EventData::new(EventType::DELETE_FAILED);
            event_data.dry_run = is_dry_run;
            event_data.key = Some(fk.key.clone());
            event_data.version_id = fk.version_id.clone();
            event_data.error_message = Some(format!("{}: {}", fk.error_code, fk.error_message));
            self.base
                .config
                .event_manager
                .trigger_event(event_data)
                .await;
        }

        // Set warning flag and optionally stop pipeline on deletion failures.
        if !delete_result.failed.is_empty() {
            self.base.set_warning();

            if self.base.config.warn_as_error {
                warn!(
                    worker_index = self.worker_index,
                    failed_count = delete_result.failed.len(),
                    "deletion failures promoted to error (--warn-as-error). cancelling pipeline.",
                );
                self.base.cancellation_token.cancel();
                return Ok(());
            }
        }

        // Forward all objects to next stage
        for obj in batch {
            if self.base.send(obj).await? == SendResult::Closed {
                return Ok(());
            }
        }

        Ok(())
    }

    /// Emit a `DeleteSkip` stat and a `DELETE_FILTERED` event for an object
    /// rejected by a content-type, metadata, or tag filter in the deleter.
    async fn emit_filter_skip(&self, object: &S3Object, filter_name: &str) {
        self.base
            .send_stats(DeletionStatistics::DeleteSkip {
                key: object.key().to_string(),
            })
            .await;

        if self.base.config.event_manager.is_callback_registered() {
            let mut event_data = EventData::new(EventType::DELETE_FILTERED);
            event_data.key = Some(object.key().to_string());
            event_data.version_id = object.version_id().map(|v| v.to_string());
            event_data.size = Some(object.size() as u64);
            event_data.last_modified = Some(
                object
                    .last_modified()
                    .fmt(DateTimeFormat::DateTime)
                    .unwrap_or_default(),
            );
            event_data.message = Some(format!("Object filtered by {filter_name}"));
            self.base
                .config
                .event_manager
                .trigger_event(event_data)
                .await;
        }
    }

    // -----------------------------------------------------------------------
    // Content-type regex filters (adapted from s3sync's ObjectSyncer)
    // -----------------------------------------------------------------------

    fn decide_delete_by_include_content_type_regex(
        &self,
        key: &str,
        content_type: Option<&str>,
    ) -> bool {
        if self
            .base
            .config
            .filter_config
            .include_content_type_regex
            .is_none()
        {
            return true;
        }

        if content_type.is_none() {
            trace!(
                name = INCLUDE_CONTENT_TYPE_REGEX_FILTER_NAME,
                worker_index = self.worker_index,
                key = key,
                "Content-Type = None",
            );
            return false;
        }

        let is_match = self
            .base
            .config
            .filter_config
            .include_content_type_regex
            .as_ref()
            .unwrap()
            .is_match(content_type.unwrap())
            .unwrap();

        trace!(
            name = INCLUDE_CONTENT_TYPE_REGEX_FILTER_NAME,
            worker_index = self.worker_index,
            key = key,
            content_type = content_type.unwrap(),
            is_match = is_match,
        );

        is_match
    }

    fn decide_delete_by_exclude_content_type_regex(
        &self,
        key: &str,
        content_type: Option<&str>,
    ) -> bool {
        if self
            .base
            .config
            .filter_config
            .exclude_content_type_regex
            .is_none()
        {
            return true;
        }

        if content_type.is_none() {
            trace!(
                name = EXCLUDE_CONTENT_TYPE_REGEX_FILTER_NAME,
                worker_index = self.worker_index,
                key = key,
                "Content-Type = None",
            );
            return true;
        }

        let is_match = self
            .base
            .config
            .filter_config
            .exclude_content_type_regex
            .as_ref()
            .unwrap()
            .is_match(content_type.unwrap())
            .unwrap();

        trace!(
            name = EXCLUDE_CONTENT_TYPE_REGEX_FILTER_NAME,
            worker_index = self.worker_index,
            key = key,
            content_type = content_type.unwrap(),
            is_match = is_match,
        );

        !is_match
    }

    // -----------------------------------------------------------------------
    // Metadata regex filters (adapted from s3sync's ObjectSyncer)
    // -----------------------------------------------------------------------

    fn decide_delete_by_include_metadata_regex(
        &self,
        key: &str,
        metadata: Option<&HashMap<String, String>>,
    ) -> bool {
        if self
            .base
            .config
            .filter_config
            .include_metadata_regex
            .is_none()
        {
            return true;
        }

        if metadata.is_none() || metadata.as_ref().unwrap().is_empty() {
            debug!(
                name = INCLUDE_METADATA_REGEX_FILTER_NAME,
                worker_index = self.worker_index,
                key = key,
                "metadata = None",
            );
            return false;
        }

        let formatted = format_metadata(metadata.unwrap());
        let is_match = self
            .base
            .config
            .filter_config
            .include_metadata_regex
            .as_ref()
            .unwrap()
            .is_match(&formatted)
            .unwrap();

        trace!(
            name = INCLUDE_METADATA_REGEX_FILTER_NAME,
            worker_index = self.worker_index,
            key = key,
            metadata = formatted,
            is_match = is_match,
        );

        is_match
    }

    fn decide_delete_by_exclude_metadata_regex(
        &self,
        key: &str,
        metadata: Option<&HashMap<String, String>>,
    ) -> bool {
        if self
            .base
            .config
            .filter_config
            .exclude_metadata_regex
            .is_none()
        {
            return true;
        }

        if metadata.is_none() || metadata.as_ref().unwrap().is_empty() {
            trace!(
                name = EXCLUDE_METADATA_REGEX_FILTER_NAME,
                worker_index = self.worker_index,
                key = key,
                "metadata = None",
            );
            return true;
        }

        let formatted = format_metadata(metadata.unwrap());
        let is_match = self
            .base
            .config
            .filter_config
            .exclude_metadata_regex
            .as_ref()
            .unwrap()
            .is_match(&formatted)
            .unwrap();

        trace!(
            name = EXCLUDE_METADATA_REGEX_FILTER_NAME,
            worker_index = self.worker_index,
            key = key,
            metadata = formatted,
            is_match = is_match,
        );

        !is_match
    }

    // -----------------------------------------------------------------------
    // Tag regex filters (adapted from s3sync's ObjectSyncer)
    // -----------------------------------------------------------------------

    fn decide_delete_by_include_tag_regex(&self, key: &str, tags: Option<&[Tag]>) -> bool {
        if self.base.config.filter_config.include_tag_regex.is_none() {
            return true;
        }

        if tags.is_none() {
            trace!(
                name = INCLUDE_TAG_REGEX_FILTER_NAME,
                worker_index = self.worker_index,
                key = key,
                "tags = None",
            );
            return false;
        }

        let formatted = format_tags(tags.unwrap());
        let is_match = self
            .base
            .config
            .filter_config
            .include_tag_regex
            .as_ref()
            .unwrap()
            .is_match(&formatted)
            .unwrap();

        trace!(
            name = INCLUDE_TAG_REGEX_FILTER_NAME,
            worker_index = self.worker_index,
            key = key,
            tags = formatted,
            is_match = is_match,
        );

        is_match
    }

    fn decide_delete_by_exclude_tag_regex(&self, key: &str, tags: Option<&[Tag]>) -> bool {
        if self.base.config.filter_config.exclude_tag_regex.is_none() {
            return true;
        }

        if tags.is_none() {
            trace!(
                name = EXCLUDE_TAG_REGEX_FILTER_NAME,
                worker_index = self.worker_index,
                key = key,
                "tags = None",
            );
            return true;
        }

        let formatted = format_tags(tags.unwrap());
        let is_match = self
            .base
            .config
            .filter_config
            .exclude_tag_regex
            .as_ref()
            .unwrap()
            .is_match(&formatted)
            .unwrap();

        trace!(
            name = EXCLUDE_TAG_REGEX_FILTER_NAME,
            worker_index = self.worker_index,
            key = key,
            tags = formatted,
            is_match = is_match,
        );

        !is_match
    }
}

// ---------------------------------------------------------------------------
// Formatting helpers (adapted from s3sync)
// ---------------------------------------------------------------------------

/// Format metadata key-value pairs as a sorted comma-separated string for regex matching.
///
/// Reused from s3sync's `format_metadata` function in `src/types/mod.rs`.
pub fn format_metadata(metadata: &HashMap<String, String>) -> String {
    let mut sorted_keys: Vec<&String> = metadata.keys().collect();
    sorted_keys.sort();

    sorted_keys
        .iter()
        .map(|key| {
            let value = encode(&metadata[*key]).to_string();
            format!("{key}={value}")
        })
        .collect::<Vec<String>>()
        .join(",")
}

/// Format tag key-value pairs as an ampersand-separated string for regex matching.
///
/// Reused from s3sync's `format_tags` function in `src/types/mod.rs`.
pub fn format_tags(tags: &[Tag]) -> String {
    let mut tags = tags
        .iter()
        .map(|tag| (tag.key(), tag.value()))
        .collect::<Vec<_>>();

    tags.sort_by(|a, b| a.0.cmp(b.0));

    tags.iter()
        .map(|(key, value)| {
            let escaped_key = encode(key).to_string();
            let encoded_value = encode(value).to_string();
            format!("{escaped_key}={encoded_value}")
        })
        .collect::<Vec<String>>()
        .join("&")
}

/// Generate a tagging string from GetObjectTaggingOutput for display/logging.
///
/// Reused from s3sync's `generate_tagging_string` function in `src/pipeline/syncer.rs`.
#[cfg(test)]
pub fn generate_tagging_string(
    get_object_tagging_output: &Option<GetObjectTaggingOutput>,
) -> Option<String> {
    let output = get_object_tagging_output.as_ref()?;

    let tags = output.tag_set();
    if tags.is_empty() {
        return Some(String::new());
    }

    Some(
        tags.iter()
            .map(|tag| format!("{}={}", encode(tag.key()), encode(tag.value())))
            .collect::<Vec<_>>()
            .join("&"),
    )
}

// ---------------------------------------------------------------------------
// Error classification helpers (adapted from s3sync)
// ---------------------------------------------------------------------------

/// Check if the error is a "not found" (404) error.
fn is_not_found_error(err: &anyhow::Error) -> bool {
    // HeadObject NotFound
    if let Some(SdkError::ServiceError(e)) = err
        .downcast_ref::<SdkError<aws_sdk_s3::operation::head_object::HeadObjectError, Response<SdkBody>>>()
    {
        if matches!(
            e.err(),
            aws_sdk_s3::operation::head_object::HeadObjectError::NotFound(_)
        ) {
            return true;
        }
    }

    // GetObjectTagging 404
    if let Some(SdkError::ServiceError(e)) = err.downcast_ref::<SdkError<
        aws_sdk_s3::operation::get_object_tagging::GetObjectTaggingError,
        Response<SdkBody>,
    >>() {
        if let Some(code) = e.err().meta().code() {
            if code == "NoSuchKey" || code == "NotFound" {
                return true;
            }
        }
        if e.raw().status().as_u16() == 404 {
            return true;
        }
    }

    false
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests;
