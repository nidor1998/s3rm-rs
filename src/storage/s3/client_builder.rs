use aws_config::meta::region::{ProvideRegion, RegionProviderChain};
use aws_config::retry::RetryConfig;
use aws_config::{BehaviorVersion, ConfigLoader};
use aws_runtime::env_config::file::{EnvConfigFileKind, EnvConfigFiles};
use aws_sdk_s3::Client;
use aws_sdk_s3::config::Builder;
use std::time::Duration;

use crate::config::ClientConfig;
use aws_smithy_runtime_api::client::stalled_stream_protection::StalledStreamProtectionConfig;
use aws_smithy_types::timeout::TimeoutConfig;
use aws_types::SdkConfig;
use aws_types::region::Region;

impl ClientConfig {
    pub async fn create_client(&self) -> Client {
        let mut config_builder = Builder::from(&self.load_sdk_config().await)
            .force_path_style(self.force_path_style)
            .request_checksum_calculation(self.request_checksum_calculation)
            .accelerate(self.accelerate);

        if let Some(timeout_config) = self.build_timeout_config() {
            config_builder = config_builder.timeout_config(timeout_config);
        }

        Client::from_conf(config_builder.build())
    }

    async fn load_sdk_config(&self) -> SdkConfig {
        let config_loader = if self.disable_stalled_stream_protection {
            aws_config::defaults(BehaviorVersion::latest())
                .stalled_stream_protection(StalledStreamProtectionConfig::disabled())
        } else {
            aws_config::defaults(BehaviorVersion::latest())
                .stalled_stream_protection(StalledStreamProtectionConfig::enabled().build())
        };
        let mut config_loader = self
            .load_config_credential(config_loader)
            .region(self.build_region_provider())
            .retry_config(self.build_retry_config());

        if let Some(endpoint_url) = &self.endpoint_url {
            config_loader = config_loader.endpoint_url(endpoint_url);
        };

        config_loader.load().await
    }

    fn load_config_credential(&self, mut config_loader: ConfigLoader) -> ConfigLoader {
        match &self.credential {
            crate::types::S3Credentials::Credentials { access_keys } => {
                let credentials = aws_sdk_s3::config::Credentials::new(
                    access_keys.access_key.to_string(),
                    access_keys.secret_access_key.to_string(),
                    access_keys.session_token.clone(),
                    None,
                    "",
                );
                config_loader = config_loader.credentials_provider(credentials);
            }
            crate::types::S3Credentials::Profile(profile_name) => {
                let mut builder = aws_config::profile::ProfileFileCredentialsProvider::builder();

                let aws_config_file = self.client_config_location.aws_config_file.as_ref();
                let aws_credentials_file = self
                    .client_config_location
                    .aws_shared_credentials_file
                    .as_ref();

                if aws_config_file.is_some() || aws_credentials_file.is_some() {
                    let mut files_builder = EnvConfigFiles::builder();
                    if let Some(path) = aws_config_file {
                        files_builder = files_builder.with_file(EnvConfigFileKind::Config, path);
                    }
                    if let Some(path) = aws_credentials_file {
                        files_builder =
                            files_builder.with_file(EnvConfigFileKind::Credentials, path);
                    }
                    builder = builder.profile_files(files_builder.build());
                }

                config_loader =
                    config_loader.credentials_provider(builder.profile_name(profile_name).build());
            }
            crate::types::S3Credentials::FromEnvironment => {}
        }
        config_loader
    }

    fn build_region_provider(&self) -> Box<dyn ProvideRegion> {
        let mut builder = aws_config::profile::ProfileFileRegionProvider::builder();

        if let crate::types::S3Credentials::Profile(profile_name) = &self.credential {
            let aws_config_file = self.client_config_location.aws_config_file.as_ref();
            let aws_credentials_file = self
                .client_config_location
                .aws_shared_credentials_file
                .as_ref();

            if aws_config_file.is_some() || aws_credentials_file.is_some() {
                let mut files_builder = EnvConfigFiles::builder();
                if let Some(path) = aws_config_file {
                    files_builder = files_builder.with_file(EnvConfigFileKind::Config, path);
                }
                if let Some(path) = aws_credentials_file {
                    files_builder = files_builder.with_file(EnvConfigFileKind::Credentials, path);
                }
                builder = builder.profile_files(files_builder.build());
            }
            builder = builder.profile_name(profile_name)
        }

        let provider_region = if matches!(
            &self.credential,
            crate::types::S3Credentials::FromEnvironment
        ) {
            RegionProviderChain::first_try(self.region.clone().map(Region::new))
                .or_default_provider()
        } else {
            RegionProviderChain::first_try(self.region.clone().map(Region::new))
                .or_else(builder.build())
        };

        Box::new(provider_region)
    }

    fn build_retry_config(&self) -> RetryConfig {
        RetryConfig::standard()
            .with_max_attempts(self.retry_config.aws_max_attempts)
            .with_initial_backoff(std::time::Duration::from_millis(
                self.retry_config.initial_backoff_milliseconds,
            ))
    }

    fn build_timeout_config(&self) -> Option<TimeoutConfig> {
        let operation_timeout = self
            .cli_timeout_config
            .operation_timeout_milliseconds
            .map(Duration::from_millis);
        let operation_attempt_timeout = self
            .cli_timeout_config
            .operation_attempt_timeout_milliseconds
            .map(Duration::from_millis);
        let connect_timeout = self
            .cli_timeout_config
            .connect_timeout_milliseconds
            .map(Duration::from_millis);
        let read_timeout = self
            .cli_timeout_config
            .read_timeout_milliseconds
            .map(Duration::from_millis);

        if operation_timeout.is_none()
            && operation_attempt_timeout.is_none()
            && connect_timeout.is_none()
            && read_timeout.is_none()
        {
            return None;
        }

        let mut builder = TimeoutConfig::builder();

        builder = if let Some(operation_timeout) = operation_timeout {
            builder.operation_timeout(operation_timeout)
        } else {
            builder
        };

        builder = if let Some(operation_attempt_timeout) = operation_attempt_timeout {
            builder.operation_attempt_timeout(operation_attempt_timeout)
        } else {
            builder
        };

        builder = if let Some(connect_timeout) = connect_timeout {
            builder.connect_timeout(connect_timeout)
        } else {
            builder
        };

        builder = if let Some(read_timeout) = read_timeout {
            builder.read_timeout(read_timeout)
        } else {
            builder
        };

        Some(builder.build())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::init_dummy_tracing_subscriber;
    use crate::types::{AccessKeys, ClientConfigLocation};
    use aws_sdk_s3::config::ProvideCredentials;
    use aws_smithy_types::checksum_config::RequestChecksumCalculation;

    #[tokio::test]
    async fn create_client_from_credentials() {
        init_dummy_tracing_subscriber();

        let client_config = ClientConfig {
            client_config_location: ClientConfigLocation {
                aws_config_file: None,
                aws_shared_credentials_file: None,
            },
            credential: crate::types::S3Credentials::Credentials {
                access_keys: AccessKeys {
                    access_key: "my_access_key".to_string(),
                    secret_access_key: "my_secret_access_key".to_string(),
                    session_token: Some("my_session_token".to_string()),
                },
            },
            region: Some("my-region".to_string()),
            endpoint_url: Some("https://my.endpoint.local".to_string()),
            force_path_style: false,
            retry_config: crate::config::RetryConfig {
                aws_max_attempts: 10,
                initial_backoff_milliseconds: 100,
            },
            cli_timeout_config: crate::config::CLITimeoutConfig {
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

        let client = client_config.create_client().await;

        let retry_config = client.config().retry_config().unwrap();
        assert_eq!(retry_config.max_attempts(), 10);
        assert_eq!(
            retry_config.initial_backoff(),
            std::time::Duration::from_millis(100)
        );

        let timeout_config = client.config().timeout_config().unwrap();
        assert!(timeout_config.operation_timeout().is_none());
        assert!(timeout_config.operation_attempt_timeout().is_none());
        assert!(timeout_config.connect_timeout().is_some());
        assert!(timeout_config.read_timeout().is_none());
        assert!(timeout_config.has_timeouts());

        assert_eq!(
            client.config().region().unwrap().to_string(),
            "my-region".to_string()
        );
    }

    #[tokio::test]
    async fn create_client_from_credentials_with_custom_timeouts() {
        init_dummy_tracing_subscriber();

        let client_config = ClientConfig {
            client_config_location: ClientConfigLocation {
                aws_config_file: None,
                aws_shared_credentials_file: None,
            },
            credential: crate::types::S3Credentials::Credentials {
                access_keys: AccessKeys {
                    access_key: "my_access_key".to_string(),
                    secret_access_key: "my_secret_access_key".to_string(),
                    session_token: None,
                },
            },
            region: None,
            endpoint_url: Some("https://my.endpoint.local".to_string()),
            force_path_style: false,
            retry_config: crate::config::RetryConfig {
                aws_max_attempts: 5,
                initial_backoff_milliseconds: 200,
            },
            cli_timeout_config: crate::config::CLITimeoutConfig {
                operation_timeout_milliseconds: Some(1000),
                operation_attempt_timeout_milliseconds: Some(2000),
                connect_timeout_milliseconds: Some(3000),
                read_timeout_milliseconds: Some(4000),
            },
            disable_stalled_stream_protection: false,
            request_checksum_calculation: RequestChecksumCalculation::WhenRequired,
            accelerate: false,
            request_payer: None,
        };

        let client = client_config.create_client().await;

        let retry_config = client.config().retry_config().unwrap();
        assert_eq!(retry_config.max_attempts(), 5);
        assert_eq!(
            retry_config.initial_backoff(),
            std::time::Duration::from_millis(200)
        );

        let timeout_config = client.config().timeout_config().unwrap();
        assert_eq!(
            timeout_config.operation_timeout(),
            Some(Duration::from_millis(1000))
        );
        assert_eq!(
            timeout_config.operation_attempt_timeout(),
            Some(Duration::from_millis(2000))
        );
        assert_eq!(
            timeout_config.connect_timeout(),
            Some(Duration::from_millis(3000))
        );
        assert_eq!(
            timeout_config.read_timeout(),
            Some(Duration::from_millis(4000))
        );
        assert!(timeout_config.has_timeouts());
    }

    #[tokio::test]
    async fn create_client_from_credentials_with_default_region() {
        init_dummy_tracing_subscriber();

        let client_config = ClientConfig {
            client_config_location: ClientConfigLocation {
                aws_config_file: None,
                aws_shared_credentials_file: None,
            },
            credential: crate::types::S3Credentials::Credentials {
                access_keys: AccessKeys {
                    access_key: "my_access_key".to_string(),
                    secret_access_key: "my_secret_access_key".to_string(),
                    session_token: Some("my_session_token".to_string()),
                },
            },
            region: None,
            endpoint_url: Some("https://my.endpoint.local".to_string()),
            force_path_style: false,
            retry_config: crate::config::RetryConfig {
                aws_max_attempts: 10,
                initial_backoff_milliseconds: 100,
            },
            cli_timeout_config: crate::config::CLITimeoutConfig {
                operation_timeout_milliseconds: Some(1000),
                operation_attempt_timeout_milliseconds: Some(2000),
                connect_timeout_milliseconds: Some(3000),
                read_timeout_milliseconds: Some(4000),
            },
            disable_stalled_stream_protection: false,
            request_checksum_calculation: RequestChecksumCalculation::WhenRequired,
            accelerate: false,
            request_payer: None,
        };

        let client = client_config.create_client().await;

        let retry_config = client.config().retry_config().unwrap();
        assert_eq!(retry_config.max_attempts(), 10);
        assert_eq!(
            retry_config.initial_backoff(),
            std::time::Duration::from_millis(100)
        );

        let timeout_config = client.config().timeout_config().unwrap();
        assert_eq!(
            timeout_config.operation_timeout(),
            Some(Duration::from_millis(1000))
        );
        assert_eq!(
            timeout_config.operation_attempt_timeout(),
            Some(Duration::from_millis(2000))
        );
        assert_eq!(
            timeout_config.connect_timeout(),
            Some(Duration::from_millis(3000))
        );
        assert_eq!(
            timeout_config.read_timeout(),
            Some(Duration::from_millis(4000))
        );
        assert!(timeout_config.has_timeouts());
    }

    #[tokio::test]
    async fn create_client_from_custom_profile() {
        init_dummy_tracing_subscriber();

        let client_config = ClientConfig {
            client_config_location: ClientConfigLocation {
                aws_config_file: Some("./test_data/test_config/config".into()),
                aws_shared_credentials_file: Some("./test_data/test_config/credentials".into()),
            },
            credential: crate::types::S3Credentials::Profile("aws".to_string()),
            region: Some("my-region".to_string()),
            endpoint_url: Some("https://my.endpoint.local".to_string()),
            force_path_style: false,
            retry_config: crate::config::RetryConfig {
                aws_max_attempts: 10,
                initial_backoff_milliseconds: 100,
            },
            cli_timeout_config: crate::config::CLITimeoutConfig {
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

        let client = client_config.create_client().await;

        let retry_config = client.config().retry_config().unwrap();
        assert_eq!(retry_config.max_attempts(), 10);
        assert_eq!(
            retry_config.initial_backoff(),
            std::time::Duration::from_millis(100)
        );

        assert_eq!(
            client.config().region().unwrap().to_string(),
            "my-region".to_string()
        );
    }

    #[tokio::test]
    async fn create_client_from_default_profile() {
        init_dummy_tracing_subscriber();

        let client_config = ClientConfig {
            client_config_location: ClientConfigLocation {
                aws_config_file: Some("./test_data/test_config/config".into()),
                aws_shared_credentials_file: Some("./test_data/test_config/credentials".into()),
            },
            credential: crate::types::S3Credentials::Profile("default".to_string()),
            region: None,
            endpoint_url: Some("https://my.endpoint.local".to_string()),
            force_path_style: false,
            retry_config: crate::config::RetryConfig {
                aws_max_attempts: 10,
                initial_backoff_milliseconds: 100,
            },
            cli_timeout_config: crate::config::CLITimeoutConfig {
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

        let client = client_config.create_client().await;

        let retry_config = client.config().retry_config().unwrap();
        assert_eq!(retry_config.max_attempts(), 10);
        assert_eq!(
            retry_config.initial_backoff(),
            std::time::Duration::from_millis(100)
        );

        assert_eq!(
            client.config().region().unwrap().to_string(),
            "us-west-1".to_string()
        );
    }

    #[tokio::test]
    async fn create_client_from_custom_profile_overriding_region() {
        init_dummy_tracing_subscriber();

        let client_config = ClientConfig {
            client_config_location: ClientConfigLocation {
                aws_config_file: Some("./test_data/test_config/config".into()),
                aws_shared_credentials_file: Some("./test_data/test_config/credentials".into()),
            },
            credential: crate::types::S3Credentials::Profile("aws".to_string()),
            region: Some("my-region2".to_string()),
            endpoint_url: Some("https://my.endpoint.local".to_string()),
            force_path_style: false,
            retry_config: crate::config::RetryConfig {
                aws_max_attempts: 10,
                initial_backoff_milliseconds: 100,
            },
            cli_timeout_config: crate::config::CLITimeoutConfig {
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

        let client = client_config.create_client().await;

        let retry_config = client.config().retry_config().unwrap();
        assert_eq!(retry_config.max_attempts(), 10);
        assert_eq!(
            retry_config.initial_backoff(),
            std::time::Duration::from_millis(100)
        );

        assert_eq!(
            client.config().region().unwrap().to_string(),
            "my-region2".to_string()
        );
    }

    #[tokio::test]
    async fn create_client_from_custom_timeout_connect_only() {
        init_dummy_tracing_subscriber();

        let client_config = ClientConfig {
            client_config_location: ClientConfigLocation {
                aws_config_file: Some("./test_data/test_config/config".into()),
                aws_shared_credentials_file: Some("./test_data/test_config/credentials".into()),
            },
            credential: crate::types::S3Credentials::Profile("aws".to_string()),
            region: Some("my-region".to_string()),
            endpoint_url: Some("https://my.endpoint.local".to_string()),
            force_path_style: false,
            retry_config: crate::config::RetryConfig {
                aws_max_attempts: 10,
                initial_backoff_milliseconds: 100,
            },
            cli_timeout_config: crate::config::CLITimeoutConfig {
                operation_timeout_milliseconds: None,
                operation_attempt_timeout_milliseconds: None,
                connect_timeout_milliseconds: Some(1000),
                read_timeout_milliseconds: None,
            },
            disable_stalled_stream_protection: false,
            request_checksum_calculation: RequestChecksumCalculation::WhenRequired,
            accelerate: false,
            request_payer: None,
        };

        let client = client_config.create_client().await;

        let timeout_config = client.config().timeout_config().unwrap();
        assert!(timeout_config.operation_timeout().is_none());
        assert!(timeout_config.operation_attempt_timeout().is_none());
        assert_eq!(
            timeout_config.connect_timeout(),
            Some(Duration::from_millis(1000))
        );
        assert!(timeout_config.read_timeout().is_none());
        assert!(timeout_config.has_timeouts());
    }

    #[tokio::test]
    async fn create_client_from_custom_timeout_operation_only() {
        init_dummy_tracing_subscriber();

        let client_config = ClientConfig {
            client_config_location: ClientConfigLocation {
                aws_config_file: Some("./test_data/test_config/config".into()),
                aws_shared_credentials_file: Some("./test_data/test_config/credentials".into()),
            },
            credential: crate::types::S3Credentials::Profile("aws".to_string()),
            region: Some("my-region".to_string()),
            endpoint_url: Some("https://my.endpoint.local".to_string()),
            force_path_style: false,
            retry_config: crate::config::RetryConfig {
                aws_max_attempts: 10,
                initial_backoff_milliseconds: 100,
            },
            cli_timeout_config: crate::config::CLITimeoutConfig {
                operation_timeout_milliseconds: Some(1000),
                operation_attempt_timeout_milliseconds: None,
                connect_timeout_milliseconds: None,
                read_timeout_milliseconds: None,
            },
            disable_stalled_stream_protection: false,
            request_checksum_calculation: RequestChecksumCalculation::WhenRequired,
            accelerate: false,
            request_payer: None,
        };

        let client = client_config.create_client().await;

        let timeout_config = client.config().timeout_config().unwrap();
        assert_eq!(
            timeout_config.operation_timeout(),
            Some(Duration::from_millis(1000))
        );
        assert!(timeout_config.operation_attempt_timeout().is_none());
        assert!(timeout_config.connect_timeout().is_some());
        assert!(timeout_config.read_timeout().is_none());
        assert!(timeout_config.has_timeouts());
    }

    // ===================================================================
    // Profile file wiring tests — verify that both aws_config_file and
    // aws_shared_credentials_file are passed to both the credentials
    // provider and the region provider.
    // ===================================================================

    /// Helper: build a profile-based ClientConfig with the given file paths.
    fn profile_client_config(
        aws_config_file: Option<&str>,
        aws_shared_credentials_file: Option<&str>,
        profile_name: &str,
        region: Option<&str>,
    ) -> ClientConfig {
        ClientConfig {
            client_config_location: ClientConfigLocation {
                aws_config_file: aws_config_file.map(Into::into),
                aws_shared_credentials_file: aws_shared_credentials_file.map(Into::into),
            },
            credential: crate::types::S3Credentials::Profile(profile_name.to_string()),
            region: region.map(|r| r.to_string()),
            endpoint_url: Some("https://localhost".to_string()),
            force_path_style: false,
            retry_config: crate::config::RetryConfig {
                aws_max_attempts: 1,
                initial_backoff_milliseconds: 100,
            },
            cli_timeout_config: crate::config::CLITimeoutConfig {
                operation_timeout_milliseconds: None,
                operation_attempt_timeout_milliseconds: None,
                connect_timeout_milliseconds: None,
                read_timeout_milliseconds: None,
            },
            disable_stalled_stream_protection: false,
            request_checksum_calculation: RequestChecksumCalculation::WhenRequired,
            accelerate: false,
            request_payer: None,
        }
    }

    /// Both files provided — credentials resolve from credentials file.
    #[tokio::test]
    async fn profile_credentials_resolved_with_both_files() {
        init_dummy_tracing_subscriber();

        let config = profile_client_config(
            Some("./test_data/test_config/config"),
            Some("./test_data/test_config/credentials"),
            "aws",
            None,
        );

        let sdk_config = config.load_sdk_config().await;
        let provider = sdk_config.credentials_provider().unwrap();
        let creds = provider.provide_credentials().await.unwrap();

        assert_eq!(creds.access_key_id(), "my_aws_profile_access_key");
        assert_eq!(
            creds.secret_access_key(),
            "my_aws_profile_secret_access_key"
        );
        assert_eq!(creds.session_token(), Some("my_aws_profile_session_token"));
    }

    /// Both files provided — region resolves from config file.
    #[tokio::test]
    async fn profile_region_resolved_with_both_files() {
        init_dummy_tracing_subscriber();

        let config = profile_client_config(
            Some("./test_data/test_config/config"),
            Some("./test_data/test_config/credentials"),
            "aws",
            None, // no CLI override — should fall back to config file
        );

        let sdk_config = config.load_sdk_config().await;

        assert_eq!(sdk_config.region().unwrap().to_string(), "ap-northeast-1");
    }

    /// Only credentials file — credentials resolve, region uses CLI value.
    #[tokio::test]
    async fn profile_credentials_resolved_with_credentials_file_only() {
        init_dummy_tracing_subscriber();

        let config = profile_client_config(
            None,
            Some("./test_data/test_config/credentials"),
            "aws",
            Some("us-east-1"),
        );

        let sdk_config = config.load_sdk_config().await;
        let provider = sdk_config.credentials_provider().unwrap();
        let creds = provider.provide_credentials().await.unwrap();

        assert_eq!(creds.access_key_id(), "my_aws_profile_access_key");
        assert_eq!(
            creds.secret_access_key(),
            "my_aws_profile_secret_access_key"
        );

        assert_eq!(sdk_config.region().unwrap().to_string(), "us-east-1");
    }

    /// Only config file — credentials defined in the config file are resolved.
    /// This is the primary scenario for the aws_config_file fix: profiles
    /// can define credentials in ~/.aws/config (e.g. for SSO, source_profile,
    /// or direct keys). Before the fix, the credentials provider never
    /// received the config file path, so these credentials would not resolve.
    #[tokio::test]
    async fn profile_credentials_resolved_from_config_file_only() {
        init_dummy_tracing_subscriber();

        let config = profile_client_config(
            Some("./test_data/test_config_creds_in_config/config"),
            None,
            "creds-in-config",
            None,
        );

        let sdk_config = config.load_sdk_config().await;
        let provider = sdk_config.credentials_provider().unwrap();
        let creds = provider.provide_credentials().await.unwrap();

        assert_eq!(creds.access_key_id(), "config_file_access_key");
        assert_eq!(creds.secret_access_key(), "config_file_secret_key");
    }

    /// Only config file — region resolves from config file.
    #[tokio::test]
    async fn profile_region_resolved_from_config_file_only() {
        init_dummy_tracing_subscriber();

        let config = profile_client_config(
            Some("./test_data/test_config_creds_in_config/config"),
            None,
            "creds-in-config",
            None,
        );

        let sdk_config = config.load_sdk_config().await;

        assert_eq!(sdk_config.region().unwrap().to_string(), "eu-central-1");
    }

    /// Both files, credentials in config file (not credentials file) —
    /// verifies the credentials provider searches the config file.
    #[tokio::test]
    async fn profile_credentials_in_config_with_both_files() {
        init_dummy_tracing_subscriber();

        let config = profile_client_config(
            Some("./test_data/test_config_creds_in_config/config"),
            Some("./test_data/test_config_creds_in_config/credentials"),
            "creds-in-config",
            None,
        );

        let sdk_config = config.load_sdk_config().await;
        let provider = sdk_config.credentials_provider().unwrap();
        let creds = provider.provide_credentials().await.unwrap();

        // "creds-in-config" profile only exists in the config file,
        // so credentials must come from there.
        assert_eq!(creds.access_key_id(), "config_file_access_key");
        assert_eq!(creds.secret_access_key(), "config_file_secret_key");
    }

    /// Default profile — both files provided, credentials from credentials file.
    #[tokio::test]
    async fn profile_default_credentials_with_both_files() {
        init_dummy_tracing_subscriber();

        let config = profile_client_config(
            Some("./test_data/test_config/config"),
            Some("./test_data/test_config/credentials"),
            "default",
            None,
        );

        let sdk_config = config.load_sdk_config().await;
        let provider = sdk_config.credentials_provider().unwrap();
        let creds = provider.provide_credentials().await.unwrap();

        assert_eq!(creds.access_key_id(), "my_default_profile_access_key");
        assert_eq!(
            creds.secret_access_key(),
            "my_default_profile_secret_access_key"
        );
        assert_eq!(
            creds.session_token(),
            Some("my_default_profile_session_token")
        );
    }

    /// Default profile — region from config file.
    #[tokio::test]
    async fn profile_default_region_from_config_file() {
        init_dummy_tracing_subscriber();

        let config = profile_client_config(
            Some("./test_data/test_config/config"),
            Some("./test_data/test_config/credentials"),
            "default",
            None,
        );

        let sdk_config = config.load_sdk_config().await;

        assert_eq!(sdk_config.region().unwrap().to_string(), "us-west-1");
    }

    /// CLI region overrides config file region.
    #[tokio::test]
    async fn profile_cli_region_overrides_config_file() {
        init_dummy_tracing_subscriber();

        let config = profile_client_config(
            Some("./test_data/test_config/config"),
            Some("./test_data/test_config/credentials"),
            "aws",
            Some("cli-override-region"),
        );

        let sdk_config = config.load_sdk_config().await;

        assert_eq!(
            sdk_config.region().unwrap().to_string(),
            "cli-override-region"
        );
    }

    #[tokio::test]
    async fn create_client_from_environment() {
        init_dummy_tracing_subscriber();

        let client_config = ClientConfig {
            client_config_location: ClientConfigLocation {
                aws_config_file: None,
                aws_shared_credentials_file: None,
            },
            credential: crate::types::S3Credentials::FromEnvironment,
            region: Some("us-east-1".to_string()),
            endpoint_url: Some("https://my.endpoint.local".to_string()),
            force_path_style: false,
            retry_config: crate::config::RetryConfig {
                aws_max_attempts: 3,
                initial_backoff_milliseconds: 100,
            },
            cli_timeout_config: crate::config::CLITimeoutConfig {
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

        let client = client_config.create_client().await;

        assert_eq!(
            client.config().region().unwrap().to_string(),
            "us-east-1".to_string()
        );
    }
}
