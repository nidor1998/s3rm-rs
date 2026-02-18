use std::fmt;
use std::fmt::{Debug, Formatter};
use std::path::PathBuf;

use aws_sdk_s3::primitives::DateTime;
use aws_sdk_s3::types::{DeleteMarkerEntry, Object, ObjectVersion};
use zeroize_derive::{Zeroize, ZeroizeOnDrop};

pub mod error;
pub mod token;

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

/// Statistics sent through the stats channel during pipeline execution.
#[derive(Debug, PartialEq)]
pub enum DeletionStatistics {
    DeleteBytes(u64),
    DeleteComplete { key: String },
    DeleteSkip { key: String },
    DeleteError { key: String },
    DeleteWarning { key: String },
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use aws_sdk_s3::types::{ObjectStorageClass, ObjectVersionStorageClass, Owner};

    fn init_dummy_tracing_subscriber() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter("dummy=trace")
            .try_init();
    }

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
}
