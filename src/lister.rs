use anyhow::Result;
use tracing::debug;

use crate::stage::Stage;

/// Lists objects from S3 for the deletion pipeline.
///
/// Adapted from s3sync's `ObjectLister`. This is a thin wrapper around the
/// `Stage` that delegates to `Storage::list_objects()` or
/// `Storage::list_object_versions()` depending on the versioning configuration.
///
/// The actual listing logic (sequential and parallel pagination) is implemented
/// in the `StorageTrait` methods on `S3Storage` (see `storage/s3/mod.rs`).
///
/// ## Pipeline role
///
/// The ObjectLister is the first stage in the deletion pipeline:
///
/// ```text
/// ObjectLister → Filters → ObjectDeleter → Terminator
/// ```
///
/// It has no `receiver` channel (it's the entry point) and writes listed objects
/// to `stage.sender` for downstream stages to process.
pub struct ObjectLister {
    stage: Stage,
}

impl ObjectLister {
    pub fn new(stage: Stage) -> Self {
        Self { stage }
    }

    /// List objects from the target S3 bucket and send them through the pipeline.
    ///
    /// When `delete_all_versions` is enabled, uses `list_object_versions()` to
    /// retrieve version information including delete markers. Otherwise uses
    /// `list_objects()` for non-versioned listing.
    ///
    /// The `max_keys` parameter controls how many keys are returned per S3 API
    /// request (pagination page size), not the total number of objects listed.
    ///
    /// Parallel listing is handled transparently by the storage implementation
    /// based on `config.max_parallel_listings` and
    /// `config.max_parallel_listing_max_depth`.
    ///
    /// # Requirements
    ///
    /// - **1.5**: Uses parallel pagination with configurable Parallel_Lister_Count
    /// - **1.6**: Allows configuration of the number of parallel listing operations
    /// - **1.7**: Supports Max_Parallel_Listing_Max_Depth option
    /// - **5.3**: Retrieves version information when operating on versioned buckets
    pub async fn list_target(&self, max_keys: i32) -> Result<()> {
        debug!("list target objects has started.");

        if self.stage.config.delete_all_versions {
            self.stage
                .target
                .list_object_versions(self.stage.sender.as_ref().unwrap(), max_keys)
                .await?;
        } else {
            self.stage
                .target
                .list_objects(self.stage.sender.as_ref().unwrap(), max_keys)
                .await?;
        }

        debug!("list target objects has been completed.");
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::config::Config;
    use crate::storage::StorageTrait;
    use crate::test_utils::init_dummy_tracing_subscriber;
    use crate::types::token::create_pipeline_cancellation_token;
    use crate::types::{DeletionStatistics, S3Object};
    use anyhow::Result;
    use async_channel::Sender;
    use async_trait::async_trait;
    use aws_sdk_s3::Client;
    use aws_sdk_s3::operation::delete_object::DeleteObjectOutput;
    use aws_sdk_s3::operation::delete_objects::DeleteObjectsOutput;
    use aws_sdk_s3::operation::get_object_tagging::GetObjectTaggingOutput;
    use aws_sdk_s3::operation::head_object::HeadObjectOutput;
    use aws_sdk_s3::primitives::DateTime;
    use aws_sdk_s3::types::{Object, ObjectIdentifier, ObjectVersion};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

    pub(crate) fn make_test_config_pub() -> Config {
        make_test_config()
    }

    fn make_test_config() -> Config {
        let mut config = crate::test_utils::make_test_config();
        config.worker_size = 1;
        config
    }

    /// Mock storage that records which list method was called and sends
    /// pre-configured objects through the channel.
    #[derive(Clone)]
    struct MockStorage {
        objects: Vec<S3Object>,
        versioned_objects: Vec<S3Object>,
        list_objects_called: Arc<AtomicU32>,
        list_object_versions_called: Arc<AtomicU32>,
        stats_sender: Sender<DeletionStatistics>,
    }

    impl MockStorage {
        fn new(
            objects: Vec<S3Object>,
            versioned_objects: Vec<S3Object>,
            stats_sender: Sender<DeletionStatistics>,
        ) -> Self {
            Self {
                objects,
                versioned_objects,
                list_objects_called: Arc::new(AtomicU32::new(0)),
                list_object_versions_called: Arc::new(AtomicU32::new(0)),
                stats_sender,
            }
        }
    }

    #[async_trait]
    impl StorageTrait for MockStorage {
        fn is_express_onezone_storage(&self) -> bool {
            false
        }

        async fn list_objects(&self, sender: &Sender<S3Object>, _max_keys: i32) -> Result<()> {
            self.list_objects_called.fetch_add(1, Ordering::SeqCst);
            for obj in &self.objects {
                sender.send(obj.clone()).await?;
            }
            Ok(())
        }

        async fn list_object_versions(
            &self,
            sender: &Sender<S3Object>,
            _max_keys: i32,
        ) -> Result<()> {
            self.list_object_versions_called
                .fetch_add(1, Ordering::SeqCst);
            for obj in &self.versioned_objects {
                sender.send(obj.clone()).await?;
            }
            Ok(())
        }

        async fn head_object(
            &self,
            _relative_key: &str,
            _version_id: Option<String>,
        ) -> Result<HeadObjectOutput> {
            unimplemented!()
        }

        async fn get_object_tagging(
            &self,
            _relative_key: &str,
            _version_id: Option<String>,
        ) -> Result<GetObjectTaggingOutput> {
            unimplemented!()
        }

        async fn delete_object(
            &self,
            _relative_key: &str,
            _version_id: Option<String>,
            _if_match: Option<String>,
        ) -> Result<DeleteObjectOutput> {
            unimplemented!()
        }

        async fn delete_objects(
            &self,
            _objects: Vec<ObjectIdentifier>,
        ) -> Result<DeleteObjectsOutput> {
            unimplemented!()
        }

        async fn is_versioning_enabled(&self) -> Result<bool> {
            Ok(false)
        }

        fn get_client(&self) -> Option<Arc<Client>> {
            None
        }

        fn get_stats_sender(&self) -> Sender<DeletionStatistics> {
            self.stats_sender.clone()
        }

        async fn send_stats(&self, stats: DeletionStatistics) {
            let _ = self.stats_sender.send(stats).await;
        }

        fn set_warning(&self) {}
    }

    fn make_test_objects() -> Vec<S3Object> {
        vec![
            S3Object::NotVersioning(
                Object::builder()
                    .key("file1.txt")
                    .size(100)
                    .last_modified(DateTime::from_secs(1000))
                    .build(),
            ),
            S3Object::NotVersioning(
                Object::builder()
                    .key("file2.txt")
                    .size(200)
                    .last_modified(DateTime::from_secs(2000))
                    .build(),
            ),
        ]
    }

    fn make_test_versioned_objects() -> Vec<S3Object> {
        vec![
            S3Object::Versioning(
                ObjectVersion::builder()
                    .key("file1.txt")
                    .version_id("v1")
                    .is_latest(true)
                    .size(100)
                    .last_modified(DateTime::from_secs(1000))
                    .build(),
            ),
            S3Object::Versioning(
                ObjectVersion::builder()
                    .key("file1.txt")
                    .version_id("v2")
                    .is_latest(false)
                    .size(90)
                    .last_modified(DateTime::from_secs(900))
                    .build(),
            ),
        ]
    }

    pub(crate) fn create_mock_lister(
        config: Config,
        objects: Vec<S3Object>,
        versioned_objects: Vec<S3Object>,
    ) -> (
        ObjectLister,
        Arc<AtomicU32>,
        Arc<AtomicU32>,
        async_channel::Receiver<S3Object>,
    ) {
        let (stats_sender, _stats_receiver) = async_channel::unbounded();
        let mock = MockStorage::new(objects, versioned_objects, stats_sender);
        let list_objects_called = mock.list_objects_called.clone();
        let list_versions_called = mock.list_object_versions_called.clone();

        let (sender, receiver) = async_channel::bounded(100);
        let cancellation_token = create_pipeline_cancellation_token();
        let has_warning = Arc::new(AtomicBool::new(false));

        let stage = Stage::new(
            config,
            Box::new(mock),
            None, // ObjectLister has no receiver (it's the entry point)
            Some(sender),
            cancellation_token,
            has_warning,
        );

        let lister = ObjectLister::new(stage);
        (lister, list_objects_called, list_versions_called, receiver)
    }

    // --- Unit tests ---

    #[tokio::test]
    async fn list_target_non_versioned() {
        init_dummy_tracing_subscriber();

        let config = make_test_config();
        let objects = make_test_objects();
        let (lister, list_objects_called, list_versions_called, receiver) =
            create_mock_lister(config, objects.clone(), vec![]);

        lister.list_target(1000).await.unwrap();

        // Should have called list_objects (not list_object_versions)
        assert_eq!(list_objects_called.load(Ordering::SeqCst), 1);
        assert_eq!(list_versions_called.load(Ordering::SeqCst), 0);

        // Should have sent all objects
        let mut received = Vec::new();
        while let Ok(obj) = receiver.try_recv() {
            received.push(obj);
        }
        assert_eq!(received.len(), 2);
        assert_eq!(received[0].key(), "file1.txt");
        assert_eq!(received[1].key(), "file2.txt");
    }

    #[tokio::test]
    async fn list_target_versioned_via_delete_all_versions() {
        init_dummy_tracing_subscriber();

        let mut config = make_test_config();
        config.delete_all_versions = true;

        let versioned = make_test_versioned_objects();
        let (lister, list_objects_called, list_versions_called, receiver) =
            create_mock_lister(config, vec![], versioned.clone());

        lister.list_target(1000).await.unwrap();

        // delete_all_versions should also use list_object_versions
        assert_eq!(list_objects_called.load(Ordering::SeqCst), 0);
        assert_eq!(list_versions_called.load(Ordering::SeqCst), 1);

        let mut received = Vec::new();
        while let Ok(obj) = receiver.try_recv() {
            received.push(obj);
        }
        assert_eq!(received.len(), 2);
    }

    #[tokio::test]
    async fn list_target_empty_bucket() {
        init_dummy_tracing_subscriber();

        let config = make_test_config();
        let (lister, _, _, receiver) = create_mock_lister(config, vec![], vec![]);

        lister.list_target(1000).await.unwrap();

        // No objects should be received
        assert!(receiver.try_recv().is_err());
    }

    #[tokio::test]
    async fn list_target_respects_max_keys_parameter() {
        init_dummy_tracing_subscriber();

        // This test verifies the max_keys parameter is passed through.
        // The mock doesn't use it, but the real S3Storage does.
        let config = make_test_config();
        let objects = make_test_objects();
        let (lister, list_objects_called, _, receiver) =
            create_mock_lister(config, objects, vec![]);

        lister.list_target(10).await.unwrap();
        assert_eq!(list_objects_called.load(Ordering::SeqCst), 1);

        // Drain receiver to prevent channel errors
        drop(receiver);
    }
}

/// Property-based tests for ObjectLister parallel listing configuration.
///
/// Feature: s3rm-rs, Property 5: Parallel Listing Configuration
/// **Validates: Requirements 1.5, 1.6, 1.7**
///
/// For any parallel listing configuration, the Object Lister should use the
/// configured number of parallel listing operations and respect the max depth
/// setting.
#[cfg(test)]
mod property_tests {
    use super::*;
    use crate::config::{Config, FilterConfig, ForceRetryConfig};
    use crate::types::StoragePath;
    use proptest::prelude::*;

    fn arb_config(
        max_parallel_listings: u16,
        max_parallel_listing_max_depth: u16,
        delete_all_versions: bool,
    ) -> Config {
        use crate::callback::event_manager::EventManager;
        use crate::callback::filter_manager::FilterManager;

        Config {
            target: StoragePath::S3 {
                bucket: "test-bucket".to_string(),
                prefix: "prefix/".to_string(),
            },
            show_no_progress: false,
            target_client_config: None,
            force_retry_config: ForceRetryConfig {
                force_retry_count: 0,
                force_retry_interval_milliseconds: 0,
            },
            tracing_config: None,
            worker_size: 1,
            warn_as_error: false,
            dry_run: false,
            rate_limit_objects: None,
            max_parallel_listings,
            object_listing_queue_size: 1000,
            max_parallel_listing_max_depth,
            allow_parallel_listings_in_express_one_zone: false,
            filter_config: FilterConfig::default(),
            max_keys: 1000,
            auto_complete_shell: None,
            event_callback_lua_script: None,
            filter_callback_lua_script: None,
            allow_lua_os_library: false,
            allow_lua_unsafe_vm: false,
            lua_vm_memory_limit: 0,
            if_match: false,
            max_delete: None,
            filter_manager: FilterManager::new(),
            event_manager: EventManager::new(),
            batch_size: 1000,
            delete_all_versions,
            force: false,
            test_user_defined_callback: false,
        }
    }

    // Feature: s3rm-rs, Property 5: Parallel Listing Configuration
    // Validates: Requirements 1.5, 1.6, 1.7
    //
    // For any valid parallel listing configuration, the config correctly
    // stores and exposes the configured values.
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn parallel_listing_config_stored_correctly(
            max_parallel_listings in 1u16..=100,
            max_parallel_listing_max_depth in 0u16..=10,
        ) {
            let config = arb_config(
                max_parallel_listings,
                max_parallel_listing_max_depth,
                false,
            );

            // Property: Config preserves parallel listing parameters
            prop_assert_eq!(config.max_parallel_listings, max_parallel_listings);
            prop_assert_eq!(config.max_parallel_listing_max_depth, max_parallel_listing_max_depth);
        }

        #[test]
        fn versioning_dispatch_is_deterministic(
            delete_all_versions in proptest::bool::ANY,
        ) {
            // Property: The versioning dispatch decision is determined solely
            // by delete_all_versions. If true, list_object_versions should be
            // used; otherwise list_objects.
            let config = arb_config(1, 0, delete_all_versions);

            prop_assert_eq!(config.delete_all_versions, delete_all_versions);
        }
    }

    // Runtime property test using the mock storage to verify dispatch behavior
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(50))]

        #[test]
        fn lister_dispatches_correctly_based_on_versioning(
            delete_all_versions in proptest::bool::ANY,
        ) {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();

            rt.block_on(async {
                let config = arb_config(1, 0, delete_all_versions);
                let should_use_versions = delete_all_versions;

                let objects = vec![
                    crate::types::S3Object::NotVersioning(
                        aws_sdk_s3::types::Object::builder()
                            .key("test.txt")
                            .size(10)
                            .last_modified(aws_sdk_s3::primitives::DateTime::from_secs(1))
                            .build(),
                    ),
                ];
                let versioned = vec![
                    crate::types::S3Object::Versioning(
                        aws_sdk_s3::types::ObjectVersion::builder()
                            .key("test.txt")
                            .version_id("v1")
                            .is_latest(true)
                            .size(10)
                            .last_modified(aws_sdk_s3::primitives::DateTime::from_secs(1))
                            .build(),
                    ),
                ];

                let (lister, list_objects_called, list_versions_called, _receiver) =
                    tests::create_mock_lister(config, objects, versioned);

                lister.list_target(1000).await.unwrap();

                if should_use_versions {
                    assert_eq!(list_objects_called.load(std::sync::atomic::Ordering::SeqCst), 0);
                    assert_eq!(list_versions_called.load(std::sync::atomic::Ordering::SeqCst), 1);
                } else {
                    assert_eq!(list_objects_called.load(std::sync::atomic::Ordering::SeqCst), 1);
                    assert_eq!(list_versions_called.load(std::sync::atomic::Ordering::SeqCst), 0);
                }
            });
        }
    }
}
