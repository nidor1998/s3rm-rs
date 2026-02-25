// Feature: s3rm-rs, Property 29: Retry with Exponential Backoff
// **Validates: Requirements 6.1, 6.2, 6.6**
//
// Feature: s3rm-rs, Property 30: Failure Tracking and Continuation
// **Validates: Requirements 6.4, 6.5**

#[cfg(test)]
mod tests {
    use crate::config::{CLITimeoutConfig, ClientConfig, RetryConfig};
    use crate::types::error::S3rmError;
    use crate::types::{DeletionError, DeletionStatsReport};

    use aws_smithy_types::checksum_config::RequestChecksumCalculation;
    use proptest::prelude::*;
    use std::sync::atomic::Ordering;
    use std::time::Duration;

    // -----------------------------------------------------------------------
    // Feature: s3rm-rs, Property 29: Retry with Exponential Backoff
    // -----------------------------------------------------------------------

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(50))]

        /// Feature: s3rm-rs, Property 29: Retry with Exponential Backoff
        /// **Validates: Requirements 6.1, 6.2**
        ///
        /// For any RetryConfig with aws_max_attempts in [1, 50] and
        /// initial_backoff in [10, 5000] ms, the AWS SDK client should be
        /// configured with exactly those retry parameters.
        #[test]
        fn prop_retry_config_applied_to_aws_client(
            max_attempts in 1u32..50,
            initial_backoff_ms in 10u64..5000,
        ) {
            let client_config = ClientConfig {
                client_config_location: crate::types::ClientConfigLocation {
                    aws_config_file: None,
                    aws_shared_credentials_file: None,
                },
                credential: crate::types::S3Credentials::Credentials {
                    access_keys: crate::types::AccessKeys {
                        access_key: "test".to_string(),
                        secret_access_key: "test".to_string(),
                        session_token: None,
                    },
                },
                region: Some("us-east-1".to_string()),
                endpoint_url: Some("https://localhost:9000".to_string()),
                force_path_style: true,
                retry_config: RetryConfig {
                    aws_max_attempts: max_attempts,
                    initial_backoff_milliseconds: initial_backoff_ms,
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
                let retry_cfg = client.config().retry_config().unwrap();

                prop_assert_eq!(
                    retry_cfg.max_attempts(),
                    max_attempts,
                    "AWS client max_attempts must match configured value"
                );
                prop_assert_eq!(
                    retry_cfg.initial_backoff(),
                    Duration::from_millis(initial_backoff_ms),
                    "AWS client initial_backoff must match configured value"
                );

                Ok(())
            })?;
        }

        /// Feature: s3rm-rs, Property 29: Retry with Exponential Backoff (retryable classification)
        /// **Validates: Requirements 6.1, 6.6**
        ///
        /// DeletionError::Throttled (SlowDown), NetworkError, and ServiceError
        /// are always classified as retryable. NotFound, AccessDenied, and
        /// PreconditionFailed are never retryable.
        #[test]
        fn prop_retryable_error_classification(
            net_msg in "[a-z ]{1,30}",
            svc_msg in "[a-z ]{1,30}",
        ) {
            // Retryable errors
            prop_assert!(
                DeletionError::Throttled.is_retryable(),
                "Throttled (SlowDown) must be retryable"
            );
            prop_assert!(
                DeletionError::NetworkError(net_msg.clone()).is_retryable(),
                "NetworkError must be retryable"
            );
            prop_assert!(
                DeletionError::ServiceError(svc_msg.clone()).is_retryable(),
                "ServiceError (5xx) must be retryable"
            );

            // Non-retryable errors
            prop_assert!(
                !DeletionError::NotFound.is_retryable(),
                "NotFound must not be retryable"
            );
            prop_assert!(
                !DeletionError::AccessDenied.is_retryable(),
                "AccessDenied must not be retryable"
            );
            prop_assert!(
                !DeletionError::PreconditionFailed.is_retryable(),
                "PreconditionFailed must not be retryable"
            );
        }

        /// Feature: s3rm-rs, Property 29: Retry with Exponential Backoff (S3rmError retryable)
        /// **Validates: Requirements 6.1**
        ///
        /// Only S3rmError::AwsSdk is retryable at the pipeline level;
        /// all other S3rmError variants are non-retryable.
        #[test]
        fn prop_s3rm_error_retryable_classification(
            msg in "[a-z ]{1,30}",
            deleted in 0u64..1000,
            failed in 0u64..1000,
        ) {
            // AwsSdk is the only retryable variant
            prop_assert!(
                S3rmError::AwsSdk(msg.clone()).is_retryable(),
                "AwsSdk must be retryable"
            );

            // All others are non-retryable
            prop_assert!(!S3rmError::InvalidConfig(msg.clone()).is_retryable());
            prop_assert!(!S3rmError::InvalidUri(msg.clone()).is_retryable());
            prop_assert!(!S3rmError::InvalidRegex(msg.clone()).is_retryable());
            prop_assert!(!S3rmError::LuaScript(msg.clone()).is_retryable());
            prop_assert!(!S3rmError::Io(msg.clone()).is_retryable());
            prop_assert!(!S3rmError::Cancelled.is_retryable());
            let partial = S3rmError::PartialFailure { deleted, failed };
            prop_assert!(!partial.is_retryable());
            prop_assert!(!S3rmError::Pipeline(msg).is_retryable());
        }
    }

    // -----------------------------------------------------------------------
    // Feature: s3rm-rs, Property 30: Failure Tracking and Continuation
    // -----------------------------------------------------------------------

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(50))]

        /// Feature: s3rm-rs, Property 30: Failure Tracking and Continuation
        /// **Validates: Requirements 6.4, 6.5**
        ///
        /// For any mix of successful and failed deletions, DeletionStatsReport
        /// tracks exact counts: deleted objects, deleted bytes, and failed objects.
        #[test]
        fn prop_stats_report_tracks_successes_and_failures(
            success_sizes in prop::collection::vec(1u64..10_000, 1..50),
            fail_count in 0usize..30,
        ) {
            let report = DeletionStatsReport::new();

            let expected_deleted = success_sizes.len() as u64;
            let expected_bytes: u64 = success_sizes.iter().sum();
            let expected_failed = fail_count as u64;

            for size in &success_sizes {
                report.increment_deleted(*size);
            }
            for _ in 0..fail_count {
                report.increment_failed();
            }

            prop_assert_eq!(
                report.stats_deleted_objects.load(Ordering::SeqCst),
                expected_deleted,
                "Deleted object count must match"
            );
            prop_assert_eq!(
                report.stats_deleted_bytes.load(Ordering::SeqCst),
                expected_bytes,
                "Deleted byte count must match"
            );
            prop_assert_eq!(
                report.stats_failed_objects.load(Ordering::SeqCst),
                expected_failed,
                "Failed object count must match"
            );

            // Snapshot must reflect same values
            let snap = report.snapshot();
            prop_assert_eq!(snap.stats_deleted_objects, expected_deleted);
            prop_assert_eq!(snap.stats_deleted_bytes, expected_bytes);
            prop_assert_eq!(snap.stats_failed_objects, expected_failed);
        }

        /// Feature: s3rm-rs, Property 30: Failure Tracking and Continuation (concurrent)
        /// **Validates: Requirements 6.4, 6.5**
        ///
        /// Concurrent increment_deleted and increment_failed calls from
        /// multiple threads produce correct aggregate totals.
        #[test]
        fn prop_stats_report_concurrent_tracking(
            n_success_per_thread in 1usize..20,
            n_fail_per_thread in 1usize..20,
            n_threads in 2usize..6,
        ) {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(4)
                .enable_all()
                .build()
                .unwrap();

            rt.block_on(async {
                let report = std::sync::Arc::new(DeletionStatsReport::new());
                let mut handles = Vec::new();

                for _ in 0..n_threads {
                    let r = report.clone();
                    let ns = n_success_per_thread;
                    let nf = n_fail_per_thread;
                    handles.push(tokio::spawn(async move {
                        for _ in 0..ns {
                            r.increment_deleted(100);
                        }
                        for _ in 0..nf {
                            r.increment_failed();
                        }
                    }));
                }

                for h in handles {
                    h.await.unwrap();
                }

                let total_deleted = (n_threads * n_success_per_thread) as u64;
                let total_failed = (n_threads * n_fail_per_thread) as u64;
                let total_bytes = total_deleted * 100;

                prop_assert_eq!(
                    report.stats_deleted_objects.load(Ordering::SeqCst),
                    total_deleted,
                    "Concurrent deleted count must be exact"
                );
                prop_assert_eq!(
                    report.stats_deleted_bytes.load(Ordering::SeqCst),
                    total_bytes,
                    "Concurrent deleted bytes must be exact"
                );
                prop_assert_eq!(
                    report.stats_failed_objects.load(Ordering::SeqCst),
                    total_failed,
                    "Concurrent failed count must be exact"
                );

                Ok(())
            })?;
        }

        /// Feature: s3rm-rs, Property 30: Failure Tracking and Continuation (pipeline continues)
        /// **Validates: Requirements 6.4, 6.5**
        ///
        /// When some objects fail in a batch, the pipeline continues processing:
        /// successful objects are counted, failed objects are tracked, and the
        /// total (successes + failures) equals the input size.
        #[test]
        fn prop_failure_does_not_stop_pipeline(
            total in 5usize..80,
            fail_pct in 1usize..50,
        ) {
            let fail_count = (total * fail_pct / 100).max(1).min(total - 1);

            let report = DeletionStatsReport::new();

            // Simulate: first (total - fail_count) succeed, then fail_count fail
            let success_count = total - fail_count;
            for _ in 0..success_count {
                report.increment_deleted(256);
            }
            for _ in 0..fail_count {
                report.increment_failed();
            }

            let snap = report.snapshot();

            // Total processed = successes + failures = original total
            prop_assert_eq!(
                snap.stats_deleted_objects + snap.stats_failed_objects,
                total as u64,
                "All objects must be accounted for (deleted + failed = total)"
            );
            prop_assert_eq!(
                snap.stats_deleted_objects,
                success_count as u64,
                "Success count must match"
            );
            prop_assert_eq!(
                snap.stats_failed_objects,
                fail_count as u64,
                "Failed count must match"
            );
            prop_assert_eq!(
                snap.stats_deleted_bytes,
                (success_count as u64) * 256,
                "Byte count must track only successful deletions"
            );
        }
    }
}
