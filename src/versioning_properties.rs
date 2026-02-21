// Property-based tests for S3 versioning support.
//
// **Property 25: Versioned Bucket Delete Marker Creation**
// For any deletion from a versioned bucket without version specification,
// the tool should use list_objects (creating delete markers server-side).
// **Validates: Requirements 5.1**
//
// **Property 26: All-Versions Deletion**
// For any deletion with delete-all-versions flag, the tool should use
// list_object_versions and deleters should include version_id.
// **Validates: Requirements 5.2**
//
// **Property 27: Version Information Retrieval**
// For any listing operation on a versioned bucket with delete_all_versions,
// the Object Lister should retrieve and include version information.
// **Validates: Requirements 5.3**
//
// **Property 28: Versioned Dry-Run Display**
// For any dry-run operation with versioned objects, each version is counted
// as one object in the progress display.
// **Validates: Requirements 5.4**

#[cfg(test)]
mod tests {
    use aws_sdk_s3::primitives::DateTime;
    use aws_sdk_s3::types::{Object, ObjectVersion, ObjectVersionStorageClass};
    use proptest::prelude::*;
    use std::sync::atomic::Ordering;

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
    // Property 25: Versioned Bucket Delete Marker Creation
    // Validates: Requirements 5.1
    //
    // When delete_all_versions is false, the lister uses list_objects
    // (not list_object_versions). This means S3 will create delete markers
    // server-side for versioned buckets, since we only delete current versions.
    // -----------------------------------------------------------------------

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(50))]

        /// **Property 25: Versioned Bucket Delete Marker Creation**
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
    // Property 26: All-Versions Deletion
    // Validates: Requirements 5.2
    //
    // When delete_all_versions is true, the lister uses list_object_versions.
    // All versions (including old ones) are sent through the pipeline, each
    // carrying its version_id for correct per-version deletion.
    // -----------------------------------------------------------------------

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(50))]

        /// **Property 26: All-Versions Deletion**
        /// **Validates: Requirements 5.2**
        ///
        /// With delete_all_versions=true, the lister dispatches to
        /// list_object_versions. All versions are sent with their version_id,
        /// enabling deletion of all versions including delete markers.
        #[test]
        fn prop_all_versions_delete_uses_list_object_versions(
            num_keys in 1usize..10,
            versions_per_key in 1usize..5,
        ) {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();

            rt.block_on(async {
                let config = make_lister_config(true); // delete_all_versions = true

                // Build versioned objects: each key has multiple versions
                let keys_and_versions: Vec<(String, Vec<String>)> = (0..num_keys)
                    .map(|k| {
                        let key = format!("key/{k}");
                        let versions: Vec<String> = (0..versions_per_key)
                            .map(|v| format!("v{v}"))
                            .collect();
                        (key, versions)
                    })
                    .collect();

                let versioned_objects: Vec<S3Object> = keys_and_versions
                    .iter()
                    .flat_map(|(key, versions)| {
                        versions.iter().enumerate().map(move |(i, vid)| {
                            S3Object::Versioning(
                                ObjectVersion::builder()
                                    .key(key.as_str())
                                    .version_id(vid.as_str())
                                    .is_latest(i == 0)
                                    .size(100)
                                    .storage_class(ObjectVersionStorageClass::Standard)
                                    .last_modified(DateTime::from_secs(1000 + i as i64))
                                    .build(),
                            )
                        })
                    })
                    .collect();

                let expected_total = num_keys * versions_per_key;

                let (lister, list_objects_called, list_versions_called, receiver) =
                    create_mock_lister(config, vec![], versioned_objects);

                lister.list_target(1000).await.unwrap();

                // Property: list_object_versions is called (not list_objects)
                prop_assert_eq!(list_objects_called.load(Ordering::SeqCst), 0);
                prop_assert_eq!(list_versions_called.load(Ordering::SeqCst), 1);

                // Property: all versions are sent through the channel
                let mut received = Vec::new();
                while let Ok(obj) = receiver.try_recv() {
                    received.push(obj);
                }
                prop_assert_eq!(received.len(), expected_total);

                // Property: every received object is Versioning with a version_id
                for obj in &received {
                    prop_assert!(
                        matches!(obj, S3Object::Versioning(_)),
                        "Versioned listing should produce Versioning objects"
                    );
                    prop_assert!(
                        obj.version_id().is_some(),
                        "Objects from list_object_versions should have version_id"
                    );
                }

                Ok(())
            })?;
        }
    }

    // -----------------------------------------------------------------------
    // Property 27: Version Information Retrieval
    // Validates: Requirements 5.3
    //
    // When listing versioned objects, each S3Object preserves its version_id,
    // key, size, and is_latest information correctly.
    // -----------------------------------------------------------------------

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(50))]

        /// **Property 27: Version Information Retrieval**
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

                // Property: each object preserves its key and version_id
                for (obj, (exp_key, exp_vid)) in received.iter().zip(expected.iter()) {
                    prop_assert_eq!(obj.key(), exp_key.as_str());
                    prop_assert_eq!(
                        obj.version_id(),
                        Some(exp_vid.as_str()),
                        "version_id must be preserved through listing"
                    );
                }

                Ok(())
            })?;
        }
    }

    // -----------------------------------------------------------------------
    // Property 28: Versioned Dry-Run Display
    // Validates: Requirements 5.4
    //
    // Each version of an object is counted as one object in the pipeline.
    // When listing versioned objects, the total count equals the total number
    // of versions across all keys (not the number of unique keys).
    // -----------------------------------------------------------------------

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(50))]

        /// **Property 28: Versioned Dry-Run Display**
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
        assert_eq!(received[0].key(), "file1.txt");
        assert_eq!(received[0].version_id(), Some("v1"));
        assert_eq!(received[1].key(), "file1.txt");
        assert_eq!(received[1].version_id(), Some("v2"));
        assert_eq!(received[2].key(), "file2.txt");
        assert_eq!(received[2].version_id(), Some("v1"));
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
