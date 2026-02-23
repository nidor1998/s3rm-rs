// Property-based tests for S3 versioning support.
//
// Feature: s3rm-rs, Property 25: Versioned Bucket Delete Marker Creation
// For any deletion from a versioned bucket without version specification,
// the tool should use list_objects (creating delete markers server-side).
// **Validates: Requirements 5.1**
//
// Feature: s3rm-rs, Property 26: All-Versions Deletion
// For any deletion with delete-all-versions, the BatchDeleter should
// include version_id in delete API calls for both ObjectVersion and
// DeleteMarker entries.
// **Validates: Requirements 5.2**
//
// Feature: s3rm-rs, Property 27: Version Information Retrieval
// For any listing operation on a versioned bucket with delete_all_versions,
// the Object Lister should retrieve and include version information.
// **Validates: Requirements 5.3**
//
// Feature: s3rm-rs, Property 28: Versioned Dry-Run Display
// For any dry-run operation with versioned objects, each version is counted
// as one object in the progress display.
// **Validates: Requirements 5.4**

#[cfg(test)]
mod tests {
    use aws_sdk_s3::primitives::DateTime;
    use aws_sdk_s3::types::{
        DeleteMarkerEntry, DeletedObject, Object, ObjectIdentifier, ObjectVersion,
        ObjectVersionStorageClass,
    };
    use proptest::prelude::*;
    use std::sync::atomic::Ordering;
    use std::sync::{Arc, Mutex};

    use crate::lister::tests::create_mock_lister;
    use crate::types::S3Object;

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn make_lister_config(delete_all_versions: bool) -> crate::config::Config {
        use crate::callback::event_manager::EventManager;
        use crate::callback::filter_manager::FilterManager;
        use crate::config::{Config, FilterConfig, ForceRetryConfig};
        use crate::types::StoragePath;

        Config {
            target: StoragePath::S3 {
                bucket: "test-bucket".to_string(),
                prefix: "prefix/".to_string(),
            },
            show_no_progress: false,
            log_deletion_summary: false,
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
            max_parallel_listings: 1,
            object_listing_queue_size: 1000,
            max_parallel_listing_max_depth: 0,
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

    fn make_non_versioned_objects(count: usize) -> Vec<S3Object> {
        (0..count)
            .map(|i| {
                S3Object::NotVersioning(
                    Object::builder()
                        .key(format!("key/{i}"))
                        .size((i as i64 + 1) * 100)
                        .last_modified(DateTime::from_secs(1000))
                        .build(),
                )
            })
            .collect()
    }

    fn make_versioned_objects(keys_and_versions: &[(&str, &[&str])]) -> Vec<S3Object> {
        let mut objects = Vec::new();
        for (key, versions) in keys_and_versions {
            for (i, vid) in versions.iter().enumerate() {
                objects.push(S3Object::Versioning(
                    ObjectVersion::builder()
                        .key(*key)
                        .version_id(*vid)
                        .is_latest(i == 0)
                        .size(100)
                        .storage_class(ObjectVersionStorageClass::Standard)
                        .last_modified(DateTime::from_secs(1000 + i as i64))
                        .build(),
                ));
            }
        }
        objects
    }

    // -----------------------------------------------------------------------
    // Feature: s3rm-rs, Property 25: Versioned Bucket Delete Marker Creation
    // Validates: Requirements 5.1
    //
    // When delete_all_versions is false, the lister uses list_objects
    // (not list_object_versions). This means S3 will create delete markers
    // server-side for versioned buckets, since we only delete current versions.
    // -----------------------------------------------------------------------

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(50))]

        /// Feature: s3rm-rs, Property 25: Versioned Bucket Delete Marker Creation
        /// **Validates: Requirements 5.1**
        ///
        /// Without delete_all_versions, the lister dispatches to list_objects
        /// regardless of how many objects exist. This ensures delete markers
        /// are created server-side for versioned buckets.
        #[test]
        fn prop_non_versioned_delete_uses_list_objects(
            obj_count in 0usize..50,
        ) {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();

            rt.block_on(async {
                let config = make_lister_config(false); // delete_all_versions = false
                let objects = make_non_versioned_objects(obj_count);

                let (lister, list_objects_called, list_versions_called, receiver) =
                    create_mock_lister(config, objects.clone(), vec![]);

                lister.list_target(1000).await.unwrap();

                // Property: list_objects is called (not list_object_versions)
                prop_assert_eq!(list_objects_called.load(Ordering::SeqCst), 1);
                prop_assert_eq!(list_versions_called.load(Ordering::SeqCst), 0);

                // Property: all objects are sent through the channel
                let mut received = Vec::new();
                while let Ok(obj) = receiver.try_recv() {
                    received.push(obj);
                }
                prop_assert_eq!(received.len(), obj_count);

                // Property: received objects are NotVersioning (no version info)
                for obj in &received {
                    prop_assert!(
                        matches!(obj, S3Object::NotVersioning(_)),
                        "Non-versioned listing should produce NotVersioning objects"
                    );
                    prop_assert!(
                        obj.version_id().is_none(),
                        "Objects from list_objects should have no version_id"
                    );
                }

                Ok(())
            })?;
        }
    }

    // -----------------------------------------------------------------------
    // Feature: s3rm-rs, Property 26: All-Versions Deletion
    // Validates: Requirements 5.2
    //
    // When delete_all_versions is true, the BatchDeleter must include
    // version_id in the DeleteObjects API call for BOTH ObjectVersion
    // entries and DeleteMarker entries. This exercises the deletion stage
    // (not just the lister) and covers S3Object::DeleteMarker.
    // -----------------------------------------------------------------------

    /// Minimal mock storage that records delete_objects calls for verification.
    /// Only implements the methods needed by BatchDeleter.
    #[derive(Clone)]
    struct VersioningMockStorage {
        delete_objects_calls: Arc<Mutex<Vec<Vec<ObjectIdentifier>>>>,
    }

    impl VersioningMockStorage {
        fn new() -> Self {
            Self {
                delete_objects_calls: Arc::new(Mutex::new(Vec::new())),
            }
        }
    }

    #[async_trait::async_trait]
    impl crate::storage::StorageTrait for VersioningMockStorage {
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
            _key: &str,
            _version_id: Option<String>,
        ) -> anyhow::Result<aws_sdk_s3::operation::head_object::HeadObjectOutput> {
            Ok(aws_sdk_s3::operation::head_object::HeadObjectOutput::builder().build())
        }

        async fn get_object_tagging(
            &self,
            _key: &str,
            _version_id: Option<String>,
        ) -> anyhow::Result<aws_sdk_s3::operation::get_object_tagging::GetObjectTaggingOutput>
        {
            Ok(
                aws_sdk_s3::operation::get_object_tagging::GetObjectTaggingOutput::builder()
                    .build()
                    .unwrap(),
            )
        }

        async fn delete_object(
            &self,
            _key: &str,
            _version_id: Option<String>,
            _if_match: Option<String>,
        ) -> anyhow::Result<aws_sdk_s3::operation::delete_object::DeleteObjectOutput> {
            Ok(aws_sdk_s3::operation::delete_object::DeleteObjectOutput::builder().build())
        }

        async fn delete_objects(
            &self,
            objects: Vec<ObjectIdentifier>,
        ) -> anyhow::Result<aws_sdk_s3::operation::delete_objects::DeleteObjectsOutput> {
            let mut builder = aws_sdk_s3::operation::delete_objects::DeleteObjectsOutput::builder();
            for ident in &objects {
                builder = builder.deleted(DeletedObject::builder().key(ident.key()).build());
            }
            self.delete_objects_calls.lock().unwrap().push(objects);
            Ok(builder.build())
        }

        async fn is_versioning_enabled(&self) -> anyhow::Result<bool> {
            Ok(true)
        }

        fn get_client(&self) -> Option<Arc<aws_sdk_s3::Client>> {
            None
        }

        fn get_stats_sender(&self) -> async_channel::Sender<crate::types::DeletionStatistics> {
            let (sender, _) = async_channel::unbounded();
            sender
        }

        async fn send_stats(&self, _stats: crate::types::DeletionStatistics) {}

        fn set_warning(&self) {}
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(50))]

        /// Feature: s3rm-rs, Property 26: All-Versions Deletion
        /// **Validates: Requirements 5.2**
        ///
        /// When delete_all_versions is true, the BatchDeleter must include
        /// version_id in delete API calls for both ObjectVersion and
        /// DeleteMarker entries. Every object passed to the deleter has
        /// its version_id forwarded to the S3 DeleteObjects API.
        #[test]
        fn prop_all_versions_deletion_includes_version_ids(
            num_versions in 1usize..8,
            num_delete_markers in 1usize..5,
        ) {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();

            rt.block_on(async {
                let config = make_lister_config(true); // delete_all_versions = true

                // Build a mix of ObjectVersion and DeleteMarker entries
                let mut objects: Vec<S3Object> = Vec::new();
                let mut expected: Vec<(String, String)> = Vec::new(); // (key, version_id)

                for i in 0..num_versions {
                    let key = format!("key/{i}");
                    let vid = format!("ver-{i}");
                    expected.push((key.clone(), vid.clone()));
                    objects.push(S3Object::Versioning(
                        ObjectVersion::builder()
                            .key(key.as_str())
                            .version_id(vid.as_str())
                            .is_latest(i == 0)
                            .size(100)
                            .storage_class(ObjectVersionStorageClass::Standard)
                            .last_modified(DateTime::from_secs(1000 + i as i64))
                            .build(),
                    ));
                }

                for i in 0..num_delete_markers {
                    let key = format!("deleted/{i}");
                    let vid = format!("dm-{i}");
                    expected.push((key.clone(), vid.clone()));
                    objects.push(S3Object::DeleteMarker(
                        DeleteMarkerEntry::builder()
                            .key(key.as_str())
                            .version_id(vid.as_str())
                            .is_latest(false)
                            .last_modified(DateTime::from_secs(2000 + i as i64))
                            .build(),
                    ));
                }

                let mock = VersioningMockStorage::new();
                let boxed: Box<dyn crate::storage::StorageTrait + Send + Sync> =
                    Box::new(mock.clone());
                let deleter = crate::deleter::BatchDeleter::new(boxed);

                let result = crate::deleter::Deleter::delete(&deleter, &objects, &config)
                    .await
                    .unwrap();

                // Property: all objects (versions + delete markers) were deleted
                prop_assert_eq!(
                    result.deleted.len(),
                    expected.len(),
                    "All versions and delete markers should be deleted"
                );

                // Property: version_ids in the API call match what we provided
                let calls = mock.delete_objects_calls.lock().unwrap();
                let mut actual: Vec<(String, String)> = calls
                    .iter()
                    .flat_map(|c| c.iter())
                    .map(|id| {
                        (
                            id.key().to_string(),
                            id.version_id().unwrap_or("").to_string(),
                        )
                    })
                    .collect();

                actual.sort();
                expected.sort();

                prop_assert_eq!(
                    actual.len(),
                    expected.len(),
                    "All objects should appear in delete API calls"
                );
                prop_assert_eq!(
                    actual,
                    expected,
                    "All (key, version_id) pairs must match"
                );

                Ok(())
            })?;
        }
    }

    // -----------------------------------------------------------------------
    // Feature: s3rm-rs, Property 27: Version Information Retrieval
    // Validates: Requirements 5.3
    //
    // When listing versioned objects, each S3Object preserves its version_id,
    // key, size, and is_latest information correctly.
    // -----------------------------------------------------------------------

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(50))]

        /// Feature: s3rm-rs, Property 27: Version Information Retrieval
        /// **Validates: Requirements 5.3**
        ///
        /// Version information (version_id, key) is preserved through the
        /// listing pipeline. Each object retains its identity.
        #[test]
        fn prop_version_info_preserved_through_listing(
            num_keys in 1usize..8,
            versions_per_key in 1usize..4,
        ) {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();

            rt.block_on(async {
                let config = make_lister_config(true);

                // Build versioned objects with known keys and version_ids
                let mut expected: Vec<(String, String)> = Vec::new();
                let mut versioned_objects = Vec::new();

                for k in 0..num_keys {
                    let key = format!("data/{k}/file.txt");
                    for v in 0..versions_per_key {
                        let vid = format!("ver-{k}-{v}");
                        expected.push((key.clone(), vid.clone()));
                        versioned_objects.push(S3Object::Versioning(
                            ObjectVersion::builder()
                                .key(key.as_str())
                                .version_id(vid.as_str())
                                .is_latest(v == 0)
                                .size(100 + v as i64)
                                .storage_class(ObjectVersionStorageClass::Standard)
                                .last_modified(DateTime::from_secs(1000))
                                .build(),
                        ));
                    }
                }

                let (lister, _, _, receiver) =
                    create_mock_lister(config, vec![], versioned_objects);

                lister.list_target(1000).await.unwrap();

                // Collect received objects
                let mut received = Vec::new();
                while let Ok(obj) = receiver.try_recv() {
                    received.push(obj);
                }

                prop_assert_eq!(received.len(), expected.len());

                // Property: each object preserves its key and version_id (order-independent)
                let mut actual: Vec<(String, String)> = received
                    .iter()
                    .map(|obj| {
                        (
                            obj.key().to_string(),
                            obj.version_id().unwrap_or("").to_string(),
                        )
                    })
                    .collect();
                actual.sort();
                expected.sort();
                prop_assert_eq!(
                    actual,
                    expected,
                    "All (key, version_id) pairs must be preserved through listing"
                );

                Ok(())
            })?;
        }
    }

    // -----------------------------------------------------------------------
    // Feature: s3rm-rs, Property 28: Versioned Dry-Run Display
    // Validates: Requirements 5.4
    //
    // Each version of an object is counted as one object in the pipeline.
    // When listing versioned objects, the total count equals the total number
    // of versions across all keys (not the number of unique keys).
    // -----------------------------------------------------------------------

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(50))]

        /// Feature: s3rm-rs, Property 28: Versioned Dry-Run Display
        /// **Validates: Requirements 5.4**
        ///
        /// Each version is treated as a separate object in the pipeline.
        /// The total object count equals the total number of versions listed.
        #[test]
        fn prop_versioned_listing_counts_each_version_as_object(
            num_keys in 1usize..10,
            versions_per_key in 1usize..6,
        ) {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();

            rt.block_on(async {
                let config = make_lister_config(true);

                let versioned_objects: Vec<S3Object> = (0..num_keys)
                    .flat_map(|k| {
                        let key = format!("key/{k}");
                        (0..versions_per_key).map(move |v| {
                            S3Object::Versioning(
                                ObjectVersion::builder()
                                    .key(key.as_str())
                                    .version_id(format!("v{v}"))
                                    .is_latest(v == 0)
                                    .size(100)
                                    .storage_class(ObjectVersionStorageClass::Standard)
                                    .last_modified(DateTime::from_secs(1000))
                                    .build(),
                            )
                        })
                    })
                    .collect();

                let expected_total = num_keys * versions_per_key;

                let (lister, _, _, receiver) =
                    create_mock_lister(config, vec![], versioned_objects);

                lister.list_target(1000).await.unwrap();

                let mut count = 0;
                while receiver.try_recv().is_ok() {
                    count += 1;
                }

                // Property: total objects in pipeline = total versions (not unique keys)
                prop_assert_eq!(count, expected_total);

                Ok(())
            })?;
        }
    }

    // -----------------------------------------------------------------------
    // Unit tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn versioned_listing_includes_version_ids() {
        let config = make_lister_config(true);
        let versioned =
            make_versioned_objects(&[("file1.txt", &["v1", "v2"]), ("file2.txt", &["v1"])]);

        let (lister, _, list_versions_called, receiver) =
            create_mock_lister(config, vec![], versioned);

        lister.list_target(1000).await.unwrap();

        assert_eq!(list_versions_called.load(Ordering::SeqCst), 1);

        let mut received = Vec::new();
        while let Ok(obj) = receiver.try_recv() {
            received.push(obj);
        }

        assert_eq!(received.len(), 3);
        let mut pairs: Vec<(String, Option<String>)> = received
            .iter()
            .map(|obj| {
                (
                    obj.key().to_string(),
                    obj.version_id().map(|v| v.to_string()),
                )
            })
            .collect();
        pairs.sort();
        assert_eq!(
            pairs,
            vec![
                ("file1.txt".to_string(), Some("v1".to_string())),
                ("file1.txt".to_string(), Some("v2".to_string())),
                ("file2.txt".to_string(), Some("v1".to_string())),
            ]
        );
    }

    #[tokio::test]
    async fn batch_deleter_handles_delete_markers_with_version_id() {
        let config = make_lister_config(true);

        let objects = vec![
            S3Object::Versioning(
                ObjectVersion::builder()
                    .key("file.txt")
                    .version_id("ver-1")
                    .is_latest(true)
                    .size(500)
                    .storage_class(ObjectVersionStorageClass::Standard)
                    .last_modified(DateTime::from_secs(1000))
                    .build(),
            ),
            S3Object::DeleteMarker(
                DeleteMarkerEntry::builder()
                    .key("file.txt")
                    .version_id("dm-1")
                    .is_latest(false)
                    .last_modified(DateTime::from_secs(900))
                    .build(),
            ),
            S3Object::DeleteMarker(
                DeleteMarkerEntry::builder()
                    .key("other.txt")
                    .version_id("dm-2")
                    .is_latest(true)
                    .last_modified(DateTime::from_secs(800))
                    .build(),
            ),
        ];

        let mock = VersioningMockStorage::new();
        let boxed: Box<dyn crate::storage::StorageTrait + Send + Sync> = Box::new(mock.clone());
        let deleter = crate::deleter::BatchDeleter::new(boxed);

        let result = crate::deleter::Deleter::delete(&deleter, &objects, &config)
            .await
            .unwrap();

        assert_eq!(result.deleted.len(), 3);
        assert_eq!(result.failed.len(), 0);

        let calls = mock.delete_objects_calls.lock().unwrap();
        let idents = &calls[0];
        assert_eq!(idents.len(), 3);

        // ObjectVersion includes version_id
        assert_eq!(idents[0].key(), "file.txt");
        assert_eq!(idents[0].version_id(), Some("ver-1"));

        // DeleteMarker entries also include version_id
        assert_eq!(idents[1].key(), "file.txt");
        assert_eq!(idents[1].version_id(), Some("dm-1"));

        assert_eq!(idents[2].key(), "other.txt");
        assert_eq!(idents[2].version_id(), Some("dm-2"));
    }

    #[tokio::test]
    async fn non_versioned_listing_has_no_version_ids() {
        let config = make_lister_config(false);
        let objects = make_non_versioned_objects(3);

        let (lister, list_objects_called, list_versions_called, receiver) =
            create_mock_lister(config, objects, vec![]);

        lister.list_target(1000).await.unwrap();

        assert_eq!(list_objects_called.load(Ordering::SeqCst), 1);
        assert_eq!(list_versions_called.load(Ordering::SeqCst), 0);

        let mut received = Vec::new();
        while let Ok(obj) = receiver.try_recv() {
            received.push(obj);
        }

        assert_eq!(received.len(), 3);
        for obj in &received {
            assert!(obj.version_id().is_none());
        }
    }
}
