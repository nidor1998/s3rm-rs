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

use super::Deleter;

/// Maximum objects per batch DeleteObjects API call (S3 limit).
pub const MAX_BATCH_SIZE: usize = 1000;

/// Deletes objects in batches using the S3 DeleteObjects API.
///
/// Groups objects into batches of up to `batch_size` (max 1000) and
/// calls DeleteObjects for each batch. Partial failures are reported
/// via logging and the returned count reflects only successful deletions.
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
    async fn delete(&self, objects: &[S3Object], config: &Config) -> Result<usize> {
        if objects.is_empty() {
            return Ok(0);
        }

        let batch_size = (config.batch_size as usize).min(MAX_BATCH_SIZE);
        let mut total_deleted = 0usize;

        for chunk in objects.chunks(batch_size) {
            let identifiers: Vec<ObjectIdentifier> = chunk
                .iter()
                .map(|obj| {
                    let mut builder = ObjectIdentifier::builder().key(obj.key());
                    if let Some(vid) = obj.version_id() {
                        builder = builder.version_id(vid);
                    }
                    builder.build().expect("ObjectIdentifier build failed")
                })
                .collect();

            let batch_count = identifiers.len();

            debug!(
                batch_size = batch_count,
                "sending DeleteObjects batch request."
            );

            let result = self.target.delete_objects(identifiers).await?;

            // Count successes (DeleteObjects returns the list of deleted objects)
            let deleted_count = result.deleted().len();
            total_deleted += deleted_count;

            // Log any errors from the batch
            let errors = result.errors();
            if !errors.is_empty() {
                for err in errors {
                    warn!(
                        key = err.key().unwrap_or("unknown"),
                        version_id = err.version_id().unwrap_or("none"),
                        code = err.code().unwrap_or("unknown"),
                        message = err.message().unwrap_or("no message"),
                        "object failed in batch delete."
                    );
                }
            }

            debug!(
                deleted = deleted_count,
                errors = errors.len(),
                "DeleteObjects batch completed."
            );
        }

        Ok(total_deleted)
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
