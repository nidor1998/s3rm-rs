//! Batch deletion using the S3 DeleteObjects API.
//!
//! Groups objects into batches of up to 1000 and calls the S3 batch
//! delete API for high-throughput deletion.

use std::borrow::Cow;
use std::collections::HashMap;

use anyhow::Result;
use async_trait::async_trait;
use aws_sdk_s3::types::{DeletedObject, ObjectIdentifier};
use tracing::{debug, warn};

use crate::config::Config;
use crate::types::{DeletionStatistics, S3Object};

use super::{DeleteResult, DeletedKey, Deleter, FailedKey};

/// Maximum objects per batch DeleteObjects API call (S3 limit).
pub const MAX_BATCH_SIZE: usize = 1000;

/// Error code recorded for a requested object that a DeleteObjects response
/// neither reported as deleted nor as failed.
pub const NOT_REPORTED_ERROR_CODE: &str = "NotReported";

/// Determines whether an S3 batch deletion error code is retryable.
///
/// Retryable errors are transient server-side issues that may succeed
/// on a subsequent attempt:
/// - `InternalError` / `ServiceUnavailable` — transient server errors
/// - `SlowDown` — throttling
/// - `RequestTimeout` — transient network/timeout
///
/// Non-retryable errors (e.g. `AccessDenied`, `NoSuchKey`) are permanent
/// and should not be retried.
pub(crate) fn is_retryable_error_code(code: &str) -> bool {
    matches!(
        code,
        "InternalError" | "SlowDown" | "ServiceUnavailable" | "RequestTimeout" | "unknown"
    )
}

/// Deletes objects in batches using the S3 DeleteObjects API.
///
/// Groups objects into batches of up to `batch_size` (max 1000) and
/// calls DeleteObjects for each batch. Partial failures are reported
/// via logging and the returned result contains per-key details.
pub struct BatchDeleter {
    target: crate::storage::Storage,
}

impl BatchDeleter {
    pub fn new(target: crate::storage::Storage) -> Self {
        Self { target }
    }

    /// Retries a failed key using individual DeleteObject calls with force-retry logic.
    ///
    /// When a batch DeleteObjects call reports a retryable error for a key,
    /// this method falls back to individual DeleteObject attempts with the
    /// configured retry count and interval.
    ///
    /// Returns `Ok(DeletedKey)` on success, or `Err(FailedKey)` if all retries
    /// are exhausted.
    async fn retry_failed_key_with_single_delete(
        &self,
        key: &str,
        version_id: Option<&str>,
        if_match_etag: Option<String>,
        code: &str,
        message: &str,
        config: &Config,
    ) -> std::result::Result<DeletedKey, FailedKey> {
        let force_retry_count = config.force_retry_config.force_retry_count;
        let force_retry_interval = config.force_retry_config.force_retry_interval_milliseconds;

        for attempt in 0..=force_retry_count {
            if attempt > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(force_retry_interval)).await;
            }

            match self
                .target
                .delete_object(
                    key,
                    version_id.map(|v| v.to_string()),
                    if_match_etag.clone(),
                )
                .await
            {
                Ok(_) => {
                    return Ok(DeletedKey {
                        key: key.to_string(),
                        version_id: version_id.map(|v| v.to_string()),
                    });
                }
                Err(e) => {
                    warn!(
                        key = key,
                        version_id = version_id,
                        attempt = attempt + 1,
                        max_attempts = force_retry_count + 1,
                        error = %e,
                        "S3 DeleteObject fallback attempt {}/{} failed for key '{}'.",
                        attempt + 1, force_retry_count + 1, key,
                    );
                }
            }
        }

        warn!(
            key = key,
            version_id = version_id,
            code = code,
            message = message,
            "S3 DeleteObject fallback exhausted all {} retries for key '{}': {} ({}).",
            force_retry_count + 1,
            key,
            code,
            message,
        );

        Err(FailedKey {
            key: key.to_string(),
            version_id: version_id.map(|v| v.to_string()),
            error_code: code.to_string(),
            error_message: message.to_string(),
        })
    }

    /// Retries an object via single DeleteObject (using its own key, version ID
    /// and, when `if_match` is set, its own ETag) and records the outcome.
    async fn retry_and_record(
        &self,
        obj: &S3Object,
        code: &str,
        message: &str,
        config: &Config,
        result: &mut DeleteResult,
    ) {
        let if_match_etag = if config.if_match {
            obj.e_tag().map(|s| s.to_string())
        } else {
            None
        };

        match self
            .retry_failed_key_with_single_delete(
                obj.key(),
                obj.version_id(),
                if_match_etag,
                code,
                message,
                config,
            )
            .await
        {
            Ok(deleted_key) => result.deleted.push(deleted_key),
            Err(failed_key) => {
                self.target
                    .send_stats(DeletionStatistics::DeleteError {
                        key: obj.key().to_string(),
                    })
                    .await;
                result.failed.push(failed_key);
            }
        }
    }

    /// Records a non-retryable failure for `obj` and reports it to the stats channel.
    async fn record_failure(
        &self,
        obj: &S3Object,
        code: &str,
        message: &str,
        result: &mut DeleteResult,
    ) {
        warn!(
            key = obj.key(),
            version_id = obj.version_id(),
            code = code,
            message = message,
            "S3 DeleteObjects partial failure for key '{}': {} ({}).",
            obj.key(),
            code,
            message,
        );
        self.target
            .send_stats(DeletionStatistics::DeleteError {
                key: obj.key().to_string(),
            })
            .await;
        result.failed.push(FailedKey {
            key: obj.key().to_string(),
            version_id: obj.version_id().map(|v| v.to_string()),
            error_code: code.to_string(),
            error_message: message.to_string(),
        });
    }

    /// Resolves what is left after the unambiguous matching pass: `Deleted`
    /// entries and `Error` entries that could not be matched to exactly one
    /// requested object (no key, an empty key, a key that was not requested,
    /// or a key shared by several requested versions without a version ID),
    /// and the requested objects no entry accounted for.
    ///
    /// - With no leftover errors, leftover `Deleted` entries may take the
    ///   first compatible object (e.g. several versions of a key reported
    ///   without version IDs).
    /// - With leftover errors and no leftover `Deleted` entries, the errors
    ///   are taken to refer to the unaccounted objects when the counts are
    ///   equal. If every error is non-retryable, each object is recorded as
    ///   failed; otherwise each object is retried with a single DeleteObject,
    ///   whose own outcome decides whether it is deleted or failed. Which
    ///   error belongs to which object is unknown, so when the codes differ a
    ///   failure is reported with code `unknown` and a message listing them.
    /// - Otherwise no guess is made.
    ///
    /// Leftover entries that are not used are only logged: they say nothing
    /// reliable about a requested object. Every requested object that is
    /// still unaccounted is recorded as failed with code
    /// [`NOT_REPORTED_ERROR_CODE`] (it may still exist) and is never retried.
    #[allow(clippy::too_many_arguments)]
    async fn resolve_leftovers(
        &self,
        chunk: &[S3Object],
        index: &HashMap<String, Vec<usize>>,
        accounted: &mut [bool],
        pending_deleted: Vec<&DeletedObject>,
        unmatched_errors: Vec<(String, String)>,
        config: &Config,
        result: &mut DeleteResult,
    ) {
        if unmatched_errors.is_empty() {
            for deleted in pending_deleted {
                match match_entry(
                    deleted.key(),
                    deleted.version_id(),
                    chunk,
                    index,
                    accounted,
                    true,
                ) {
                    Some(i) => {
                        accounted[i] = true;
                        result.deleted.push(deleted_key_of(&chunk[i]));
                    }
                    None => warn_ignored_deleted(deleted),
                }
            }
        } else {
            let unaccounted: Vec<&S3Object> = unaccounted_objects(chunk, accounted).collect();
            if pending_deleted.is_empty() && unaccounted.len() == unmatched_errors.len() {
                self.attribute_errors(unaccounted, unmatched_errors, config, result)
                    .await;
                return;
            }

            for deleted in pending_deleted {
                warn_ignored_deleted(deleted);
            }
            for (code, message) in &unmatched_errors {
                warn!(
                    code = code,
                    message = message,
                    "S3 DeleteObjects returned an error that cannot be attributed to a \
                     requested object; ignoring it: {} ({}).",
                    code,
                    message,
                );
            }
        }

        let unaccounted: Vec<&S3Object> = unaccounted_objects(chunk, accounted).collect();
        for obj in unaccounted {
            self.record_failure(
                obj,
                NOT_REPORTED_ERROR_CODE,
                "DeleteObjects response did not report this object as deleted or failed",
                result,
            )
            .await;
        }
    }

    /// Applies `errors` to `objects` (same length) as described in
    /// [`resolve_leftovers`](Self::resolve_leftovers).
    async fn attribute_errors(
        &self,
        objects: Vec<&S3Object>,
        errors: Vec<(String, String)>,
        config: &Config,
        result: &mut DeleteResult,
    ) {
        warn!(
            errors = errors.len(),
            "S3 DeleteObjects returned {} error(s) without an identifiable object; attributing \
             them to the object(s) not reported as deleted or failed.",
            errors.len(),
        );

        let (code, message) = if errors.iter().all(|(c, _)| *c == errors[0].0) {
            errors[0].clone()
        } else {
            let codes: Vec<&str> = errors.iter().map(|(c, _)| c.as_str()).collect();
            (
                "unknown".to_string(),
                format!(
                    "DeleteObjects errors without an identifiable object: {}",
                    codes.join(", ")
                ),
            )
        };
        let all_non_retryable = errors.iter().all(|(c, _)| !is_retryable_error_code(c));

        for obj in objects {
            if all_non_retryable {
                self.record_failure(obj, &code, &message, result).await;
            } else {
                self.retry_and_record(obj, &code, &message, config, result)
                    .await;
            }
        }
    }
}

fn warn_ignored_deleted(deleted: &DeletedObject) {
    warn!(
        key = deleted.key(),
        version_id = deleted.version_id(),
        "S3 DeleteObjects reported a deleted entry that matches no requested object; \
         ignoring it.",
    );
}

fn unaccounted_objects<'a>(
    chunk: &'a [S3Object],
    accounted: &'a [bool],
) -> impl Iterator<Item = &'a S3Object> {
    chunk
        .iter()
        .zip(accounted)
        .filter(|(_, done)| !**done)
        .map(|(obj, _)| obj)
}

/// The [`DeletedKey`] for a requested object, using its own identity.
fn deleted_key_of(obj: &S3Object) -> DeletedKey {
    DeletedKey {
        key: obj.key().to_string(),
        version_id: obj.version_id().map(|v| v.to_string()),
    }
}

/// Normalizes line endings the way an XML parser does (CR LF and lone CR
/// become LF), so a key echoed in a parsed response still matches the
/// requested key.
fn normalize_key(key: &str) -> Cow<'_, str> {
    if key.contains('\r') {
        Cow::Owned(key.replace("\r\n", "\n").replace('\r', "\n"))
    } else {
        Cow::Borrowed(key)
    }
}

/// Indices of the requested objects in `chunk`, by normalized key.
fn index_by_key(chunk: &[S3Object]) -> HashMap<String, Vec<usize>> {
    let mut index: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, obj) in chunk.iter().enumerate() {
        index
            .entry(normalize_key(obj.key()).into_owned())
            .or_default()
            .push(i);
    }
    index
}

/// Returns true if a response entry's version ID is compatible with a
/// requested object's: they must be equal when both are present; an entry
/// without a version ID, or a request without one (S3 may echo a
/// delete-marker version ID), is compatible.
fn versions_compatible(entry: Option<&str>, requested: Option<&str>) -> bool {
    match (entry, requested) {
        (Some(a), Some(b)) => a == b,
        _ => true,
    }
}

/// Finds the not-yet-accounted object in `chunk` that a response entry
/// refers to. An exact version match wins; otherwise the entry must be
/// compatible with exactly one object with that key. With `first_fit`, an
/// entry compatible with several objects takes the first of them instead.
/// `None` means the entry is ambiguous or names no requested object.
fn match_entry(
    key: Option<&str>,
    version_id: Option<&str>,
    chunk: &[S3Object],
    index: &HashMap<String, Vec<usize>>,
    accounted: &[bool],
    first_fit: bool,
) -> Option<usize> {
    let key = key.filter(|k| !k.is_empty())?;
    let indices = index.get(normalize_key(key).as_ref())?;
    let open = || indices.iter().copied().filter(|&i| !accounted[i]);

    if let Some(i) = open().find(|&i| chunk[i].version_id() == version_id) {
        return Some(i);
    }

    let mut candidates = open().filter(|&i| versions_compatible(version_id, chunk[i].version_id()));
    match (candidates.next(), candidates.next()) {
        (Some(i), None) => Some(i),
        (Some(i), Some(_)) if first_fit => Some(i),
        _ => None,
    }
}

#[async_trait]
impl Deleter for BatchDeleter {
    async fn delete(&self, objects: &[S3Object], config: &Config) -> Result<DeleteResult> {
        let mut result = DeleteResult::default();

        if objects.is_empty() {
            return Ok(result);
        }

        let batch_size = (config.batch_size as usize).min(MAX_BATCH_SIZE);

        for chunk in objects.chunks(batch_size) {
            let identifiers: Vec<ObjectIdentifier> = chunk
                .iter()
                .map(|obj| {
                    let mut builder = ObjectIdentifier::builder().key(obj.key());
                    if let Some(vid) = obj.version_id() {
                        builder = builder.version_id(vid);
                    }
                    // Include ETag for conditional deletion when if_match is enabled.
                    // S3 DeleteObjects API supports per-object ETag conditions.
                    if config.if_match {
                        if let Some(etag) = obj.e_tag() {
                            builder = builder.e_tag(etag);
                        }
                    }
                    builder.build().expect("ObjectIdentifier build failed")
                })
                .collect();

            let batch_count = identifiers.len();

            debug!(
                batch_size = batch_count,
                "sending DeleteObjects batch request."
            );

            let response = self.target.delete_objects(identifiers).await?;

            let deleted_before = result.deleted.len();
            let failed_before = result.failed.len();

            let index = index_by_key(chunk);
            // Tracks which requested objects a response entry has accounted for.
            let mut accounted = vec![false; chunk.len()];

            // Every recorded outcome uses the identity of a requested object,
            // never a key taken verbatim from the response, so a malformed
            // response can neither cause a delete outside the batch nor report
            // one. This first pass only accepts unambiguous matches; the rest
            // is handled by `resolve_leftovers`.
            let mut pending_deleted = Vec::new();
            for deleted in response.deleted() {
                match match_entry(
                    deleted.key(),
                    deleted.version_id(),
                    chunk,
                    &index,
                    &accounted,
                    false,
                ) {
                    Some(i) => {
                        accounted[i] = true;
                        result.deleted.push(deleted_key_of(&chunk[i]));
                    }
                    None => pending_deleted.push(deleted),
                }
            }

            let mut unmatched_errors: Vec<(String, String)> = Vec::new();
            for err in response.errors() {
                let code = err.code().unwrap_or("unknown").to_string();
                let message = err.message().unwrap_or("no message").to_string();

                let Some(i) = match_entry(
                    err.key(),
                    err.version_id(),
                    chunk,
                    &index,
                    &accounted,
                    false,
                ) else {
                    unmatched_errors.push((code, message));
                    continue;
                };
                accounted[i] = true;
                let obj = &chunk[i];

                if is_retryable_error_code(&code) {
                    // Retryable error: fall back to individual DeleteObject with retries
                    self.retry_and_record(obj, &code, &message, config, &mut result)
                        .await;
                } else {
                    // Non-retryable error: treat as warning, not error.
                    // The caller (ObjectDeleter) handles warn-as-error promotion.
                    self.record_failure(obj, &code, &message, &mut result).await;
                }
            }

            self.resolve_leftovers(
                chunk,
                &index,
                &mut accounted,
                pending_deleted,
                unmatched_errors,
                config,
                &mut result,
            )
            .await;

            debug!(
                deleted = result.deleted.len() - deleted_before,
                failed = result.failed.len() - failed_before,
                "DeleteObjects batch completed."
            );
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn max_batch_size_is_1000() {
        assert_eq!(MAX_BATCH_SIZE, 1000);
    }
}
