//! Smaller-size filter stage.
//!
//! Reused from s3sync's `pipeline/filter/smaller_size.rs`.
//! Passes objects whose size is strictly less than the configured threshold.
//! Delete markers always pass (they have no meaningful size).

use anyhow::Result;
use async_trait::async_trait;
use tracing::debug;

use crate::config::FilterConfig;
use crate::filters::{ObjectFilter, ObjectFilterBase};
use crate::stage::Stage;
use crate::types::S3Object;

pub struct SmallerSizeFilter<'a> {
    base: ObjectFilterBase<'a>,
}

const FILTER_NAME: &str = "SmallerSizeFilter";

impl SmallerSizeFilter<'_> {
    pub fn new(base: Stage) -> Self {
        Self {
            base: ObjectFilterBase {
                base,
                name: FILTER_NAME,
            },
        }
    }
}

#[async_trait]
impl ObjectFilter for SmallerSizeFilter<'_> {
    async fn filter(&self) -> Result<()> {
        self.base.filter(is_smaller).await
    }
}

fn is_smaller(object: &S3Object, config: &FilterConfig) -> bool {
    if object.is_delete_marker() {
        return true;
    }

    if object.size() >= config.smaller_size.unwrap() as i64 {
        let key = object.key();
        let content_length = object.size();
        let delete_marker = object.is_delete_marker();
        let version_id = object.version_id();
        let config_size = config.smaller_size.unwrap();

        debug!(
            name = FILTER_NAME,
            key = key,
            content_length = content_length,
            delete_marker = delete_marker,
            version_id = version_id,
            config_size = config_size,
            "object filtered."
        );
        return false;
    }

    true
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::config::FilterConfig;
    use aws_sdk_s3::types::{DeleteMarkerEntry, Object};

    /// Test helper: expose is_smaller for property tests.
    pub(crate) fn test_is_smaller(object: &S3Object, config: &FilterConfig) -> bool {
        is_smaller(object, config)
    }

    fn init_dummy_tracing_subscriber() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter("dummy=trace")
            .try_init();
    }

    #[test]
    fn larger() {
        init_dummy_tracing_subscriber();

        let object = S3Object::NotVersioning(Object::builder().key("test").size(6).build());
        let config = FilterConfig {
            smaller_size: Some(5),
            ..Default::default()
        };

        assert!(!is_smaller(&object, &config));
    }

    #[test]
    fn smaller() {
        init_dummy_tracing_subscriber();

        let object = S3Object::NotVersioning(Object::builder().size(4).build());
        let config = FilterConfig {
            smaller_size: Some(5),
            ..Default::default()
        };

        assert!(is_smaller(&object, &config));
    }

    #[test]
    fn equal() {
        init_dummy_tracing_subscriber();

        let object = S3Object::NotVersioning(Object::builder().key("test").size(5).build());
        let config = FilterConfig {
            smaller_size: Some(5),
            ..Default::default()
        };

        // Equal means NOT smaller, so filtered out
        assert!(!is_smaller(&object, &config));
    }

    #[test]
    fn delete_marker() {
        init_dummy_tracing_subscriber();

        let delete_marker =
            S3Object::DeleteMarker(DeleteMarkerEntry::builder().key("test").build());
        let config = FilterConfig {
            smaller_size: Some(5),
            ..Default::default()
        };

        assert!(is_smaller(&delete_marker, &config));
    }
}
