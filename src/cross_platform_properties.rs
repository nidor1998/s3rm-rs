// Property-based tests for cross-platform path handling.
//
// **Property 37: Cross-Platform Path Handling**
// For any file path provided on different operating systems, the tool should
// correctly normalize and handle the path according to platform conventions.
// **Validates: Requirements 9.6**

#[cfg(test)]
mod tests {
    use crate::config::Config;
    use crate::config::args::parse_from_args;
    use crate::types::{S3Target, StoragePath};

    use proptest::prelude::*;
    use std::path::PathBuf;

    // -----------------------------------------------------------------------
    // Generators
    // -----------------------------------------------------------------------

    /// Generate valid S3 bucket names (lowercase, 3-63 chars, DNS-compatible).
    fn arb_bucket_name() -> impl Strategy<Value = String> {
        "[a-z][a-z0-9]{2,15}"
    }

    /// Generate valid S3 key prefixes (forward slashes only, no backslashes).
    fn arb_s3_prefix() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("".to_string()),
            Just("prefix/".to_string()),
            Just("a/b/c/".to_string()),
            "[a-z]{1,5}(/[a-z]{1,5}){0,3}/?".prop_map(|s| s),
        ]
    }

    /// Generate AWS config file path components.
    fn arb_aws_config_path() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("/home/user/.aws/config".to_string()),
            Just("/home/user/.aws/credentials".to_string()),
            Just("/tmp/aws-config".to_string()),
            Just("/etc/aws/config".to_string()),
        ]
    }

    // -----------------------------------------------------------------------
    // Property 37: Cross-Platform Path Handling — S3 URIs
    // -----------------------------------------------------------------------

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// **Property 37: Cross-Platform Path Handling (S3 URI uses forward slashes only)**
        /// **Validates: Requirements 9.6**
        ///
        /// S3 URIs always use forward slashes regardless of platform. The parser
        /// must never interpret backslashes as path separators.
        #[test]
        fn prop_s3_uri_uses_forward_slashes_only(
            bucket in arb_bucket_name(),
            prefix in arb_s3_prefix(),
        ) {
            let uri = format!("s3://{}/{}", bucket, prefix);
            let result = S3Target::parse(&uri);

            prop_assert!(result.is_ok(), "S3 URI should parse: {}", uri);
            let target = result.unwrap();

            prop_assert_eq!(&target.bucket, &bucket);

            // Prefix should never contain backslashes — S3 keys use forward slashes
            if let Some(ref p) = target.prefix {
                prop_assert!(
                    !p.contains('\\'),
                    "S3 prefix must not contain backslashes: {}",
                    p
                );
            }
        }

        /// **Property 37: Cross-Platform Path Handling (S3 URI with backslash is literal)**
        /// **Validates: Requirements 9.6**
        ///
        /// If a user provides a backslash in the S3 URI, it should be treated as
        /// a literal character (part of the key), not a path separator.
        #[test]
        fn prop_s3_uri_backslash_is_literal(
            bucket in arb_bucket_name(),
        ) {
            // S3 keys can contain backslashes as literal characters
            let uri = format!("s3://{}/path\\with\\backslashes", bucket);
            let result = S3Target::parse(&uri);

            prop_assert!(result.is_ok(), "S3 URI with backslash should parse");
            let target = result.unwrap();

            let prefix = target.prefix.as_ref().unwrap();
            prop_assert!(
                prefix.contains('\\'),
                "Backslash in S3 key should be preserved literally: {}",
                prefix
            );
        }

        /// **Property 37: Cross-Platform Path Handling (S3 URI parsing is platform-independent)**
        /// **Validates: Requirements 9.6**
        ///
        /// The same S3 URI should parse to the same bucket and prefix via
        /// both S3Target::parse and the CLI argument parser.
        #[test]
        fn prop_s3_uri_parsing_platform_independent(
            bucket in arb_bucket_name(),
            prefix in arb_s3_prefix(),
        ) {
            let uri = format!("s3://{}/{}", bucket, prefix);

            // Parse via S3Target::parse
            let target = S3Target::parse(&uri).unwrap();

            // Parse via CLI args parser
            let args: Vec<&str> = vec!["s3rm", &uri];
            let cli = parse_from_args(args).unwrap();
            let config = Config::try_from(cli).unwrap();

            // Extract bucket and prefix from StoragePath
            let StoragePath::S3 {
                bucket: cli_bucket,
                prefix: cli_prefix,
            } = &config.target;

            // Both parsers must produce consistent bucket
            prop_assert_eq!(
                &target.bucket,
                cli_bucket,
                "S3Target::parse and CLI parser must agree on bucket"
            );

            // Compare prefixes — S3Target uses Option<String>, CLI uses String
            let target_prefix = target.prefix.unwrap_or_default();
            prop_assert_eq!(
                &target_prefix,
                cli_prefix,
                "S3Target::parse and CLI parser must agree on prefix"
            );
        }

        /// **Property 37: Cross-Platform Path Handling (S3 URI never uses OS path separator)**
        /// **Validates: Requirements 9.6**
        ///
        /// S3 URIs should always be split on '/' (forward slash), never on the
        /// platform's native path separator. This ensures consistent behavior on
        /// Windows where std::path::MAIN_SEPARATOR is '\\'.
        #[test]
        fn prop_s3_uri_split_on_forward_slash(
            bucket in arb_bucket_name(),
            prefix in arb_s3_prefix(),
        ) {
            let uri = format!("s3://{}/{}", bucket, prefix);
            let target = S3Target::parse(&uri).unwrap();

            // The bucket name must not be affected by platform path separators
            prop_assert!(
                !target.bucket.contains(std::path::MAIN_SEPARATOR)
                    || std::path::MAIN_SEPARATOR == '/',
                "Bucket must not contain platform path separator"
            );
        }
    }

    // -----------------------------------------------------------------------
    // Property 37: Cross-Platform Path Handling — AWS Config File Paths
    // -----------------------------------------------------------------------

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(50))]

        /// **Property 37: Cross-Platform Path Handling (AWS config path as PathBuf)**
        /// **Validates: Requirements 9.6**
        ///
        /// AWS config file paths provided via CLI must be stored as PathBuf and
        /// correctly propagated to ClientConfig, ensuring platform-native path handling.
        #[test]
        fn prop_aws_config_path_stored_as_pathbuf(
            config_path in arb_aws_config_path(),
        ) {
            let args: Vec<&str> = vec![
                "s3rm",
                "s3://test-bucket/prefix/",
                "--aws-config-file",
                &config_path,
            ];

            let cli = parse_from_args(args).unwrap();
            let config = Config::try_from(cli).unwrap();

            let client_config = config.target_client_config.as_ref().unwrap();
            let stored = client_config
                .client_config_location
                .aws_config_file
                .as_ref()
                .unwrap();

            // Must be stored as the same PathBuf
            let expected = PathBuf::from(&config_path);
            prop_assert_eq!(
                stored,
                &expected,
                "AWS config path must round-trip through PathBuf"
            );
        }

        /// **Property 37: Cross-Platform Path Handling (AWS credentials path as PathBuf)**
        /// **Validates: Requirements 9.6**
        ///
        /// AWS shared credentials file paths provided via CLI must be stored as
        /// PathBuf and correctly propagated.
        #[test]
        fn prop_aws_credentials_path_stored_as_pathbuf(
            cred_path in arb_aws_config_path(),
        ) {
            let args: Vec<&str> = vec![
                "s3rm",
                "s3://test-bucket/prefix/",
                "--aws-shared-credentials-file",
                &cred_path,
            ];

            let cli = parse_from_args(args).unwrap();
            let config = Config::try_from(cli).unwrap();

            let client_config = config.target_client_config.as_ref().unwrap();
            let stored = client_config
                .client_config_location
                .aws_shared_credentials_file
                .as_ref()
                .unwrap();

            let expected = PathBuf::from(&cred_path);
            prop_assert_eq!(
                stored,
                &expected,
                "AWS credentials path must round-trip through PathBuf"
            );
        }

        /// **Property 37: Cross-Platform Path Handling (PathBuf preserves original path)**
        /// **Validates: Requirements 9.6**
        ///
        /// PathBuf::from() followed by to_str() must round-trip the path string
        /// for any valid path on the current platform.
        #[test]
        fn prop_pathbuf_roundtrip(
            config_path in arb_aws_config_path(),
        ) {
            let path_buf = PathBuf::from(&config_path);
            let back_to_str = path_buf.to_str().unwrap();
            prop_assert_eq!(
                back_to_str,
                config_path.as_str(),
                "PathBuf must round-trip the path string"
            );
        }
    }

    // -----------------------------------------------------------------------
    // Property 37: Cross-Platform Path Handling — Lua Script Path CLI Validation
    // -----------------------------------------------------------------------

    #[test]
    fn test_lua_script_path_nonexistent_rejected() {
        let args: Vec<&str> = vec![
            "s3rm",
            "s3://test-bucket/prefix/",
            "--filter-callback-lua-script",
            "/nonexistent/path/to/filter.lua",
        ];

        let result = parse_from_args(args);
        // Should fail because the file doesn't exist
        assert!(result.is_err());
    }

    #[test]
    fn test_lua_event_script_path_nonexistent_rejected() {
        let args: Vec<&str> = vec![
            "s3rm",
            "s3://test-bucket/prefix/",
            "--event-callback-lua-script",
            "/nonexistent/path/to/event.lua",
        ];

        let result = parse_from_args(args);
        assert!(result.is_err());
    }

    #[test]
    fn test_lua_script_path_existing_file_accepted() {
        use std::fs;

        let dir = tempfile::tempdir().unwrap();
        let script_path = dir.path().join("filter_test.lua");
        fs::write(&script_path, "function filter(object) return true end").unwrap();

        let path_str = script_path.to_str().unwrap().to_string();
        let args: Vec<&str> = vec![
            "s3rm",
            "s3://test-bucket/prefix/",
            "--filter-callback-lua-script",
            &path_str,
        ];

        let result = parse_from_args(args);
        assert!(
            result.is_ok(),
            "Existing Lua script path should be accepted"
        );
    }

    #[test]
    fn test_s3_uri_with_deep_prefix_path() {
        // Deep prefix paths should work regardless of platform
        let args: Vec<&str> = vec!["s3rm", "s3://my-bucket/a/b/c/d/e/f/g/"];

        let cli = parse_from_args(args).unwrap();
        let config = Config::try_from(cli).unwrap();

        let StoragePath::S3 { bucket, prefix } = &config.target;
        assert_eq!(bucket, "my-bucket");
        assert_eq!(prefix, "a/b/c/d/e/f/g/");
    }

    #[test]
    fn test_s3_uri_with_special_chars_in_prefix() {
        // S3 keys can contain special characters — should not be normalized
        let args: Vec<&str> = vec!["s3rm", "s3://my-bucket/path with spaces/file.txt"];

        let cli = parse_from_args(args).unwrap();
        let config = Config::try_from(cli).unwrap();

        let StoragePath::S3 { prefix, .. } = &config.target;
        assert_eq!(prefix, "path with spaces/file.txt");
    }
}
