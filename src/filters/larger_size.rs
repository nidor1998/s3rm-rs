//! Larger-size filter stage.
//!
//! Reused from s3sync's `pipeline/filter/larger_size.rs`.
//! Passes objects whose size is greater than or equal to the configured threshold.
//! Delete markers always pass (they have no meaningful size).

use anyhow::Result;
use async_trait::async_trait;
use tracing::{debug, warn};

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

    let Some(larger_size) = config.larger_size else {
        warn!(
            name = FILTER_NAME,
            "larger_size config is None, skipping object to be safe."
        );
        return false;
    };

    let object_size = object.size();
    if object_size < 0 {
        warn!(
            name = FILTER_NAME,
            size = object_size,
            "object has negative size, skipping to be safe."
        );
        return false;
    }

    if (object_size as u64) < larger_size {
        let key = object.key();
        let content_length = object.size();
        let delete_marker = object.is_delete_marker();
        let version_id = object.version_id();

        debug!(
            name = FILTER_NAME,
            key = key,
            content_length = content_length,
            delete_marker = delete_marker,
            version_id = version_id,
            config_size = larger_size,
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
    use crate::test_utils::init_dummy_tracing_subscriber;
    use aws_sdk_s3::types::{DeleteMarkerEntry, Object};

    /// Test helper: expose is_larger_or_equal for property tests.
    pub(crate) fn test_is_larger_or_equal(object: &S3Object, config: &FilterConfig) -> bool {
        is_larger_or_equal(object, config)
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

    #[test]
    fn none_config_returns_false() {
        init_dummy_tracing_subscriber();

        let object = S3Object::NotVersioning(Object::builder().key("test").size(100).build());
        let config = FilterConfig {
            larger_size: None,
            ..Default::default()
        };

        assert!(!is_larger_or_equal(&object, &config));
    }
}
