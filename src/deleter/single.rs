//! Single-object deletion using the S3 DeleteObject API.
//!
//! Deletes objects one at a time. Used when batch_size is 1 or when
//! individual delete control is needed.

use anyhow::Result;
use async_trait::async_trait;
use tracing::{debug, error};

use crate::config::Config;
use crate::types::S3Object;

use super::Deleter;

/// Deletes objects one at a time using the S3 DeleteObject API.
///
/// Each object is deleted individually, which is useful when
/// per-object control is needed (e.g., If-Match conditions).
pub struct SingleDeleter {
    target: crate::storage::Storage,
}

impl SingleDeleter {
    pub fn new(target: crate::storage::Storage) -> Self {
        Self { target }
    }
}

#[async_trait]
impl Deleter for SingleDeleter {
    async fn delete(&self, objects: &[S3Object], _config: &Config) -> Result<usize> {
        let mut total_deleted = 0usize;

        for obj in objects {
            let key = obj.key();
            let version_id = obj.version_id().map(|v| v.to_string());

            debug!(
                key = key,
                version_id = version_id,
                "sending DeleteObject request."
            );

            let result = self
                .target
                .delete_object(key, version_id.clone(), None)
                .await;

            match result {
                Ok(_) => {
                    total_deleted += 1;
                    debug!(key = key, "DeleteObject succeeded.");
                }
                Err(e) => {
                    error!(
                        key = key,
                        version_id = version_id,
                        error = e.to_string(),
                        "DeleteObject failed."
                    );
                    return Err(e);
                }
            }
        }

        Ok(total_deleted)
    }
}
