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

/// Placeholder key recorded for a DeleteObjects error entry that has no key
/// and cannot be matched to any requested object.
pub const UNKNOWN_KEY: &str = "<unknown>";

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

    /// Retries a key via single DeleteObject and records the outcome in `result`.
    #[allow(clippy::too_many_arguments)]
    async fn retry_and_record(
        &self,
        key: &str,
        version_id: Option<&str>,
        if_match_etag: Option<String>,
        code: &str,
        message: &str,
        config: &Config,
        result: &mut DeleteResult,
    ) {
        match self
            .retry_failed_key_with_single_delete(
                key,
                version_id,
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
                        key: key.to_string(),
                    })
                    .await;
                result.failed.push(failed_key);
            }
        }
    }

    /// Records a failed key in `result` and reports it to the stats channel.
    async fn record_failure(
        &self,
        key: String,
        version_id: Option<String>,
        error_code: String,
        error_message: String,
        result: &mut DeleteResult,
    ) {
        self.target
            .send_stats(DeletionStatistics::DeleteError { key: key.clone() })
            .await;
        result.failed.push(FailedKey {
            key,
            version_id,
            error_code,
            error_message,
        });
    }

    /// Resolves DeleteObjects error entries that carry no key.
    ///
    /// The objects such errors refer to are inferred as the objects in
    /// `chunk` that no `Deleted` entry and no keyed `Error` entry accounts
    /// for. Those objects are then handled by their real key:
    /// - if every keyless error is retryable, each is retried with a single
    ///   DeleteObject (with its own version ID and, when `if_match` is set,
    ///   its own ETag);
    /// - otherwise each is recorded as a failure under its real key.
    ///
    /// If no unaccounted object can be found, the keyless errors are recorded
    /// as failures under the placeholder key `<unknown>`.
    async fn resolve_keyless_errors(
        &self,
        chunk: &[S3Object],
        response: &aws_sdk_s3::operation::delete_objects::DeleteObjectsOutput,
        keyless_errors: Vec<(String, String)>,
        config: &Config,
        result: &mut DeleteResult,
    ) {
        let reported = response
            .deleted()
            .iter()
            .map(|d| (d.key(), d.version_id()))
            .chain(response.errors().iter().map(|e| (e.key(), e.version_id())))
            .filter_map(|(k, v)| k.filter(|k| !k.is_empty()).map(|k| (k, v)))
            .collect::<Vec<_>>();

        // A response entry accounts for a requested object when the keys match
        // and the entry either echoes the same version ID or omits it.
        let unaccounted: Vec<&S3Object> = chunk
            .iter()
            .filter(|obj| {
                !reported
                    .iter()
                    .any(|(k, v)| *k == obj.key() && (v.is_none() || *v == obj.version_id()))
            })
            .collect();

        if unaccounted.is_empty() {
            for (code, message) in keyless_errors {
                warn!(
                    code = code,
                    message = message,
                    "S3 DeleteObjects returned an error without a key that could not be \
                     matched to any object in the batch: {} ({}).",
                    code,
                    message,
                );
                self.record_failure(UNKNOWN_KEY.to_string(), None, code, message, result)
                    .await;
            }
            return;
        }

        warn!(
            keyless_errors = keyless_errors.len(),
            unaccounted_objects = unaccounted.len(),
            "S3 DeleteObjects returned {} error(s) without a key; attributing them to {} \
             object(s) not reported as deleted or failed.",
            keyless_errors.len(),
            unaccounted.len(),
        );

        let all_retryable = keyless_errors
            .iter()
            .all(|(code, _)| is_retryable_error_code(code));
        // Prefer a non-retryable error as the reported cause, if there is one.
        let (code, message) = keyless_errors
            .iter()
            .find(|(code, _)| !is_retryable_error_code(code))
            .unwrap_or(&keyless_errors[0])
            .clone();

        for obj in unaccounted {
            let key = obj.key().to_string();
            let version_id = obj.version_id().map(|v| v.to_string());

            if all_retryable {
                let if_match_etag = if config.if_match {
                    obj.e_tag().map(|s| s.to_string())
                } else {
                    None
                };
                self.retry_and_record(
                    &key,
                    version_id.as_deref(),
                    if_match_etag,
                    &code,
                    &message,
                    config,
                    result,
                )
                .await;
            } else {
                warn!(
                    key = key,
                    version_id = version_id,
                    code = code,
                    message = message,
                    "S3 DeleteObjects partial failure for key '{}' (reported without a key): \
                     {} ({}).",
                    key,
                    code,
                    message,
                );
                self.record_failure(key, version_id, code.clone(), message.clone(), result)
                    .await;
            }
        }
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
            // Build a lookup map from (key, version_id) -> &S3Object for ETag retrieval
            // during single-delete fallback with if_match.
            let obj_lookup: std::collections::HashMap<(String, Option<String>), &S3Object> = chunk
                .iter()
                .map(|obj| {
                    (
                        (
                            obj.key().to_string(),
                            obj.version_id().map(|v| v.to_string()),
                        ),
                        obj,
                    )
                })
                .collect();

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

            for deleted in response.deleted() {
                result.deleted.push(DeletedKey {
                    key: deleted.key().unwrap_or_default().to_string(),
                    version_id: deleted.version_id().map(|v| v.to_string()),
                });
            }

            // Handle partial failures: classify by error code and fall back
            // to individual DeleteObject calls for retryable errors.
            //
            // Some S3-compatible stores may omit the key (or send an empty
            // one) in an error entry. Those cannot be matched to an object
            // directly, so they are collected and resolved after this loop
            // against the objects in the chunk that no response entry
            // accounts for.
            let mut keyless_errors: Vec<(String, String)> = Vec::new();
            for err in response.errors() {
                let code = err.code().unwrap_or("unknown").to_string();
                let message = err.message().unwrap_or("no message").to_string();

                let Some(key) = err.key().filter(|k| !k.is_empty()).map(str::to_string) else {
                    keyless_errors.push((code, message));
                    continue;
                };
                let version_id = err.version_id().map(|v| v.to_string());

                if is_retryable_error_code(&code) {
                    // Retryable error: fall back to individual DeleteObject with retries
                    let if_match_etag = if config.if_match {
                        obj_lookup
                            .get(&(key.clone(), version_id.clone()))
                            .and_then(|obj| obj.e_tag().map(|s| s.to_string()))
                    } else {
                        None
                    };

                    self.retry_and_record(
                        &key,
                        version_id.as_deref(),
                        if_match_etag,
                        &code,
                        &message,
                        config,
                        &mut result,
                    )
                    .await;
                } else {
                    // Non-retryable error: treat as warning, not error.
                    // The caller (ObjectDeleter) handles warn-as-error promotion.
                    warn!(
                        key = key,
                        version_id = version_id,
                        code = code,
                        message = message,
                        "S3 DeleteObjects partial failure for key '{}': {} ({}).",
                        key,
                        code,
                        message,
                    );
                    self.record_failure(key, version_id, code, message, &mut result)
                        .await;
                }
            }

            if !keyless_errors.is_empty() {
                self.resolve_keyless_errors(chunk, &response, keyless_errors, config, &mut result)
                    .await;
            }

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
