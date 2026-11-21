//! Modified-time "before" filter stage.
//!
//! Reused from s3sync's `pipeline/filter/mtime_before.rs`.
//! Passes objects whose last_modified time is strictly before the configured threshold.

use crate::types::datetime::{Rfc3339Ext, from_unix_millis};
use anyhow::Result;
use async_trait::async_trait;
use tracing::{debug, warn};

use crate::config::FilterConfig;
use crate::filters::{ObjectFilter, ObjectFilterBase};
use crate::stage::Stage;
use crate::types::S3Object;

pub struct MtimeBeforeFilter<'a> {
    base: ObjectFilterBase<'a>,
}

const FILTER_NAME: &str = "MtimeBeforeFilter";

impl MtimeBeforeFilter<'_> {
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
impl ObjectFilter for MtimeBeforeFilter<'_> {
    async fn filter(&self) -> Result<()> {
        self.base.filter(is_before).await
    }
}

fn is_before(object: &S3Object, config: &FilterConfig) -> bool {
    let Some(before_time) = config.before_time else {
        warn!(
            name = FILTER_NAME,
            "before_time config is None, skipping object to be safe."
        );
        return false;
    };

    let millis = match object.last_modified().to_millis() {
        Ok(m) => m,
        Err(e) => {
            warn!(
                name = FILTER_NAME,
                key = object.key(),
                error = %e,
                "failed to convert last_modified to millis, skipping object to be safe."
            );
            return false;
        }
    };

    let last_modified = match from_unix_millis(millis) {
        Some(dt) => dt,
        None => {
            warn!(
                name = FILTER_NAME,
                key = object.key(),
                millis = millis,
                "last_modified is out of the supported date-time range, skipping object to be safe."
            );
            return false;
        }
    };

    if before_time <= last_modified {
        let key = object.key();
        let delete_marker = object.is_delete_marker();
        let version_id = object.version_id();
        let last_modified = last_modified.to_rfc3339();
        let config_time = before_time.to_rfc3339();

        debug!(
            name = FILTER_NAME,
            key = key,
            delete_marker = delete_marker,
            version_id = version_id,
            last_modified = last_modified,
            config_time = config_time,
            "object filtered."
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
    use aws_sdk_s3::primitives::{DateTime, DateTimeFormat};
    use aws_sdk_s3::types::Object;

    /// Test helper: expose is_before for property tests.
    pub(crate) fn test_is_before(object: &S3Object, config: &FilterConfig) -> bool {
        is_before(object, config)
    }

    #[test]
    fn before() {
        init_dummy_tracing_subscriber();

        let object = S3Object::NotVersioning(
            Object::builder()
                .last_modified(
                    DateTime::from_str("2023-01-20T00:00:00.000Z", DateTimeFormat::DateTime)
                        .unwrap(),
                )
                .build(),
        );

        let config = FilterConfig {
            before_time: Some(
                crate::types::datetime::parse_rfc3339_utc("2023-01-20T00:00:00.001Z").unwrap(),
            ),
            ..Default::default()
        };

        assert!(is_before(&object, &config));
    }

    #[test]
    fn after() {
        init_dummy_tracing_subscriber();

        let object = S3Object::NotVersioning(
            Object::builder()
                .key("test")
                .last_modified(
                    DateTime::from_str("2023-01-20T00:00:00.002Z", DateTimeFormat::DateTime)
                        .unwrap(),
                )
                .build(),
        );

        let config = FilterConfig {
            before_time: Some(
                crate::types::datetime::parse_rfc3339_utc("2023-01-20T00:00:00.001Z").unwrap(),
            ),
            ..Default::default()
        };

        assert!(!is_before(&object, &config));
    }

    #[test]
    fn equal() {
        init_dummy_tracing_subscriber();

        let object = S3Object::NotVersioning(
            Object::builder()
                .key("test")
                .last_modified(
                    DateTime::from_str("2023-01-20T00:00:00.001Z", DateTimeFormat::DateTime)
                        .unwrap(),
                )
                .build(),
        );

        let config = FilterConfig {
            before_time: Some(
                crate::types::datetime::parse_rfc3339_utc("2023-01-20T00:00:00.001Z").unwrap(),
            ),
            ..Default::default()
        };

        // Equal means NOT before, so filtered out
        assert!(!is_before(&object, &config));
    }

    #[test]
    fn none_config_returns_false() {
        init_dummy_tracing_subscriber();

        let object = S3Object::NotVersioning(
            Object::builder()
                .last_modified(
                    DateTime::from_str("2023-01-20T00:00:00.000Z", DateTimeFormat::DateTime)
                        .unwrap(),
                )
                .build(),
        );

        let config = FilterConfig {
            before_time: None,
            ..Default::default()
        };

        assert!(!is_before(&object, &config));
    }

    #[test]
    fn to_millis_overflow_returns_false() {
        init_dummy_tracing_subscriber();

        // DateTime::from_secs(i64::MAX) overflows when converting seconds to
        // milliseconds, so to_millis() returns an error and the object is
        // skipped (kept) to be safe.
        let object = S3Object::NotVersioning(
            Object::builder()
                .key("overflow-millis")
                .last_modified(DateTime::from_secs(i64::MAX))
                .build(),
        );

        let config = FilterConfig {
            before_time: Some(
                crate::types::datetime::parse_rfc3339_utc("2023-01-20T00:00:00.001Z").unwrap(),
            ),
            ..Default::default()
        };

        assert!(!is_before(&object, &config));
    }

    #[test]
    fn time_conversion_overflow_returns_false() {
        init_dummy_tracing_subscriber();

        // A timestamp whose seconds*1000 still fits in an i64 (so to_millis()
        // succeeds) but is far beyond the `time` crate's representable range, so the
        // conversion to an OffsetDateTime fails and the object is skipped
        // (kept) to be safe.
        let object = S3Object::NotVersioning(
            Object::builder()
                .key("overflow-time")
                .last_modified(DateTime::from_secs(9_000_000_000_000_000))
                .build(),
        );

        let config = FilterConfig {
            before_time: Some(
                crate::types::datetime::parse_rfc3339_utc("2023-01-20T00:00:00.001Z").unwrap(),
            ),
            ..Default::default()
        };

        assert!(!is_before(&object, &config));
    }
}
