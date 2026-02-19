//! Larger-size filter stage.
//!
//! Reused from s3sync's `pipeline/filter/larger_size.rs`.
//! Passes objects whose size is greater than or equal to the configured threshold.
//! Delete markers always pass (they have no meaningful size).

use anyhow::Result;
use async_trait::async_trait;
use tracing::debug;

use crate::config::FilterConfig;
use crate::filters::{ObjectFilter, ObjectFilterBase};
use crate::stage::Stage;
use crate::types::S3Object;

pub struct LargerSizeFilter<'a> {
    base: ObjectFilterBase<'a>,
}

const FILTER_NAME: &str = "LargerSizeFilter";

impl LargerSizeFilter<'_> {
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
impl ObjectFilter for LargerSizeFilter<'_> {
    async fn filter(&self) -> Result<()> {
        self.base.filter(is_larger_or_equal).await
    }
}

fn is_larger_or_equal(object: &S3Object, config: &FilterConfig) -> bool {
    if object.is_delete_marker() {
        return true;
    }

    if object.size() < config.larger_size.unwrap() as i64 {
        let key = object.key();
        let content_length = object.size();
        let delete_marker = object.is_delete_marker();
        let version_id = object.version_id();
        let config_size = config.larger_size.unwrap();

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

    /// Test helper: expose is_larger_or_equal for property tests.
    pub(crate) fn test_is_larger_or_equal(object: &S3Object, config: &FilterConfig) -> bool {
        is_larger_or_equal(object, config)
    }

    fn init_dummy_tracing_subscriber() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter("dummy=trace")
            .try_init();
    }

    #[test]
    fn larger() {
        init_dummy_tracing_subscriber();

        let object = S3Object::NotVersioning(Object::builder().size(6).build());
        let config = FilterConfig {
            larger_size: Some(5),
            ..Default::default()
        };

        assert!(is_larger_or_equal(&object, &config));
    }

    #[test]
    fn smaller() {
        init_dummy_tracing_subscriber();

        let object = S3Object::NotVersioning(Object::builder().key("test").size(4).build());
        let config = FilterConfig {
            larger_size: Some(5),
            ..Default::default()
        };

        assert!(!is_larger_or_equal(&object, &config));
    }

    #[test]
    fn equal() {
        init_dummy_tracing_subscriber();

        let object = S3Object::NotVersioning(Object::builder().size(4).build());
        let config = FilterConfig {
            larger_size: Some(4),
            ..Default::default()
        };

        assert!(is_larger_or_equal(&object, &config));
    }

    #[test]
    fn delete_marker() {
        init_dummy_tracing_subscriber();

        let delete_marker =
            S3Object::DeleteMarker(DeleteMarkerEntry::builder().key("test").build());
        let config = FilterConfig {
            larger_size: Some(4),
            ..Default::default()
        };

        assert!(is_larger_or_equal(&delete_marker, &config));
    }
}
