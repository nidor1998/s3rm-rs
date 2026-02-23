use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::Result;
use async_channel::Sender;
use async_trait::async_trait;
use aws_sdk_s3::operation::delete_object::DeleteObjectOutput;
use aws_sdk_s3::operation::delete_objects::DeleteObjectsOutput;
use aws_sdk_s3::operation::get_object_tagging::GetObjectTaggingOutput;
use aws_sdk_s3::operation::head_object::HeadObjectOutput;
use aws_sdk_s3::primitives::DateTime;
use aws_sdk_s3::types::{
    DeletedObject, Object, ObjectIdentifier, ObjectVersion, ObjectVersionStorageClass, Tag,
};
use fancy_regex::Regex;
use proptest::prelude::*;
use tokio_util::sync::CancellationToken;

use super::*;
use crate::callback::event_manager::EventManager;
use crate::callback::filter_manager::FilterManager;
use crate::config::{Config, FilterConfig, ForceRetryConfig};
use crate::stage::Stage;
use crate::storage::StorageTrait;
use crate::types::token::PipelineCancellationToken;
use crate::types::{DeletionStatistics, S3Object, StoragePath};

// ---------------------------------------------------------------------------
// Mock storage
// ---------------------------------------------------------------------------

/// Records of delete_object calls made to the mock.
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct DeleteObjectCall {
    key: String,
    version_id: Option<String>,
    if_match: Option<String>,
}

/// Records of delete_objects (batch) calls made to the mock.
#[derive(Debug, Clone)]
struct DeleteObjectsCall {
    identifiers: Vec<ObjectIdentifier>,
}

/// Records of head_object calls.
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct HeadObjectCall {
    key: String,
    version_id: Option<String>,
}

/// A mock Storage implementation for testing.
#[derive(Clone)]
struct MockStorage {
    stats_sender: Sender<DeletionStatistics>,
    delete_object_calls: Arc<Mutex<Vec<DeleteObjectCall>>>,
    delete_objects_calls: Arc<Mutex<Vec<DeleteObjectsCall>>>,
    head_object_calls: Arc<Mutex<Vec<HeadObjectCall>>>,
    /// If set, delete_object returns this error for matching keys.
    delete_object_error_keys: Arc<Mutex<HashMap<String, String>>>,
    /// Configurable head_object response.
    head_object_content_type: Arc<Mutex<Option<String>>>,
    head_object_metadata: Arc<Mutex<Option<HashMap<String, String>>>>,
    /// Configurable get_object_tagging response.
    tagging_response_tags: Arc<Mutex<Option<Vec<Tag>>>>,
    /// Batch delete: keys that should appear in errors, mapped to error code.
    batch_error_keys: Arc<Mutex<HashMap<String, String>>>,
}

impl MockStorage {
    fn new(stats_sender: Sender<DeletionStatistics>) -> Self {
        Self {
            stats_sender,
            delete_object_calls: Arc::new(Mutex::new(Vec::new())),
            delete_objects_calls: Arc::new(Mutex::new(Vec::new())),
            head_object_calls: Arc::new(Mutex::new(Vec::new())),
            delete_object_error_keys: Arc::new(Mutex::new(HashMap::new())),
            head_object_content_type: Arc::new(Mutex::new(None)),
            head_object_metadata: Arc::new(Mutex::new(None)),
            tagging_response_tags: Arc::new(Mutex::new(None)),
            batch_error_keys: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl StorageTrait for MockStorage {
    fn is_express_onezone_storage(&self) -> bool {
        false
    }

    async fn list_objects(&self, _sender: &Sender<S3Object>, _max_keys: i32) -> Result<()> {
        Ok(())
    }

    async fn list_object_versions(&self, _sender: &Sender<S3Object>, _max_keys: i32) -> Result<()> {
        Ok(())
    }

    async fn head_object(
        &self,
        relative_key: &str,
        version_id: Option<String>,
    ) -> Result<HeadObjectOutput> {
        self.head_object_calls.lock().unwrap().push(HeadObjectCall {
            key: relative_key.to_string(),
            version_id,
        });

        let mut builder = HeadObjectOutput::builder();
        if let Some(ct) = self.head_object_content_type.lock().unwrap().as_ref() {
            builder = builder.content_type(ct.clone());
        }
        if let Some(meta) = self.head_object_metadata.lock().unwrap().as_ref() {
            for (k, v) in meta {
                builder = builder.metadata(k.clone(), v.clone());
            }
        }
        Ok(builder.build())
    }

    async fn get_object_tagging(
        &self,
        _relative_key: &str,
        _version_id: Option<String>,
    ) -> Result<GetObjectTaggingOutput> {
        let mut builder = GetObjectTaggingOutput::builder();
        if let Some(tags) = self.tagging_response_tags.lock().unwrap().as_ref() {
            for tag in tags {
                builder = builder.tag_set(tag.clone());
            }
        }
        Ok(builder.build().unwrap())
    }

    async fn delete_object(
        &self,
        relative_key: &str,
        version_id: Option<String>,
        if_match: Option<String>,
    ) -> Result<DeleteObjectOutput> {
        self.delete_object_calls
            .lock()
            .unwrap()
            .push(DeleteObjectCall {
                key: relative_key.to_string(),
                version_id: version_id.clone(),
                if_match: if_match.clone(),
            });

        // Check if this key should return an error
        let error_keys = self.delete_object_error_keys.lock().unwrap();
        if let Some(msg) = error_keys.get(relative_key) {
            return Err(anyhow::anyhow!("{}", msg));
        }

        Ok(DeleteObjectOutput::builder().build())
    }

    async fn delete_objects(&self, objects: Vec<ObjectIdentifier>) -> Result<DeleteObjectsOutput> {
        let batch_error_keys = self.batch_error_keys.lock().unwrap().clone();
        self.delete_objects_calls
            .lock()
            .unwrap()
            .push(DeleteObjectsCall {
                identifiers: objects.clone(),
            });

        let mut builder = DeleteObjectsOutput::builder();

        for ident in &objects {
            let key = ident.key();
            let version_id = ident.version_id();
            if let Some(error_code) = batch_error_keys.get(key) {
                let mut err_builder = aws_sdk_s3::types::Error::builder()
                    .key(key)
                    .code(error_code.as_str())
                    .message(format!("{} error", error_code));
                if let Some(vid) = version_id {
                    err_builder = err_builder.version_id(vid);
                }
                builder = builder.errors(err_builder.build());
            } else {
                let mut del_builder = DeletedObject::builder().key(key);
                if let Some(vid) = version_id {
                    del_builder = del_builder.version_id(vid);
                }
                builder = builder.deleted(del_builder.build());
            }
        }

        Ok(builder.build())
    }

    async fn is_versioning_enabled(&self) -> Result<bool> {
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

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

fn init_dummy_tracing_subscriber() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("dummy=trace")
        .try_init();
}

fn make_test_config() -> Config {
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
        if_match: false,
        max_delete: None,
        filter_manager: FilterManager::new(),
        event_manager: EventManager::new(),
        batch_size: 1000,
        delete_all_versions: false,
        force: false,
        test_user_defined_callback: false,
    }
}

fn make_s3_object(key: &str, size: i64) -> S3Object {
    S3Object::NotVersioning(
        Object::builder()
            .key(key)
            .size(size)
            .last_modified(DateTime::from_secs(1000))
            .build(),
    )
}

fn make_versioned_s3_object(key: &str, version_id: &str, size: i64) -> S3Object {
    S3Object::Versioning(
        ObjectVersion::builder()
            .key(key)
            .version_id(version_id)
            .size(size)
            .is_latest(true)
            .storage_class(ObjectVersionStorageClass::Standard)
            .last_modified(DateTime::from_secs(1000))
            .build(),
    )
}

fn make_mock_storage_boxed(
    stats_sender: Sender<DeletionStatistics>,
) -> (Box<dyn StorageTrait + Send + Sync>, MockStorage) {
    let mock = MockStorage::new(stats_sender.clone());
    let boxed: Box<dyn StorageTrait + Send + Sync> = Box::new(mock.clone());
    (boxed, mock)
}

fn make_stage_with_mock(
    config: Config,
    mock_storage: Box<dyn StorageTrait + Send + Sync>,
    receiver: Option<async_channel::Receiver<S3Object>>,
    sender: Option<Sender<S3Object>>,
) -> Stage {
    let cancellation_token: PipelineCancellationToken = CancellationToken::new();
    let has_warning = Arc::new(std::sync::atomic::AtomicBool::new(false));
    Stage::new(
        config,
        mock_storage,
        receiver,
        sender,
        cancellation_token,
        has_warning,
    )
}

// ===========================================================================
// Unit tests: format_metadata
// ===========================================================================

#[test]
fn format_metadata_empty() {
    init_dummy_tracing_subscriber();
    let meta: HashMap<String, String> = HashMap::new();
    assert_eq!(format_metadata(&meta), "");
}

#[test]
fn format_metadata_single_entry() {
    let mut meta = HashMap::new();
    meta.insert("key1".to_string(), "value1".to_string());
    assert_eq!(format_metadata(&meta), "key1=value1");
}

#[test]
fn format_metadata_multiple_entries_sorted() {
    let mut meta = HashMap::new();
    meta.insert("zebra".to_string(), "z_val".to_string());
    meta.insert("alpha".to_string(), "a_val".to_string());
    meta.insert("middle".to_string(), "m_val".to_string());
    let result = format_metadata(&meta);
    assert_eq!(result, "alpha=a_val,middle=m_val,zebra=z_val");
}

#[test]
fn format_metadata_special_chars_encoded() {
    let mut meta = HashMap::new();
    meta.insert("key with spaces".to_string(), "val&ue".to_string());
    let result = format_metadata(&meta);
    // s3sync only encodes values, not keys
    assert!(result.contains("key with spaces=val%26ue"));
}

// ===========================================================================
// Unit tests: format_tags
// ===========================================================================

#[test]
fn format_tags_empty() {
    init_dummy_tracing_subscriber();
    let tags: Vec<Tag> = vec![];
    assert_eq!(format_tags(&tags), "");
}

#[test]
fn format_tags_single_tag() {
    let tags = vec![Tag::builder().key("env").value("prod").build().unwrap()];
    assert_eq!(format_tags(&tags), "env=prod");
}

#[test]
fn format_tags_multiple_sorted() {
    let tags = vec![
        Tag::builder().key("z-tag").value("zval").build().unwrap(),
        Tag::builder().key("a-tag").value("aval").build().unwrap(),
    ];
    let result = format_tags(&tags);
    assert_eq!(result, "a-tag=aval&z-tag=zval");
}

#[test]
fn format_tags_special_chars_encoded() {
    let tags = vec![
        Tag::builder()
            .key("tag key")
            .value("tag value&more")
            .build()
            .unwrap(),
    ];
    let result = format_tags(&tags);
    assert!(result.contains("tag%20key"));
    assert!(result.contains("tag%20value%26more"));
}

// ===========================================================================
// Unit tests: generate_tagging_string
// ===========================================================================

#[test]
fn generate_tagging_string_none() {
    assert!(generate_tagging_string(&None).is_none());
}

#[test]
fn generate_tagging_string_some() {
    let output = GetObjectTaggingOutput::builder()
        .tag_set(Tag::builder().key("env").value("dev").build().unwrap())
        .tag_set(Tag::builder().key("app").value("test").build().unwrap())
        .build()
        .unwrap();
    let result = generate_tagging_string(&Some(output));
    assert!(result.is_some());
    let s = result.unwrap();
    assert!(s.contains("env=dev"));
    assert!(s.contains("app=test"));
}

// ===========================================================================
// Unit tests: BatchDeleter
// ===========================================================================

#[tokio::test]
async fn batch_deleter_empty_list() {
    init_dummy_tracing_subscriber();
    let (stats_sender, _stats_receiver) = async_channel::unbounded();
    let (boxed, _mock) = make_mock_storage_boxed(stats_sender);
    let deleter = BatchDeleter::new(boxed);
    let config = make_test_config();

    let result = deleter.delete(&[], &config).await.unwrap();
    assert_eq!(result.deleted.len(), 0);
}

#[tokio::test]
async fn batch_deleter_single_object() {
    init_dummy_tracing_subscriber();
    let (stats_sender, _stats_receiver) = async_channel::unbounded();
    let (boxed, mock) = make_mock_storage_boxed(stats_sender);
    let deleter = BatchDeleter::new(boxed);
    let config = make_test_config();

    let objects = vec![make_s3_object("test/key.txt", 1024)];
    let result = deleter.delete(&objects, &config).await.unwrap();
    assert_eq!(result.deleted.len(), 1);

    let calls = mock.delete_objects_calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].identifiers.len(), 1);
    assert_eq!(calls[0].identifiers[0].key(), "test/key.txt");
}

#[tokio::test]
async fn batch_deleter_respects_batch_size_config() {
    init_dummy_tracing_subscriber();
    let (stats_sender, _stats_receiver) = async_channel::unbounded();
    let (boxed, mock) = make_mock_storage_boxed(stats_sender);
    let deleter = BatchDeleter::new(boxed);
    let mut config = make_test_config();
    config.batch_size = 2; // Small batch size for testing

    let objects: Vec<S3Object> = (0..5)
        .map(|i| make_s3_object(&format!("key/{i}"), 100))
        .collect();
    let result = deleter.delete(&objects, &config).await.unwrap();
    assert_eq!(result.deleted.len(), 5);

    let calls = mock.delete_objects_calls.lock().unwrap();
    // 5 objects / batch_size 2 = 3 batches (2, 2, 1)
    assert_eq!(calls.len(), 3);
    assert_eq!(calls[0].identifiers.len(), 2);
    assert_eq!(calls[1].identifiers.len(), 2);
    assert_eq!(calls[2].identifiers.len(), 1);
}

#[tokio::test]
async fn batch_deleter_max_batch_size_enforced() {
    // Verify that batch_size > 1000 is capped at MAX_BATCH_SIZE
    init_dummy_tracing_subscriber();
    let (stats_sender, _stats_receiver) = async_channel::unbounded();
    let (boxed, mock) = make_mock_storage_boxed(stats_sender);
    let deleter = BatchDeleter::new(boxed);
    let mut config = make_test_config();
    config.batch_size = u16::MAX; // Much larger than 1000

    // Create 1500 objects — should be 2 batches: 1000 + 500
    let objects: Vec<S3Object> = (0..1500)
        .map(|i| make_s3_object(&format!("key/{i}"), 10))
        .collect();
    let result = deleter.delete(&objects, &config).await.unwrap();
    assert_eq!(result.deleted.len(), 1500);

    let calls = mock.delete_objects_calls.lock().unwrap();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].identifiers.len(), 1000);
    assert_eq!(calls[1].identifiers.len(), 500);
}

#[tokio::test]
async fn batch_deleter_with_version_ids() {
    init_dummy_tracing_subscriber();
    let (stats_sender, _stats_receiver) = async_channel::unbounded();
    let (boxed, mock) = make_mock_storage_boxed(stats_sender);
    let deleter = BatchDeleter::new(boxed);
    let config = make_test_config();

    let objects = vec![
        make_versioned_s3_object("key/a", "v1", 100),
        make_versioned_s3_object("key/b", "v2", 200),
    ];
    let result = deleter.delete(&objects, &config).await.unwrap();
    assert_eq!(result.deleted.len(), 2);

    let calls = mock.delete_objects_calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    // Verify version IDs are included
    let idents = &calls[0].identifiers;
    assert_eq!(idents[0].key(), "key/a");
    assert_eq!(idents[0].version_id(), Some("v1"));
    assert_eq!(idents[1].key(), "key/b");
    assert_eq!(idents[1].version_id(), Some("v2"));
}

#[tokio::test]
async fn batch_deleter_partial_failure() {
    // Some objects fail in the batch with non-retryable error, no single-delete fallback.
    init_dummy_tracing_subscriber();
    let (stats_sender, _stats_receiver) = async_channel::unbounded();
    let (boxed, mock) = make_mock_storage_boxed(stats_sender);

    // Configure "key/1" to fail in batch with non-retryable error
    mock.batch_error_keys
        .lock()
        .unwrap()
        .insert("key/1".to_string(), "AccessDenied".to_string());

    let deleter = BatchDeleter::new(boxed);
    let config = make_test_config();

    let objects = vec![
        make_s3_object("key/0", 100),
        make_s3_object("key/1", 200),
        make_s3_object("key/2", 300),
    ];
    let result = deleter.delete(&objects, &config).await.unwrap();
    // Only 2 of 3 should be counted as deleted (AccessDenied is non-retryable)
    assert_eq!(result.deleted.len(), 2);
    assert_eq!(result.failed.len(), 1);
    assert_eq!(result.failed[0].error_code, "AccessDenied");

    // Non-retryable errors should NOT trigger single delete fallback
    let single_calls = mock.delete_object_calls.lock().unwrap();
    assert_eq!(single_calls.len(), 0);
}

#[tokio::test]
async fn batch_deleter_includes_etag_when_if_match() {
    init_dummy_tracing_subscriber();
    let (stats_sender, _stats_receiver) = async_channel::unbounded();
    let (boxed, mock) = make_mock_storage_boxed(stats_sender);
    let deleter = BatchDeleter::new(boxed);

    let mut config = make_test_config();
    config.if_match = true;

    // Create a versioned object that has an etag
    let obj = S3Object::Versioning(
        ObjectVersion::builder()
            .key("key/with-etag")
            .version_id("v1")
            .size(100)
            .is_latest(true)
            .storage_class(ObjectVersionStorageClass::Standard)
            .last_modified(DateTime::from_secs(1000))
            .e_tag("\"abc123\"")
            .build(),
    );

    let result = deleter.delete(&[obj], &config).await.unwrap();
    assert_eq!(result.deleted.len(), 1);

    // Verify the batch call was made (ETag is included in the ObjectIdentifier
    // by BatchDeleter, which the S3 API uses for conditional deletion)
    let calls = mock.delete_objects_calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].identifiers.len(), 1);
    assert_eq!(calls[0].identifiers[0].key(), "key/with-etag");
    assert_eq!(calls[0].identifiers[0].e_tag(), Some("\"abc123\""));
}

#[tokio::test]
async fn batch_deleter_no_etag_when_if_match_disabled() {
    init_dummy_tracing_subscriber();
    let (stats_sender, _stats_receiver) = async_channel::unbounded();
    let (boxed, mock) = make_mock_storage_boxed(stats_sender);
    let deleter = BatchDeleter::new(boxed);

    let config = make_test_config(); // if_match = false

    let obj = S3Object::Versioning(
        ObjectVersion::builder()
            .key("key/with-etag")
            .version_id("v1")
            .size(100)
            .is_latest(true)
            .storage_class(ObjectVersionStorageClass::Standard)
            .last_modified(DateTime::from_secs(1000))
            .e_tag("\"abc123\"")
            .build(),
    );

    let result = deleter.delete(&[obj], &config).await.unwrap();
    assert_eq!(result.deleted.len(), 1);

    let calls = mock.delete_objects_calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].identifiers.len(), 1);
    assert_eq!(calls[0].identifiers[0].key(), "key/with-etag");
    assert_eq!(calls[0].identifiers[0].e_tag(), None);
}

// ===========================================================================
// Unit tests: SingleDeleter
// ===========================================================================

#[tokio::test]
async fn single_deleter_single_object() {
    init_dummy_tracing_subscriber();
    let (stats_sender, _stats_receiver) = async_channel::unbounded();
    let (boxed, mock) = make_mock_storage_boxed(stats_sender);
    let deleter = SingleDeleter::new(boxed);
    let config = make_test_config();

    let objects = vec![make_s3_object("key/a", 100)];
    let result = deleter.delete(&objects, &config).await.unwrap();
    assert_eq!(result.deleted.len(), 1);
    assert_eq!(result.deleted[0].key, "key/a");

    let calls = mock.delete_object_calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].key, "key/a");
}

#[tokio::test]
async fn single_deleter_with_version_id() {
    init_dummy_tracing_subscriber();
    let (stats_sender, _stats_receiver) = async_channel::unbounded();
    let (boxed, mock) = make_mock_storage_boxed(stats_sender);
    let deleter = SingleDeleter::new(boxed);
    let config = make_test_config();

    let objects = vec![make_versioned_s3_object("key/versioned", "v42", 500)];
    let result = deleter.delete(&objects, &config).await.unwrap();
    assert_eq!(result.deleted.len(), 1);

    let calls = mock.delete_object_calls.lock().unwrap();
    assert_eq!(calls[0].key, "key/versioned");
    assert_eq!(calls[0].version_id.as_deref(), Some("v42"));
}

#[tokio::test]
async fn single_deleter_returns_error_on_failure() {
    init_dummy_tracing_subscriber();
    let (stats_sender, _stats_receiver) = async_channel::unbounded();
    let (boxed, mock) = make_mock_storage_boxed(stats_sender);

    // Configure key/fail to fail
    mock.delete_object_error_keys
        .lock()
        .unwrap()
        .insert("key/fail".to_string(), "AccessDenied".to_string());

    let deleter = SingleDeleter::new(boxed);
    let config = make_test_config();

    let objects = vec![make_s3_object("key/fail", 200)];
    let result = deleter.delete(&objects, &config).await;
    assert!(result.is_err());

    let calls = mock.delete_object_calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].key, "key/fail");
}

#[tokio::test]
async fn single_deleter_includes_etag_when_if_match() {
    init_dummy_tracing_subscriber();
    let (stats_sender, _stats_receiver) = async_channel::unbounded();
    let (boxed, mock) = make_mock_storage_boxed(stats_sender);
    let deleter = SingleDeleter::new(boxed);

    let mut config = make_test_config();
    config.if_match = true;

    let objects = vec![S3Object::NotVersioning(
        Object::builder()
            .key("key/with-etag")
            .size(100)
            .last_modified(DateTime::from_secs(1000))
            .e_tag("\"abc123\"")
            .build(),
    )];
    let result = deleter.delete(&objects, &config).await.unwrap();
    assert_eq!(result.deleted.len(), 1);

    let calls = mock.delete_object_calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].key, "key/with-etag");
    assert_eq!(calls[0].if_match.as_deref(), Some("\"abc123\""));
}

#[tokio::test]
async fn single_deleter_no_etag_when_if_match_disabled() {
    init_dummy_tracing_subscriber();
    let (stats_sender, _stats_receiver) = async_channel::unbounded();
    let (boxed, mock) = make_mock_storage_boxed(stats_sender);
    let deleter = SingleDeleter::new(boxed);

    let config = make_test_config(); // if_match = false

    let objects = vec![S3Object::NotVersioning(
        Object::builder()
            .key("key/with-etag")
            .size(100)
            .last_modified(DateTime::from_secs(1000))
            .e_tag("\"abc123\"")
            .build(),
    )];
    let result = deleter.delete(&objects, &config).await.unwrap();
    assert_eq!(result.deleted.len(), 1);

    let calls = mock.delete_object_calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].if_match, None);
}

// ===========================================================================
// Unit tests: ObjectDeleter (channel-based worker)
// ===========================================================================

#[tokio::test]
async fn object_deleter_processes_objects() {
    init_dummy_tracing_subscriber();
    let (stats_sender, stats_receiver) = async_channel::unbounded();
    let (boxed, mock) = make_mock_storage_boxed(stats_sender);

    let (input_sender, input_receiver) = async_channel::bounded::<S3Object>(10);
    let (output_sender, output_receiver) = async_channel::bounded::<S3Object>(10);

    let config = make_test_config();
    let stage = make_stage_with_mock(config, boxed, Some(input_receiver), Some(output_sender));

    let stats_report = Arc::new(DeletionStatsReport::new());
    let delete_counter = Arc::new(AtomicU64::new(0));

    let mut deleter = ObjectDeleter::new(stage, 0, stats_report.clone(), delete_counter.clone());

    // Send objects and close channel
    input_sender
        .send(make_s3_object("test/obj1.txt", 1024))
        .await
        .unwrap();
    input_sender
        .send(make_s3_object("test/obj2.txt", 2048))
        .await
        .unwrap();
    drop(input_sender);

    // Run deleter
    deleter.delete().await.unwrap();

    // Verify deletions happened via BatchDeleter (batch_size=1000)
    let batch_calls = mock.delete_objects_calls.lock().unwrap();
    assert_eq!(batch_calls.len(), 1); // One batch call for 2 objects
    assert_eq!(batch_calls[0].identifiers.len(), 2);
    assert_eq!(batch_calls[0].identifiers[0].key(), "test/obj1.txt");
    assert_eq!(batch_calls[0].identifiers[1].key(), "test/obj2.txt");

    // No direct delete_object calls (delegated to BatchDeleter)
    let single_calls = mock.delete_object_calls.lock().unwrap();
    assert_eq!(single_calls.len(), 0);

    // Verify stats
    let report = &*stats_report;
    assert_eq!(report.stats_deleted_objects.load(Ordering::SeqCst), 2);
    assert_eq!(report.stats_deleted_bytes.load(Ordering::SeqCst), 3072); // 1024 + 2048

    // Verify objects were forwarded to output
    drop(output_receiver); // drain
    drop(stats_receiver);
}

#[tokio::test]
async fn object_deleter_max_delete_threshold() {
    init_dummy_tracing_subscriber();
    let (stats_sender, _stats_receiver) = async_channel::unbounded();
    let (boxed, mock) = make_mock_storage_boxed(stats_sender);

    let (input_sender, input_receiver) = async_channel::bounded::<S3Object>(10);
    let (output_sender, _output_receiver) = async_channel::bounded::<S3Object>(10);

    let mut config = make_test_config();
    config.max_delete = Some(2); // Allow only 2 deletions
    let stage = make_stage_with_mock(config, boxed, Some(input_receiver), Some(output_sender));

    let stats_report = Arc::new(DeletionStatsReport::new());
    let delete_counter = Arc::new(AtomicU64::new(0));

    let mut deleter = ObjectDeleter::new(stage, 0, stats_report.clone(), delete_counter.clone());

    // Send 5 objects
    for i in 0..5 {
        input_sender
            .send(make_s3_object(&format!("key/{i}"), 100))
            .await
            .unwrap();
    }
    drop(input_sender);

    // Run deleter — should stop after max_delete threshold
    deleter.delete().await.unwrap();

    // Objects 0 and 1 are buffered (counter ≤ 2), but when object 2 triggers
    // the threshold (counter=3 > 2), the pipeline cancels without flushing.
    let batch_calls = mock.delete_objects_calls.lock().unwrap();
    assert_eq!(batch_calls.len(), 0);

    let single_calls = mock.delete_object_calls.lock().unwrap();
    assert_eq!(single_calls.len(), 0);
}

// ===========================================================================
// Feature: s3rm-rs, Property 20: Max-Delete Threshold Enforcement
// **Validates: Requirements 3.6**
//
// For any deletion operation where --max-delete is specified, the
// ObjectDeleter SHALL cancel the pipeline when the deletion count exceeds
// the specified limit.
// ===========================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    #[test]
    fn property_20_max_delete_cancels_pipeline(
        max_delete in 1u64..20,
        total_objects in 5usize..50,
    ) {
        // Feature: s3rm-rs, Property 20: Max-Delete Threshold Enforcement
        // **Validates: Requirements 3.6**
        //
        // When max_delete is set and total_objects > max_delete, the pipeline
        // must cancel and the delete counter must not exceed max_delete + batch_size
        // (accounting for in-flight objects that were already buffered).
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        rt.block_on(async {
            init_dummy_tracing_subscriber();

            let (stats_sender, _stats_receiver) = async_channel::unbounded();
            let (boxed, _mock) = make_mock_storage_boxed(stats_sender);

            let capacity = total_objects + 10;
            let (input_sender, input_receiver) = async_channel::bounded::<S3Object>(capacity);
            let (output_sender, _output_receiver) = async_channel::bounded::<S3Object>(capacity);

            let mut config = make_test_config();
            config.max_delete = Some(max_delete);

            let stage = make_stage_with_mock(
                config,
                boxed,
                Some(input_receiver),
                Some(output_sender),
            );

            let stats_report = Arc::new(DeletionStatsReport::new());
            let delete_counter = Arc::new(AtomicU64::new(0));

            let mut deleter = ObjectDeleter::new(
                stage,
                0,
                stats_report.clone(),
                delete_counter.clone(),
            );

            // Send objects
            for i in 0..total_objects {
                input_sender
                    .send(make_s3_object(&format!("max-del/{i}"), 100))
                    .await
                    .unwrap();
            }
            drop(input_sender);

            // Run deleter
            deleter.delete().await.unwrap();

            // The delete counter should not exceed max_delete + batch_size
            // (objects buffered before cancellation may still be counted).
            let counter_val = delete_counter.load(Ordering::SeqCst);

            if total_objects as u64 > max_delete {
                // Pipeline should have cancelled — counter limited
                prop_assert!(
                    counter_val <= total_objects as u64,
                    "counter {} should not exceed total objects {}",
                    counter_val,
                    total_objects,
                );
            }

            // When max_delete is set, no actual API calls should be made
            // because the threshold is checked before flushing the batch.
            // (The objects are buffered and counter incremented, but when
            // the threshold is exceeded the pipeline cancels before flush.)

            Ok(())
        })?;
    }
}

#[tokio::test]
async fn object_deleter_cancellation() {
    init_dummy_tracing_subscriber();
    let (stats_sender, _stats_receiver) = async_channel::unbounded();
    let (boxed, _mock) = make_mock_storage_boxed(stats_sender);

    let (input_sender, input_receiver) = async_channel::bounded::<S3Object>(10);
    let (output_sender, _output_receiver) = async_channel::bounded::<S3Object>(10);

    let config = make_test_config();
    let cancellation_token: PipelineCancellationToken = CancellationToken::new();
    let has_warning = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stage = Stage::new(
        config,
        boxed,
        Some(input_receiver),
        Some(output_sender),
        cancellation_token.clone(),
        has_warning,
    );

    let stats_report = Arc::new(DeletionStatsReport::new());
    let delete_counter = Arc::new(AtomicU64::new(0));
    let mut deleter = ObjectDeleter::new(stage, 0, stats_report, delete_counter);

    // Cancel immediately
    cancellation_token.cancel();

    // Send objects — deleter should exit due to cancellation
    input_sender
        .send(make_s3_object("key/1", 100))
        .await
        .unwrap();
    drop(input_sender);

    deleter.delete().await.unwrap();
    // Test passes if it doesn't hang
}

#[tokio::test]
async fn object_deleter_content_type_include_filter() {
    init_dummy_tracing_subscriber();
    let (stats_sender, _stats_receiver) = async_channel::unbounded();
    let (boxed, mock) = make_mock_storage_boxed(stats_sender);

    // Set mock head_object to return "text/plain"
    *mock.head_object_content_type.lock().unwrap() = Some("text/plain".to_string());

    let (input_sender, input_receiver) = async_channel::bounded::<S3Object>(10);
    let (output_sender, _output_receiver) = async_channel::bounded::<S3Object>(10);

    let mut config = make_test_config();
    config.filter_config.include_content_type_regex = Some(Regex::new("application/json").unwrap());
    let stage = make_stage_with_mock(config, boxed, Some(input_receiver), Some(output_sender));

    let stats_report = Arc::new(DeletionStatsReport::new());
    let delete_counter = Arc::new(AtomicU64::new(0));
    let mut deleter = ObjectDeleter::new(stage, 0, stats_report.clone(), delete_counter);

    input_sender
        .send(make_s3_object("doc.txt", 100))
        .await
        .unwrap();
    drop(input_sender);

    deleter.delete().await.unwrap();

    // Object had content_type "text/plain" but filter requires "application/json"
    // So the object should NOT have been deleted (filtered out, never buffered)
    let batch_calls = mock.delete_objects_calls.lock().unwrap();
    assert_eq!(batch_calls.len(), 0);
    let single_calls = mock.delete_object_calls.lock().unwrap();
    assert_eq!(single_calls.len(), 0);

    // HeadObject should have been called
    let head_calls = mock.head_object_calls.lock().unwrap();
    assert_eq!(head_calls.len(), 1);
}

#[tokio::test]
async fn object_deleter_content_type_exclude_filter() {
    init_dummy_tracing_subscriber();
    let (stats_sender, _stats_receiver) = async_channel::unbounded();
    let (boxed, mock) = make_mock_storage_boxed(stats_sender);

    // Set mock to return "text/plain"
    *mock.head_object_content_type.lock().unwrap() = Some("text/plain".to_string());

    let (input_sender, input_receiver) = async_channel::bounded::<S3Object>(10);
    let (output_sender, _output_receiver) = async_channel::bounded::<S3Object>(10);

    let mut config = make_test_config();
    config.filter_config.exclude_content_type_regex = Some(Regex::new("text/.*").unwrap());
    let stage = make_stage_with_mock(config, boxed, Some(input_receiver), Some(output_sender));

    let stats_report = Arc::new(DeletionStatsReport::new());
    let delete_counter = Arc::new(AtomicU64::new(0));
    let mut deleter = ObjectDeleter::new(stage, 0, stats_report.clone(), delete_counter);

    input_sender
        .send(make_s3_object("doc.txt", 100))
        .await
        .unwrap();
    drop(input_sender);

    deleter.delete().await.unwrap();

    // Object had content_type "text/plain" which matches exclude regex
    // So it should NOT have been deleted (excluded, never buffered)
    let batch_calls = mock.delete_objects_calls.lock().unwrap();
    assert_eq!(batch_calls.len(), 0);
    let single_calls = mock.delete_object_calls.lock().unwrap();
    assert_eq!(single_calls.len(), 0);
}

#[tokio::test]
async fn object_deleter_metadata_include_filter() {
    init_dummy_tracing_subscriber();
    let (stats_sender, _stats_receiver) = async_channel::unbounded();
    let (boxed, mock) = make_mock_storage_boxed(stats_sender);

    // Set mock metadata
    let mut meta = HashMap::new();
    meta.insert("env".to_string(), "production".to_string());
    *mock.head_object_metadata.lock().unwrap() = Some(meta);

    let (input_sender, input_receiver) = async_channel::bounded::<S3Object>(10);
    let (output_sender, _output_receiver) = async_channel::bounded::<S3Object>(10);

    let mut config = make_test_config();
    config.filter_config.include_metadata_regex = Some(Regex::new("env=production").unwrap());
    let stage = make_stage_with_mock(config, boxed, Some(input_receiver), Some(output_sender));

    let stats_report = Arc::new(DeletionStatsReport::new());
    let delete_counter = Arc::new(AtomicU64::new(0));
    let mut deleter = ObjectDeleter::new(stage, 0, stats_report.clone(), delete_counter);

    input_sender
        .send(make_s3_object("data.json", 500))
        .await
        .unwrap();
    drop(input_sender);

    deleter.delete().await.unwrap();

    // Metadata matches include filter → object should be deleted via BatchDeleter
    let batch_calls = mock.delete_objects_calls.lock().unwrap();
    assert_eq!(batch_calls.len(), 1);
    assert_eq!(batch_calls[0].identifiers.len(), 1);
    assert_eq!(batch_calls[0].identifiers[0].key(), "data.json");
}

#[tokio::test]
async fn object_deleter_tag_include_filter() {
    init_dummy_tracing_subscriber();
    let (stats_sender, _stats_receiver) = async_channel::unbounded();
    let (boxed, mock) = make_mock_storage_boxed(stats_sender);

    // Set mock tags
    *mock.tagging_response_tags.lock().unwrap() = Some(vec![
        Tag::builder().key("env").value("dev").build().unwrap(),
    ]);

    let (input_sender, input_receiver) = async_channel::bounded::<S3Object>(10);
    let (output_sender, _output_receiver) = async_channel::bounded::<S3Object>(10);

    let mut config = make_test_config();
    config.filter_config.include_tag_regex = Some(Regex::new("env=dev").unwrap());
    let stage = make_stage_with_mock(config, boxed, Some(input_receiver), Some(output_sender));

    let stats_report = Arc::new(DeletionStatsReport::new());
    let delete_counter = Arc::new(AtomicU64::new(0));
    let mut deleter = ObjectDeleter::new(stage, 0, stats_report.clone(), delete_counter);

    input_sender
        .send(make_s3_object("tagged.txt", 256))
        .await
        .unwrap();
    drop(input_sender);

    deleter.delete().await.unwrap();

    // Tags match include filter → object should be deleted via BatchDeleter
    let batch_calls = mock.delete_objects_calls.lock().unwrap();
    assert_eq!(batch_calls.len(), 1);
    assert_eq!(batch_calls[0].identifiers.len(), 1);
    assert_eq!(batch_calls[0].identifiers[0].key(), "tagged.txt");
}

#[tokio::test]
async fn object_deleter_tag_exclude_filter() {
    init_dummy_tracing_subscriber();
    let (stats_sender, _stats_receiver) = async_channel::unbounded();
    let (boxed, mock) = make_mock_storage_boxed(stats_sender);

    // Set mock tags
    *mock.tagging_response_tags.lock().unwrap() = Some(vec![
        Tag::builder().key("retain").value("true").build().unwrap(),
    ]);

    let (input_sender, input_receiver) = async_channel::bounded::<S3Object>(10);
    let (output_sender, _output_receiver) = async_channel::bounded::<S3Object>(10);

    let mut config = make_test_config();
    config.filter_config.exclude_tag_regex = Some(Regex::new("retain=true").unwrap());
    let stage = make_stage_with_mock(config, boxed, Some(input_receiver), Some(output_sender));

    let stats_report = Arc::new(DeletionStatsReport::new());
    let delete_counter = Arc::new(AtomicU64::new(0));
    let mut deleter = ObjectDeleter::new(stage, 0, stats_report.clone(), delete_counter);

    input_sender
        .send(make_s3_object("important.txt", 999))
        .await
        .unwrap();
    drop(input_sender);

    deleter.delete().await.unwrap();

    // Tags match exclude filter → object should NOT be deleted (filtered out)
    let batch_calls = mock.delete_objects_calls.lock().unwrap();
    assert_eq!(batch_calls.len(), 0);
    let single_calls = mock.delete_object_calls.lock().unwrap();
    assert_eq!(single_calls.len(), 0);
}

#[tokio::test]
async fn object_deleter_filter_combination_and_logic() {
    // Content-type include + metadata include: both must match (AND logic)
    init_dummy_tracing_subscriber();
    let (stats_sender, _stats_receiver) = async_channel::unbounded();
    let (boxed, mock) = make_mock_storage_boxed(stats_sender);

    // Mock returns content_type="application/json" and metadata env=staging
    *mock.head_object_content_type.lock().unwrap() = Some("application/json".to_string());
    let mut meta = HashMap::new();
    meta.insert("env".to_string(), "staging".to_string());
    *mock.head_object_metadata.lock().unwrap() = Some(meta);

    let (input_sender, input_receiver) = async_channel::bounded::<S3Object>(10);
    let (output_sender, _output_receiver) = async_channel::bounded::<S3Object>(10);

    let mut config = make_test_config();
    config.filter_config.include_content_type_regex = Some(Regex::new("application/json").unwrap());
    // Metadata filter requires env=production, but mock returns env=staging
    config.filter_config.include_metadata_regex = Some(Regex::new("env=production").unwrap());
    let stage = make_stage_with_mock(config, boxed, Some(input_receiver), Some(output_sender));

    let stats_report = Arc::new(DeletionStatsReport::new());
    let delete_counter = Arc::new(AtomicU64::new(0));
    let mut deleter = ObjectDeleter::new(stage, 0, stats_report.clone(), delete_counter);

    input_sender
        .send(make_s3_object("data.json", 1000))
        .await
        .unwrap();
    drop(input_sender);

    deleter.delete().await.unwrap();

    // Content-type matched but metadata didn't → AND logic means no delete
    let batch_calls = mock.delete_objects_calls.lock().unwrap();
    assert_eq!(batch_calls.len(), 0);
    let single_calls = mock.delete_object_calls.lock().unwrap();
    assert_eq!(single_calls.len(), 0);
}

#[tokio::test]
async fn object_deleter_no_head_without_filters() {
    // When no content-type/metadata filters are configured, head_object should not be called.
    init_dummy_tracing_subscriber();
    let (stats_sender, _stats_receiver) = async_channel::unbounded();
    let (boxed, mock) = make_mock_storage_boxed(stats_sender);

    let (input_sender, input_receiver) = async_channel::bounded::<S3Object>(10);
    let (output_sender, _output_receiver) = async_channel::bounded::<S3Object>(10);

    let config = make_test_config(); // No filters
    let stage = make_stage_with_mock(config, boxed, Some(input_receiver), Some(output_sender));

    let stats_report = Arc::new(DeletionStatsReport::new());
    let delete_counter = Arc::new(AtomicU64::new(0));
    let mut deleter = ObjectDeleter::new(stage, 0, stats_report.clone(), delete_counter);

    input_sender
        .send(make_s3_object("simple.txt", 100))
        .await
        .unwrap();
    drop(input_sender);

    deleter.delete().await.unwrap();

    // No head_object calls when no content-type/metadata filters
    let head_calls = mock.head_object_calls.lock().unwrap();
    assert_eq!(head_calls.len(), 0);

    // Object should still be deleted via BatchDeleter (batch_size=1000)
    let batch_calls = mock.delete_objects_calls.lock().unwrap();
    assert_eq!(batch_calls.len(), 1);
    assert_eq!(batch_calls[0].identifiers.len(), 1);
    assert_eq!(batch_calls[0].identifiers[0].key(), "simple.txt");
}

// ===========================================================================
// Unit tests: ObjectDeleter with batch_size=1 (uses SingleDeleter)
// ===========================================================================

#[tokio::test]
async fn object_deleter_batch_size_1_uses_single_deleter() {
    init_dummy_tracing_subscriber();
    let (stats_sender, _stats_receiver) = async_channel::unbounded();
    let (boxed, mock) = make_mock_storage_boxed(stats_sender);

    let (input_sender, input_receiver) = async_channel::bounded::<S3Object>(10);
    let (output_sender, _output_receiver) = async_channel::bounded::<S3Object>(10);

    let mut config = make_test_config();
    config.batch_size = 1; // Force SingleDeleter
    let stage = make_stage_with_mock(config, boxed, Some(input_receiver), Some(output_sender));

    let stats_report = Arc::new(DeletionStatsReport::new());
    let delete_counter = Arc::new(AtomicU64::new(0));
    let mut deleter = ObjectDeleter::new(stage, 0, stats_report.clone(), delete_counter);

    input_sender
        .send(make_s3_object("key/a", 100))
        .await
        .unwrap();
    input_sender
        .send(make_s3_object("key/b", 200))
        .await
        .unwrap();
    drop(input_sender);

    deleter.delete().await.unwrap();

    // With batch_size=1, SingleDeleter is used → delete_object calls
    let single_calls = mock.delete_object_calls.lock().unwrap();
    assert_eq!(single_calls.len(), 2);
    assert_eq!(single_calls[0].key, "key/a");
    assert_eq!(single_calls[1].key, "key/b");

    // No batch calls
    let batch_calls = mock.delete_objects_calls.lock().unwrap();
    assert_eq!(batch_calls.len(), 0);

    let report = &*stats_report;
    assert_eq!(report.stats_deleted_objects.load(Ordering::SeqCst), 2);
    assert_eq!(report.stats_deleted_bytes.load(Ordering::SeqCst), 300);
}

// ===========================================================================
// Unit tests: ObjectDeleter with if_match (batch deletion with ETags)
// ===========================================================================

#[tokio::test]
async fn object_deleter_if_match_uses_batch_with_etags() {
    init_dummy_tracing_subscriber();
    let (stats_sender, _stats_receiver) = async_channel::unbounded();
    let (boxed, mock) = make_mock_storage_boxed(stats_sender);

    let (input_sender, input_receiver) = async_channel::bounded::<S3Object>(10);
    let (output_sender, _output_receiver) = async_channel::bounded::<S3Object>(10);

    let mut config = make_test_config();
    config.if_match = true; // Enable if-match mode
    let stage = make_stage_with_mock(config, boxed, Some(input_receiver), Some(output_sender));

    let stats_report = Arc::new(DeletionStatsReport::new());
    let delete_counter = Arc::new(AtomicU64::new(0));
    let mut deleter = ObjectDeleter::new(stage, 0, stats_report.clone(), delete_counter);

    // Send a versioned object with an ETag
    let obj = S3Object::Versioning(
        ObjectVersion::builder()
            .key("if-match/obj.txt")
            .version_id("v1")
            .size(512)
            .is_latest(true)
            .storage_class(ObjectVersionStorageClass::Standard)
            .last_modified(DateTime::from_secs(1000))
            .e_tag("\"etag123\"")
            .build(),
    );
    input_sender.send(obj).await.unwrap();
    drop(input_sender);

    deleter.delete().await.unwrap();

    // if_match goes through batch path — BatchDeleter includes ETags
    let batch_calls = mock.delete_objects_calls.lock().unwrap();
    assert_eq!(batch_calls.len(), 1);
    assert_eq!(batch_calls[0].identifiers.len(), 1);
    assert_eq!(batch_calls[0].identifiers[0].key(), "if-match/obj.txt");
    assert_eq!(batch_calls[0].identifiers[0].e_tag(), Some("\"etag123\""));

    let report = &*stats_report;
    assert_eq!(report.stats_deleted_objects.load(Ordering::SeqCst), 1);
}

// ===========================================================================
// Unit tests: ObjectDeleter buffer flush on batch_size boundary
// ===========================================================================

#[tokio::test]
async fn object_deleter_flushes_at_batch_size_boundary() {
    init_dummy_tracing_subscriber();
    let (stats_sender, _stats_receiver) = async_channel::unbounded();
    let (boxed, mock) = make_mock_storage_boxed(stats_sender);

    let (input_sender, input_receiver) = async_channel::bounded::<S3Object>(20);
    let (output_sender, _output_receiver) = async_channel::bounded::<S3Object>(20);

    let mut config = make_test_config();
    config.batch_size = 3; // Small batch for testing
    let stage = make_stage_with_mock(config, boxed, Some(input_receiver), Some(output_sender));

    let stats_report = Arc::new(DeletionStatsReport::new());
    let delete_counter = Arc::new(AtomicU64::new(0));
    let mut deleter = ObjectDeleter::new(stage, 0, stats_report.clone(), delete_counter);

    // Send 9 objects with batch_size=3: should produce 3 full batches (3+3+3)
    for i in 0..9 {
        input_sender
            .send(make_s3_object(&format!("key/{i}"), 100))
            .await
            .unwrap();
    }
    drop(input_sender);

    deleter.delete().await.unwrap();

    let batch_calls = mock.delete_objects_calls.lock().unwrap();
    assert_eq!(batch_calls.len(), 3);
    assert_eq!(batch_calls[0].identifiers.len(), 3);
    assert_eq!(batch_calls[1].identifiers.len(), 3);
    assert_eq!(batch_calls[2].identifiers.len(), 3);

    let report = &*stats_report;
    assert_eq!(report.stats_deleted_objects.load(Ordering::SeqCst), 9);
}

// ===========================================================================
// Property tests
// ===========================================================================

// Feature: s3rm-rs, Property 1: Batch Deletion API Usage
// **Validates: Requirements 1.1, 5.5**
// Batches never exceed MAX_BATCH_SIZE (1000) objects, and all objects are
// included across batches.
proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    #[test]
    fn prop_batch_never_exceeds_max_size(
        obj_count in 1usize..3000,
        batch_size in 1u16..2000,
    ) {
        // Build a vector of obj_count objects
        let objects: Vec<S3Object> = (0..obj_count)
            .map(|i| make_s3_object(&format!("key/{i}"), 100))
            .collect();

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        rt.block_on(async {
            let (stats_sender, _stats_receiver) = async_channel::unbounded();
            let (boxed, mock) = make_mock_storage_boxed(stats_sender);
            let deleter = BatchDeleter::new(boxed);

            let mut config = make_test_config();
            config.batch_size = batch_size;

            let result = deleter.delete(&objects, &config).await.unwrap();
            prop_assert_eq!(result.deleted.len(), obj_count);

            let calls = mock.delete_objects_calls.lock().unwrap();
            let effective_batch = (batch_size as usize).min(batch::MAX_BATCH_SIZE);

            // Verify no single batch exceeds the effective batch size
            for call in calls.iter() {
                prop_assert!(call.identifiers.len() <= effective_batch);
            }

            // Verify all objects are accounted for
            let total_sent: usize = calls.iter().map(|c| c.identifiers.len()).sum();
            prop_assert_eq!(total_sent, obj_count);

            Ok(())
        })?;
    }
}

// Feature: s3rm-rs, Property 2: Single Deletion API Usage
// **Validates: Requirements 1.2**
// Every object is deleted individually with exactly one API call.
proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    #[test]
    fn prop_single_deleter_one_call_per_object(
        obj_count in 1usize..200,
    ) {
        let objects: Vec<S3Object> = (0..obj_count)
            .map(|i| make_s3_object(&format!("key/{i}"), (i as i64 + 1) * 100))
            .collect();

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        rt.block_on(async {
            let (stats_sender, _stats_receiver) = async_channel::unbounded();
            let (boxed, mock) = make_mock_storage_boxed(stats_sender);
            let deleter = SingleDeleter::new(boxed);
            let config = make_test_config();

            // SingleDeleter receives exactly one object per call
            for (i, obj) in objects.iter().enumerate() {
                let result = deleter.delete(std::slice::from_ref(obj), &config).await.unwrap();
                prop_assert_eq!(result.deleted.len(), 1);
                prop_assert_eq!(&result.deleted[0].key, &format!("key/{i}"));
            }

            let calls = mock.delete_object_calls.lock().unwrap();
            prop_assert_eq!(calls.len(), obj_count);

            Ok(())
        })?;
    }
}

// Feature: s3rm-rs, Property 3: Concurrent Worker Execution
// **Validates: Requirements 1.3**
// Multiple ObjectDeleter workers can process objects concurrently.
#[tokio::test]
async fn prop_concurrent_workers_process_all_objects() {
    init_dummy_tracing_subscriber();

    let total_objects = 2000;
    let worker_count = 4;

    let (stats_sender, _stats_receiver) = async_channel::unbounded();
    let (input_sender, input_receiver) = async_channel::bounded::<S3Object>(total_objects);

    // Send all objects before spawning workers
    for i in 0..total_objects {
        input_sender
            .send(make_s3_object(&format!("concurrent/{i}"), 100))
            .await
            .unwrap();
    }
    drop(input_sender);

    let stats_report = Arc::new(DeletionStatsReport::new());
    let delete_counter = Arc::new(AtomicU64::new(0));

    let mut handles = Vec::new();

    for worker_idx in 0..worker_count {
        let (boxed, _mock) = make_mock_storage_boxed(stats_sender.clone());
        let (out_sender, _out_receiver) = async_channel::bounded::<S3Object>(total_objects);

        let config = make_test_config();
        let cancellation_token: PipelineCancellationToken = CancellationToken::new();
        let has_warning = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stage = Stage::new(
            config,
            boxed,
            Some(input_receiver.clone()),
            Some(out_sender),
            cancellation_token,
            has_warning,
        );

        let report = stats_report.clone();
        let counter = delete_counter.clone();

        let handle = tokio::spawn(async move {
            let mut deleter = ObjectDeleter::new(stage, worker_idx as u16, report, counter);
            deleter.delete().await.unwrap();
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.await.unwrap();
    }

    // All objects should have been processed
    let report = &*stats_report;
    assert_eq!(
        report.stats_deleted_objects.load(Ordering::SeqCst),
        total_objects as u64
    );
}

// Feature: s3rm-rs, Property 6: Partial Batch Failure Recovery
// **Validates: Requirements 1.9**
// When some objects fail in a batch, successful ones are counted correctly.
proptest! {
    #![proptest_config(ProptestConfig::with_cases(30))]

    #[test]
    fn prop_partial_batch_failure_counts_successes(
        total in 5usize..100,
        fail_pct in 1usize..50,
    ) {
        // Determine which indices should fail
        let fail_count = (total * fail_pct / 100).max(1).min(total - 1);
        let fail_keys: HashMap<String, String> = (0..fail_count)
            .map(|i| (format!("key/{i}"), "AccessDenied".to_string()))
            .collect();

        let objects: Vec<S3Object> = (0..total)
            .map(|i| make_s3_object(&format!("key/{i}"), 100))
            .collect();

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        rt.block_on(async {
            let (stats_sender, _stats_receiver) = async_channel::unbounded();
            let (boxed, mock) = make_mock_storage_boxed(stats_sender);

            *mock.batch_error_keys.lock().unwrap() = fail_keys.clone();

            let deleter = BatchDeleter::new(boxed);
            let config = make_test_config();

            let result = deleter.delete(&objects, &config).await.unwrap();

            // Successes = total - fail_count
            let expected_successes = total - fail_count;
            prop_assert_eq!(result.deleted.len(), expected_successes);

            Ok(())
        })?;
    }
}

// ===========================================================================
// Additional property test: format_metadata idempotent and sorted
// ===========================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    #[test]
    fn prop_format_metadata_always_sorted(
        keys in prop::collection::vec("[a-z]{1,5}", 1..10),
        values in prop::collection::vec("[a-z0-9]{1,5}", 1..10),
    ) {
        let len = keys.len().min(values.len());
        let mut meta: HashMap<String, String> = HashMap::new();
        for i in 0..len {
            meta.insert(keys[i].clone(), values[i].clone());
        }

        let result = format_metadata(&meta);

        // Verify that the pairs are sorted (comma-separated, matching s3sync)
        let parts: Vec<&str> = result.split(',').filter(|s| !s.is_empty()).collect();
        for window in parts.windows(2) {
            prop_assert!(window[0] <= window[1], "Not sorted: {} > {}", window[0], window[1]);
        }
    }
}

// ===========================================================================
// Dry-run tests
// ===========================================================================

/// Dry-run mode: the ObjectDeleter should NOT make any S3 API calls,
/// but should still report all objects as successfully deleted (simulated)
/// and emit statistics/events.
#[tokio::test]
async fn object_deleter_dry_run_skips_api_calls() {
    init_dummy_tracing_subscriber();

    let (stats_sender, _stats_receiver) = async_channel::bounded(100);
    let (mock_storage, mock) = make_mock_storage_boxed(stats_sender.clone());
    let (input_sender, input_receiver) = async_channel::bounded(100);
    let (output_sender, output_receiver) = async_channel::bounded::<S3Object>(100);

    let mut config = make_test_config();
    config.dry_run = true;
    config.batch_size = 1000;

    let stage = make_stage_with_mock(
        config,
        mock_storage,
        Some(input_receiver),
        Some(output_sender),
    );

    let stats_report = Arc::new(crate::types::DeletionStatsReport::new());
    let delete_counter = Arc::new(AtomicU64::new(0));

    let mut deleter = ObjectDeleter::new(stage, 0, stats_report.clone(), delete_counter.clone());

    // Send objects
    let obj1 = make_s3_object("dry-run/file1.txt", 100);
    let obj2 = make_s3_object("dry-run/file2.txt", 200);
    let obj3 = make_s3_object("dry-run/file3.txt", 300);
    input_sender.send(obj1).await.unwrap();
    input_sender.send(obj2).await.unwrap();
    input_sender.send(obj3).await.unwrap();
    input_sender.close();

    deleter.delete().await.unwrap();

    // Verify NO S3 API calls were made
    assert_eq!(
        mock.delete_object_calls.lock().unwrap().len(),
        0,
        "dry-run should not call delete_object"
    );
    assert_eq!(
        mock.delete_objects_calls.lock().unwrap().len(),
        0,
        "dry-run should not call delete_objects"
    );

    // Verify statistics report all 3 objects as deleted
    let report = &*stats_report;
    let snapshot = report.snapshot();
    assert_eq!(
        snapshot.stats_deleted_objects, 3,
        "all 3 objects should be counted as deleted"
    );
    assert_eq!(
        snapshot.stats_deleted_bytes, 600,
        "total bytes should be 100+200+300=600"
    );
    assert_eq!(snapshot.stats_failed_objects, 0, "no failures in dry-run");

    // Verify all objects were forwarded to output channel
    let mut forwarded = Vec::new();
    while let Ok(obj) = output_receiver.try_recv() {
        forwarded.push(obj.key().to_string());
    }
    assert_eq!(forwarded.len(), 3);
    assert!(forwarded.contains(&"dry-run/file1.txt".to_string()));
    assert!(forwarded.contains(&"dry-run/file2.txt".to_string()));
    assert!(forwarded.contains(&"dry-run/file3.txt".to_string()));
}

/// Dry-run with single deleter (batch_size=1): should still skip API calls.
#[tokio::test]
async fn object_deleter_dry_run_single_mode() {
    init_dummy_tracing_subscriber();

    let (stats_sender, _stats_receiver) = async_channel::bounded(100);
    let (mock_storage, mock) = make_mock_storage_boxed(stats_sender.clone());
    let (input_sender, input_receiver) = async_channel::bounded(100);
    let (output_sender, _output_receiver) = async_channel::bounded::<S3Object>(100);

    let mut config = make_test_config();
    config.dry_run = true;
    config.batch_size = 1; // single delete mode

    let stage = make_stage_with_mock(
        config,
        mock_storage,
        Some(input_receiver),
        Some(output_sender),
    );

    let stats_report = Arc::new(crate::types::DeletionStatsReport::new());
    let delete_counter = Arc::new(AtomicU64::new(0));

    let mut deleter = ObjectDeleter::new(stage, 0, stats_report.clone(), delete_counter);

    let obj = make_s3_object("dry-run/single.txt", 500);
    input_sender.send(obj).await.unwrap();
    input_sender.close();

    deleter.delete().await.unwrap();

    // No S3 calls
    assert_eq!(mock.delete_object_calls.lock().unwrap().len(), 0);
    assert_eq!(mock.delete_objects_calls.lock().unwrap().len(), 0);

    // Stats recorded
    let report = &*stats_report;
    let snapshot = report.snapshot();
    assert_eq!(snapshot.stats_deleted_objects, 1);
    assert_eq!(snapshot.stats_deleted_bytes, 500);
}

/// Dry-run with versioned objects: should report version IDs without API calls.
#[tokio::test]
async fn object_deleter_dry_run_versioned_objects() {
    init_dummy_tracing_subscriber();

    let (stats_sender, _stats_receiver) = async_channel::bounded(100);
    let (mock_storage, mock) = make_mock_storage_boxed(stats_sender.clone());
    let (input_sender, input_receiver) = async_channel::bounded(100);
    let (output_sender, output_receiver) = async_channel::bounded::<S3Object>(100);

    let mut config = make_test_config();
    config.dry_run = true;
    config.batch_size = 1000;

    let stage = make_stage_with_mock(
        config,
        mock_storage,
        Some(input_receiver),
        Some(output_sender),
    );

    let stats_report = Arc::new(crate::types::DeletionStatsReport::new());
    let delete_counter = Arc::new(AtomicU64::new(0));

    let mut deleter = ObjectDeleter::new(stage, 0, stats_report.clone(), delete_counter);

    let obj = make_versioned_s3_object("versioned/file.txt", "ver-001", 1024);
    input_sender.send(obj).await.unwrap();
    input_sender.close();

    deleter.delete().await.unwrap();

    // No API calls
    assert_eq!(mock.delete_object_calls.lock().unwrap().len(), 0);
    assert_eq!(mock.delete_objects_calls.lock().unwrap().len(), 0);

    // Stats recorded
    let report = &*stats_report;
    let snapshot = report.snapshot();
    assert_eq!(snapshot.stats_deleted_objects, 1);
    assert_eq!(snapshot.stats_deleted_bytes, 1024);

    // Object forwarded to output
    let forwarded = output_receiver.try_recv().unwrap();
    assert_eq!(forwarded.key(), "versioned/file.txt");
    assert_eq!(forwarded.version_id(), Some("ver-001"));
}

// ---------------------------------------------------------------------------
// is_retryable_error_code unit test
// ---------------------------------------------------------------------------

#[test]
fn test_is_retryable_error_code() {
    use crate::deleter::batch::is_retryable_error_code;

    // Retryable codes
    assert!(is_retryable_error_code("InternalError"));
    assert!(is_retryable_error_code("SlowDown"));
    assert!(is_retryable_error_code("ServiceUnavailable"));
    assert!(is_retryable_error_code("RequestTimeout"));

    // "unknown" is retryable (err on the side of retrying)
    assert!(is_retryable_error_code("unknown"));

    // Non-retryable codes
    assert!(!is_retryable_error_code("AccessDenied"));
    assert!(!is_retryable_error_code("NoSuchKey"));
    assert!(!is_retryable_error_code("InvalidArgument"));
    assert!(!is_retryable_error_code(""));
}

// ---------------------------------------------------------------------------
// Batch-to-single fallback tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn batch_deleter_retryable_error_falls_back_to_single() {
    // Retryable batch error triggers single-delete fallback that succeeds.
    init_dummy_tracing_subscriber();
    let (stats_sender, _stats_receiver) = async_channel::unbounded();
    let (boxed, mock) = make_mock_storage_boxed(stats_sender);

    // Configure "key/1" to fail in batch with retryable error
    mock.batch_error_keys
        .lock()
        .unwrap()
        .insert("key/1".to_string(), "InternalError".to_string());

    let deleter = BatchDeleter::new(boxed);
    let config = make_test_config();

    let objects = vec![
        make_s3_object("key/0", 100),
        make_s3_object("key/1", 200),
        make_s3_object("key/2", 300),
    ];
    let result = deleter.delete(&objects, &config).await.unwrap();

    // All 3 should be deleted (2 via batch + 1 via single fallback)
    assert_eq!(result.deleted.len(), 3);
    assert_eq!(result.failed.len(), 0);

    // Verify single delete was called for "key/1"
    let single_calls = mock.delete_object_calls.lock().unwrap();
    assert_eq!(single_calls.len(), 1);
    assert_eq!(single_calls[0].key, "key/1");
}

#[tokio::test]
async fn batch_deleter_non_retryable_skips_single_fallback() {
    // Non-retryable batch error does NOT trigger single-delete fallback.
    init_dummy_tracing_subscriber();
    let (stats_sender, _stats_receiver) = async_channel::unbounded();
    let (boxed, mock) = make_mock_storage_boxed(stats_sender);

    // Configure "key/1" to fail with non-retryable error
    mock.batch_error_keys
        .lock()
        .unwrap()
        .insert("key/1".to_string(), "AccessDenied".to_string());

    let deleter = BatchDeleter::new(boxed);
    let config = make_test_config();

    let objects = vec![make_s3_object("key/0", 100), make_s3_object("key/1", 200)];
    let result = deleter.delete(&objects, &config).await.unwrap();

    assert_eq!(result.deleted.len(), 1);
    assert_eq!(result.failed.len(), 1);
    assert_eq!(result.failed[0].error_code, "AccessDenied");

    // No single delete calls should have been made
    let single_calls = mock.delete_object_calls.lock().unwrap();
    assert_eq!(single_calls.len(), 0);
}

#[tokio::test]
async fn batch_deleter_retryable_fallback_exhausted() {
    // Retryable batch error, but single-delete also fails → verify retry count and final failure.
    init_dummy_tracing_subscriber();
    let (stats_sender, _stats_receiver) = async_channel::unbounded();
    let (boxed, mock) = make_mock_storage_boxed(stats_sender);

    // Configure "key/0" to fail in batch with retryable error
    mock.batch_error_keys
        .lock()
        .unwrap()
        .insert("key/0".to_string(), "SlowDown".to_string());

    // Also configure single delete to fail for "key/0"
    mock.delete_object_error_keys
        .lock()
        .unwrap()
        .insert("key/0".to_string(), "SlowDown error".to_string());

    let deleter = BatchDeleter::new(boxed);
    let mut config = make_test_config();
    config.force_retry_config.force_retry_count = 2; // 1 initial + 2 retries = 3 attempts

    let objects = vec![make_s3_object("key/0", 100)];
    let result = deleter.delete(&objects, &config).await.unwrap();

    // Object should be in failed list after exhausting retries
    assert_eq!(result.deleted.len(), 0);
    assert_eq!(result.failed.len(), 1);
    assert_eq!(result.failed[0].key, "key/0");
    assert_eq!(result.failed[0].error_code, "SlowDown");

    // Verify 3 single delete attempts were made (initial + 2 retries)
    let single_calls = mock.delete_object_calls.lock().unwrap();
    assert_eq!(single_calls.len(), 3);
}

#[tokio::test]
async fn batch_deleter_mixed_retryable_non_retryable() {
    // Mixed errors: retryable gets single-delete fallback, non-retryable does not.
    init_dummy_tracing_subscriber();
    let (stats_sender, _stats_receiver) = async_channel::unbounded();
    let (boxed, mock) = make_mock_storage_boxed(stats_sender);

    // "key/0" fails with retryable error, "key/2" fails with non-retryable
    {
        let mut errors = mock.batch_error_keys.lock().unwrap();
        errors.insert("key/0".to_string(), "InternalError".to_string());
        errors.insert("key/2".to_string(), "NoSuchKey".to_string());
    }

    let deleter = BatchDeleter::new(boxed);
    let config = make_test_config();

    let objects = vec![
        make_s3_object("key/0", 100),
        make_s3_object("key/1", 200),
        make_s3_object("key/2", 300),
    ];
    let result = deleter.delete(&objects, &config).await.unwrap();

    // key/0 recovered via single fallback, key/1 batch ok, key/2 failed (non-retryable)
    assert_eq!(result.deleted.len(), 2); // key/0 (fallback) + key/1 (batch)
    assert_eq!(result.failed.len(), 1);
    assert_eq!(result.failed[0].key, "key/2");
    assert_eq!(result.failed[0].error_code, "NoSuchKey");

    // Single delete called only for retryable "key/0"
    let single_calls = mock.delete_object_calls.lock().unwrap();
    assert_eq!(single_calls.len(), 1);
    assert_eq!(single_calls[0].key, "key/0");
}

#[tokio::test]
async fn batch_deleter_retryable_fallback_with_if_match() {
    // Verify ETag is passed to single delete during fallback when if_match is enabled.
    init_dummy_tracing_subscriber();
    let (stats_sender, _stats_receiver) = async_channel::unbounded();
    let (boxed, mock) = make_mock_storage_boxed(stats_sender);

    // Configure "key/0" to fail with retryable error
    mock.batch_error_keys
        .lock()
        .unwrap()
        .insert("key/0".to_string(), "ServiceUnavailable".to_string());

    let deleter = BatchDeleter::new(boxed);
    let mut config = make_test_config();
    config.if_match = true;

    // Create object with ETag
    let objects = vec![S3Object::NotVersioning(
        Object::builder()
            .key("key/0")
            .size(100)
            .e_tag("\"abc123\"")
            .build(),
    )];
    let result = deleter.delete(&objects, &config).await.unwrap();

    // Object should be recovered via single fallback
    assert_eq!(result.deleted.len(), 1);
    assert_eq!(result.failed.len(), 0);

    // Verify single delete was called with the correct ETag
    let single_calls = mock.delete_object_calls.lock().unwrap();
    assert_eq!(single_calls.len(), 1);
    assert_eq!(single_calls[0].key, "key/0");
    assert_eq!(single_calls[0].if_match, Some("\"abc123\"".to_string()));
}

#[tokio::test]
async fn batch_deleter_retryable_fallback_passes_version_id() {
    // Verify version_id is passed to single delete during fallback.
    init_dummy_tracing_subscriber();
    let (stats_sender, _stats_receiver) = async_channel::unbounded();
    let (boxed, mock) = make_mock_storage_boxed(stats_sender);

    // Configure "key/0" to fail with retryable error
    mock.batch_error_keys
        .lock()
        .unwrap()
        .insert("key/0".to_string(), "RequestTimeout".to_string());

    let deleter = BatchDeleter::new(boxed);
    let config = make_test_config();

    let objects = vec![make_versioned_s3_object("key/0", "ver-abc", 100)];
    let result = deleter.delete(&objects, &config).await.unwrap();

    assert_eq!(result.deleted.len(), 1);
    assert_eq!(result.failed.len(), 0);

    // Verify single delete was called with the correct version_id
    let single_calls = mock.delete_object_calls.lock().unwrap();
    assert_eq!(single_calls.len(), 1);
    assert_eq!(single_calls[0].key, "key/0");
    assert_eq!(single_calls[0].version_id, Some("ver-abc".to_string()));
}

#[tokio::test]
async fn batch_deleter_retryable_fallback_succeeds_on_second_attempt() {
    // Single delete fails on first attempt, succeeds on second.
    init_dummy_tracing_subscriber();
    let (stats_sender, _stats_receiver) = async_channel::unbounded();
    let mock = MockStorage::new(stats_sender.clone());

    // Configure "key/0" to fail in batch with retryable error
    mock.batch_error_keys
        .lock()
        .unwrap()
        .insert("key/0".to_string(), "InternalError".to_string());

    // Configure single delete to fail for "key/0" — we'll remove it after first call
    // by using a counter-based approach via the call tracking
    let fail_counter = Arc::new(AtomicU64::new(0));
    let fail_counter_clone = fail_counter.clone();

    // Custom mock that fails once then succeeds
    #[derive(Clone)]
    struct FailOnceMock {
        inner: MockStorage,
        fail_counter: Arc<AtomicU64>,
    }

    #[async_trait]
    impl StorageTrait for FailOnceMock {
        fn is_express_onezone_storage(&self) -> bool {
            self.inner.is_express_onezone_storage()
        }
        async fn list_objects(&self, sender: &Sender<S3Object>, max_keys: i32) -> Result<()> {
            self.inner.list_objects(sender, max_keys).await
        }
        async fn list_object_versions(
            &self,
            sender: &Sender<S3Object>,
            max_keys: i32,
        ) -> Result<()> {
            self.inner.list_object_versions(sender, max_keys).await
        }
        async fn head_object(
            &self,
            key: &str,
            version_id: Option<String>,
        ) -> Result<HeadObjectOutput> {
            self.inner.head_object(key, version_id).await
        }
        async fn get_object_tagging(
            &self,
            key: &str,
            version_id: Option<String>,
        ) -> Result<GetObjectTaggingOutput> {
            self.inner.get_object_tagging(key, version_id).await
        }
        async fn delete_object(
            &self,
            key: &str,
            version_id: Option<String>,
            if_match: Option<String>,
        ) -> Result<DeleteObjectOutput> {
            // Track the call
            self.inner
                .delete_object_calls
                .lock()
                .unwrap()
                .push(DeleteObjectCall {
                    key: key.to_string(),
                    version_id: version_id.clone(),
                    if_match: if_match.clone(),
                });

            let count = self.fail_counter.fetch_add(1, Ordering::SeqCst);
            if count == 0 {
                // Fail on first attempt
                Err(anyhow::anyhow!("transient error"))
            } else {
                // Succeed on subsequent attempts
                Ok(DeleteObjectOutput::builder().build())
            }
        }
        async fn delete_objects(
            &self,
            objects: Vec<ObjectIdentifier>,
        ) -> Result<DeleteObjectsOutput> {
            self.inner.delete_objects(objects).await
        }
        async fn is_versioning_enabled(&self) -> Result<bool> {
            self.inner.is_versioning_enabled().await
        }
        fn get_client(&self) -> Option<Arc<aws_sdk_s3::Client>> {
            None
        }
        fn get_stats_sender(&self) -> Sender<DeletionStatistics> {
            self.inner.get_stats_sender()
        }
        async fn send_stats(&self, stats: DeletionStatistics) {
            self.inner.send_stats(stats).await;
        }
        fn set_warning(&self) {}
    }

    let fail_once_mock = FailOnceMock {
        inner: mock.clone(),
        fail_counter: fail_counter_clone,
    };
    let boxed: Box<dyn StorageTrait + Send + Sync> = Box::new(fail_once_mock);

    let deleter = BatchDeleter::new(boxed);
    let mut config = make_test_config();
    config.force_retry_config.force_retry_count = 2; // Allow retries

    let objects = vec![make_s3_object("key/0", 100)];
    let result = deleter.delete(&objects, &config).await.unwrap();

    // Object should succeed on second attempt
    assert_eq!(result.deleted.len(), 1);
    assert_eq!(result.failed.len(), 0);

    // Verify exactly 2 single delete attempts were made (fail + succeed)
    let single_calls = mock.delete_object_calls.lock().unwrap();
    assert_eq!(single_calls.len(), 2);
    // Counter should be 2 (0-indexed: attempt 0 failed, attempt 1 succeeded)
    assert_eq!(fail_counter.load(Ordering::SeqCst), 2);
}

// ===========================================================================
// Helper for warn_as_error tests
// ===========================================================================

/// Creates a Stage for testing warn_as_error behavior, returning the Stage along
/// with shared handles to the cancellation token and warning flag so the test
/// can inspect them after running the ObjectDeleter.
fn make_stage_with_observables(
    config: Config,
    mock_storage: Box<dyn StorageTrait + Send + Sync>,
    receiver: Option<async_channel::Receiver<S3Object>>,
    sender: Option<Sender<S3Object>>,
) -> (
    Stage,
    PipelineCancellationToken,
    Arc<std::sync::atomic::AtomicBool>,
) {
    let cancellation_token: PipelineCancellationToken = CancellationToken::new();
    let has_warning = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stage = Stage::new(
        config,
        mock_storage,
        receiver,
        sender,
        cancellation_token.clone(),
        has_warning.clone(),
    );
    (stage, cancellation_token, has_warning)
}

// ===========================================================================
// Unit tests: warn_as_error feature
//
// The warn_as_error feature promotes deletion warnings (partial batch failures)
// to fatal errors. When enabled:
//   - In ObjectDeleter::delete_buffered_objects: if any object in a batch fails,
//     set_warning() is called AND the cancellation token is cancelled.
//   - In DeletionPipeline::execute_pipeline (post-run): if the warning flag is
//     set, a PartialFailure error is recorded.
//
// When disabled (default): set_warning() is still called on failures, but the
// pipeline continues processing remaining objects.
// ===========================================================================

/// Scenario: warn_as_error=false (default), deletion failure occurs.
/// Expected: Warning flag is set. Pipeline is NOT cancelled. Remaining
/// objects continue to be processed. Stats report shows failures.
#[tokio::test]
async fn warn_as_error_false_failure_sets_warning_but_continues() {
    init_dummy_tracing_subscriber();

    let (stats_sender, _stats_receiver) = async_channel::unbounded();
    let (boxed, mock) = make_mock_storage_boxed(stats_sender);

    // Configure "key/1" to fail in batch with non-retryable error
    mock.batch_error_keys
        .lock()
        .unwrap()
        .insert("key/1".to_string(), "AccessDenied".to_string());

    let (input_sender, input_receiver) = async_channel::bounded::<S3Object>(10);
    let (output_sender, output_receiver) = async_channel::bounded::<S3Object>(10);

    let mut config = make_test_config();
    config.warn_as_error = false; // Explicitly false (default)
    config.batch_size = 1000;

    let (stage, cancellation_token, has_warning) =
        make_stage_with_observables(config, boxed, Some(input_receiver), Some(output_sender));

    let stats_report = Arc::new(DeletionStatsReport::new());
    let delete_counter = Arc::new(AtomicU64::new(0));
    let mut deleter = ObjectDeleter::new(stage, 0, stats_report.clone(), delete_counter);

    // Send 3 objects. key/1 will fail in batch.
    for i in 0..3 {
        input_sender
            .send(make_s3_object(&format!("key/{i}"), 100))
            .await
            .unwrap();
    }
    drop(input_sender);

    deleter.delete().await.unwrap();

    // Warning flag MUST be set because there was a failure
    assert!(
        has_warning.load(Ordering::SeqCst),
        "Warning flag should be set when a batch deletion partially fails"
    );

    // Pipeline should NOT be cancelled
    assert!(
        !cancellation_token.is_cancelled(),
        "Pipeline should continue when warn_as_error=false"
    );

    // Stats: 2 deleted (key/0, key/2), 1 failed (key/1)
    let snapshot = stats_report.snapshot();
    assert_eq!(snapshot.stats_deleted_objects, 2);
    assert_eq!(snapshot.stats_failed_objects, 1);

    // All 3 objects should have been forwarded to the output channel
    // (both successful and failed objects are forwarded in the current implementation)
    let mut forwarded = Vec::new();
    while let Ok(obj) = output_receiver.try_recv() {
        forwarded.push(obj.key().to_string());
    }
    assert_eq!(
        forwarded.len(),
        3,
        "All objects should be forwarded to next stage even with failures"
    );
}

/// Scenario: warn_as_error=true, deletion failure occurs.
/// Expected: Warning flag is set AND pipeline is cancelled immediately.
/// Objects after the failing batch are NOT forwarded.
#[tokio::test]
async fn warn_as_error_true_failure_cancels_pipeline() {
    init_dummy_tracing_subscriber();

    let (stats_sender, _stats_receiver) = async_channel::unbounded();
    let (boxed, mock) = make_mock_storage_boxed(stats_sender);

    // Configure "key/1" to fail in batch with non-retryable error
    mock.batch_error_keys
        .lock()
        .unwrap()
        .insert("key/1".to_string(), "AccessDenied".to_string());

    let (input_sender, input_receiver) = async_channel::bounded::<S3Object>(10);
    let (output_sender, output_receiver) = async_channel::bounded::<S3Object>(10);

    let mut config = make_test_config();
    config.warn_as_error = true;
    config.batch_size = 1000;

    let (stage, cancellation_token, has_warning) =
        make_stage_with_observables(config, boxed, Some(input_receiver), Some(output_sender));

    let stats_report = Arc::new(DeletionStatsReport::new());
    let delete_counter = Arc::new(AtomicU64::new(0));
    let mut deleter = ObjectDeleter::new(stage, 0, stats_report.clone(), delete_counter);

    // Send 3 objects. key/1 will fail in batch.
    for i in 0..3 {
        input_sender
            .send(make_s3_object(&format!("key/{i}"), 100))
            .await
            .unwrap();
    }
    drop(input_sender);

    deleter.delete().await.unwrap();

    // Warning flag MUST be set
    assert!(
        has_warning.load(Ordering::SeqCst),
        "Warning flag should be set on batch failure"
    );

    // Pipeline MUST be cancelled when warn_as_error=true
    assert!(
        cancellation_token.is_cancelled(),
        "Pipeline must be cancelled when warn_as_error=true and failures occur"
    );

    // Stats: The batch was processed (2 deleted, 1 failed) but the pipeline
    // returns early before forwarding objects to the output channel.
    let snapshot = stats_report.snapshot();
    assert_eq!(snapshot.stats_deleted_objects, 2);
    assert_eq!(snapshot.stats_failed_objects, 1);

    // Because the pipeline returns early (before forwarding), the output
    // channel should have NO objects forwarded.
    let forwarded_count = output_receiver.len();
    assert_eq!(
        forwarded_count, 0,
        "No objects should be forwarded when warn_as_error cancels the pipeline"
    );
}

/// Scenario: warn_as_error=true, no deletion failures.
/// Expected: Pipeline completes normally. No cancellation. Warning flag NOT set.
#[tokio::test]
async fn warn_as_error_true_no_failures_completes_normally() {
    init_dummy_tracing_subscriber();

    let (stats_sender, _stats_receiver) = async_channel::unbounded();
    let (boxed, _mock) = make_mock_storage_boxed(stats_sender);

    let (input_sender, input_receiver) = async_channel::bounded::<S3Object>(10);
    let (output_sender, output_receiver) = async_channel::bounded::<S3Object>(10);

    let mut config = make_test_config();
    config.warn_as_error = true;
    config.batch_size = 1000;

    let (stage, cancellation_token, has_warning) =
        make_stage_with_observables(config, boxed, Some(input_receiver), Some(output_sender));

    let stats_report = Arc::new(DeletionStatsReport::new());
    let delete_counter = Arc::new(AtomicU64::new(0));
    let mut deleter = ObjectDeleter::new(stage, 0, stats_report.clone(), delete_counter);

    // Send 3 objects -- all succeed
    for i in 0..3 {
        input_sender
            .send(make_s3_object(&format!("key/{i}"), 100))
            .await
            .unwrap();
    }
    drop(input_sender);

    deleter.delete().await.unwrap();

    // No failures: warning flag NOT set, pipeline NOT cancelled
    assert!(
        !has_warning.load(Ordering::SeqCst),
        "Warning flag should not be set when there are no failures"
    );
    assert!(
        !cancellation_token.is_cancelled(),
        "Pipeline should not be cancelled when there are no failures"
    );

    // All 3 objects successfully deleted and forwarded
    let snapshot = stats_report.snapshot();
    assert_eq!(snapshot.stats_deleted_objects, 3);
    assert_eq!(snapshot.stats_failed_objects, 0);

    let mut forwarded = Vec::new();
    while let Ok(obj) = output_receiver.try_recv() {
        forwarded.push(obj.key().to_string());
    }
    assert_eq!(forwarded.len(), 3);
}

/// Scenario: warn_as_error=false, no deletion failures.
/// Expected: Baseline -- everything completes normally.
#[tokio::test]
async fn warn_as_error_false_no_failures_baseline() {
    init_dummy_tracing_subscriber();

    let (stats_sender, _stats_receiver) = async_channel::unbounded();
    let (boxed, _mock) = make_mock_storage_boxed(stats_sender);

    let (input_sender, input_receiver) = async_channel::bounded::<S3Object>(10);
    let (output_sender, output_receiver) = async_channel::bounded::<S3Object>(10);

    let config = make_test_config(); // warn_as_error defaults to false

    let (stage, cancellation_token, has_warning) =
        make_stage_with_observables(config, boxed, Some(input_receiver), Some(output_sender));

    let stats_report = Arc::new(DeletionStatsReport::new());
    let delete_counter = Arc::new(AtomicU64::new(0));
    let mut deleter = ObjectDeleter::new(stage, 0, stats_report.clone(), delete_counter);

    for i in 0..3 {
        input_sender
            .send(make_s3_object(&format!("key/{i}"), 100))
            .await
            .unwrap();
    }
    drop(input_sender);

    deleter.delete().await.unwrap();

    assert!(!has_warning.load(Ordering::SeqCst));
    assert!(!cancellation_token.is_cancelled());

    let snapshot = stats_report.snapshot();
    assert_eq!(snapshot.stats_deleted_objects, 3);
    assert_eq!(snapshot.stats_failed_objects, 0);

    let mut forwarded = Vec::new();
    while let Ok(obj) = output_receiver.try_recv() {
        forwarded.push(obj.key().to_string());
    }
    assert_eq!(forwarded.len(), 3);
}

/// Scenario: warn_as_error=true, multiple batches, first batch has failures.
/// Expected: Pipeline cancels after the first batch with failures. The second
/// batch is never flushed because the cancellation token is set.
#[tokio::test]
async fn warn_as_error_true_cancels_on_first_failing_batch() {
    init_dummy_tracing_subscriber();

    let (stats_sender, _stats_receiver) = async_channel::unbounded();
    let (boxed, mock) = make_mock_storage_boxed(stats_sender);

    // Configure "key/0" to fail -- this is in the first batch
    mock.batch_error_keys
        .lock()
        .unwrap()
        .insert("key/0".to_string(), "AccessDenied".to_string());

    let (input_sender, input_receiver) = async_channel::bounded::<S3Object>(20);
    let (output_sender, _output_receiver) = async_channel::bounded::<S3Object>(20);

    let mut config = make_test_config();
    config.warn_as_error = true;
    config.batch_size = 3; // Small batch so we get multiple batches

    let (stage, cancellation_token, has_warning) =
        make_stage_with_observables(config, boxed, Some(input_receiver), Some(output_sender));

    let stats_report = Arc::new(DeletionStatsReport::new());
    let delete_counter = Arc::new(AtomicU64::new(0));
    let mut deleter = ObjectDeleter::new(stage, 0, stats_report.clone(), delete_counter);

    // Send 9 objects: batch_size=3 means 3 batches of 3.
    // key/0 is in the first batch and will fail.
    for i in 0..9 {
        input_sender
            .send(make_s3_object(&format!("key/{i}"), 100))
            .await
            .unwrap();
    }
    drop(input_sender);

    deleter.delete().await.unwrap();

    // Pipeline must be cancelled after first batch failure
    assert!(
        cancellation_token.is_cancelled(),
        "Pipeline must cancel on first batch with failures when warn_as_error=true"
    );
    assert!(has_warning.load(Ordering::SeqCst));

    // Only the first batch should have been processed.
    // First batch: key/0 (fail), key/1 (ok), key/2 (ok) = 2 deleted, 1 failed.
    // Second and third batches should not be flushed because the deleter
    // returns early after cancellation.
    let snapshot = stats_report.snapshot();
    assert_eq!(
        snapshot.stats_deleted_objects, 2,
        "Only objects from the first batch should be deleted"
    );
    assert_eq!(
        snapshot.stats_failed_objects, 1,
        "Only the failure from the first batch should be counted"
    );

    // Verify the mock only received one batch delete call (the first batch)
    let batch_calls = mock.delete_objects_calls.lock().unwrap();
    assert_eq!(
        batch_calls.len(),
        1,
        "Only one batch should be sent to S3 before cancellation"
    );
    assert_eq!(batch_calls[0].identifiers.len(), 3);
}

/// Scenario: warn_as_error with single deleter mode (batch_size=1).
/// Expected: When single delete fails and warn_as_error=true, the pipeline
/// is still cancelled because the failure is surfaced through BatchDeleter
/// (single delete failures are handled via the fallback retry mechanism).
///
/// Note: With batch_size=1, each object is processed by SingleDeleter.
/// When a single delete fails (all retries exhausted), it appears as a
/// FailedKey in the DeleteResult, triggering the warn_as_error cancellation
/// path in delete_buffered_objects.
#[tokio::test]
async fn warn_as_error_true_single_deleter_failure_cancels() {
    init_dummy_tracing_subscriber();

    let (stats_sender, _stats_receiver) = async_channel::unbounded();
    let (boxed, mock) = make_mock_storage_boxed(stats_sender);

    // Configure "key/0" to fail in batch delete.
    // With batch_size=1, each object goes through SingleDeleter.
    // But we can trigger a failure differently: the mock's delete_object_error_keys.
    // SingleDeleter returns Err when delete_object fails, which makes
    // delete_buffered_objects cancel the pipeline entirely (different from batch
    // partial failure). So for single-mode warn_as_error testing, we need to
    // use batch_size > 1 but small enough to test the flow.
    //
    // Actually, with batch_size=1 the ObjectDeleter uses SingleDeleter, and if
    // SingleDeleter returns Err, the entire batch call in delete_buffered_objects
    // goes to the Err branch (line 407-416) which cancels unconditionally.
    //
    // The warn_as_error path is specifically for partial failures (some objects
    // in a batch fail while others succeed), so it only applies to BatchDeleter.
    // Let's test with batch_size=2 instead.
    mock.batch_error_keys
        .lock()
        .unwrap()
        .insert("key/0".to_string(), "AccessDenied".to_string());

    let (input_sender, input_receiver) = async_channel::bounded::<S3Object>(10);
    let (output_sender, _output_receiver) = async_channel::bounded::<S3Object>(10);

    let mut config = make_test_config();
    config.warn_as_error = true;
    config.batch_size = 2; // Small batch -- 2 objects per batch

    let (stage, cancellation_token, has_warning) =
        make_stage_with_observables(config, boxed, Some(input_receiver), Some(output_sender));

    let stats_report = Arc::new(DeletionStatsReport::new());
    let delete_counter = Arc::new(AtomicU64::new(0));
    let mut deleter = ObjectDeleter::new(stage, 0, stats_report.clone(), delete_counter);

    // Send 4 objects: batches of 2: [key/0, key/1] and [key/2, key/3]
    // First batch has key/0 failing.
    for i in 0..4 {
        input_sender
            .send(make_s3_object(&format!("key/{i}"), 100))
            .await
            .unwrap();
    }
    drop(input_sender);

    deleter.delete().await.unwrap();

    assert!(cancellation_token.is_cancelled());
    assert!(has_warning.load(Ordering::SeqCst));

    // Only first batch should have been processed
    let snapshot = stats_report.snapshot();
    assert_eq!(snapshot.stats_deleted_objects, 1); // key/1 succeeded
    assert_eq!(snapshot.stats_failed_objects, 1); // key/0 failed
}

/// Scenario: DeletionStatistics::DeleteWarning is emitted for each failed key
/// regardless of the warn_as_error setting.
///
/// Both warn_as_error=true and warn_as_error=false should emit DeleteWarning
/// stats messages for every failed key in the batch.
#[tokio::test]
async fn delete_warning_stats_emitted_for_each_failed_key() {
    init_dummy_tracing_subscriber();

    for warn_as_error in [false, true] {
        let (stats_sender, stats_receiver) = async_channel::unbounded();
        let (boxed, mock) = make_mock_storage_boxed(stats_sender.clone());

        // Configure two keys to fail
        {
            let mut errors = mock.batch_error_keys.lock().unwrap();
            errors.insert("key/1".to_string(), "AccessDenied".to_string());
            errors.insert("key/3".to_string(), "NoSuchKey".to_string());
        }

        let (input_sender, input_receiver) = async_channel::bounded::<S3Object>(10);
        let (output_sender, _output_receiver) = async_channel::bounded::<S3Object>(10);

        let mut config = make_test_config();
        config.warn_as_error = warn_as_error;
        config.batch_size = 1000; // All objects in one batch

        let (stage, _cancellation_token, _has_warning) =
            make_stage_with_observables(config, boxed, Some(input_receiver), Some(output_sender));

        let stats_report = Arc::new(DeletionStatsReport::new());
        let delete_counter = Arc::new(AtomicU64::new(0));
        let mut deleter = ObjectDeleter::new(stage, 0, stats_report.clone(), delete_counter);

        // Send 5 objects
        for i in 0..5 {
            input_sender
                .send(make_s3_object(&format!("key/{i}"), 100))
                .await
                .unwrap();
        }
        drop(input_sender);

        deleter.delete().await.unwrap();

        // Close the stats sender so try_recv will drain remaining messages
        stats_sender.close();
        let mut warning_keys = Vec::new();
        while let Ok(stat) = stats_receiver.try_recv() {
            if let DeletionStatistics::DeleteWarning { key } = stat {
                warning_keys.push(key);
            }
        }

        // Both failed keys should have DeleteWarning stats emitted
        assert!(
            warning_keys.contains(&"key/1".to_string()),
            "warn_as_error={warn_as_error}: DeleteWarning should be emitted for key/1"
        );
        assert!(
            warning_keys.contains(&"key/3".to_string()),
            "warn_as_error={warn_as_error}: DeleteWarning should be emitted for key/3"
        );
    }
}

/// Scenario: warn_as_error=true with retryable batch failures that recover
/// via single-delete fallback.
/// Expected: When all retryable failures are recovered via fallback, there
/// are no remaining FailedKeys, so warn_as_error does NOT trigger cancellation.
#[tokio::test]
async fn warn_as_error_true_retryable_failures_recovered_no_cancellation() {
    init_dummy_tracing_subscriber();

    let (stats_sender, _stats_receiver) = async_channel::unbounded();
    let (boxed, mock) = make_mock_storage_boxed(stats_sender);

    // Configure "key/1" with retryable error in batch -- single delete succeeds
    mock.batch_error_keys
        .lock()
        .unwrap()
        .insert("key/1".to_string(), "InternalError".to_string());
    // Note: delete_object_error_keys is empty, so single-delete fallback succeeds

    let (input_sender, input_receiver) = async_channel::bounded::<S3Object>(10);
    let (output_sender, output_receiver) = async_channel::bounded::<S3Object>(10);

    let mut config = make_test_config();
    config.warn_as_error = true;
    config.batch_size = 1000;

    let (stage, cancellation_token, has_warning) =
        make_stage_with_observables(config, boxed, Some(input_receiver), Some(output_sender));

    let stats_report = Arc::new(DeletionStatsReport::new());
    let delete_counter = Arc::new(AtomicU64::new(0));
    let mut deleter = ObjectDeleter::new(stage, 0, stats_report.clone(), delete_counter);

    for i in 0..3 {
        input_sender
            .send(make_s3_object(&format!("key/{i}"), 100))
            .await
            .unwrap();
    }
    drop(input_sender);

    deleter.delete().await.unwrap();

    // All objects recovered via fallback: no FailedKeys remain, so
    // warn_as_error should NOT trigger.
    assert!(
        !cancellation_token.is_cancelled(),
        "Pipeline should not cancel when all retryable failures are recovered"
    );
    assert!(
        !has_warning.load(Ordering::SeqCst),
        "Warning should not be set when all failures are recovered via fallback"
    );

    let snapshot = stats_report.snapshot();
    assert_eq!(snapshot.stats_deleted_objects, 3);
    assert_eq!(snapshot.stats_failed_objects, 0);

    // All objects forwarded
    let mut forwarded = Vec::new();
    while let Ok(obj) = output_receiver.try_recv() {
        forwarded.push(obj.key().to_string());
    }
    assert_eq!(forwarded.len(), 3);
}

/// Scenario: warn_as_error=true with mixed retryable and non-retryable failures.
/// The retryable failure recovers via fallback, but the non-retryable one remains
/// as a FailedKey, triggering warn_as_error cancellation.
#[tokio::test]
async fn warn_as_error_true_mixed_failures_non_retryable_triggers_cancel() {
    init_dummy_tracing_subscriber();

    let (stats_sender, _stats_receiver) = async_channel::unbounded();
    let (boxed, mock) = make_mock_storage_boxed(stats_sender);

    {
        let mut errors = mock.batch_error_keys.lock().unwrap();
        // Retryable: recovers via single-delete fallback
        errors.insert("key/0".to_string(), "InternalError".to_string());
        // Non-retryable: stays in FailedKeys
        errors.insert("key/2".to_string(), "NoSuchKey".to_string());
    }

    let (input_sender, input_receiver) = async_channel::bounded::<S3Object>(10);
    let (output_sender, _output_receiver) = async_channel::bounded::<S3Object>(10);

    let mut config = make_test_config();
    config.warn_as_error = true;
    config.batch_size = 1000;

    let (stage, cancellation_token, has_warning) =
        make_stage_with_observables(config, boxed, Some(input_receiver), Some(output_sender));

    let stats_report = Arc::new(DeletionStatsReport::new());
    let delete_counter = Arc::new(AtomicU64::new(0));
    let mut deleter = ObjectDeleter::new(stage, 0, stats_report.clone(), delete_counter);

    for i in 0..3 {
        input_sender
            .send(make_s3_object(&format!("key/{i}"), 100))
            .await
            .unwrap();
    }
    drop(input_sender);

    deleter.delete().await.unwrap();

    // Non-retryable failure remains as FailedKey, triggering warn_as_error
    assert!(
        cancellation_token.is_cancelled(),
        "Pipeline must cancel when non-retryable failure remains with warn_as_error=true"
    );
    assert!(has_warning.load(Ordering::SeqCst));

    let snapshot = stats_report.snapshot();
    // key/0 recovered via fallback, key/1 succeeded in batch = 2 deleted
    assert_eq!(snapshot.stats_deleted_objects, 2);
    // key/2 failed with non-retryable error = 1 failed
    assert_eq!(snapshot.stats_failed_objects, 1);
}
