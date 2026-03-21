//! Single-object deletion using the S3 DeleteObject API.
//!
//! Deletes objects one at a time. Used when batch_size is 1 or when
//! individual delete control is needed.

use anyhow::Result;
use async_trait::async_trait;
use aws_sdk_s3::operation::delete_object::DeleteObjectError;
use aws_smithy_runtime_api::client::result::SdkError;
use aws_smithy_runtime_api::http::Response;
use aws_smithy_types::body::SdkBody;
use tracing::{debug, warn};

use crate::config::Config;
use crate::types::{DeletionStatistics, S3Object};

use super::{DeleteResult, DeletedKey, Deleter, FailedKey};

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
    async fn delete(&self, objects: &[S3Object], config: &Config) -> Result<DeleteResult> {
        debug_assert_eq!(objects.len(), 1, "SingleDeleter expects exactly one object");

        let mut result = DeleteResult::default();
        let obj = &objects[0];
        let key = obj.key();
        let version_id = obj.version_id().map(|v| v.to_string());
        let if_match = if config.if_match {
            obj.e_tag().map(|etag| etag.to_string())
        } else {
            None
        };

        debug!(
            key = key,
            version_id = version_id,
            if_match = if_match,
            "sending DeleteObject request."
        );

        let delete_result = self
            .target
            .delete_object(key, version_id.clone(), if_match)
            .await;

        match delete_result {
            Ok(_) => {
                debug!(key = key, "DeleteObject succeeded.");
                result.deleted.push(DeletedKey {
                    key: key.to_string(),
                    version_id,
                });
            }
            Err(e) => {
                let error_code = extract_delete_object_error_code(&e);
                warn!(
                    key = key,
                    version_id = version_id,
                    error_code = error_code,
                    error = %e,
                    "S3 DeleteObject API call failed for key '{}'.",
                    key,
                );
                self.target
                    .send_stats(DeletionStatistics::DeleteError {
                        key: key.to_string(),
                    })
                    .await;
                result.failed.push(FailedKey {
                    key: key.to_string(),
                    version_id,
                    error_code,
                    error_message: e.to_string(),
                });
            }
        }

        Ok(result)
    }
}

/// Extract the S3 error code from an anyhow::Error that wraps an SdkError<DeleteObjectError>.
///
/// The storage layer wraps the SdkError via `anyhow!(e).context(...)`, so the
/// original SdkError is preserved in the error chain and can be downcast.
pub(crate) fn extract_delete_object_error_code(err: &anyhow::Error) -> String {
    for cause in err.chain() {
        if let Some(sdk_err) =
            cause.downcast_ref::<SdkError<DeleteObjectError, Response<SdkBody>>>()
        {
            if let Some(service_err) = sdk_err.as_service_error() {
                if let Some(code) = service_err.meta().code() {
                    return code.to_string();
                }
            }
        }
    }
    "unknown".to_string()
}
