//! Filter stages for the deletion pipeline.
//!
//! Reused from s3sync's pipeline/filter infrastructure with adaptations:
//! - Simplified for deletion-only operations (no source/target key map comparison)
//! - Uses `S3Object` instead of `S3syncObject`
//! - Uses `DeletionStatistics` instead of `SyncStatistics`
//!
//! Each filter reads objects from its input channel, applies filtering logic,
//! and forwards passing objects to its output channel. Filters are chained
//! in sequence to form the filter pipeline, with logical AND semantics.
//!
//! **Note**: Content-type, metadata, and tag regex filters are NOT separate stages.
//! They are implemented within ObjectDeleter (Task 8) because they require
//! fetching object metadata/tags via HeadObject/GetObjectTagging API calls.

use anyhow::Result;
use async_trait::async_trait;
use aws_sdk_s3::primitives::DateTimeFormat;
use tracing::debug;

use crate::config::FilterConfig;
use crate::stage::{SendResult, Stage};
use crate::types::event_callback::{EventData, EventType};
use crate::types::{DeletionStatistics, S3Object};

pub mod exclude_regex;
mod filter_properties;
pub mod include_regex;
pub mod larger_size;
pub mod mtime_after;
pub mod mtime_before;
pub mod smaller_size;
pub mod user_defined;

pub use exclude_regex::ExcludeRegexFilter;
pub use include_regex::IncludeRegexFilter;
pub use larger_size::LargerSizeFilter;
pub use mtime_after::MtimeAfterFilter;
pub use mtime_before::MtimeBeforeFilter;
pub use smaller_size::SmallerSizeFilter;

/// Trait implemented by all filter stages in the deletion pipeline.
///
/// Reused from s3sync's ObjectFilter trait.
#[async_trait]
pub trait ObjectFilter {
    async fn filter(&self) -> Result<()>;
}

/// Base implementation for receive-and-filter loop shared by all simple filters.
///
/// Adapted from s3sync's ObjectFilterBase. Simplified by removing:
/// - `ObjectKeyMap` parameter (not needed for deletion, only for sync comparison)
/// - Test simulation code
pub struct ObjectFilterBase<'a> {
    name: &'a str,
    base: Stage,
}

impl ObjectFilterBase<'_> {
    /// Run the receive-and-filter loop with the given filter function.
    ///
    /// Objects where `filter_fn` returns `true` are forwarded to the next stage.
    /// Objects where it returns `false` are skipped (a `DeleteSkip` stat is sent).
    pub async fn filter<F>(&self, filter_fn: F) -> Result<()>
    where
        F: Fn(&S3Object, &FilterConfig) -> bool,
    {
        self.receive_and_filter(filter_fn).await
    }

    async fn receive_and_filter<F>(&self, filter_fn: F) -> Result<()>
    where
        F: Fn(&S3Object, &FilterConfig) -> bool,
    {
        // Yield to prevent task starvation under high load.
        // (from s3sync pipeline/filter/mod.rs)
        loop {
            tokio::task::yield_now().await;
            if self.base.cancellation_token.is_cancelled() {
                debug!(name = self.name, "filter has been cancelled.");
                return Ok(());
            }

            tokio::task::yield_now().await;
            match self.base.receiver.as_ref().unwrap().recv().await {
                Ok(object) => {
                    tokio::task::yield_now().await;
                    if !filter_fn(&object, &self.base.config.filter_config) {
                        tokio::task::yield_now().await;

                        self.base
                            .send_stats(DeletionStatistics::DeleteSkip {
                                key: object.key().to_string(),
                            })
                            .await;

                        if self.base.config.event_manager.is_callback_registered() {
                            let mut event_data = EventData::new(EventType::DELETE_FILTERED);
                            event_data.key = Some(object.key().to_string());
                            event_data.version_id = object.version_id().map(|v| v.to_string());
                            event_data.size = Some(object.size() as u64);
                            event_data.last_modified = Some(
                                object
                                    .last_modified()
                                    .fmt(DateTimeFormat::DateTime)
                                    .unwrap_or_default(),
                            );
                            event_data.message = Some(format!("Object filtered by {}", self.name));
                            self.base
                                .config
                                .event_manager
                                .trigger_event(event_data)
                                .await;
                        }

                        continue;
                    }

                    tokio::task::yield_now().await;
                    if self.base.send(object).await? == SendResult::Closed {
                        return Ok(());
                    }
                }
                Err(_) => {
                    debug!(name = self.name, "filter has been completed.");
                    return Ok(());
                }
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::config::Config;
    use crate::storage::Storage;
    use crate::test_utils::init_dummy_tracing_subscriber;
    use crate::types::token;
    use async_channel::Receiver;
    use aws_sdk_s3::types::Object;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;

    #[tokio::test]
    async fn filter_true_passes_object() {
        init_dummy_tracing_subscriber();

        let (sender, receiver) = async_channel::bounded::<S3Object>(1000);
        let cancellation_token = token::create_pipeline_cancellation_token();
        let (base, next_stage_receiver) =
            create_base_helper(receiver, cancellation_token.clone()).await;
        let filter_base = ObjectFilterBase {
            base,
            name: "unittest",
        };
        let object = S3Object::NotVersioning(Object::builder().key("test").build());

        sender.send(object).await.unwrap();
        sender.close();

        filter_base.filter(|_, _| true).await.unwrap();

        let received_object = next_stage_receiver.recv().await.unwrap();
        assert_eq!(received_object.key(), "test");
    }

    #[tokio::test]
    async fn filter_false_skips_object() {
        init_dummy_tracing_subscriber();

        let (sender, receiver) = async_channel::bounded::<S3Object>(1000);
        let cancellation_token = token::create_pipeline_cancellation_token();
        let (base, next_stage_receiver) =
            create_base_helper(receiver, cancellation_token.clone()).await;
        let filter_base = ObjectFilterBase {
            base,
            name: "unittest",
        };
        let object = S3Object::NotVersioning(Object::builder().key("test").build());

        sender.send(object).await.unwrap();
        sender.close();

        filter_base.filter(|_, _| false).await.unwrap();

        assert!(next_stage_receiver.try_recv().is_err());
    }

    #[tokio::test]
    async fn filter_cancelled() {
        init_dummy_tracing_subscriber();

        let (_, receiver) = async_channel::bounded::<S3Object>(1000);
        let cancellation_token = token::create_pipeline_cancellation_token();
        let (base, next_stage_receiver) =
            create_base_helper(receiver, cancellation_token.clone()).await;
        let filter_base = ObjectFilterBase {
            base,
            name: "unittest",
        };

        cancellation_token.cancel();
        filter_base.filter(|_, _| false).await.unwrap();

        assert!(next_stage_receiver.try_recv().is_err());
    }

    #[tokio::test]
    async fn filter_receiver_closed() {
        init_dummy_tracing_subscriber();

        let (sender, receiver) = async_channel::bounded::<S3Object>(1000);
        let cancellation_token = token::create_pipeline_cancellation_token();
        let (base, next_stage_receiver) =
            create_base_helper(receiver, cancellation_token.clone()).await;
        let filter_base = ObjectFilterBase {
            base,
            name: "unittest",
        };
        let object = S3Object::NotVersioning(Object::builder().key("test").build());

        next_stage_receiver.close();
        sender.send(object).await.unwrap();

        filter_base.filter(|_, _| true).await.unwrap();
    }

    #[tokio::test]
    async fn filter_false_emits_delete_filtered_event() {
        use crate::callback::event_manager::EventManager;
        use crate::types::event_callback::{EventCallback, EventData, EventType};
        use async_trait::async_trait;
        use std::sync::Arc;
        use tokio::sync::Mutex;

        /// Collects events for test assertions.
        struct CollectingCallback {
            events: Arc<Mutex<Vec<EventData>>>,
        }

        #[async_trait]
        impl EventCallback for CollectingCallback {
            async fn on_event(&mut self, event_data: EventData) {
                self.events.lock().await.push(event_data);
            }
        }

        init_dummy_tracing_subscriber();

        let (sender, receiver) = async_channel::bounded::<S3Object>(1000);
        let cancellation_token = token::create_pipeline_cancellation_token();
        let (mut base, _next_stage_receiver) =
            create_base_helper(receiver, cancellation_token.clone()).await;

        // Register a collecting event callback
        let events = Arc::new(Mutex::new(Vec::new()));
        let callback = CollectingCallback {
            events: events.clone(),
        };
        let mut event_manager = EventManager::new();
        event_manager.register_callback(EventType::ALL_EVENTS, callback, false);
        base.config.event_manager = event_manager;

        let filter_base = ObjectFilterBase {
            base,
            name: "test_filter",
        };
        let object = S3Object::NotVersioning(
            Object::builder()
                .key("filtered-key")
                .size(100)
                .last_modified(aws_sdk_s3::primitives::DateTime::from_secs(1_700_000_000))
                .build(),
        );

        sender.send(object).await.unwrap();
        sender.close();

        filter_base.filter(|_, _| false).await.unwrap();

        let collected = events.lock().await;
        assert_eq!(collected.len(), 1);
        assert_eq!(collected[0].event_type, EventType::DELETE_FILTERED);
        assert_eq!(collected[0].key.as_deref(), Some("filtered-key"));
        assert!(collected[0].last_modified.is_some());
        assert!(
            collected[0]
                .message
                .as_ref()
                .unwrap()
                .contains("test_filter")
        );
    }

    // --- Test helpers ---

    pub(crate) async fn create_base_helper(
        receiver: Receiver<S3Object>,
        cancellation_token: crate::types::token::PipelineCancellationToken,
    ) -> (Stage, Receiver<S3Object>) {
        let (stats_sender, _stats_receiver) = async_channel::unbounded();
        let has_warning = Arc::new(AtomicBool::new(false));

        let storage = create_mock_storage(stats_sender, has_warning.clone());

        let (sender, next_stage_receiver) = async_channel::bounded::<S3Object>(1000);

        (
            Stage {
                config: create_test_config(),
                target: storage,
                receiver: Some(receiver),
                sender: Some(sender),
                cancellation_token,
                has_warning,
            },
            next_stage_receiver,
        )
    }

    pub(crate) fn create_test_config() -> Config {
        use crate::types::StoragePath;

        let mut config = crate::test_utils::make_test_config();
        config.target = StoragePath::S3 {
            bucket: "test-bucket".to_string(),
            prefix: String::new(),
        };
        config.worker_size = 1;
        config
    }

    pub(crate) fn create_mock_storage(
        stats_sender: async_channel::Sender<DeletionStatistics>,
        has_warning: Arc<AtomicBool>,
    ) -> Storage {
        Box::new(MockStorage {
            stats_sender,
            has_warning,
        })
    }

    /// Minimal mock storage for filter tests.
    ///
    /// Only implements get_stats_sender() and send_stats() since filters
    /// don't call S3 APIs directly.
    #[derive(Clone)]
    pub(crate) struct MockStorage {
        stats_sender: async_channel::Sender<DeletionStatistics>,
        has_warning: Arc<AtomicBool>,
    }

    #[async_trait]
    impl crate::storage::StorageTrait for MockStorage {
        fn is_express_onezone_storage(&self) -> bool {
            false
        }

        async fn list_objects(
            &self,
            _sender: &async_channel::Sender<S3Object>,
            _max_keys: i32,
        ) -> anyhow::Result<()> {
            Ok(())
        }

        async fn list_object_versions(
            &self,
            _sender: &async_channel::Sender<S3Object>,
            _max_keys: i32,
        ) -> anyhow::Result<()> {
            Ok(())
        }

        async fn head_object(
            &self,
            _relative_key: &str,
            _version_id: Option<String>,
        ) -> anyhow::Result<aws_sdk_s3::operation::head_object::HeadObjectOutput> {
            unimplemented!()
        }

        async fn get_object_tagging(
            &self,
            _relative_key: &str,
            _version_id: Option<String>,
        ) -> anyhow::Result<aws_sdk_s3::operation::get_object_tagging::GetObjectTaggingOutput>
        {
            unimplemented!()
        }

        async fn delete_object(
            &self,
            _relative_key: &str,
            _version_id: Option<String>,
            _if_match: Option<String>,
        ) -> anyhow::Result<aws_sdk_s3::operation::delete_object::DeleteObjectOutput> {
            unimplemented!()
        }

        async fn delete_objects(
            &self,
            _objects: Vec<aws_sdk_s3::types::ObjectIdentifier>,
        ) -> anyhow::Result<aws_sdk_s3::operation::delete_objects::DeleteObjectsOutput> {
            unimplemented!()
        }

        async fn is_versioning_enabled(&self) -> anyhow::Result<bool> {
            Ok(false)
        }

        fn get_client(&self) -> Option<Arc<aws_sdk_s3::Client>> {
            None
        }

        fn get_stats_sender(&self) -> async_channel::Sender<DeletionStatistics> {
            self.stats_sender.clone()
        }

        async fn send_stats(&self, stats: DeletionStatistics) {
            let _ = self.stats_sender.send(stats).await;
        }

        fn set_warning(&self) {
            self.has_warning
                .store(true, std::sync::atomic::Ordering::SeqCst);
        }
    }
}
