//! Additional property-based tests.
//!
//! Feature: s3rm-rs, Property 4: Worker Count Configuration Validation
//! For any worker count configuration, the tool should accept values from 1
//! to 65535 and reject values outside this range.
//! **Validates: Requirements 1.4**
//!
//! Feature: s3rm-rs, Property 12: Rust Filter Callback Execution
//! For any Rust filter callback registered via the library API, the tool
//! should execute the callback for each object and delete only objects where
//! the callback returns true.
//! **Validates: Requirements 2.9**
//!
//! Feature: s3rm-rs, Property 13: Delete-All Behavior
//! When a bucket-only target is provided (no prefix), the tool targets all
//! objects in the bucket for deletion. With no filters configured, every
//! listed object passes through the pipeline unfiltered.
//! **Validates: Requirements 2.10**

#[cfg(test)]
mod tests {
    use crate::callback::filter_manager::FilterManager;
    use crate::config::args::parse_from_args;
    use crate::config::{Config, FilterConfig};
    use crate::types::filter_callback::FilterCallback;
    use crate::types::{S3Object, StoragePath};
    use anyhow::Result;
    use async_trait::async_trait;
    use aws_sdk_s3::types::Object;
    use proptest::prelude::*;

    // -----------------------------------------------------------------------
    // Test callback implementations
    // -----------------------------------------------------------------------

    /// A filter that accepts objects whose key contains a given substring.
    struct KeyContainsFilter {
        substring: String,
    }

    #[async_trait]
    impl FilterCallback for KeyContainsFilter {
        async fn filter(&mut self, object: &S3Object) -> Result<bool> {
            Ok(object.key().contains(&self.substring))
        }
    }

    /// A filter that accepts objects with size >= min and size < max.
    struct SizeRangeFilter {
        min: i64,
        max: i64,
    }

    #[async_trait]
    impl FilterCallback for SizeRangeFilter {
        async fn filter(&mut self, object: &S3Object) -> Result<bool> {
            let size = object.size();
            Ok(size >= self.min && size < self.max)
        }
    }

    /// A filter that always returns a fixed result.
    struct FixedFilter {
        result: bool,
    }

    #[async_trait]
    impl FilterCallback for FixedFilter {
        async fn filter(&mut self, _object: &S3Object) -> Result<bool> {
            Ok(self.result)
        }
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn make_test_object(key: &str, size: i64) -> S3Object {
        S3Object::NotVersioning(
            Object::builder()
                .key(key)
                .size(size)
                .last_modified(aws_sdk_s3::primitives::DateTime::from_secs(1_700_000_000))
                .build(),
        )
    }

    // -----------------------------------------------------------------------
    // Feature: s3rm-rs, Property 4: Worker Count Configuration Validation
    // **Validates: Requirements 1.4**
    //
    // The tool shall accept worker_size values from 1 to 65535 (u16 range
    // enforced by clap value_parser) and reject 0 or values exceeding u16.
    // -----------------------------------------------------------------------

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// Feature: s3rm-rs, Property 4: Worker Count Configuration Validation (valid range)
        /// Any worker count in 1..=65535 should be accepted by the CLI parser.
        #[test]
        fn property_4_valid_worker_count_accepted(
            worker_count in 1u16..=65535,
        ) {
            // Feature: s3rm-rs, Property 4: Worker Count Configuration Validation
            // **Validates: Requirements 1.4**
            let args = vec![
                "s3rm".to_string(),
                "s3://bucket/".to_string(),
                "--worker-size".to_string(),
                worker_count.to_string(),
            ];
            let result = parse_from_args(args);
            prop_assert!(result.is_ok(), "worker_size={} should be accepted", worker_count);

            let cli_args = result.unwrap();
            prop_assert_eq!(cli_args.worker_size, worker_count);
        }

        /// Feature: s3rm-rs, Property 4: Worker Count Configuration Validation (Config propagation)
        /// Worker count from CLI args should propagate to Config unchanged.
        #[test]
        fn property_4_worker_count_propagated_to_config(
            worker_count in 1u16..=1000u16,
        ) {
            // Feature: s3rm-rs, Property 4: Worker Count Configuration Validation
            // **Validates: Requirements 1.4**
            let args = vec![
                "s3rm".to_string(),
                "s3://bucket/prefix/".to_string(),
                "--worker-size".to_string(),
                worker_count.to_string(),
            ];
            let cli_args = parse_from_args(args).unwrap();
            let config = Config::try_from(cli_args).unwrap();

            prop_assert_eq!(config.worker_size, worker_count);
        }
    }

    #[test]
    fn property_4_worker_count_zero_rejected() {
        // Feature: s3rm-rs, Property 4: Worker Count Configuration Validation
        // **Validates: Requirements 1.4**
        let args = vec!["s3rm", "s3://bucket/", "--worker-size", "0"];
        let result = parse_from_args(args);
        assert!(result.is_err(), "worker_size=0 must be rejected");
    }

    #[test]
    fn property_4_worker_count_exceeds_u16_rejected() {
        // Feature: s3rm-rs, Property 4: Worker Count Configuration Validation
        // **Validates: Requirements 1.4**
        let args = vec!["s3rm", "s3://bucket/", "--worker-size", "65536"];
        let result = parse_from_args(args);
        assert!(result.is_err(), "worker_size=65536 must be rejected");
    }

    #[test]
    fn property_4_worker_count_negative_rejected() {
        // Feature: s3rm-rs, Property 4: Worker Count Configuration Validation
        // **Validates: Requirements 1.4**
        let args = vec!["s3rm", "s3://bucket/", "--worker-size", "-1"];
        let result = parse_from_args(args);
        assert!(result.is_err(), "worker_size=-1 must be rejected");
    }

    #[test]
    fn property_4_worker_count_max_accepted() {
        // Feature: s3rm-rs, Property 4: Worker Count Configuration Validation
        // **Validates: Requirements 1.4**
        let args = vec!["s3rm", "s3://bucket/", "--worker-size", "65535"];
        let result = parse_from_args(args);
        assert!(result.is_ok(), "worker_size=65535 must be accepted");
        assert_eq!(result.unwrap().worker_size, 65535);
    }

    // -----------------------------------------------------------------------
    // Feature: s3rm-rs, Property 12: Rust Filter Callback Execution
    // **Validates: Requirements 2.9**
    //
    // For any Rust filter callback registered via the library API, the tool
    // should execute the callback for each object and delete only objects
    // where the callback returns true.
    // -----------------------------------------------------------------------

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// Feature: s3rm-rs, Property 12: Rust Filter Callback Execution (KeyContains)
        /// A Rust filter that checks if the key contains a substring
        /// should accept matching objects and reject non-matching ones.
        #[test]
        fn property_12_key_contains_filter(
            substring in "[a-z]{1,5}",
            key_prefix in "[a-z]{0,5}",
            key_suffix in "[a-z]{0,5}",
            should_contain in any::<bool>(),
        ) {
            // Feature: s3rm-rs, Property 12: Rust Filter Callback Execution
            // **Validates: Requirements 2.9**
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let mut manager = FilterManager::new();
                manager.register_callback(KeyContainsFilter {
                    substring: substring.clone(),
                });
                prop_assert!(manager.is_callback_registered());

                let key = if should_contain {
                    format!("{key_prefix}{substring}{key_suffix}")
                } else {
                    // Generate a key that does NOT contain the substring
                    // by using uppercase characters (substring is lowercase)
                    format!("{}XXXX{}", key_prefix.to_uppercase(), key_suffix.to_uppercase())
                };

                let object = make_test_object(&key, 100);
                let result = manager.execute_filter(&object).await.unwrap();

                if should_contain {
                    prop_assert!(result, "Key '{}' contains '{}' so filter should accept", key, substring);
                } else {
                    // The key might accidentally contain the substring if
                    // the uppercase transformation matches, so verify the invariant.
                    let expected = key.contains(&substring);
                    prop_assert_eq!(result, expected,
                        "Key '{}' {} '{}' — filter result should match",
                        key, if expected { "contains" } else { "does not contain" }, substring);
                }
                Ok(())
            })?;
        }

        /// Feature: s3rm-rs, Property 12: Rust Filter Callback Execution (SizeRange)
        /// A Rust filter that checks object size range should correctly
        /// accept objects within the range and reject those outside.
        #[test]
        fn property_12_size_range_filter(
            min in 0i64..5_000_000,
            range_width in 1i64..5_000_000,
            object_size in 0i64..10_000_000,
        ) {
            // Feature: s3rm-rs, Property 12: Rust Filter Callback Execution
            // **Validates: Requirements 2.9**
            let max = min.saturating_add(range_width);

            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let mut manager = FilterManager::new();
                manager.register_callback(SizeRangeFilter { min, max });

                let object = make_test_object("test/key.txt", object_size);
                let result = manager.execute_filter(&object).await.unwrap();

                let expected = object_size >= min && object_size < max;
                prop_assert_eq!(result, expected,
                    "Object size {} in [{}, {}) — expected {}, got {}",
                    object_size, min, max, expected, result);
                Ok(())
            })?;
        }

        /// Feature: s3rm-rs, Property 12: Rust Filter Callback Execution (FixedResult)
        /// A Rust filter that always returns a fixed value should always
        /// produce that value regardless of the input object.
        #[test]
        fn property_12_fixed_filter_consistency(
            fixed_result in any::<bool>(),
            key in "[a-z0-9/]{1,30}",
            size in 0i64..10_000_000,
        ) {
            // Feature: s3rm-rs, Property 12: Rust Filter Callback Execution
            // **Validates: Requirements 2.9**
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let mut manager = FilterManager::new();
                manager.register_callback(FixedFilter { result: fixed_result });

                let object = make_test_object(&key, size);
                let result = manager.execute_filter(&object).await.unwrap();

                prop_assert_eq!(result, fixed_result,
                    "FixedFilter({}) should always return {} for any object",
                    fixed_result, fixed_result);
                Ok(())
            })?;
        }

        /// Feature: s3rm-rs, Property 12: Rust Filter Callback Execution (Multiple Objects)
        /// A registered Rust callback should be invoked for each object in
        /// a sequence, producing correct results each time.
        #[test]
        fn property_12_callback_invoked_for_each_object(
            num_objects in 1usize..20,
            threshold in 100i64..5_000,
        ) {
            // Feature: s3rm-rs, Property 12: Rust Filter Callback Execution
            // **Validates: Requirements 2.9**
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let mut manager = FilterManager::new();
                manager.register_callback(SizeRangeFilter { min: threshold, max: i64::MAX });

                let mut pass_count = 0usize;
                let mut reject_count = 0usize;

                for i in 0..num_objects {
                    // Alternate small and large objects
                    let size = if i % 2 == 0 { threshold + 1 } else { threshold - 1 };
                    let object = make_test_object(&format!("obj_{i}"), size);
                    let result = manager.execute_filter(&object).await.unwrap();

                    if result {
                        pass_count += 1;
                    } else {
                        reject_count += 1;
                    }

                    let expected = size >= threshold;
                    prop_assert_eq!(result, expected);
                }

                prop_assert_eq!(pass_count + reject_count, num_objects);
                Ok(())
            })?;
        }
    }

    // -----------------------------------------------------------------------
    // Feature: s3rm-rs, Property 13: Delete-All Behavior
    // **Validates: Requirements 2.10**
    //
    // "Delete all" is expressed by providing a bucket-only target with no
    // prefix (e.g., `s3://bucket`) and no filter options. There is no
    // dedicated --delete-all flag; the empty prefix causes the lister to
    // enumerate every object, and the absence of filters lets them all
    // pass through the pipeline to the deletion stage.
    // -----------------------------------------------------------------------

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// Feature: s3rm-rs, Property 13: Delete-All Behavior (Empty Prefix)
        /// When a bucket-only target is provided (no prefix), the resulting
        /// Config should have an empty prefix, meaning all objects in the
        /// bucket are targeted for deletion.
        #[test]
        fn property_13_bucket_only_target_has_empty_prefix(
            bucket in "[a-z][a-z0-9-]{2,20}",
        ) {
            // Feature: s3rm-rs, Property 13: Delete-All Behavior
            // **Validates: Requirements 2.10**
            let args = vec![
                "s3rm".to_string(),
                format!("s3://{bucket}"),
                "--force".to_string(),
            ];
            let cli_args = parse_from_args(args).unwrap();
            let config = Config::try_from(cli_args).unwrap();

            let StoragePath::S3 {
                bucket: cfg_bucket,
                prefix,
            } = &config.target;
            prop_assert_eq!(cfg_bucket, &bucket);
            prop_assert!(prefix.is_empty(),
                "Bucket-only target should produce empty prefix, got '{}'", prefix);
        }

        /// Feature: s3rm-rs, Property 13: Delete-All Behavior (No Default Filters)
        /// When no filter options are provided, FilterConfig should have all
        /// filter fields set to None/default, meaning no additional filtering
        /// is applied and all objects pass through.
        #[test]
        fn property_13_no_filters_means_all_objects_pass(
            bucket in "[a-z][a-z0-9-]{2,20}",
        ) {
            // Feature: s3rm-rs, Property 13: Delete-All Behavior
            // **Validates: Requirements 2.10**
            let args = vec![
                "s3rm".to_string(),
                format!("s3://{bucket}"),
                "--force".to_string(),
            ];
            let cli_args = parse_from_args(args).unwrap();
            let config = Config::try_from(cli_args).unwrap();

            // All filter fields should be None/default
            let fc = &config.filter_config;
            prop_assert!(fc.include_regex.is_none(), "include_regex should be None");
            prop_assert!(fc.exclude_regex.is_none(), "exclude_regex should be None");
            prop_assert!(fc.include_content_type_regex.is_none());
            prop_assert!(fc.exclude_content_type_regex.is_none());
            prop_assert!(fc.include_metadata_regex.is_none());
            prop_assert!(fc.exclude_metadata_regex.is_none());
            prop_assert!(fc.include_tag_regex.is_none());
            prop_assert!(fc.exclude_tag_regex.is_none());
            prop_assert!(fc.larger_size.is_none(), "larger_size should be None");
            prop_assert!(fc.smaller_size.is_none(), "smaller_size should be None");
            prop_assert!(fc.before_time.is_none(), "before_time should be None");
            prop_assert!(fc.after_time.is_none(), "after_time should be None");

            // No filter callback registered
            prop_assert!(!config.filter_manager.is_callback_registered());
        }

        /// Feature: s3rm-rs, Property 13: Delete-All Behavior (Prefix vs Bucket-Only)
        /// When a prefix IS provided, only objects under that prefix are
        /// targeted; when no prefix is given, ALL objects are targeted.
        /// This verifies that a bucket-only target (delete all) differs
        /// from a prefixed target.
        #[test]
        fn property_13_prefix_vs_bucket_only(
            bucket in "[a-z][a-z0-9-]{2,20}",
            prefix in "[a-z0-9]{1,10}/",
        ) {
            // Feature: s3rm-rs, Property 13: Delete-All Behavior
            // **Validates: Requirements 2.10**

            // With prefix — prefix is set
            let args_with_prefix = vec![
                "s3rm".to_string(),
                format!("s3://{bucket}/{prefix}"),
                "--force".to_string(),
            ];
            let cli_with = parse_from_args(args_with_prefix).unwrap();
            let config_with = Config::try_from(cli_with).unwrap();

            // Without prefix — targets all objects in the bucket
            let args_no_prefix = vec![
                "s3rm".to_string(),
                format!("s3://{bucket}"),
                "--force".to_string(),
            ];
            let cli_no = parse_from_args(args_no_prefix).unwrap();
            let config_no = Config::try_from(cli_no).unwrap();

            let StoragePath::S3 { prefix: p_with, .. } = &config_with.target;
            let StoragePath::S3 { prefix: p_no, .. } = &config_no.target;

            prop_assert!(!p_with.is_empty(), "Prefix target should have non-empty prefix");
            prop_assert!(p_no.is_empty(), "Bucket-only target should have empty prefix");
        }

    }

    /// Feature: s3rm-rs, Property 13: Delete-All Behavior (Default FilterConfig passes all)
    /// A default FilterConfig should not reject any object — all filter
    /// fields being None means every object passes through unfiltered.
    #[test]
    fn property_13_default_filter_config_passes_all() {
        // Feature: s3rm-rs, Property 13: Delete-All Behavior
        // **Validates: Requirements 2.10**
        let fc = FilterConfig::default();

        // Every field should be None
        assert!(fc.include_regex.is_none());
        assert!(fc.exclude_regex.is_none());
        assert!(fc.include_content_type_regex.is_none());
        assert!(fc.exclude_content_type_regex.is_none());
        assert!(fc.include_metadata_regex.is_none());
        assert!(fc.exclude_metadata_regex.is_none());
        assert!(fc.include_tag_regex.is_none());
        assert!(fc.exclude_tag_regex.is_none());
        assert!(fc.larger_size.is_none());
        assert!(fc.smaller_size.is_none());
        assert!(fc.before_time.is_none());
        assert!(fc.after_time.is_none());
    }
}
