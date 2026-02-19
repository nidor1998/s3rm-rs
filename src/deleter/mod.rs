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
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use aws_sdk_s3::operation::delete_object::DeleteObjectError;
use aws_sdk_s3::operation::get_object_tagging::GetObjectTaggingOutput;
use aws_sdk_s3::types::Tag;
use aws_smithy_runtime_api::client::result::SdkError;
use aws_smithy_runtime_api::http::Response;
use aws_smithy_types::body::SdkBody;
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};
use tracing::{debug, error, info, trace, warn};

use crate::config::Config;
use crate::stage::{SendResult, Stage};
use crate::types::event_callback::{EventData, EventType};
use crate::types::{DeletionStatistics, DeletionStatsReport, ObjectKeyMap, S3Object};

pub mod batch;
pub mod single;

pub use batch::BatchDeleter;
pub use single::SingleDeleter;

// ---------------------------------------------------------------------------
// Deleter trait
// ---------------------------------------------------------------------------

/// Trait for deletion backends (batch or single mode).
///
/// Both BatchDeleter and SingleDeleter implement this trait.
#[async_trait]
pub trait Deleter: Send + Sync {
    /// Delete the given objects from S3.
    ///
    /// Returns the number of successfully deleted objects.
    async fn delete(&self, objects: &[S3Object], config: &Config) -> Result<usize>;
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

/// Characters that MUST be percent-encoded in tag keys/values (matching s3sync's urlencoding crate).
const TAG_ENCODE_SET: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'~');

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
    deletion_stats_report: Arc<Mutex<DeletionStatsReport>>,
    target_key_map: Option<ObjectKeyMap>,
    delete_counter: Arc<AtomicU64>,
}

impl ObjectDeleter {
    pub fn new(
        base: Stage,
        worker_index: u16,
        deletion_stats_report: Arc<Mutex<DeletionStatsReport>>,
        target_key_map: Option<ObjectKeyMap>,
        delete_counter: Arc<AtomicU64>,
    ) -> Self {
        Self {
            worker_index,
            base,
            deletion_stats_report,
            target_key_map,
            delete_counter,
        }
    }

    /// Main entry point: read objects from channel, filter, and delete.
    pub async fn delete(&self) -> Result<()> {
        debug!(worker_index = self.worker_index, "delete worker started.");
        self.receive_and_delete().await
    }

    async fn receive_and_delete(&self) -> Result<()> {
        loop {
            tokio::select! {
                recv_result = self.base.receiver.as_ref().unwrap().recv() => {
                    match recv_result {
                        Ok(object) => {
                            self.process_object(object).await?;
                        },
                        Err(_) => {
                            // Channel closed — normal shutdown
                            debug!(worker_index = self.worker_index, "delete worker has been completed.");
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

    async fn process_object(&self, object: S3Object) -> Result<()> {
        // Check max_delete threshold
        self.delete_counter.fetch_add(1, Ordering::SeqCst);
        let deleted_count = self.delete_counter.load(Ordering::SeqCst);
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
                        return Ok(());
                    }
                    if !self.decide_delete_by_exclude_content_type_regex(
                        key,
                        head_output.content_type(),
                    ) {
                        return Ok(());
                    }

                    // Metadata filters
                    if !self.decide_delete_by_include_metadata_regex(key, head_output.metadata()) {
                        return Ok(());
                    }
                    if !self.decide_delete_by_exclude_metadata_regex(key, head_output.metadata()) {
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
                        return Ok(());
                    }
                    if !self.decide_delete_by_exclude_tag_regex(key, Some(tags)) {
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

        // --- Delete the object ---
        let target_etag = if self.base.config.if_match {
            self.get_etag_from_target_key_map(key)
        } else {
            None
        };

        let result = self
            .base
            .target
            .delete_object(
                key,
                object.version_id().map(|v| v.to_string()),
                target_etag.clone(),
            )
            .await;

        if let Err(e) = result {
            if is_precondition_failed_error(&e) {
                self.base
                    .send_stats(DeletionStatistics::DeleteWarning {
                        key: key.to_string(),
                    })
                    .await;
                self.base.set_warning();

                let message = "delete precondition(if-match) failed. skipping.";
                warn!(
                    key = key,
                    if_match = target_etag,
                    error = e.to_string(),
                    message
                );

                let mut event_data = EventData::new(EventType::DELETE_WARNING);
                event_data.key = Some(key.to_string());
                event_data.version_id = object.version_id().map(|v| v.to_string());
                event_data.message = Some(message.to_string());
                self.base
                    .config
                    .event_manager
                    .trigger_event(event_data)
                    .await;

                if self.base.config.warn_as_error {
                    self.base.cancellation_token.cancel();
                    error!(
                        worker_index = self.worker_index,
                        "delete worker has been cancelled with error."
                    );
                    return Err(e);
                }

                self.deletion_stats_report
                    .lock()
                    .unwrap()
                    .increment_failed();
                return Ok(());
            }

            self.base.cancellation_token.cancel();
            error!(
                worker_index = self.worker_index,
                error = e.to_string(),
                "delete worker has been cancelled with error."
            );

            self.deletion_stats_report
                .lock()
                .unwrap()
                .increment_failed();

            let mut event_data = EventData::new(EventType::DELETE_FAILED);
            event_data.key = Some(key.to_string());
            event_data.version_id = object.version_id().map(|v| v.to_string());
            event_data.error_message = Some(e.to_string());
            self.base
                .config
                .event_manager
                .trigger_event(event_data)
                .await;

            return Err(anyhow!("delete worker has been cancelled with error."));
        }

        // Success
        let size = object.size() as u64;
        self.deletion_stats_report
            .lock()
            .unwrap()
            .increment_deleted(size);

        self.base
            .send_stats(DeletionStatistics::DeleteComplete {
                key: key.to_string(),
            })
            .await;
        self.base
            .send_stats(DeletionStatistics::DeleteBytes(size))
            .await;

        let mut event_data = EventData::new(EventType::DELETE_COMPLETE);
        event_data.key = Some(key.to_string());
        event_data.version_id = object.version_id().map(|v| v.to_string());
        event_data.size = Some(size);
        self.base
            .config
            .event_manager
            .trigger_event(event_data)
            .await;

        if self.base.send(object).await? == SendResult::Closed {
            return Ok(());
        }

        Ok(())
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

    // -----------------------------------------------------------------------
    // ETag helper (adapted from s3sync's ObjectDeleter)
    // -----------------------------------------------------------------------

    fn get_etag_from_target_key_map(&self, key: &str) -> Option<String> {
        if let Some(target_key_map) = self.target_key_map.as_ref() {
            let target_key_map_map = target_key_map.lock().unwrap();
            let result = target_key_map_map.get(key);
            if let Some(entry) = result {
                return entry.e_tag().map(|e| e.to_string());
            }
        }

        None
    }
}

// ---------------------------------------------------------------------------
// Formatting helpers (adapted from s3sync)
// ---------------------------------------------------------------------------

/// Format metadata key-value pairs as a URL-query-style string for regex matching.
///
/// Adapted from s3sync's `format_metadata` function.
pub fn format_metadata(metadata: &HashMap<String, String>) -> String {
    let mut pairs: Vec<String> = metadata
        .iter()
        .map(|(k, v)| {
            format!(
                "{}={}",
                utf8_percent_encode(k, TAG_ENCODE_SET),
                utf8_percent_encode(v, TAG_ENCODE_SET),
            )
        })
        .collect();
    pairs.sort();
    pairs.join("&")
}

/// Format tag key-value pairs as a URL-query-style string for regex matching.
///
/// Adapted from s3sync's `format_tags` function.
pub fn format_tags(tags: &[Tag]) -> String {
    let mut pairs: Vec<String> = tags
        .iter()
        .map(|tag| {
            format!(
                "{}={}",
                utf8_percent_encode(tag.key(), TAG_ENCODE_SET),
                utf8_percent_encode(tag.value(), TAG_ENCODE_SET),
            )
        })
        .collect();
    pairs.sort();
    pairs.join("&")
}

/// Generate a tagging string from GetObjectTaggingOutput for display/logging.
pub fn generate_tagging_string(
    get_object_tagging_output: &Option<GetObjectTaggingOutput>,
) -> Option<String> {
    get_object_tagging_output.as_ref().map(|output| {
        output
            .tag_set()
            .iter()
            .map(|tag| {
                format!(
                    "{}={}",
                    utf8_percent_encode(tag.key(), TAG_ENCODE_SET),
                    utf8_percent_encode(tag.value(), TAG_ENCODE_SET),
                )
            })
            .collect::<Vec<_>>()
            .join("&")
    })
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

/// Check if the error is a precondition-failed (412) error from DeleteObject.
fn is_precondition_failed_error(err: &anyhow::Error) -> bool {
    if let Some(SdkError::ServiceError(e)) =
        err.downcast_ref::<SdkError<DeleteObjectError, Response<SdkBody>>>()
    {
        if let Some(code) = e.err().meta().code() {
            return code == "PreconditionFailed";
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests;
