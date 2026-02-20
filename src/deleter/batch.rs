//! Batch deletion using the S3 DeleteObjects API.
//!
//! Groups objects into batches of up to 1000 and calls the S3 batch
//! delete API for high-throughput deletion.

use anyhow::Result;
use async_trait::async_trait;
use aws_sdk_s3::types::ObjectIdentifier;
use tracing::{debug, warn};

use crate::config::Config;
use crate::types::S3Object;

use super::{DeleteResult, DeletedKey, Deleter, FailedKey};

/// Maximum objects per batch DeleteObjects API call (S3 limit).
pub const MAX_BATCH_SIZE: usize = 1000;

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

            for deleted in response.deleted() {
                result.deleted.push(DeletedKey {
                    key: deleted.key().unwrap_or_default().to_string(),
                    version_id: deleted.version_id().map(|v| v.to_string()),
                });
            }

            let errors = response.errors();
            for err in errors {
                let key = err.key().unwrap_or("unknown").to_string();
                let version_id = err.version_id().map(|v| v.to_string());
                let code = err.code().unwrap_or("unknown").to_string();
                let message = err.message().unwrap_or("no message").to_string();

                warn!(
                    key = key,
                    version_id = version_id,
                    code = code,
                    message = message,
                    "object failed in batch delete."
                );

                result.failed.push(FailedKey {
                    key,
                    version_id,
                    error_code: code,
                    error_message: message,
                });
            }

            debug!(
                deleted = result.deleted.len(),
                errors = errors.len(),
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
