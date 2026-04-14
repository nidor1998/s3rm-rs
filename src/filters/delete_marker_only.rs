//! Delete-marker-only filter stage.
//!
//! Passes only objects that are delete markers (`S3Object::DeleteMarker`).
//! All other object types (versioned objects, non-versioned objects) are filtered out.
//! This filter is off by default and activated via `--filter-delete-marker-only`.

use anyhow::Result;
use async_trait::async_trait;
use tracing::debug;

use crate::config::FilterConfig;
use crate::filters::{ObjectFilter, ObjectFilterBase};
use crate::stage::Stage;
use crate::types::S3Object;

pub struct DeleteMarkerOnlyFilter<'a> {
    base: ObjectFilterBase<'a>,
}

const FILTER_NAME: &str = "DeleteMarkerOnlyFilter";

impl DeleteMarkerOnlyFilter<'_> {
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
impl ObjectFilter for DeleteMarkerOnlyFilter<'_> {
    async fn filter(&self) -> Result<()> {
        self.base.filter(is_delete_marker).await
    }
}

fn is_delete_marker(object: &S3Object, _config: &FilterConfig) -> bool {
    if !object.is_delete_marker() {
        debug!(
            name = FILTER_NAME,
            key = object.key(),
            version_id = object.version_id(),
            "object filtered (not a delete marker)."
        );

        return false;
    }

    true
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::config::FilterConfig;
    use crate::filters::tests::create_base_helper;
    use crate::test_utils::init_dummy_tracing_subscriber;
    use crate::types::token;
    use aws_sdk_s3::types::{DeleteMarkerEntry, Object, ObjectVersion};

    // --- Predicate unit tests ---

    #[test]
    fn latest_delete_marker_passes_through() {
        init_dummy_tracing_subscriber();

        let config = FilterConfig {
            delete_marker_only: true,
            ..Default::default()
        };

        let object = S3Object::DeleteMarker(
            DeleteMarkerEntry::builder()
                .key("test-key")
                .version_id("dm1")
                .is_latest(true)
                .build(),
        );
        assert!(is_delete_marker(&object, &config));
    }

    #[test]
    fn non_latest_delete_marker_passes_through() {
        init_dummy_tracing_subscriber();

        let config = FilterConfig {
            delete_marker_only: true,
            ..Default::default()
        };

        let object = S3Object::DeleteMarker(
            DeleteMarkerEntry::builder()
                .key("test-key")
                .version_id("dm2")
                .is_latest(false)
                .build(),
        );
        assert!(is_delete_marker(&object, &config));
    }

    #[test]
    fn delete_marker_with_none_is_latest_passes_through() {
        init_dummy_tracing_subscriber();

        let config = FilterConfig {
            delete_marker_only: true,
            ..Default::default()
        };

        // is_latest omitted (None) — still a delete marker, must pass.
        let object = S3Object::DeleteMarker(
            DeleteMarkerEntry::builder()
                .key("test-key")
                .version_id("dm-none")
                .build(),
        );
        assert!(is_delete_marker(&object, &config));
    }

    #[test]
    fn delete_marker_without_version_id_passes_through() {
        init_dummy_tracing_subscriber();

        let config = FilterConfig::default();

        // Minimal delete marker — no version_id set.
        let object = S3Object::DeleteMarker(DeleteMarkerEntry::builder().key("test-key").build());
        assert!(is_delete_marker(&object, &config));
    }

    #[test]
    fn delete_marker_with_empty_key_passes_through() {
        init_dummy_tracing_subscriber();

        let config = FilterConfig::default();

        let object = S3Object::DeleteMarker(
            DeleteMarkerEntry::builder()
                .key("")
                .version_id("dm-empty")
                .build(),
        );
        assert!(is_delete_marker(&object, &config));
    }

    #[test]
    fn latest_versioned_object_is_filtered_out() {
        init_dummy_tracing_subscriber();

        let config = FilterConfig {
            delete_marker_only: true,
            ..Default::default()
        };

        let object = S3Object::Versioning(
            ObjectVersion::builder()
                .key("test-key")
                .version_id("v1")
                .is_latest(true)
                .build(),
        );
        assert!(!is_delete_marker(&object, &config));
    }

    #[test]
    fn non_latest_versioned_object_is_filtered_out() {
        init_dummy_tracing_subscriber();

        let config = FilterConfig {
            delete_marker_only: true,
            ..Default::default()
        };

        let object = S3Object::Versioning(
            ObjectVersion::builder()
                .key("test-key")
                .version_id("v2")
                .is_latest(false)
                .build(),
        );
        assert!(!is_delete_marker(&object, &config));
    }

    #[test]
    fn versioned_object_with_none_is_latest_is_filtered_out() {
        init_dummy_tracing_subscriber();

        let config = FilterConfig::default();

        let object = S3Object::Versioning(
            ObjectVersion::builder()
                .key("test-key")
                .version_id("v-none")
                .build(),
        );
        assert!(!is_delete_marker(&object, &config));
    }

    #[test]
    fn non_versioned_object_is_filtered_out() {
        init_dummy_tracing_subscriber();

        let config = FilterConfig {
            delete_marker_only: true,
            ..Default::default()
        };

        let object = S3Object::NotVersioning(Object::builder().key("test-key").build());
        assert!(!is_delete_marker(&object, &config));
    }

    #[test]
    fn non_versioned_object_with_size_is_filtered_out() {
        init_dummy_tracing_subscriber();

        let config = FilterConfig::default();

        let object = S3Object::NotVersioning(Object::builder().key("test-key").size(1024).build());
        assert!(!is_delete_marker(&object, &config));
    }

    #[test]
    fn filter_ignores_config_fields() {
        init_dummy_tracing_subscriber();

        // Filter decision depends only on S3Object variant, not config values.
        let config_with_extras = FilterConfig {
            delete_marker_only: false, // even when false, predicate still works
            keep_latest_only: true,
            larger_size: Some(999),
            ..Default::default()
        };

        let marker = S3Object::DeleteMarker(
            DeleteMarkerEntry::builder()
                .key("k")
                .version_id("dm")
                .build(),
        );
        assert!(is_delete_marker(&marker, &config_with_extras));

        let version =
            S3Object::Versioning(ObjectVersion::builder().key("k").version_id("v").build());
        assert!(!is_delete_marker(&version, &config_with_extras));
    }

    // --- Integration tests (full filter stage with async channels) ---

    #[tokio::test]
    async fn stage_passes_delete_marker_to_next_stage() {
        init_dummy_tracing_subscriber();

        let (sender, receiver) = async_channel::bounded::<S3Object>(1000);
        let cancellation_token = token::create_pipeline_cancellation_token();
        let (mut base, next_stage_receiver) =
            create_base_helper(receiver, cancellation_token.clone()).await;
        base.config.filter_config.delete_marker_only = true;

        let filter = DeleteMarkerOnlyFilter::new(base);

        let object = S3Object::DeleteMarker(
            DeleteMarkerEntry::builder()
                .key("passed-marker")
                .version_id("dm1")
                .is_latest(true)
                .build(),
        );
        sender.send(object).await.unwrap();
        sender.close();

        filter.filter().await.unwrap();

        let received = next_stage_receiver.recv().await.unwrap();
        assert_eq!(received.key(), "passed-marker");
    }

    #[tokio::test]
    async fn stage_filters_out_versioned_object() {
        init_dummy_tracing_subscriber();

        let (sender, receiver) = async_channel::bounded::<S3Object>(1000);
        let cancellation_token = token::create_pipeline_cancellation_token();
        let (mut base, next_stage_receiver) =
            create_base_helper(receiver, cancellation_token.clone()).await;
        base.config.filter_config.delete_marker_only = true;

        let filter = DeleteMarkerOnlyFilter::new(base);

        let object = S3Object::Versioning(
            ObjectVersion::builder()
                .key("blocked-version")
                .version_id("v1")
                .is_latest(true)
                .build(),
        );
        sender.send(object).await.unwrap();
        sender.close();

        filter.filter().await.unwrap();

        assert!(next_stage_receiver.try_recv().is_err());
    }

    #[tokio::test]
    async fn stage_filters_out_non_versioned_object() {
        init_dummy_tracing_subscriber();

        let (sender, receiver) = async_channel::bounded::<S3Object>(1000);
        let cancellation_token = token::create_pipeline_cancellation_token();
        let (mut base, next_stage_receiver) =
            create_base_helper(receiver, cancellation_token.clone()).await;
        base.config.filter_config.delete_marker_only = true;

        let filter = DeleteMarkerOnlyFilter::new(base);

        let object =
            S3Object::NotVersioning(Object::builder().key("blocked-object").size(512).build());
        sender.send(object).await.unwrap();
        sender.close();

        filter.filter().await.unwrap();

        assert!(next_stage_receiver.try_recv().is_err());
    }

    #[tokio::test]
    async fn stage_mixed_objects_only_delete_markers_pass() {
        init_dummy_tracing_subscriber();

        let (sender, receiver) = async_channel::bounded::<S3Object>(1000);
        let cancellation_token = token::create_pipeline_cancellation_token();
        let (mut base, next_stage_receiver) =
            create_base_helper(receiver, cancellation_token.clone()).await;
        base.config.filter_config.delete_marker_only = true;

        let filter = DeleteMarkerOnlyFilter::new(base);

        // Send a mix of object types
        let objects: Vec<S3Object> = vec![
            S3Object::DeleteMarker(
                DeleteMarkerEntry::builder()
                    .key("marker-1")
                    .version_id("dm1")
                    .is_latest(true)
                    .build(),
            ),
            S3Object::Versioning(
                ObjectVersion::builder()
                    .key("version-1")
                    .version_id("v1")
                    .is_latest(true)
                    .build(),
            ),
            S3Object::NotVersioning(Object::builder().key("object-1").size(100).build()),
            S3Object::DeleteMarker(
                DeleteMarkerEntry::builder()
                    .key("marker-2")
                    .version_id("dm2")
                    .is_latest(false)
                    .build(),
            ),
            S3Object::Versioning(
                ObjectVersion::builder()
                    .key("version-2")
                    .version_id("v2")
                    .is_latest(false)
                    .build(),
            ),
        ];

        for obj in objects {
            sender.send(obj).await.unwrap();
        }
        sender.close();

        filter.filter().await.unwrap();

        // Only the two delete markers should pass through
        let first = next_stage_receiver.recv().await.unwrap();
        assert_eq!(first.key(), "marker-1");

        let second = next_stage_receiver.recv().await.unwrap();
        assert_eq!(second.key(), "marker-2");

        assert!(next_stage_receiver.try_recv().is_err());
    }

    #[tokio::test]
    async fn stage_completes_when_cancelled() {
        init_dummy_tracing_subscriber();

        let (_sender, receiver) = async_channel::bounded::<S3Object>(1000);
        let cancellation_token = token::create_pipeline_cancellation_token();
        let (mut base, next_stage_receiver) =
            create_base_helper(receiver, cancellation_token.clone()).await;
        base.config.filter_config.delete_marker_only = true;

        let filter = DeleteMarkerOnlyFilter::new(base);

        cancellation_token.cancel();
        filter.filter().await.unwrap();

        assert!(next_stage_receiver.try_recv().is_err());
    }

    #[tokio::test]
    async fn stage_completes_when_output_channel_closed() {
        init_dummy_tracing_subscriber();

        let (sender, receiver) = async_channel::bounded::<S3Object>(1000);
        let cancellation_token = token::create_pipeline_cancellation_token();
        let (mut base, next_stage_receiver) =
            create_base_helper(receiver, cancellation_token.clone()).await;
        base.config.filter_config.delete_marker_only = true;

        // Close the output channel before sending
        next_stage_receiver.close();

        let filter = DeleteMarkerOnlyFilter::new(base);

        let object = S3Object::DeleteMarker(
            DeleteMarkerEntry::builder()
                .key("marker")
                .version_id("dm1")
                .build(),
        );
        sender.send(object).await.unwrap();

        // Should not hang or panic — graceful shutdown
        filter.filter().await.unwrap();
    }

    #[tokio::test]
    async fn stage_empty_input_completes_immediately() {
        init_dummy_tracing_subscriber();

        let (sender, receiver) = async_channel::bounded::<S3Object>(1000);
        let cancellation_token = token::create_pipeline_cancellation_token();
        let (mut base, next_stage_receiver) =
            create_base_helper(receiver, cancellation_token.clone()).await;
        base.config.filter_config.delete_marker_only = true;

        let filter = DeleteMarkerOnlyFilter::new(base);

        // Close input immediately — no objects
        sender.close();

        filter.filter().await.unwrap();

        assert!(next_stage_receiver.try_recv().is_err());
    }
}
