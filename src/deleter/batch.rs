//! Batch deletion using the S3 DeleteObjects API.
//!
//! Groups objects into batches of up to 1000 and calls the S3 batch
//! delete API for high-throughput deletion.

use anyhow::Result;
use async_trait::async_trait;
use aws_sdk_s3::types::ObjectIdentifier;
use tracing::{debug, warn};

use crate::config::Config;
use crate::types::{DeletionStatistics, S3Object};

use super::{DeleteResult, DeletedKey, Deleter, FailedKey};

/// Maximum objects per batch DeleteObjects API call (S3 limit).
pub const MAX_BATCH_SIZE: usize = 1000;

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

    /// Handles error entries that could not be matched to exactly one
    /// requested object (no key, an empty key, a key that was not in the
    /// request, or an ambiguous key without a version ID).
    ///
    /// When the number of such errors equals the number of requested objects
    /// that no response entry accounts for, the errors are taken to refer to
    /// those objects. Their real identities are then used:
    /// - if every error is non-retryable, each object is recorded as failed;
    /// - otherwise each object is retried with a single DeleteObject, whose
    ///   own outcome decides whether it is deleted or failed.
    ///
    /// Otherwise the errors cannot be attributed. No object is touched, and
    /// the errors are reported in [`DeleteResult::unattributed_errors`] so the
    /// run ends with a warning without inflating per-object failure counts.
    async fn resolve_unattributed_errors(
        &self,
        unaccounted: Vec<&S3Object>,
        errors: Vec<(String, String)>,
        config: &Config,
        result: &mut DeleteResult,
    ) {
        if errors.is_empty() {
            return;
        }

        if unaccounted.len() != errors.len() {
            for (code, message) in errors {
                warn!(
                    code = code,
                    message = message,
                    unaccounted_objects = unaccounted.len(),
                    "S3 DeleteObjects returned an error that cannot be attributed to a \
                     requested object: {} ({}).",
                    code,
                    message,
                );
                result.unattributed_errors += 1;
            }
            return;
        }

        warn!(
            errors = errors.len(),
            "S3 DeleteObjects returned {} error(s) without an identifiable key; attributing \
             them to the object(s) not reported as deleted or failed.",
            errors.len(),
        );

        // With several errors, which error belongs to which object is unknown,
        // so a code is only reported per object when all errors agree.
        let (code, message) = if errors.iter().all(|(c, _)| *c == errors[0].0) {
            errors[0].clone()
        } else {
            let codes: Vec<&str> = errors.iter().map(|(c, _)| c.as_str()).collect();
            (
                "unknown".to_string(),
                format!(
                    "DeleteObjects errors without an identifiable key: {}",
                    codes.join(", ")
                ),
            )
        };
        let all_non_retryable = errors.iter().all(|(c, _)| !is_retryable_error_code(c));

        for obj in unaccounted {
            if all_non_retryable {
                self.record_failure(obj, &code, &message, result).await;
            } else {
                self.retry_and_record(obj, &code, &message, config, result)
                    .await;
            }
        }
    }
}

/// Returns true if a response entry `(key, version_id)` can refer to `obj`.
///
/// The keys must match. Version IDs must match when both sides have one; an
/// entry without a version ID, or a request without one (S3 may echo a
/// delete-marker version ID), is compatible.
fn entry_matches(key: &str, version_id: Option<&str>, obj: &S3Object) -> bool {
    key == obj.key()
        && match (version_id, obj.version_id()) {
            (Some(a), Some(b)) => a == b,
            _ => true,
        }
}

/// Finds the single not-yet-accounted object in `chunk` that a response entry
/// refers to. An exact `(key, version_id)` match wins; otherwise the entry
/// must be compatible with exactly one object, else `None` (ambiguous or not
/// part of the request).
fn match_entry(
    key: Option<&str>,
    version_id: Option<&str>,
    chunk: &[S3Object],
    accounted: &[bool],
) -> Option<usize> {
    let key = key.filter(|k| !k.is_empty())?;
    let open = || (0..chunk.len()).filter(|&i| !accounted[i]);

    if let Some(i) = open().find(|&i| chunk[i].key() == key && chunk[i].version_id() == version_id)
    {
        return Some(i);
    }

    let mut candidates = open().filter(|&i| entry_matches(key, version_id, &chunk[i]));
    match (candidates.next(), candidates.next()) {
        (Some(i), None) => Some(i),
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

            // Tracks which requested objects a response entry has accounted for.
            let mut accounted = vec![false; chunk.len()];

            for deleted in response.deleted() {
                if let Some(i) = match_entry(deleted.key(), deleted.version_id(), chunk, &accounted)
                {
                    accounted[i] = true;
                }
                result.deleted.push(DeletedKey {
                    key: deleted.key().unwrap_or_default().to_string(),
                    version_id: deleted.version_id().map(|v| v.to_string()),
                });
            }

            // Handle partial failures. Every action taken uses the identity of
            // a requested object, never a key taken verbatim from the response,
            // so a malformed response cannot cause a delete outside the batch.
            // Entries that cannot be matched to exactly one requested object
            // are resolved afterwards.
            let mut unmatched_errors: Vec<(String, String)> = Vec::new();
            for err in response.errors() {
                let code = err.code().unwrap_or("unknown").to_string();
                let message = err.message().unwrap_or("no message").to_string();

                let Some(i) = match_entry(err.key(), err.version_id(), chunk, &accounted) else {
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

            let unaccounted: Vec<&S3Object> = chunk
                .iter()
                .zip(&accounted)
                .filter(|(_, done)| !**done)
                .map(|(obj, _)| obj)
                .collect();
            self.resolve_unattributed_errors(unaccounted, unmatched_errors, config, &mut result)
                .await;

            debug!(
                deleted = result.deleted.len(),
                failed = result.failed.len(),
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
