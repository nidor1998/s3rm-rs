use std::collections::HashMap;
use std::fmt;
use std::fmt::{Debug, Display, Formatter};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use aws_sdk_s3::primitives::DateTime;
use aws_sdk_s3::types::{DeleteMarkerEntry, Object, ObjectVersion};
use zeroize_derive::{Zeroize, ZeroizeOnDrop};

pub mod error;
pub mod event_callback;
pub mod filter_callback;
pub mod token;

// ---------------------------------------------------------------------------
// S3 Object types (from Task 2, reused from s3sync)
// ---------------------------------------------------------------------------

/// S3 object representation used throughout the deletion pipeline.
///
/// Adapted from s3sync's S3syncObject enum, representing the different
/// kinds of objects that can be listed from S3.
#[derive(Debug, Clone, PartialEq)]
pub enum S3Object {
    NotVersioning(Object),
    Versioning(ObjectVersion),
    DeleteMarker(DeleteMarkerEntry),
}

impl S3Object {
    pub fn key(&self) -> &str {
        match &self {
            Self::Versioning(object) => object.key().unwrap(),
            Self::NotVersioning(object) => object.key().unwrap(),
            Self::DeleteMarker(marker) => marker.key().unwrap(),
        }
    }

    pub fn last_modified(&self) -> &DateTime {
        match &self {
            Self::Versioning(object) => object.last_modified().unwrap(),
            Self::NotVersioning(object) => object.last_modified().unwrap(),
            Self::DeleteMarker(marker) => marker.last_modified().unwrap(),
        }
    }

    pub fn size(&self) -> i64 {
        match &self {
            Self::Versioning(object) => object.size().unwrap(),
            Self::NotVersioning(object) => object.size().unwrap(),
            Self::DeleteMarker(_) => 0,
        }
    }

    pub fn version_id(&self) -> Option<&str> {
        match &self {
            Self::Versioning(object) => object.version_id(),
            Self::NotVersioning(_) => None,
            Self::DeleteMarker(object) => object.version_id(),
        }
    }

    pub fn e_tag(&self) -> Option<&str> {
        match &self {
            Self::Versioning(object) => object.e_tag(),
            Self::NotVersioning(object) => object.e_tag(),
            Self::DeleteMarker(_) => None,
        }
    }

    pub fn is_latest(&self) -> bool {
        match &self {
            Self::Versioning(object) => object.is_latest().unwrap_or(false),
            Self::NotVersioning(_) => false,
            Self::DeleteMarker(marker) => marker.is_latest().unwrap_or(false),
        }
    }

    pub fn is_delete_marker(&self) -> bool {
        matches!(self, Self::DeleteMarker(_))
    }
}

/// Type alias for a thread-safe map of object keys to S3Objects.
///
/// Used for tracking objects during pipeline execution.
/// Reused from s3sync's ObjectKeyMap pattern.
pub type ObjectKeyMap = Arc<Mutex<HashMap<String, S3Object>>>;

// ---------------------------------------------------------------------------
// Statistics types (from Task 2 + Task 3)
// ---------------------------------------------------------------------------

/// Statistics sent through the stats channel during pipeline execution.
///
/// Each variant represents a single event that is sent from workers to the
/// progress reporter via an async channel. Adapted from s3sync's SyncStatistics.
#[derive(Debug, PartialEq)]
pub enum DeletionStatistics {
    DeleteBytes(u64),
    DeleteComplete { key: String },
    DeleteSkip { key: String },
    DeleteError { key: String },
    DeleteWarning { key: String },
}

/// Aggregate deletion statistics report with atomic counters.
///
/// Used for thread-safe statistics tracking across worker threads.
/// Workers call `increment_deleted()` and `increment_failed()` concurrently.
/// Adapted from s3sync's SyncStatsReport.
#[derive(Debug)]
pub struct DeletionStatsReport {
    pub stats_deleted_objects: AtomicU64,
    pub stats_deleted_bytes: AtomicU64,
    pub stats_failed_objects: AtomicU64,
}

impl DeletionStatsReport {
    pub fn new() -> Self {
        Self {
            stats_deleted_objects: AtomicU64::new(0),
            stats_deleted_bytes: AtomicU64::new(0),
            stats_failed_objects: AtomicU64::new(0),
        }
    }

    /// Record a successful deletion of an object with the given byte size.
    pub fn increment_deleted(&self, bytes: u64) {
        self.stats_deleted_objects.fetch_add(1, Ordering::Relaxed);
        self.stats_deleted_bytes.fetch_add(bytes, Ordering::Relaxed);
    }

    /// Record a failed deletion attempt.
    pub fn increment_failed(&self) {
        self.stats_failed_objects.fetch_add(1, Ordering::Relaxed);
    }

    /// Take a point-in-time snapshot of the current statistics.
    ///
    /// The `duration` field in the returned `DeletionStats` is set to
    /// `Duration::default()` and should be overridden by the caller.
    pub fn snapshot(&self) -> DeletionStats {
        DeletionStats {
            stats_deleted_objects: self.stats_deleted_objects.load(Ordering::Relaxed),
            stats_deleted_bytes: self.stats_deleted_bytes.load(Ordering::Relaxed),
            stats_failed_objects: self.stats_failed_objects.load(Ordering::Relaxed),
            duration: Duration::default(),
        }
    }
}

impl Default for DeletionStatsReport {
    fn default() -> Self {
        Self::new()
    }
}

/// Public API deletion statistics returned after pipeline completion.
///
/// Provides a summary of the deletion operation including counts and timing.
#[derive(Debug, Clone, PartialEq)]
pub struct DeletionStats {
    pub stats_deleted_objects: u64,
    pub stats_deleted_bytes: u64,
    pub stats_failed_objects: u64,
    pub duration: Duration,
}

// ---------------------------------------------------------------------------
// Deletion outcome and error types (Task 3.3)
// ---------------------------------------------------------------------------

/// Outcome of a single object deletion attempt.
///
/// Returned by `BatchDeleter` and `SingleDeleter` for each object processed.
#[derive(Debug, Clone, PartialEq)]
pub enum DeletionOutcome {
    /// Object was successfully deleted.
    Success {
        key: String,
        version_id: Option<String>,
    },
    /// Object deletion failed after retries.
    Failed {
        key: String,
        version_id: Option<String>,
        error: DeletionError,
        retry_count: u32,
    },
}

impl DeletionOutcome {
    /// Returns true if this outcome represents a successful deletion.
    pub fn is_success(&self) -> bool {
        matches!(self, DeletionOutcome::Success { .. })
    }

    /// Returns the object key for this outcome.
    pub fn key(&self) -> &str {
        match self {
            DeletionOutcome::Success { key, .. } => key,
            DeletionOutcome::Failed { key, .. } => key,
        }
    }

    /// Returns the version ID for this outcome, if any.
    pub fn version_id(&self) -> Option<&str> {
        match self {
            DeletionOutcome::Success { version_id, .. } => version_id.as_deref(),
            DeletionOutcome::Failed { version_id, .. } => version_id.as_deref(),
        }
    }
}

/// Error types for individual object deletion failures.
///
/// These represent specific failure modes when attempting to delete
/// a single S3 object. Used in `DeletionOutcome::Failed` and
/// `DeletionEvent::ObjectFailed`.
#[derive(Debug, Clone, PartialEq)]
pub enum DeletionError {
    /// Object was not found (404).
    NotFound,
    /// Access denied to delete the object (403).
    AccessDenied,
    /// If-Match precondition failed (ETag mismatch, 412).
    PreconditionFailed,
    /// Request was throttled by S3 (SlowDown/TooManyRequests).
    Throttled,
    /// Network-level error (connection timeout, DNS failure, etc.).
    NetworkError(String),
    /// S3 service error (5xx or other service-side failure).
    ServiceError(String),
}

impl DeletionError {
    /// Returns true if this error is retryable.
    ///
    /// Throttled, NetworkError, and ServiceError are retryable.
    /// NotFound, AccessDenied, and PreconditionFailed are not.
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            DeletionError::Throttled
                | DeletionError::NetworkError(_)
                | DeletionError::ServiceError(_)
        )
    }
}

impl Display for DeletionError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            DeletionError::NotFound => write!(f, "Object not found"),
            DeletionError::AccessDenied => write!(f, "Access denied"),
            DeletionError::PreconditionFailed => {
                write!(f, "Precondition failed (ETag mismatch)")
            }
            DeletionError::Throttled => write!(f, "Request throttled"),
            DeletionError::NetworkError(msg) => write!(f, "Network error: {msg}"),
            DeletionError::ServiceError(msg) => write!(f, "Service error: {msg}"),
        }
    }
}

// ---------------------------------------------------------------------------
// Deletion event types (Task 3.4)
// ---------------------------------------------------------------------------

/// Events emitted during pipeline execution for callbacks and monitoring.
///
/// These events are sent to registered event callbacks (Lua or Rust)
/// to provide real-time visibility into deletion operations.
#[derive(Debug, Clone, PartialEq)]
pub enum DeletionEvent {
    /// Pipeline has started processing.
    PipelineStart,
    /// An object was successfully deleted.
    ObjectDeleted {
        key: String,
        version_id: Option<String>,
        size: u64,
    },
    /// An object deletion failed.
    ObjectFailed {
        key: String,
        version_id: Option<String>,
        error: DeletionError,
    },
    /// Pipeline has completed all processing.
    PipelineEnd,
    /// A pipeline-level error occurred.
    PipelineError { message: String },
}

// ---------------------------------------------------------------------------
// S3 Target type (Task 3.5)
// ---------------------------------------------------------------------------

/// S3 target specification parsed from an s3:// URI.
///
/// Represents the target bucket and optional prefix for deletion operations,
/// along with optional endpoint and region overrides.
#[derive(Debug, Clone, PartialEq)]
pub struct S3Target {
    pub bucket: String,
    pub prefix: Option<String>,
    pub endpoint: Option<String>,
    pub region: Option<String>,
}

impl S3Target {
    /// Parse an S3 URI in the format `s3://bucket[/prefix]`.
    ///
    /// The endpoint and region fields are not set by this method;
    /// they should be configured separately from CLI arguments or
    /// environment variables.
    ///
    /// # Examples
    ///
    /// ```
    /// use s3rm_rs::types::S3Target;
    ///
    /// let target = S3Target::parse("s3://my-bucket/logs/2023/").unwrap();
    /// assert_eq!(target.bucket, "my-bucket");
    /// assert_eq!(target.prefix.as_deref(), Some("logs/2023/"));
    ///
    /// let target = S3Target::parse("s3://my-bucket").unwrap();
    /// assert_eq!(target.bucket, "my-bucket");
    /// assert!(target.prefix.is_none());
    /// ```
    pub fn parse(s3_uri: &str) -> anyhow::Result<Self> {
        if !s3_uri.starts_with("s3://") {
            return Err(anyhow::anyhow!(error::S3rmError::InvalidUri(format!(
                "Target URI must start with 's3://': {s3_uri}"
            ))));
        }

        let without_scheme = &s3_uri[5..]; // Remove "s3://"

        if without_scheme.is_empty() {
            return Err(anyhow::anyhow!(error::S3rmError::InvalidUri(format!(
                "Bucket name cannot be empty: {s3_uri}"
            ))));
        }

        let (bucket, prefix) = match without_scheme.find('/') {
            Some(idx) => {
                let bucket = &without_scheme[..idx];
                let prefix = &without_scheme[idx + 1..];
                (
                    bucket.to_string(),
                    if prefix.is_empty() {
                        None
                    } else {
                        Some(prefix.to_string())
                    },
                )
            }
            None => (without_scheme.to_string(), None),
        };

        if bucket.is_empty() {
            return Err(anyhow::anyhow!(error::S3rmError::InvalidUri(format!(
                "Bucket name cannot be empty: {s3_uri}"
            ))));
        }

        Ok(S3Target {
            bucket,
            prefix,
            endpoint: None,
            region: None,
        })
    }
}

impl Display for S3Target {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match &self.prefix {
            Some(prefix) => write!(f, "s3://{}/{}", self.bucket, prefix),
            None => write!(f, "s3://{}", self.bucket),
        }
    }
}

// ---------------------------------------------------------------------------
// Credential types (from Task 2, reused from s3sync)
// ---------------------------------------------------------------------------

/// S3 storage path specification.
#[derive(Debug, Clone)]
pub enum StoragePath {
    S3 { bucket: String, prefix: String },
}

/// AWS configuration file locations.
#[derive(Debug, Clone)]
pub struct ClientConfigLocation {
    pub aws_config_file: Option<PathBuf>,
    pub aws_shared_credentials_file: Option<PathBuf>,
}

/// AWS credential types supported by s3rm-rs.
///
/// Reused from s3sync's credential handling with secure memory clearing.
#[derive(Debug, Clone)]
pub enum S3Credentials {
    Profile(String),
    Credentials { access_keys: AccessKeys },
    FromEnvironment,
}

/// AWS access key pair with secure zeroization.
///
/// The secret_access_key and session_token are securely cleared from memory
/// when this struct is dropped, using the zeroize crate.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct AccessKeys {
    pub access_key: String,
    pub secret_access_key: String,
    pub session_token: Option<String>,
}

impl Debug for AccessKeys {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let mut keys = f.debug_struct("AccessKeys");
        let session_token = self
            .session_token
            .as_ref()
            .map_or("None", |_| "** redacted **");
        keys.field("access_key", &self.access_key)
            .field("secret_access_key", &"** redacted **")
            .field("session_token", &session_token);
        keys.finish()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::init_dummy_tracing_subscriber;
    use aws_sdk_s3::types::{ObjectStorageClass, ObjectVersionStorageClass, Owner};

    // --- S3Object tests (existing from Task 2) ---

    #[test]
    fn non_versioning_object_getters() {
        init_dummy_tracing_subscriber();

        let object = Object::builder()
            .key("test/key.txt")
            .size(1024)
            .e_tag("my-etag")
            .storage_class(ObjectStorageClass::Standard)
            .owner(
                Owner::builder()
                    .id("test_id")
                    .display_name("test_name")
                    .build(),
            )
            .last_modified(DateTime::from_secs(777))
            .build();

        let s3_object = S3Object::NotVersioning(object);

        assert_eq!(s3_object.key(), "test/key.txt");
        assert_eq!(s3_object.size(), 1024);
        assert_eq!(s3_object.e_tag().unwrap(), "my-etag");
        assert_eq!(*s3_object.last_modified(), DateTime::from_secs(777));
        assert!(s3_object.version_id().is_none());
        assert!(!s3_object.is_latest());
        assert!(!s3_object.is_delete_marker());
    }

    #[test]
    fn versioning_object_getters() {
        init_dummy_tracing_subscriber();

        let object = ObjectVersion::builder()
            .key("test/key.txt")
            .version_id("version1")
            .is_latest(true)
            .size(2048)
            .e_tag("my-etag-v1")
            .storage_class(ObjectVersionStorageClass::Standard)
            .last_modified(DateTime::from_secs(888))
            .build();

        let s3_object = S3Object::Versioning(object);

        assert_eq!(s3_object.key(), "test/key.txt");
        assert_eq!(s3_object.size(), 2048);
        assert_eq!(s3_object.e_tag().unwrap(), "my-etag-v1");
        assert_eq!(*s3_object.last_modified(), DateTime::from_secs(888));
        assert_eq!(s3_object.version_id().unwrap(), "version1");
        assert!(s3_object.is_latest());
        assert!(!s3_object.is_delete_marker());
    }

    #[test]
    fn delete_marker_getters() {
        init_dummy_tracing_subscriber();

        let marker = DeleteMarkerEntry::builder()
            .key("test/deleted.txt")
            .version_id("dm-version1")
            .is_latest(true)
            .last_modified(DateTime::from_secs(999))
            .build();

        let s3_object = S3Object::DeleteMarker(marker);

        assert_eq!(s3_object.key(), "test/deleted.txt");
        assert_eq!(s3_object.size(), 0);
        assert!(s3_object.e_tag().is_none());
        assert_eq!(*s3_object.last_modified(), DateTime::from_secs(999));
        assert_eq!(s3_object.version_id().unwrap(), "dm-version1");
        assert!(s3_object.is_latest());
        assert!(s3_object.is_delete_marker());
    }

    #[test]
    fn debug_print_access_keys_redacts_secrets() {
        let access_keys = AccessKeys {
            access_key: "AKIAIOSFODNN7EXAMPLE".to_string(),
            secret_access_key: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".to_string(),
            session_token: Some("session_token_value".to_string()),
        };
        let debug_string = format!("{access_keys:?}");

        assert!(debug_string.contains("secret_access_key: \"** redacted **\""));
        assert!(debug_string.contains("session_token: \"** redacted **\""));
        assert!(!debug_string.contains("wJalrXUtnFEMI"));
    }

    // --- ObjectKeyMap tests (Task 3.1) ---

    #[test]
    fn object_key_map_insert_and_retrieve() {
        let map: ObjectKeyMap = Arc::new(Mutex::new(HashMap::new()));
        let object = S3Object::NotVersioning(
            Object::builder()
                .key("test/key.txt")
                .size(100)
                .last_modified(DateTime::from_secs(1000))
                .build(),
        );

        map.lock()
            .unwrap()
            .insert("test/key.txt".to_string(), object.clone());
        let retrieved = map.lock().unwrap().get("test/key.txt").cloned();
        assert_eq!(retrieved, Some(object));
    }

    #[test]
    fn object_key_map_concurrent_access() {
        let map: ObjectKeyMap = Arc::new(Mutex::new(HashMap::new()));
        let map_clone = Arc::clone(&map);

        // Simulate concurrent access
        map.lock().unwrap().insert(
            "key1".to_string(),
            S3Object::NotVersioning(
                Object::builder()
                    .key("key1")
                    .size(10)
                    .last_modified(DateTime::from_secs(1))
                    .build(),
            ),
        );
        map_clone.lock().unwrap().insert(
            "key2".to_string(),
            S3Object::NotVersioning(
                Object::builder()
                    .key("key2")
                    .size(20)
                    .last_modified(DateTime::from_secs(2))
                    .build(),
            ),
        );

        assert_eq!(map.lock().unwrap().len(), 2);
    }

    // --- DeletionStatsReport tests (Task 3.2) ---

    #[test]
    fn deletion_stats_report_new() {
        let report = DeletionStatsReport::new();
        assert_eq!(report.stats_deleted_objects.load(Ordering::SeqCst), 0);
        assert_eq!(report.stats_deleted_bytes.load(Ordering::SeqCst), 0);
        assert_eq!(report.stats_failed_objects.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn deletion_stats_report_default() {
        let report = DeletionStatsReport::default();
        assert_eq!(report.stats_deleted_objects.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn deletion_stats_report_increment_deleted() {
        let report = DeletionStatsReport::new();
        report.increment_deleted(1024);
        report.increment_deleted(2048);

        assert_eq!(report.stats_deleted_objects.load(Ordering::SeqCst), 2);
        assert_eq!(report.stats_deleted_bytes.load(Ordering::SeqCst), 3072);
        assert_eq!(report.stats_failed_objects.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn deletion_stats_report_increment_failed() {
        let report = DeletionStatsReport::new();
        report.increment_failed();
        report.increment_failed();
        report.increment_failed();

        assert_eq!(report.stats_deleted_objects.load(Ordering::SeqCst), 0);
        assert_eq!(report.stats_failed_objects.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn deletion_stats_report_snapshot() {
        let report = DeletionStatsReport::new();
        report.increment_deleted(500);
        report.increment_deleted(300);
        report.increment_failed();

        let stats = report.snapshot();
        assert_eq!(stats.stats_deleted_objects, 2);
        assert_eq!(stats.stats_deleted_bytes, 800);
        assert_eq!(stats.stats_failed_objects, 1);
        assert_eq!(stats.duration, Duration::default());
    }

    // --- DeletionStats tests (Task 3.2) ---

    #[test]
    fn deletion_stats_clone() {
        let stats = DeletionStats {
            stats_deleted_objects: 100,
            stats_deleted_bytes: 50_000,
            stats_failed_objects: 5,
            duration: Duration::from_secs(10),
        };
        let cloned = stats.clone();
        assert_eq!(stats, cloned);
    }

    // --- DeletionOutcome tests (Task 3.3) ---

    #[test]
    fn deletion_outcome_success() {
        let outcome = DeletionOutcome::Success {
            key: "test/key.txt".to_string(),
            version_id: Some("v1".to_string()),
        };
        assert!(outcome.is_success());
        assert_eq!(outcome.key(), "test/key.txt");
        assert_eq!(outcome.version_id(), Some("v1"));
    }

    #[test]
    fn deletion_outcome_success_no_version() {
        let outcome = DeletionOutcome::Success {
            key: "test/key.txt".to_string(),
            version_id: None,
        };
        assert!(outcome.is_success());
        assert!(outcome.version_id().is_none());
    }

    #[test]
    fn deletion_outcome_failed() {
        let outcome = DeletionOutcome::Failed {
            key: "test/key.txt".to_string(),
            version_id: None,
            error: DeletionError::AccessDenied,
            retry_count: 3,
        };
        assert!(!outcome.is_success());
        assert_eq!(outcome.key(), "test/key.txt");
    }

    // --- DeletionError tests (Task 3.3) ---

    #[test]
    fn deletion_error_is_retryable() {
        assert!(!DeletionError::NotFound.is_retryable());
        assert!(!DeletionError::AccessDenied.is_retryable());
        assert!(!DeletionError::PreconditionFailed.is_retryable());
        assert!(DeletionError::Throttled.is_retryable());
        assert!(DeletionError::NetworkError("timeout".to_string()).is_retryable());
        assert!(DeletionError::ServiceError("500".to_string()).is_retryable());
    }

    #[test]
    fn deletion_error_display() {
        assert_eq!(DeletionError::NotFound.to_string(), "Object not found");
        assert_eq!(DeletionError::AccessDenied.to_string(), "Access denied");
        assert_eq!(
            DeletionError::PreconditionFailed.to_string(),
            "Precondition failed (ETag mismatch)"
        );
        assert_eq!(DeletionError::Throttled.to_string(), "Request throttled");
        assert_eq!(
            DeletionError::NetworkError("conn reset".to_string()).to_string(),
            "Network error: conn reset"
        );
        assert_eq!(
            DeletionError::ServiceError("Internal".to_string()).to_string(),
            "Service error: Internal"
        );
    }

    // --- DeletionEvent tests (Task 3.4) ---

    #[test]
    fn deletion_event_pipeline_start() {
        let event = DeletionEvent::PipelineStart;
        assert_eq!(event, DeletionEvent::PipelineStart);
    }

    #[test]
    fn deletion_event_object_deleted() {
        let event = DeletionEvent::ObjectDeleted {
            key: "test/key.txt".to_string(),
            version_id: Some("v1".to_string()),
            size: 1024,
        };
        if let DeletionEvent::ObjectDeleted {
            key,
            version_id,
            size,
        } = &event
        {
            assert_eq!(key, "test/key.txt");
            assert_eq!(version_id.as_deref(), Some("v1"));
            assert_eq!(*size, 1024);
        } else {
            panic!("Expected ObjectDeleted event");
        }
    }

    #[test]
    fn deletion_event_object_failed() {
        let event = DeletionEvent::ObjectFailed {
            key: "test/key.txt".to_string(),
            version_id: None,
            error: DeletionError::AccessDenied,
        };
        if let DeletionEvent::ObjectFailed { key, error, .. } = &event {
            assert_eq!(key, "test/key.txt");
            assert_eq!(*error, DeletionError::AccessDenied);
        } else {
            panic!("Expected ObjectFailed event");
        }
    }

    #[test]
    fn deletion_event_pipeline_end() {
        let event = DeletionEvent::PipelineEnd;
        assert_eq!(event, DeletionEvent::PipelineEnd);
    }

    #[test]
    fn deletion_event_pipeline_error() {
        let event = DeletionEvent::PipelineError {
            message: "something went wrong".to_string(),
        };
        if let DeletionEvent::PipelineError { message } = &event {
            assert_eq!(message, "something went wrong");
        } else {
            panic!("Expected PipelineError event");
        }
    }

    #[test]
    fn deletion_event_clone() {
        let event = DeletionEvent::ObjectDeleted {
            key: "key".to_string(),
            version_id: None,
            size: 42,
        };
        let cloned = event.clone();
        assert_eq!(event, cloned);
    }

    // --- S3Target tests (Task 3.5) ---

    #[test]
    fn s3_target_parse_bucket_only() {
        let target = S3Target::parse("s3://my-bucket").unwrap();
        assert_eq!(target.bucket, "my-bucket");
        assert!(target.prefix.is_none());
        assert!(target.endpoint.is_none());
        assert!(target.region.is_none());
    }

    #[test]
    fn s3_target_parse_bucket_with_trailing_slash() {
        let target = S3Target::parse("s3://my-bucket/").unwrap();
        assert_eq!(target.bucket, "my-bucket");
        assert!(target.prefix.is_none());
    }

    #[test]
    fn s3_target_parse_bucket_with_prefix() {
        let target = S3Target::parse("s3://my-bucket/logs/2023/").unwrap();
        assert_eq!(target.bucket, "my-bucket");
        assert_eq!(target.prefix.as_deref(), Some("logs/2023/"));
    }

    #[test]
    fn s3_target_parse_bucket_with_simple_prefix() {
        let target = S3Target::parse("s3://my-bucket/prefix").unwrap();
        assert_eq!(target.bucket, "my-bucket");
        assert_eq!(target.prefix.as_deref(), Some("prefix"));
    }

    #[test]
    fn s3_target_parse_bucket_with_deep_prefix() {
        let target = S3Target::parse("s3://my-bucket/a/b/c/d/e").unwrap();
        assert_eq!(target.bucket, "my-bucket");
        assert_eq!(target.prefix.as_deref(), Some("a/b/c/d/e"));
    }

    #[test]
    fn s3_target_parse_invalid_no_scheme() {
        let result = S3Target::parse("my-bucket/prefix");
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Target URI must start with 's3://'"));
    }

    #[test]
    fn s3_target_parse_invalid_wrong_scheme() {
        let result = S3Target::parse("http://my-bucket/prefix");
        assert!(result.is_err());
    }

    #[test]
    fn s3_target_parse_invalid_empty_bucket() {
        let result = S3Target::parse("s3://");
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Bucket name cannot be empty"));
    }

    #[test]
    fn s3_target_parse_invalid_empty_bucket_with_prefix() {
        let result = S3Target::parse("s3:///prefix");
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Bucket name cannot be empty"));
    }

    #[test]
    fn s3_target_display_bucket_only() {
        let target = S3Target {
            bucket: "my-bucket".to_string(),
            prefix: None,
            endpoint: None,
            region: None,
        };
        assert_eq!(target.to_string(), "s3://my-bucket");
    }

    #[test]
    fn s3_target_display_with_prefix() {
        let target = S3Target {
            bucket: "my-bucket".to_string(),
            prefix: Some("logs/2023/".to_string()),
            endpoint: None,
            region: None,
        };
        assert_eq!(target.to_string(), "s3://my-bucket/logs/2023/");
    }

    #[test]
    fn s3_target_roundtrip() {
        // Parse then display should give back the original URI
        let uri = "s3://my-bucket/some/prefix/";
        let target = S3Target::parse(uri).unwrap();
        assert_eq!(target.to_string(), uri);
    }

    #[test]
    fn s3_target_clone_and_eq() {
        let target = S3Target::parse("s3://bucket/key").unwrap();
        let cloned = target.clone();
        assert_eq!(target, cloned);
    }
}
