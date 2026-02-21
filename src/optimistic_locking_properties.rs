// **Property 41: If-Match Conditional Deletion**
// **Validates: Requirements 11.1, 11.2**
//
// **Property 42: ETag Input Parsing**
// **Validates: Requirements 11.3**
//
// **Property 43: Batch Conditional Deletion Handling**
// **Validates: Requirements 11.4**

#[cfg(test)]
mod tests {
    use crate::config::args::parse_from_args;
    use crate::config::{Config, FilterConfig, ForceRetryConfig};
    use crate::types::{DeletionError, StoragePath};

    use proptest::prelude::*;

    /// Helper: create a minimal Config with the given `if_match` flag.
    fn make_config(if_match: bool) -> Config {
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
            worker_size: 4,
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
            if_match,
            max_delete: None,
            filter_manager: crate::callback::filter_manager::FilterManager::new(),
            event_manager: crate::callback::event_manager::EventManager::new(),
            batch_size: 1000,
            delete_all_versions: false,
            force: false,
            test_user_defined_callback: false,
        }
    }

    // -----------------------------------------------------------------------
    // Property 41: If-Match Conditional Deletion
    // -----------------------------------------------------------------------

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// **Property 41: If-Match Conditional Deletion**
        /// **Validates: Requirements 11.1, 11.2**
        ///
        /// When if_match is enabled, SingleDeleter must pass the object's ETag
        /// to the storage layer. When disabled, no ETag is passed regardless
        /// of whether the object has one.
        #[test]
        fn prop_single_deleter_if_match_etag_inclusion(
            if_match_enabled in proptest::bool::ANY,
            has_etag in proptest::bool::ANY,
            etag_value in "[a-f0-9]{32}",
        ) {
            use aws_sdk_s3::primitives::DateTime;
            use aws_sdk_s3::types::Object;
            use crate::types::S3Object;
            use crate::deleter::single::SingleDeleter;
            use crate::deleter::Deleter;

            let config = make_config(if_match_enabled);

            // Build object with or without ETag
            let mut builder = Object::builder()
                .key("test/key")
                .size(100)
                .last_modified(DateTime::from_secs(1000));
            if has_etag {
                builder = builder.e_tag(format!("\"{}\"", etag_value));
            }
            let obj = S3Object::NotVersioning(builder.build());

            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();

            rt.block_on(async {
                use std::sync::{Arc, Mutex};
                use crate::types::DeletionStatistics;

                // Create a recording mock to capture the if_match argument
                let (stats_sender, _stats_rx) = async_channel::unbounded::<DeletionStatistics>();
                let recorded_if_match: Arc<Mutex<Option<Option<String>>>> =
                    Arc::new(Mutex::new(None));
                let recorded_clone = recorded_if_match.clone();

                // We use the real SingleDeleter + a mock storage that records calls
                let mock = RecordingMockStorage {
                    stats_sender: stats_sender.clone(),
                    recorded_if_match: recorded_clone,
                };
                let boxed: Box<dyn crate::storage::StorageTrait + Send + Sync> = Box::new(mock);
                let deleter = SingleDeleter::new(boxed);

                let result = deleter.delete(&[obj], &config).await;
                prop_assert!(result.is_ok(), "Deletion should succeed");

                let captured = recorded_if_match.lock().unwrap();
                let captured_if_match = captured.as_ref().expect("delete_object should have been called");

                if if_match_enabled && has_etag {
                    // Must pass the ETag
                    prop_assert!(
                        captured_if_match.is_some(),
                        "When if_match=true and object has ETag, If-Match must be set"
                    );
                    let expected_etag = format!("\"{}\"", etag_value);
                    prop_assert_eq!(
                        captured_if_match.as_deref().unwrap(),
                        expected_etag.as_str(),
                        "ETag value must match the object's ETag"
                    );
                } else {
                    // Must NOT pass the ETag
                    prop_assert!(
                        captured_if_match.is_none(),
                        "When if_match=false or object has no ETag, If-Match must be None"
                    );
                }

                Ok(())
            })?;
        }

        /// **Property 41: If-Match Conditional Deletion (PreconditionFailed is non-retryable)**
        /// **Validates: Requirements 11.2**
        ///
        /// PreconditionFailed errors (ETag mismatch) are never retryable — the
        /// object should be skipped rather than retried.
        #[test]
        fn prop_precondition_failed_is_not_retryable(_dummy in 0u32..1) {
            prop_assert!(
                !DeletionError::PreconditionFailed.is_retryable(),
                "PreconditionFailed must not be retryable — object should be skipped"
            );
        }
    }

    // -----------------------------------------------------------------------
    // Property 42: ETag Input Parsing
    // -----------------------------------------------------------------------

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// **Property 42: ETag Input Parsing**
        /// **Validates: Requirements 11.3**
        ///
        /// The --if-match CLI flag is a boolean toggle. When present, Config.if_match
        /// is true; when absent, it is false. The flag correctly propagates through
        /// parse_from_args -> Config::try_from.
        #[test]
        fn prop_if_match_flag_parsing(include_flag in proptest::bool::ANY) {
            let _ = tracing_subscriber::fmt()
                .with_env_filter("dummy=trace")
                .try_init();

            let mut args = vec!["s3rm".to_string(), "s3://bucket/prefix/".to_string()];
            if include_flag {
                args.push("--if-match".to_string());
            }

            let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
            let parsed = parse_from_args(args_ref).unwrap();
            let config = Config::try_from(parsed).unwrap();

            prop_assert_eq!(
                config.if_match, include_flag,
                "Config.if_match must match whether --if-match flag was provided"
            );
        }

        /// **Property 42: ETag Input Parsing (default)**
        /// **Validates: Requirements 11.3**
        ///
        /// When --if-match is omitted and the IF_MATCH env var is unset,
        /// Config.if_match defaults to false for any valid bucket name.
        #[test]
        fn prop_if_match_default_is_false(
            bucket in "[a-z][a-z0-9-]{2,10}",
        ) {
            let _ = tracing_subscriber::fmt()
                .with_env_filter("dummy=trace")
                .try_init();

            // Ensure the env var is not influencing the result.
            // SAFETY: This test is the only writer of IF_MATCH in this process;
            // proptest runs cases sequentially within a single test function.
            unsafe { std::env::remove_var("IF_MATCH") };

            let target = format!("s3://{}/", bucket);
            let args = vec!["s3rm", target.as_str()];
            let parsed = parse_from_args(args).unwrap();
            let config = Config::try_from(parsed).unwrap();
            prop_assert!(!config.if_match, "Default if_match must be false when flag and env var are absent");
        }
    }

    // -----------------------------------------------------------------------
    // Property 43: Batch Conditional Deletion Handling
    // -----------------------------------------------------------------------

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(50))]

        /// **Property 43: Batch Conditional Deletion Handling**
        /// **Validates: Requirements 11.4**
        ///
        /// When if_match is enabled, BatchDeleter must include per-object ETags
        /// in the ObjectIdentifier list sent to DeleteObjects API. When disabled,
        /// no ETags are included even if objects have them.
        #[test]
        fn prop_batch_deleter_if_match_per_object_etags(
            if_match_enabled in proptest::bool::ANY,
            n_objects in 1usize..10,
            etag_base in "[a-f0-9]{8}",
        ) {
            use aws_sdk_s3::primitives::DateTime;
            use aws_sdk_s3::types::{ObjectVersion, ObjectVersionStorageClass};
            use crate::types::S3Object;
            use crate::deleter::batch::BatchDeleter;
            use crate::deleter::Deleter;

            let config = make_config(if_match_enabled);

            // Create n objects with unique ETags
            let objects: Vec<S3Object> = (0..n_objects)
                .map(|i| {
                    S3Object::Versioning(
                        ObjectVersion::builder()
                            .key(format!("key/{}", i))
                            .version_id(format!("v{}", i))
                            .size(100)
                            .is_latest(true)
                            .storage_class(ObjectVersionStorageClass::Standard)
                            .last_modified(DateTime::from_secs(1000))
                            .e_tag(format!("\"{}{}\"", etag_base, i))
                            .build(),
                    )
                })
                .collect();

            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();

            rt.block_on(async {
                use std::sync::{Arc, Mutex};
                use aws_sdk_s3::types::ObjectIdentifier;
                use crate::types::DeletionStatistics;

                let (stats_sender, _stats_rx) = async_channel::unbounded::<DeletionStatistics>();
                let recorded_identifiers: Arc<Mutex<Vec<Vec<ObjectIdentifier>>>> =
                    Arc::new(Mutex::new(Vec::new()));
                let recorded_clone = recorded_identifiers.clone();

                let mock = BatchRecordingMockStorage {
                    stats_sender: stats_sender.clone(),
                    recorded_identifiers: recorded_clone,
                };
                let boxed: Box<dyn crate::storage::StorageTrait + Send + Sync> = Box::new(mock);
                let deleter = BatchDeleter::new(boxed);

                let result = deleter.delete(&objects, &config).await;
                prop_assert!(result.is_ok(), "Batch deletion should succeed");

                let batches = recorded_identifiers.lock().unwrap();
                prop_assert!(!batches.is_empty(), "At least one batch should be sent");

                // Collect all identifiers across batches
                let all_idents: Vec<&ObjectIdentifier> =
                    batches.iter().flat_map(|b| b.iter()).collect();
                prop_assert_eq!(all_idents.len(), n_objects, "All objects should be in batches");

                for (i, ident) in all_idents.iter().enumerate() {
                    let expected_key = format!("key/{}", i);
                    prop_assert_eq!(ident.key(), expected_key.as_str());

                    if if_match_enabled {
                        let expected_etag = format!("\"{}{}\"", etag_base, i);
                        prop_assert_eq!(
                            ident.e_tag(),
                            Some(expected_etag.as_str()),
                            "When if_match=true, ETag must be included for object {}",
                            i
                        );
                    } else {
                        prop_assert_eq!(
                            ident.e_tag(),
                            None,
                            "When if_match=false, ETag must be None for object {}",
                            i
                        );
                    }
                }

                Ok(())
            })?;
        }

        /// **Property 43: Batch Conditional Deletion Handling (batch size respected)**
        /// **Validates: Requirements 11.4**
        ///
        /// When if_match is enabled and the batch contains more objects than
        /// the configured batch_size, objects are split into multiple batches
        /// and each includes per-object ETags.
        #[test]
        fn prop_batch_if_match_respects_batch_size(
            batch_size in 2u16..10,
            extra_objects in 1usize..8,
        ) {
            use aws_sdk_s3::primitives::DateTime;
            use aws_sdk_s3::types::{ObjectVersion, ObjectVersionStorageClass};
            use crate::types::S3Object;
            use crate::deleter::batch::BatchDeleter;
            use crate::deleter::Deleter;

            let mut config = make_config(true); // if_match enabled
            config.batch_size = batch_size;

            let n_objects = batch_size as usize + extra_objects;
            let objects: Vec<S3Object> = (0..n_objects)
                .map(|i| {
                    S3Object::Versioning(
                        ObjectVersion::builder()
                            .key(format!("key/{}", i))
                            .version_id(format!("v{}", i))
                            .size(50)
                            .is_latest(true)
                            .storage_class(ObjectVersionStorageClass::Standard)
                            .last_modified(DateTime::from_secs(2000))
                            .e_tag(format!("\"etag{}\"", i))
                            .build(),
                    )
                })
                .collect();

            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();

            rt.block_on(async {
                use std::sync::{Arc, Mutex};
                use aws_sdk_s3::types::ObjectIdentifier;
                use crate::types::DeletionStatistics;

                let (stats_sender, _stats_rx) = async_channel::unbounded::<DeletionStatistics>();
                let recorded_identifiers: Arc<Mutex<Vec<Vec<ObjectIdentifier>>>> =
                    Arc::new(Mutex::new(Vec::new()));
                let recorded_clone = recorded_identifiers.clone();

                let mock = BatchRecordingMockStorage {
                    stats_sender: stats_sender.clone(),
                    recorded_identifiers: recorded_clone,
                };
                let boxed: Box<dyn crate::storage::StorageTrait + Send + Sync> = Box::new(mock);
                let deleter = BatchDeleter::new(boxed);

                let result = deleter.delete(&objects, &config).await;
                prop_assert!(result.is_ok(), "Batch deletion should succeed");

                let batches = recorded_identifiers.lock().unwrap();
                let expected_batches = n_objects.div_ceil(batch_size as usize);
                prop_assert_eq!(
                    batches.len(),
                    expected_batches,
                    "Objects should be split into correct number of batches"
                );

                // Each batch should be at most batch_size
                for batch in batches.iter() {
                    prop_assert!(
                        batch.len() <= batch_size as usize,
                        "Batch size {} exceeds configured max {}",
                        batch.len(),
                        batch_size
                    );
                }

                // All objects across batches should have ETags (if_match=true)
                let all_idents: Vec<&ObjectIdentifier> =
                    batches.iter().flat_map(|b| b.iter()).collect();
                prop_assert_eq!(all_idents.len(), n_objects);
                for ident in &all_idents {
                    prop_assert!(
                        ident.e_tag().is_some(),
                        "Every identifier must include ETag when if_match=true"
                    );
                }

                Ok(())
            })?;
        }
    }

    // -----------------------------------------------------------------------
    // Mock storage implementations for property tests
    // -----------------------------------------------------------------------

    use async_channel::Sender;
    use async_trait::async_trait;
    use std::sync::{Arc, Mutex};

    use aws_sdk_s3::operation::delete_object::DeleteObjectOutput;
    use aws_sdk_s3::operation::delete_objects::DeleteObjectsOutput;
    use aws_sdk_s3::operation::get_object_tagging::GetObjectTaggingOutput;
    use aws_sdk_s3::operation::head_object::HeadObjectOutput;
    use aws_sdk_s3::types::{DeletedObject, ObjectIdentifier};

    use crate::storage::StorageTrait;
    use crate::types::{DeletionStatistics, S3Object};

    /// Mock storage that records the `if_match` argument passed to `delete_object`.
    #[derive(Clone)]
    struct RecordingMockStorage {
        stats_sender: Sender<DeletionStatistics>,
        recorded_if_match: Arc<Mutex<Option<Option<String>>>>,
    }

    #[async_trait]
    impl StorageTrait for RecordingMockStorage {
        fn is_express_onezone_storage(&self) -> bool {
            false
        }
        async fn list_objects(
            &self,
            _sender: &Sender<S3Object>,
            _max_keys: i32,
        ) -> anyhow::Result<()> {
            Ok(())
        }
        async fn list_object_versions(
            &self,
            _sender: &Sender<S3Object>,
            _max_keys: i32,
        ) -> anyhow::Result<()> {
            Ok(())
        }
        async fn head_object(
            &self,
            _key: &str,
            _version_id: Option<String>,
        ) -> anyhow::Result<HeadObjectOutput> {
            Ok(HeadObjectOutput::builder().build())
        }
        async fn get_object_tagging(
            &self,
            _key: &str,
            _version_id: Option<String>,
        ) -> anyhow::Result<GetObjectTaggingOutput> {
            Ok(GetObjectTaggingOutput::builder().build().unwrap())
        }
        async fn delete_object(
            &self,
            _key: &str,
            _version_id: Option<String>,
            if_match: Option<String>,
        ) -> anyhow::Result<DeleteObjectOutput> {
            *self.recorded_if_match.lock().unwrap() = Some(if_match);
            Ok(DeleteObjectOutput::builder().build())
        }
        async fn delete_objects(
            &self,
            objects: Vec<ObjectIdentifier>,
        ) -> anyhow::Result<DeleteObjectsOutput> {
            let mut builder = DeleteObjectsOutput::builder();
            for ident in &objects {
                builder = builder.deleted(DeletedObject::builder().key(ident.key()).build());
            }
            Ok(builder.build())
        }
        async fn is_versioning_enabled(&self) -> anyhow::Result<bool> {
            Ok(false)
        }
        fn get_client(&self) -> Option<Arc<aws_sdk_s3::Client>> {
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

    /// Mock storage that records the ObjectIdentifier lists passed to `delete_objects`.
    #[derive(Clone)]
    struct BatchRecordingMockStorage {
        stats_sender: Sender<DeletionStatistics>,
        recorded_identifiers: Arc<Mutex<Vec<Vec<ObjectIdentifier>>>>,
    }

    #[async_trait]
    impl StorageTrait for BatchRecordingMockStorage {
        fn is_express_onezone_storage(&self) -> bool {
            false
        }
        async fn list_objects(
            &self,
            _sender: &Sender<S3Object>,
            _max_keys: i32,
        ) -> anyhow::Result<()> {
            Ok(())
        }
        async fn list_object_versions(
            &self,
            _sender: &Sender<S3Object>,
            _max_keys: i32,
        ) -> anyhow::Result<()> {
            Ok(())
        }
        async fn head_object(
            &self,
            _key: &str,
            _version_id: Option<String>,
        ) -> anyhow::Result<HeadObjectOutput> {
            Ok(HeadObjectOutput::builder().build())
        }
        async fn get_object_tagging(
            &self,
            _key: &str,
            _version_id: Option<String>,
        ) -> anyhow::Result<GetObjectTaggingOutput> {
            Ok(GetObjectTaggingOutput::builder().build().unwrap())
        }
        async fn delete_object(
            &self,
            _key: &str,
            _version_id: Option<String>,
            _if_match: Option<String>,
        ) -> anyhow::Result<DeleteObjectOutput> {
            Ok(DeleteObjectOutput::builder().build())
        }
        async fn delete_objects(
            &self,
            objects: Vec<ObjectIdentifier>,
        ) -> anyhow::Result<DeleteObjectsOutput> {
            self.recorded_identifiers
                .lock()
                .unwrap()
                .push(objects.clone());
            let mut builder = DeleteObjectsOutput::builder();
            for ident in &objects {
                builder = builder.deleted(DeletedObject::builder().key(ident.key()).build());
            }
            Ok(builder.build())
        }
        async fn is_versioning_enabled(&self) -> anyhow::Result<bool> {
            Ok(false)
        }
        fn get_client(&self) -> Option<Arc<aws_sdk_s3::Client>> {
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
}
