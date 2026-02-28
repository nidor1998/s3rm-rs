//! Property-based tests for the KeepLatestOnlyFilter.
//!
//! Validates that the filter correctly retains latest versions and
//! passes through non-latest versions for deletion across all S3Object variants.

#[cfg(test)]
mod tests {
    use crate::config::FilterConfig;
    use crate::types::S3Object;
    use aws_sdk_s3::primitives::DateTime;
    use aws_sdk_s3::types::{DeleteMarkerEntry, Object, ObjectVersion};
    use proptest::prelude::*;

    // --- Generators ---

    fn arbitrary_key() -> impl Strategy<Value = String> {
        proptest::collection::vec("[a-z0-9]{1,8}", 1..=3).prop_map(|segments| segments.join("/"))
    }

    fn arbitrary_version_id() -> impl Strategy<Value = String> {
        "[a-zA-Z0-9]{16,32}"
    }

    /// Generate a versioned S3Object (ObjectVersion) with configurable is_latest.
    fn arbitrary_versioned_object(key: String, version_id: String, is_latest: bool) -> S3Object {
        S3Object::Versioning(
            ObjectVersion::builder()
                .key(key)
                .version_id(version_id)
                .is_latest(is_latest)
                .size(100)
                .last_modified(DateTime::from_secs(1_700_000_000))
                .build(),
        )
    }

    /// Generate a delete marker S3Object with configurable is_latest.
    fn arbitrary_delete_marker(key: String, version_id: String, is_latest: bool) -> S3Object {
        S3Object::DeleteMarker(
            DeleteMarkerEntry::builder()
                .key(key)
                .version_id(version_id)
                .is_latest(is_latest)
                .build(),
        )
    }

    /// Generate a versioned S3Object (ObjectVersion) with is_latest omitted (None).
    fn arbitrary_versioned_object_none_is_latest(key: String, version_id: String) -> S3Object {
        S3Object::Versioning(
            ObjectVersion::builder()
                .key(key)
                .version_id(version_id)
                .size(100)
                .last_modified(DateTime::from_secs(1_700_000_000))
                .build(),
        )
    }

    /// Generate a delete marker S3Object with is_latest omitted (None).
    fn arbitrary_delete_marker_none_is_latest(key: String, version_id: String) -> S3Object {
        S3Object::DeleteMarker(
            DeleteMarkerEntry::builder()
                .key(key)
                .version_id(version_id)
                .build(),
        )
    }

    /// Generate a non-versioned S3Object.
    fn arbitrary_non_versioned_object(key: String) -> S3Object {
        S3Object::NotVersioning(
            Object::builder()
                .key(key)
                .size(100)
                .last_modified(DateTime::from_secs(1_700_000_000))
                .build(),
        )
    }

    fn keep_latest_only_config() -> FilterConfig {
        FilterConfig {
            keep_latest_only: true,
            ..Default::default()
        }
    }

    // -----------------------------------------------------------------------
    // Property: Latest versioned objects are always filtered out (kept).
    // -----------------------------------------------------------------------
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn latest_versioned_objects_are_always_filtered_out(
            key in arbitrary_key(),
            version_id in arbitrary_version_id(),
        ) {
            let config = keep_latest_only_config();
            let object = arbitrary_versioned_object(key, version_id, true);

            let passes = crate::filters::keep_latest_only::tests::test_is_not_latest(&object, &config);

            // Latest versions must NOT pass (they are kept, not deleted)
            prop_assert!(!passes, "Latest versioned object should be filtered out (kept)");
        }

        // -----------------------------------------------------------------------
        // Property: Non-latest versioned objects always pass through (deleted).
        // -----------------------------------------------------------------------
        #[test]
        fn non_latest_versioned_objects_always_pass_through(
            key in arbitrary_key(),
            version_id in arbitrary_version_id(),
        ) {
            let config = keep_latest_only_config();
            let object = arbitrary_versioned_object(key, version_id, false);

            let passes = crate::filters::keep_latest_only::tests::test_is_not_latest(&object, &config);

            // Non-latest versions must pass (they are deleted)
            prop_assert!(passes, "Non-latest versioned object should pass through (deleted)");
        }

        // -----------------------------------------------------------------------
        // Property: Latest delete markers are always filtered out (kept).
        // -----------------------------------------------------------------------
        #[test]
        fn latest_delete_markers_are_always_filtered_out(
            key in arbitrary_key(),
            version_id in arbitrary_version_id(),
        ) {
            let config = keep_latest_only_config();
            let object = arbitrary_delete_marker(key, version_id, true);

            let passes = crate::filters::keep_latest_only::tests::test_is_not_latest(&object, &config);

            // Latest delete markers must NOT pass (they are kept)
            prop_assert!(!passes, "Latest delete marker should be filtered out (kept)");
        }

        // -----------------------------------------------------------------------
        // Property: Non-latest delete markers always pass through (deleted).
        // -----------------------------------------------------------------------
        #[test]
        fn non_latest_delete_markers_always_pass_through(
            key in arbitrary_key(),
            version_id in arbitrary_version_id(),
        ) {
            let config = keep_latest_only_config();
            let object = arbitrary_delete_marker(key, version_id, false);

            let passes = crate::filters::keep_latest_only::tests::test_is_not_latest(&object, &config);

            // Non-latest delete markers must pass (they are deleted)
            prop_assert!(passes, "Non-latest delete marker should pass through (deleted)");
        }

        // -----------------------------------------------------------------------
        // Property: Non-versioned objects are always filtered out (kept).
        //
        // Defensive: non-versioned objects are treated as "latest" and kept.
        // In normal operation, NotVersioning objects never appear because
        // --keep-latest-only requires --delete-all-versions (list_object_versions).
        // This guard prevents accidental deletion if a bug introduces them.
        // -----------------------------------------------------------------------
        #[test]
        fn non_versioned_objects_are_always_filtered_out(
            key in arbitrary_key(),
        ) {
            let config = keep_latest_only_config();
            let object = arbitrary_non_versioned_object(key);

            let passes = crate::filters::keep_latest_only::tests::test_is_not_latest(&object, &config);

            prop_assert!(!passes, "Non-versioned object should be filtered out (kept)");
        }

        // -----------------------------------------------------------------------
        // Property: Versioned objects with is_latest=None are always kept.
        //
        // Defensive: if S3 omits the is_latest field, the object must not be
        // deleted. Only explicit Some(false) is treated as deletable.
        // -----------------------------------------------------------------------
        #[test]
        fn versioned_objects_with_none_is_latest_are_always_kept(
            key in arbitrary_key(),
            version_id in arbitrary_version_id(),
        ) {
            let config = keep_latest_only_config();
            let object = arbitrary_versioned_object_none_is_latest(key, version_id);

            let passes = crate::filters::keep_latest_only::tests::test_is_not_latest(&object, &config);

            prop_assert!(!passes, "Versioned object with is_latest=None should be filtered out (kept)");
        }

        // -----------------------------------------------------------------------
        // Property: Delete markers with is_latest=None are always kept.
        // -----------------------------------------------------------------------
        #[test]
        fn delete_markers_with_none_is_latest_are_always_kept(
            key in arbitrary_key(),
            version_id in arbitrary_version_id(),
        ) {
            let config = keep_latest_only_config();
            let object = arbitrary_delete_marker_none_is_latest(key, version_id);

            let passes = crate::filters::keep_latest_only::tests::test_is_not_latest(&object, &config);

            prop_assert!(!passes, "Delete marker with is_latest=None should be filtered out (kept)");
        }

        // -----------------------------------------------------------------------
        // Property: For any key, exactly the is_latest flag determines the
        // filter outcome — the key content is irrelevant.
        // -----------------------------------------------------------------------
        #[test]
        fn filter_depends_only_on_is_latest_not_key(
            key in arbitrary_key(),
            version_id in arbitrary_version_id(),
            is_latest in proptest::bool::ANY,
        ) {
            let config = keep_latest_only_config();
            let object = arbitrary_versioned_object(key, version_id, is_latest);

            let passes = crate::filters::keep_latest_only::tests::test_is_not_latest(&object, &config);

            // passes == !is_latest: filter keeps latest, deletes non-latest
            prop_assert_eq!(passes, !is_latest,
                "Filter result should be the inverse of is_latest");
        }

        // -----------------------------------------------------------------------
        // Property: Mixed collection of all S3Object variants is correctly
        // partitioned — only explicit Some(false) objects pass through.
        // -----------------------------------------------------------------------
        #[test]
        fn mixed_variants_are_correctly_partitioned(
            key in arbitrary_key(),
            version_id in arbitrary_version_id(),
            is_latest in proptest::bool::ANY,
        ) {
            let config = keep_latest_only_config();

            let objects = vec![
                arbitrary_versioned_object(key.clone(), version_id.clone(), is_latest),
                arbitrary_delete_marker(key.clone(), version_id.clone(), is_latest),
                arbitrary_non_versioned_object(key.clone()),
                arbitrary_versioned_object_none_is_latest(key.clone(), version_id.clone()),
                arbitrary_delete_marker_none_is_latest(key, version_id),
            ];

            for object in &objects {
                let passes = crate::filters::keep_latest_only::tests::test_is_not_latest(object, &config);
                let expected = match object {
                    S3Object::Versioning(v) => v.is_latest() == Some(false),
                    S3Object::DeleteMarker(dm) => dm.is_latest() == Some(false),
                    S3Object::NotVersioning(_) => false,
                };
                prop_assert_eq!(passes, expected,
                    "Filter result mismatch for {:?} variant",
                    match object {
                        S3Object::Versioning(_) => "Versioning",
                        S3Object::DeleteMarker(_) => "DeleteMarker",
                        S3Object::NotVersioning(_) => "NotVersioning",
                    });
            }
        }
    }
}
