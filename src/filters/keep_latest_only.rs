//! Keep-latest-only filter stage.
//!
//! Only passes objects whose `is_latest` is explicitly `Some(false)` (i.e.,
//! confirmed non-latest versions that should be deleted). Filters out (keeps)
//! everything else: `Some(true)`, `None`, and `NotVersioning` objects.

use anyhow::Result;
use async_trait::async_trait;
use tracing::debug;

use crate::config::FilterConfig;
use crate::filters::{ObjectFilter, ObjectFilterBase};
use crate::stage::Stage;
use crate::types::S3Object;

pub struct KeepLatestOnlyFilter<'a> {
    base: ObjectFilterBase<'a>,
}

const FILTER_NAME: &str = "KeepLatestOnlyFilter";

impl KeepLatestOnlyFilter<'_> {
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
impl ObjectFilter for KeepLatestOnlyFilter<'_> {
    async fn filter(&self) -> Result<()> {
        self.base.filter(is_not_latest).await
    }
}

fn is_not_latest(object: &S3Object, _config: &FilterConfig) -> bool {
    // Only delete objects that are explicitly marked as non-latest (Some(false)).
    // Keep everything else: latest (Some(true)), unknown (None), and non-versioned.
    // This prevents accidental deletion when is_latest is missing from the S3
    // response or when NotVersioning objects are introduced by a bug.
    let explicitly_non_latest = match object {
        S3Object::Versioning(v) => v.is_latest() == Some(false),
        S3Object::DeleteMarker(dm) => dm.is_latest() == Some(false),
        S3Object::NotVersioning(_) => false,
    };

    if !explicitly_non_latest {
        let key = object.key();
        let delete_marker = object.is_delete_marker();
        let version_id = object.version_id();

        debug!(
            name = FILTER_NAME,
            key = key,
            delete_marker = delete_marker,
            version_id = version_id,
            "object filtered (not explicitly non-latest, keeping)."
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
    use aws_sdk_s3::types::{DeleteMarkerEntry, Object, ObjectVersion};

    /// Test helper: expose is_not_latest for property tests.
    pub(crate) fn test_is_not_latest(object: &S3Object, config: &FilterConfig) -> bool {
        is_not_latest(object, config)
    }

    #[test]
    fn latest_version_is_filtered_out() {
        init_dummy_tracing_subscriber();

        let config = FilterConfig {
            keep_latest_only: true,
            ..Default::default()
        };

        let object = S3Object::Versioning(
            ObjectVersion::builder()
                .key("test-key")
                .version_id("v1")
                .is_latest(true)
                .build(),
        );
        assert!(!is_not_latest(&object, &config));
    }

    #[test]
    fn non_latest_version_passes_through() {
        init_dummy_tracing_subscriber();

        let config = FilterConfig {
            keep_latest_only: true,
            ..Default::default()
        };

        let object = S3Object::Versioning(
            ObjectVersion::builder()
                .key("test-key")
                .version_id("v2")
                .is_latest(false)
                .build(),
        );
        assert!(is_not_latest(&object, &config));
    }

    #[test]
    fn latest_delete_marker_is_filtered_out() {
        init_dummy_tracing_subscriber();

        let config = FilterConfig {
            keep_latest_only: true,
            ..Default::default()
        };

        let object = S3Object::DeleteMarker(
            DeleteMarkerEntry::builder()
                .key("test-key")
                .version_id("dm1")
                .is_latest(true)
                .build(),
        );
        assert!(!is_not_latest(&object, &config));
    }

    #[test]
    fn non_latest_delete_marker_passes_through() {
        init_dummy_tracing_subscriber();

        let config = FilterConfig {
            keep_latest_only: true,
            ..Default::default()
        };

        let object = S3Object::DeleteMarker(
            DeleteMarkerEntry::builder()
                .key("test-key")
                .version_id("dm2")
                .is_latest(false)
                .build(),
        );
        assert!(is_not_latest(&object, &config));
    }

    #[test]
    fn non_versioned_object_is_filtered_out() {
        init_dummy_tracing_subscriber();

        let config = FilterConfig {
            keep_latest_only: true,
            ..Default::default()
        };

        // Defensive: non-versioned objects are treated as "latest" and kept,
        // preventing accidental deletion if a bug introduces them into the pipeline.
        let object = S3Object::NotVersioning(Object::builder().key("test-key").build());
        assert!(!is_not_latest(&object, &config));
    }

    #[test]
    fn versioned_object_with_none_is_latest_is_filtered_out() {
        init_dummy_tracing_subscriber();

        let config = FilterConfig {
            keep_latest_only: true,
            ..Default::default()
        };

        // is_latest omitted (None) — must be kept, not deleted.
        let object = S3Object::Versioning(
            ObjectVersion::builder()
                .key("test-key")
                .version_id("v-none")
                .build(),
        );
        assert!(!is_not_latest(&object, &config));
    }

    #[test]
    fn delete_marker_with_none_is_latest_is_filtered_out() {
        init_dummy_tracing_subscriber();

        let config = FilterConfig {
            keep_latest_only: true,
            ..Default::default()
        };

        // is_latest omitted (None) — must be kept, not deleted.
        let object = S3Object::DeleteMarker(
            DeleteMarkerEntry::builder()
                .key("test-key")
                .version_id("dm-none")
                .build(),
        );
        assert!(!is_not_latest(&object, &config));
    }
}
