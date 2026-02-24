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
}

#[async_trait]
impl Deleter for BatchDeleter {
    async fn delete(&self, objects: &[S3Object], config: &Config) -> Result<DeleteResult> {
        let mut result = DeleteResult::default();

        if objects.is_empty() {
            return Ok(result);
        }

        let batch_size = (config.batch_size as usize).min(MAX_BATCH_SIZE);
        let force_retry_count = config.force_retry_config.force_retry_count;
        let force_retry_interval = config.force_retry_config.force_retry_interval_milliseconds;

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
            for err in response.errors() {
                let key = err.key().unwrap_or("unknown").to_string();
                let version_id = err.version_id().map(|v| v.to_string());
                let code = err.code().unwrap_or("unknown").to_string();
                let message = err.message().unwrap_or("no message").to_string();

                if is_retryable_error_code(&code) {
                    // Retryable error: fall back to individual DeleteObject with retries
                    let if_match_etag = if config.if_match {
                        obj_lookup
                            .get(&(key.clone(), version_id.clone()))
                            .and_then(|obj| obj.e_tag().map(|s| s.to_string()))
                    } else {
                        None
                    };

                    let mut single_succeeded = false;
                    for attempt in 0..=force_retry_count {
                        if attempt > 0 {
                            tokio::time::sleep(std::time::Duration::from_millis(
                                force_retry_interval,
                            ))
                            .await;
                        }

                        match self
                            .target
                            .delete_object(&key, version_id.clone(), if_match_etag.clone())
                            .await
                        {
                            Ok(_) => {
                                result.deleted.push(DeletedKey {
                                    key: key.clone(),
                                    version_id: version_id.clone(),
                                });
                                single_succeeded = true;
                                break;
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

                    if !single_succeeded {
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
                        self.target
                            .send_stats(DeletionStatistics::DeleteError { key: key.clone() })
                            .await;
                        result.failed.push(FailedKey {
                            key,
                            version_id,
                            error_code: code,
                            error_message: message,
                        });
                    }
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
                    self.target
                        .send_stats(DeletionStatistics::DeleteError { key: key.clone() })
                        .await;

                    result.failed.push(FailedKey {
                        key,
                        version_id,
                        error_code: code,
                        error_message: message,
                    });
                }
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
