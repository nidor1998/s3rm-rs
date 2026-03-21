//! Include regex filter stage.
//!
//! Reused from s3sync's `pipeline/filter/include_regex.rs`.
//! Passes objects whose key matches the configured include regex pattern.

use anyhow::Result;
use async_trait::async_trait;
use tracing::{debug, warn};

use crate::config::FilterConfig;
use crate::filters::{ObjectFilter, ObjectFilterBase};
use crate::stage::Stage;
use crate::types::S3Object;

pub struct IncludeRegexFilter<'a> {
    base: ObjectFilterBase<'a>,
}

const FILTER_NAME: &str = "IncludeRegexFilter";

impl IncludeRegexFilter<'_> {
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
impl ObjectFilter for IncludeRegexFilter<'_> {
    async fn filter(&self) -> Result<()> {
        self.base.filter(is_match).await
    }
}

fn is_match(object: &S3Object, config: &FilterConfig) -> bool {
    let Some(regex) = config.include_regex.as_ref() else {
        warn!(
            name = FILTER_NAME,
            "include_regex config is None, skipping object to be safe."
        );
        return false;
    };

    let match_result = match regex.is_match(object.key()) {
        Ok(matched) => matched,
        Err(e) => {
            warn!(
                name = FILTER_NAME,
                key = object.key(),
                error = %e,
                "regex match failed, skipping object to be safe."
            );
            false
        }
    };

    if !match_result {
        let key = object.key();
        let delete_marker = object.is_delete_marker();
        let version_id = object.version_id();
        let include_regex = regex.as_str();

        debug!(
            name = FILTER_NAME,
            key = key,
            delete_marker = delete_marker,
            version_id = version_id,
            include_regex = include_regex,
            "object filtered."
        );
    }

    match_result
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::config::FilterConfig;
    use crate::test_utils::init_dummy_tracing_subscriber;
    use aws_sdk_s3::types::Object;
    use fancy_regex::Regex;

    /// Test helper: expose is_match for property tests.
    pub(crate) fn test_is_match(object: &S3Object, config: &FilterConfig) -> bool {
        is_match(object, config)
    }

    #[test]
    fn is_match_true() {
        init_dummy_tracing_subscriber();

        let config = FilterConfig {
            include_regex: Some(Regex::new(r".+\.(csv|pdf)$").unwrap()),
            ..Default::default()
        };

        let object = S3Object::NotVersioning(Object::builder().key("dir1/aaa.csv").build());
        assert!(is_match(&object, &config));

        let object = S3Object::NotVersioning(Object::builder().key("abcdefg.pdf").build());
        assert!(is_match(&object, &config));
    }

    #[test]
    fn is_match_false() {
        init_dummy_tracing_subscriber();

        let config = FilterConfig {
            include_regex: Some(Regex::new(r".+\.(csv|pdf)$").unwrap()),
            ..Default::default()
        };

        let object = S3Object::NotVersioning(Object::builder().key("aaa.txt").build());
        assert!(!is_match(&object, &config));

        let object = S3Object::NotVersioning(Object::builder().key("abcdefg").build());
        assert!(!is_match(&object, &config));
    }

    #[test]
    fn is_match_none_config_returns_false() {
        init_dummy_tracing_subscriber();

        let config = FilterConfig {
            include_regex: None,
            ..Default::default()
        };

        let object = S3Object::NotVersioning(Object::builder().key("anything.csv").build());
        assert!(!is_match(&object, &config));
    }

    #[test]
    fn is_match_regex_error_returns_false() {
        use fancy_regex::RegexBuilder;

        init_dummy_tracing_subscriber();

        // Backreference forces fancy_regex VM (inner regex crate can't handle it).
        // With backtrack_limit(1), matching against a non-matching string triggers RuntimeError.
        let regex = RegexBuilder::new(r"(a+)\1b")
            .backtrack_limit(1)
            .build()
            .unwrap();

        // Verify this actually produces an error (not Ok)
        assert!(regex.is_match("aaaaaaaaaaaaaaac").is_err());

        let config = FilterConfig {
            include_regex: Some(regex),
            ..Default::default()
        };

        let object = S3Object::NotVersioning(Object::builder().key("aaaaaaaaaaaaaaac").build());
        assert!(!is_match(&object, &config));
    }
}
