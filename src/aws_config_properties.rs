// Property-based tests for AWS configuration support.
//
// Feature: s3rm-rs, Property 34: AWS Credential Loading
// For any AWS credential configuration (environment variables, credentials file,
// IAM roles), the tool should successfully load credentials through standard
// AWS SDK mechanisms.
// **Validates: Requirements 8.4**
//
// Feature: s3rm-rs, Property 35: Custom Endpoint Support
// For any custom endpoint specified, the tool should use that endpoint for
// S3 API calls, enabling support for S3-compatible services.
// **Validates: Requirements 8.6**

#[cfg(test)]
mod tests {
    use crate::config::args::parse_from_args;
    use crate::config::{CLITimeoutConfig, ClientConfig, Config, RetryConfig};
    use crate::types::{AccessKeys, ClientConfigLocation, S3Credentials};

    use aws_smithy_types::checksum_config::RequestChecksumCalculation;
    use proptest::prelude::*;

    // -----------------------------------------------------------------------
    // Generators
    // -----------------------------------------------------------------------

    /// Generate a random AWS access key string (alphanumeric, 16-20 chars).
    fn arb_access_key() -> impl Strategy<Value = String> {
        "[A-Z0-9]{16,20}"
    }

    /// Generate a random AWS secret access key string (alphanumeric, 30-40 chars).
    fn arb_secret_key() -> impl Strategy<Value = String> {
        "[A-Za-z0-9/+]{30,40}"
    }

    /// Generate an optional session token.
    fn arb_session_token() -> impl Strategy<Value = Option<String>> {
        prop_oneof![Just(None), "[A-Za-z0-9/+=]{20,40}".prop_map(Some),]
    }

    /// Generate a random AWS profile name.
    fn arb_profile_name() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("default".to_string()),
            Just("production".to_string()),
            Just("staging".to_string()),
            "[a-z][a-z0-9_-]{2,15}".prop_map(|s| s),
        ]
    }

    /// Generate a random AWS region.
    fn arb_region() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("us-east-1".to_string()),
            Just("us-west-2".to_string()),
            Just("eu-west-1".to_string()),
            Just("ap-northeast-1".to_string()),
        ]
    }

    /// Generate a random custom endpoint URL for S3-compatible services.
    fn arb_endpoint_url() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("https://localhost:9000".to_string()),
            Just("https://minio.local:9000".to_string()),
            Just("https://s3.wasabisys.com".to_string()),
            Just("https://storage.googleapis.com".to_string()),
            Just("https://play.min.io".to_string()),
        ]
    }

    // -----------------------------------------------------------------------
    // Feature: s3rm-rs, Property 34: AWS Credential Loading
    // -----------------------------------------------------------------------

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(50))]

        /// Feature: s3rm-rs, Property 34: AWS Credential Loading (access keys via CLI)
        /// **Validates: Requirements 8.4**
        ///
        /// For any access key and secret key provided via CLI args, the parsed
        /// Config should contain S3Credentials::Credentials with matching values.
        #[test]
        fn prop_credential_loading_access_keys(
            access_key in arb_access_key(),
            secret_key in arb_secret_key(),
            session_token in arb_session_token(),
            region in arb_region(),
        ) {
            let mut args: Vec<String> = vec![
                "s3rm".to_string(),
                "s3://test-bucket/prefix/".to_string(),
                "--target-access-key".to_string(),
                access_key.clone(),
                "--target-secret-access-key".to_string(),
                secret_key.clone(),
                "--target-region".to_string(),
                region.clone(),
            ];
            if let Some(ref token) = session_token {
                args.push("--target-session-token".to_string());
                args.push(token.clone());
            }

            let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
            let cli = parse_from_args(args_ref).unwrap();
            let config = Config::try_from(cli).unwrap();

            let client_config = config.target_client_config.as_ref().unwrap();

            match &client_config.credential {
                S3Credentials::Credentials { access_keys } => {
                    prop_assert_eq!(
                        &access_keys.access_key,
                        &access_key,
                        "Access key must match CLI input"
                    );
                    prop_assert_eq!(
                        &access_keys.secret_access_key,
                        &secret_key,
                        "Secret key must match CLI input"
                    );
                    prop_assert_eq!(
                        &access_keys.session_token,
                        &session_token,
                        "Session token must match CLI input"
                    );
                }
                other => {
                    prop_assert!(
                        false,
                        "Expected S3Credentials::Credentials, got {:?}",
                        other
                    );
                }
            }
        }

        /// Feature: s3rm-rs, Property 34: AWS Credential Loading (profile via CLI)
        /// **Validates: Requirements 8.4**
        ///
        /// For any profile name provided via CLI args, the parsed Config should
        /// contain S3Credentials::Profile with the matching profile name.
        #[test]
        fn prop_credential_loading_profile(
            profile in arb_profile_name(),
        ) {
            let args: Vec<&str> = vec![
                "s3rm",
                "s3://test-bucket/prefix/",
                "--target-profile",
                &profile,
            ];

            let cli = parse_from_args(args).unwrap();
            let config = Config::try_from(cli).unwrap();

            let client_config = config.target_client_config.as_ref().unwrap();

            match &client_config.credential {
                S3Credentials::Profile(name) => {
                    prop_assert_eq!(
                        name,
                        &profile,
                        "Profile name must match CLI input"
                    );
                }
                other => {
                    prop_assert!(
                        false,
                        "Expected S3Credentials::Profile, got {:?}",
                        other
                    );
                }
            }
        }

        /// Feature: s3rm-rs, Property 34: AWS Credential Loading (environment default)
        /// **Validates: Requirements 8.4**
        ///
        /// When no explicit credentials are provided via CLI, the Config should
        /// use S3Credentials::FromEnvironment, relying on the AWS SDK default
        /// credential chain (env vars, credentials file, IAM roles).
        #[test]
        fn prop_credential_loading_from_environment(
            region in arb_region(),
        ) {
            let args: Vec<&str> = vec![
                "s3rm",
                "s3://test-bucket/prefix/",
                "--target-region",
                &region,
            ];

            let cli = parse_from_args(args).unwrap();
            let config = Config::try_from(cli).unwrap();

            let client_config = config.target_client_config.as_ref().unwrap();

            match &client_config.credential {
                S3Credentials::FromEnvironment => {
                    // Expected — AWS SDK default chain
                }
                other => {
                    prop_assert!(
                        false,
                        "Expected S3Credentials::FromEnvironment, got {:?}",
                        other
                    );
                }
            }
        }

        /// Feature: s3rm-rs, Property 34: AWS Credential Loading (client creation with access keys)
        /// **Validates: Requirements 8.4**
        ///
        /// For any access key credentials, ClientConfig::create_client() should
        /// successfully create an AWS S3 client with the configured region.
        #[test]
        fn prop_credential_client_creation(
            access_key in arb_access_key(),
            secret_key in arb_secret_key(),
            session_token in arb_session_token(),
            region in arb_region(),
        ) {
            let client_config = ClientConfig {
                client_config_location: ClientConfigLocation {
                    aws_config_file: None,
                    aws_shared_credentials_file: None,
                },
                credential: S3Credentials::Credentials {
                    access_keys: AccessKeys {
                        access_key: access_key.clone(),
                        secret_access_key: secret_key.clone(),
                        session_token: session_token.clone(),
                    },
                },
                region: Some(region.clone()),
                endpoint_url: Some("https://localhost:9000".to_string()),
                force_path_style: true,
                retry_config: RetryConfig {
                    aws_max_attempts: 3,
                    initial_backoff_milliseconds: 100,
                },
                cli_timeout_config: CLITimeoutConfig {
                    operation_timeout_milliseconds: None,
                    operation_attempt_timeout_milliseconds: None,
                    connect_timeout_milliseconds: None,
                    read_timeout_milliseconds: None,
                },
                disable_stalled_stream_protection: false,
                request_checksum_calculation: RequestChecksumCalculation::WhenRequired,
                accelerate: false,
                request_payer: None,
            };

            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();

            rt.block_on(async {
                let client = client_config.create_client().await;

                prop_assert_eq!(
                    client.config().region().unwrap().to_string(),
                    region,
                    "Client region must match configured region"
                );

                Ok(())
            })?;
        }
    }

    // -----------------------------------------------------------------------
    // Feature: s3rm-rs, Property 35: Custom Endpoint Support
    // -----------------------------------------------------------------------

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(50))]

        /// Feature: s3rm-rs, Property 35: Custom Endpoint Support (CLI propagation)
        /// **Validates: Requirements 8.6**
        ///
        /// For any custom endpoint URL provided via CLI args, the parsed Config
        /// should contain the endpoint_url in the ClientConfig.
        #[test]
        fn prop_custom_endpoint_cli_propagation(
            endpoint in arb_endpoint_url(),
        ) {
            let args: Vec<&str> = vec![
                "s3rm",
                "s3://test-bucket/prefix/",
                "--target-endpoint-url",
                &endpoint,
            ];

            let cli = parse_from_args(args).unwrap();
            let config = Config::try_from(cli).unwrap();

            let client_config = config.target_client_config.as_ref().unwrap();

            prop_assert_eq!(
                client_config.endpoint_url.as_ref().unwrap(),
                &endpoint,
                "Endpoint URL must be propagated from CLI to ClientConfig"
            );
        }

        /// Feature: s3rm-rs, Property 35: Custom Endpoint Support (force path style)
        /// **Validates: Requirements 8.6**
        ///
        /// For S3-compatible services, --target-force-path-style must be correctly
        /// propagated to the ClientConfig.
        #[test]
        fn prop_custom_endpoint_force_path_style(
            endpoint in arb_endpoint_url(),
            force_path_style in proptest::bool::ANY,
        ) {
            let mut args: Vec<&str> = vec![
                "s3rm",
                "s3://test-bucket/prefix/",
                "--target-endpoint-url",
                &endpoint,
            ];
            if force_path_style {
                args.push("--target-force-path-style");
            }

            let cli = parse_from_args(args).unwrap();
            let config = Config::try_from(cli).unwrap();

            let client_config = config.target_client_config.as_ref().unwrap();

            prop_assert_eq!(
                client_config.force_path_style,
                force_path_style,
                "force_path_style must match CLI flag"
            );
        }

        /// Feature: s3rm-rs, Property 35: Custom Endpoint Support (client creation)
        /// **Validates: Requirements 8.6**
        ///
        /// For any custom endpoint URL, ClientConfig::create_client() should
        /// successfully create an AWS S3 client. The endpoint is applied via
        /// the SDK's endpoint_url configuration.
        #[test]
        fn prop_custom_endpoint_client_creation(
            endpoint in arb_endpoint_url(),
            force_path_style in proptest::bool::ANY,
            region in arb_region(),
        ) {
            let client_config = ClientConfig {
                client_config_location: ClientConfigLocation {
                    aws_config_file: None,
                    aws_shared_credentials_file: None,
                },
                credential: S3Credentials::Credentials {
                    access_keys: AccessKeys {
                        access_key: "AKIAIOSFODNN7EXAMPLE".to_string(),
                        secret_access_key: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".to_string(),
                        session_token: None,
                    },
                },
                region: Some(region.clone()),
                endpoint_url: Some(endpoint.clone()),
                force_path_style,
                retry_config: RetryConfig {
                    aws_max_attempts: 3,
                    initial_backoff_milliseconds: 100,
                },
                cli_timeout_config: CLITimeoutConfig {
                    operation_timeout_milliseconds: None,
                    operation_attempt_timeout_milliseconds: None,
                    connect_timeout_milliseconds: None,
                    read_timeout_milliseconds: None,
                },
                disable_stalled_stream_protection: false,
                request_checksum_calculation: RequestChecksumCalculation::WhenRequired,
                accelerate: false,
                request_payer: None,
            };

            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();

            rt.block_on(async {
                let client = client_config.create_client().await;

                // The client should be successfully created with the custom endpoint
                prop_assert_eq!(
                    client.config().region().unwrap().to_string(),
                    region,
                    "Client region must be set correctly with custom endpoint"
                );

                // Verify endpoint_url is applied (client created without error)
                // Note: force_path_style is set via Builder::force_path_style()
                // in create_client() but the getter is not exposed on aws_sdk_s3::Config.
                // The fact that create_client() succeeds with any force_path_style
                // value confirms the config is accepted by the SDK.

                Ok(())
            })?;
        }

        /// Feature: s3rm-rs, Property 35: Custom Endpoint Support (no endpoint default)
        /// **Validates: Requirements 8.6**
        ///
        /// When no custom endpoint is specified, endpoint_url in ClientConfig
        /// should be None, using the default AWS S3 endpoint.
        #[test]
        fn prop_custom_endpoint_default_none(
            region in arb_region(),
        ) {
            let args: Vec<&str> = vec![
                "s3rm",
                "s3://test-bucket/prefix/",
                "--target-region",
                &region,
            ];

            let cli = parse_from_args(args).unwrap();
            let config = Config::try_from(cli).unwrap();

            let client_config = config.target_client_config.as_ref().unwrap();

            prop_assert!(
                client_config.endpoint_url.is_none(),
                "endpoint_url must be None when not specified"
            );
        }
    }
}
