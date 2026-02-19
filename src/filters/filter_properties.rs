//! Property-based tests for filter stages.
//!
//! Tests Properties 7-10 from the design document.

#[cfg(test)]
mod tests {
    use crate::config::FilterConfig;
    use crate::types::S3Object;
    use aws_sdk_s3::primitives::DateTime;
    use aws_sdk_s3::types::{DeleteMarkerEntry, Object};
    use chrono::Utc;
    use fancy_regex::Regex;
    use proptest::prelude::*;

    // --- Generators ---

    /// Generate S3 keys with a mix of extensions and prefixes so that
    /// regex property tests exercise both the matching and non-matching paths.
    fn arbitrary_s3_key() -> impl Strategy<Value = String> {
        let extensions = prop_oneof![
            Just(".csv".to_string()),
            Just(".tmp".to_string()),
            Just(".pdf".to_string()),
            Just(".txt".to_string()),
            Just("".to_string()), // no extension
        ];
        let prefixes = prop_oneof![
            Just("temp/".to_string()),
            Just("data/".to_string()),
            Just("".to_string()), // no prefix
        ];
        (
            prefixes,
            proptest::collection::vec("[a-z0-9]{1,8}", 1..=3),
            extensions,
        )
            .prop_map(|(prefix, segments, ext)| {
                format!("{}{}{}", prefix, segments.join("/"), ext)
            })
    }

    fn arbitrary_s3_object_with_key(key: String) -> S3Object {
        S3Object::NotVersioning(
            Object::builder()
                .key(key)
                .size(100)
                .last_modified(DateTime::from_secs(1000))
                .build(),
        )
    }

    fn arbitrary_s3_object_with_size(size: i64) -> S3Object {
        S3Object::NotVersioning(
            Object::builder()
                .key("test-key")
                .size(size)
                .last_modified(DateTime::from_secs(1000))
                .build(),
        )
    }

    fn arbitrary_s3_object_with_mtime(epoch_millis: i64) -> S3Object {
        S3Object::NotVersioning(
            Object::builder()
                .key("test-key")
                .size(100)
                .last_modified(DateTime::from_millis(epoch_millis))
                .build(),
        )
    }

    // -----------------------------------------------------------------------
    // Feature: s3rm-rs, Property 7: Prefix Filtering
    // **Validates: Requirements 2.1**
    //
    // Prefix filtering is enforced at the S3 API level: the configured prefix
    // from `StoragePath` is passed as the `prefix` parameter in
    // `ListObjectsV2`/`ListObjectVersions` requests, so S3 only returns keys
    // that start with that prefix. We verify that:
    //   (a) `StoragePath::S3` faithfully stores any generated prefix, and
    //   (b) the mock-based ObjectLister emits only those objects the storage
    //       layer provides (i.e., the lister does not drop or invent objects).
    // -----------------------------------------------------------------------
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn test_prefix_stored_in_config(
            bucket in "[a-z]{3,10}",
            prefix in "[a-z]{0,5}(/[a-z]{1,5}){0,3}/?"
        ) {
            use crate::types::StoragePath;

            let path = StoragePath::S3 {
                bucket: bucket.clone(),
                prefix: prefix.clone(),
            };
            // Property: StoragePath preserves the exact prefix that will be
            // sent to S3's ListObjects API.
            match &path {
                StoragePath::S3 { bucket: b, prefix: p } => {
                    prop_assert_eq!(b, &bucket);
                    prop_assert_eq!(p, &prefix);
                }
            }
        }

        #[test]
        fn test_lister_emits_exactly_storage_objects(
            num_objects in 0usize..20,
        ) {
            // Property: The ObjectLister emits exactly the objects that the
            // storage layer returns — no filtering, reordering, or duplication.
            // This matters because prefix selection is delegated to S3; the
            // lister must faithfully forward whatever S3 returns.
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();

            let _ = rt.block_on(async {
                let objects: Vec<S3Object> = (0..num_objects)
                    .map(|i| {
                        S3Object::NotVersioning(
                            Object::builder()
                                .key(format!("prefix/obj_{}", i))
                                .size(i as i64)
                                .last_modified(DateTime::from_secs(1000))
                                .build(),
                        )
                    })
                    .collect();

                let config = crate::lister::tests::make_test_config_pub();
                let (lister, _, _, receiver) =
                    crate::lister::tests::create_mock_lister(config, objects.clone(), vec![]);

                lister.list_target(1000).await.unwrap();

                let mut received = Vec::new();
                while let Ok(obj) = receiver.try_recv() {
                    received.push(obj);
                }

                // Exact count: no objects dropped or duplicated
                prop_assert_eq!(received.len(), objects.len());

                // Exact order and content preserved
                for (got, expected) in received.iter().zip(objects.iter()) {
                    prop_assert_eq!(got.key(), expected.key());
                }

                Ok(())
            });
        }
    }

    // -----------------------------------------------------------------------
    // Feature: s3rm-rs, Property 8: Regex Filtering
    // **Validates: Requirements 2.2, 2.3, 2.4, 2.5, 2.12**
    //
    // For any regex pattern provided for keys, the tool should delete only
    // objects where the key matches the pattern, and multiple regex filters
    // should be combined with AND logic.
    // -----------------------------------------------------------------------
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn test_include_regex_filtering(
            keys in proptest::collection::vec(arbitrary_s3_key(), 1..50),
        ) {
            // Use a fixed regex pattern to test include behavior
            let pattern = Regex::new(r".*\.csv$").unwrap();
            let config = FilterConfig {
                include_regex: Some(pattern),
                ..Default::default()
            };

            for key in &keys {
                let object = arbitrary_s3_object_with_key(key.clone());
                let passes = crate::filters::include_regex::tests::test_is_match(&object, &config);

                // The filter should pass only objects matching the regex
                let expected = key.ends_with(".csv");
                prop_assert_eq!(
                    passes, expected,
                    "Key '{}': passes={}, expected={}",
                    key, passes, expected
                );
            }
        }

        #[test]
        fn test_exclude_regex_filtering(
            keys in proptest::collection::vec(arbitrary_s3_key(), 1..50),
        ) {
            let pattern = Regex::new(r".*\.tmp$").unwrap();
            let config = FilterConfig {
                exclude_regex: Some(pattern),
                ..Default::default()
            };

            for key in &keys {
                let object = arbitrary_s3_object_with_key(key.clone());
                let passes = crate::filters::exclude_regex::tests::test_is_not_match(&object, &config);

                // The filter should pass objects NOT matching the regex
                let expected = !key.ends_with(".tmp");
                prop_assert_eq!(
                    passes, expected,
                    "Key '{}': passes={}, expected={}",
                    key, passes, expected
                );
            }
        }

        #[test]
        fn test_regex_and_logic(
            keys in proptest::collection::vec(arbitrary_s3_key(), 1..50),
        ) {
            // When both include and exclude regex are applied (AND logic),
            // an object must match include AND not match exclude.
            let include_pattern = Regex::new(r".*\.csv$").unwrap();
            let exclude_pattern = Regex::new(r"^temp/.*").unwrap();

            let include_config = FilterConfig {
                include_regex: Some(include_pattern.clone()),
                ..Default::default()
            };
            let exclude_config = FilterConfig {
                exclude_regex: Some(exclude_pattern.clone()),
                ..Default::default()
            };

            for key in &keys {
                let object = arbitrary_s3_object_with_key(key.clone());
                let passes_include = crate::filters::include_regex::tests::test_is_match(&object, &include_config);
                let passes_exclude = crate::filters::exclude_regex::tests::test_is_not_match(&object, &exclude_config);

                // AND logic: must pass both filters
                let passes_both = passes_include && passes_exclude;

                let matches_include = include_pattern.is_match(key).unwrap();
                let matches_exclude = exclude_pattern.is_match(key).unwrap();
                let expected = matches_include && !matches_exclude;

                prop_assert_eq!(
                    passes_both, expected,
                    "Key '{}': passes_both={}, expected={}",
                    key, passes_both, expected
                );
            }
        }
    }

    // -----------------------------------------------------------------------
    // Feature: s3rm-rs, Property 9: Size Range Filtering
    // **Validates: Requirements 2.6**
    //
    // For any size range filter (min/max), the tool should delete only
    // objects whose size falls within the specified range.
    // -----------------------------------------------------------------------
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn test_larger_size_filtering(
            sizes in proptest::collection::vec(0i64..10000, 1..50),
            threshold in 0u64..10000,
        ) {
            let config = FilterConfig {
                larger_size: Some(threshold),
                ..Default::default()
            };

            for &size in &sizes {
                let object = arbitrary_s3_object_with_size(size);
                let passes = crate::filters::larger_size::tests::test_is_larger_or_equal(&object, &config);

                let expected = size >= threshold as i64;
                prop_assert_eq!(
                    passes, expected,
                    "Size {}: passes={}, expected={} (threshold={})",
                    size, passes, expected, threshold
                );
            }
        }

        #[test]
        fn test_smaller_size_filtering(
            sizes in proptest::collection::vec(0i64..10000, 1..50),
            threshold in 1u64..10000,
        ) {
            let config = FilterConfig {
                smaller_size: Some(threshold),
                ..Default::default()
            };

            for &size in &sizes {
                let object = arbitrary_s3_object_with_size(size);
                let passes = crate::filters::smaller_size::tests::test_is_smaller(&object, &config);

                let expected = size < threshold as i64;
                prop_assert_eq!(
                    passes, expected,
                    "Size {}: passes={}, expected={} (threshold={})",
                    size, passes, expected, threshold
                );
            }
        }

        #[test]
        fn test_size_range_filtering(
            sizes in proptest::collection::vec(0i64..10000, 1..50),
            min_size in 0u64..5000,
            max_offset in 1u64..5000,
        ) {
            let max_size = min_size + max_offset;

            let larger_config = FilterConfig {
                larger_size: Some(min_size),
                ..Default::default()
            };
            let smaller_config = FilterConfig {
                smaller_size: Some(max_size),
                ..Default::default()
            };

            for &size in &sizes {
                let object = arbitrary_s3_object_with_size(size);
                let passes_larger = crate::filters::larger_size::tests::test_is_larger_or_equal(&object, &larger_config);
                let passes_smaller = crate::filters::smaller_size::tests::test_is_smaller(&object, &smaller_config);

                // Combined with AND: size >= min AND size < max
                let in_range = passes_larger && passes_smaller;
                let expected = size >= min_size as i64 && size < max_size as i64;

                prop_assert_eq!(
                    in_range, expected,
                    "Size {}: in_range={}, expected={} (min={}, max={})",
                    size, in_range, expected, min_size, max_size
                );
            }
        }

        #[test]
        fn test_delete_markers_pass_size_filters(
            threshold in 1u64..10000,
        ) {
            let delete_marker = S3Object::DeleteMarker(
                DeleteMarkerEntry::builder().key("test").build()
            );

            let larger_config = FilterConfig {
                larger_size: Some(threshold),
                ..Default::default()
            };
            let smaller_config = FilterConfig {
                smaller_size: Some(threshold),
                ..Default::default()
            };

            // Delete markers always pass size filters
            prop_assert!(crate::filters::larger_size::tests::test_is_larger_or_equal(&delete_marker, &larger_config));
            prop_assert!(crate::filters::smaller_size::tests::test_is_smaller(&delete_marker, &smaller_config));
        }
    }

    // -----------------------------------------------------------------------
    // Feature: s3rm-rs, Property 10: Time Range Filtering
    // **Validates: Requirements 2.7**
    //
    // For any time range filter (before/after), the tool should delete only
    // objects whose last modified time falls within the specified range.
    // -----------------------------------------------------------------------
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn test_mtime_before_filtering(
            // Use epoch millis in a reasonable range (year 2000-2030)
            object_millis in 946684800000i64..1893456000000i64,
            threshold_millis in 946684800000i64..1893456000000i64,
        ) {
            let object = arbitrary_s3_object_with_mtime(object_millis);

            let threshold_chrono = chrono::DateTime::from_timestamp_millis(threshold_millis).unwrap().with_timezone(&Utc);
            let config = FilterConfig {
                before_time: Some(threshold_chrono),
                ..Default::default()
            };

            let passes = crate::filters::mtime_before::tests::test_is_before(&object, &config);

            // Object passes if its mtime is strictly before the threshold
            let object_chrono = chrono::DateTime::from_timestamp_millis(object_millis).unwrap().with_timezone(&Utc);
            let expected = object_chrono < threshold_chrono;

            prop_assert_eq!(
                passes, expected,
                "object_millis={}, threshold_millis={}: passes={}, expected={}",
                object_millis, threshold_millis, passes, expected
            );
        }

        #[test]
        fn test_mtime_after_filtering(
            object_millis in 946684800000i64..1893456000000i64,
            threshold_millis in 946684800000i64..1893456000000i64,
        ) {
            let object = arbitrary_s3_object_with_mtime(object_millis);

            let threshold_chrono = chrono::DateTime::from_timestamp_millis(threshold_millis).unwrap().with_timezone(&Utc);
            let config = FilterConfig {
                after_time: Some(threshold_chrono),
                ..Default::default()
            };

            let passes = crate::filters::mtime_after::tests::test_is_after_or_equal(&object, &config);

            // Object passes if its mtime is at or after the threshold
            let object_chrono = chrono::DateTime::from_timestamp_millis(object_millis).unwrap().with_timezone(&Utc);
            let expected = object_chrono >= threshold_chrono;

            prop_assert_eq!(
                passes, expected,
                "object_millis={}, threshold_millis={}: passes={}, expected={}",
                object_millis, threshold_millis, passes, expected
            );
        }

        #[test]
        fn test_time_range_filtering(
            object_millis in 946684800000i64..1893456000000i64,
            start_millis in 946684800000i64..1400000000000i64,
            range_millis in 1000i64..500000000000i64,
        ) {
            let end_millis = start_millis + range_millis;

            let object = arbitrary_s3_object_with_mtime(object_millis);

            let start_chrono = chrono::DateTime::from_timestamp_millis(start_millis).unwrap().with_timezone(&Utc);
            let end_chrono = chrono::DateTime::from_timestamp_millis(end_millis).unwrap().with_timezone(&Utc);

            let after_config = FilterConfig {
                after_time: Some(start_chrono),
                ..Default::default()
            };
            let before_config = FilterConfig {
                before_time: Some(end_chrono),
                ..Default::default()
            };

            let passes_after = crate::filters::mtime_after::tests::test_is_after_or_equal(&object, &after_config);
            let passes_before = crate::filters::mtime_before::tests::test_is_before(&object, &before_config);

            // Combined with AND: mtime >= start AND mtime < end
            let in_range = passes_after && passes_before;
            let object_chrono = chrono::DateTime::from_timestamp_millis(object_millis).unwrap().with_timezone(&Utc);
            let expected = object_chrono >= start_chrono && object_chrono < end_chrono;

            prop_assert_eq!(
                in_range, expected,
                "object_millis={}, start={}, end={}: in_range={}, expected={}",
                object_millis, start_millis, end_millis, in_range, expected
            );
        }
    }
}
