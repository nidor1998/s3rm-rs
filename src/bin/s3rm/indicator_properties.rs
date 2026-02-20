// Property-based tests for the progress indicator.
//
// **Property 31: Progress Reporting**
// For any deletion operation in progress, the tool should display progress
// information including objects deleted, unless quiet mode is enabled.
// **Validates: Requirements 7.1, 7.3, 7.4**

#[cfg(test)]
mod tests {
    use crate::indicator::show_indicator;
    use proptest::prelude::*;
    use s3rm_rs::types::DeletionStatistics;
    use std::time::Duration;

    /// Generate a random sequence of DeletionStatistics events.
    fn arb_stats_sequence() -> impl Strategy<Value = Vec<DeletionStatistics>> {
        prop::collection::vec(
            prop_oneof![
                any::<u16>().prop_map(|_| {
                    DeletionStatistics::DeleteComplete {
                        key: "key".to_string(),
                    }
                }),
                (1u64..=10_000u64).prop_map(DeletionStatistics::DeleteBytes),
                any::<u16>().prop_map(|_| {
                    DeletionStatistics::DeleteError {
                        key: "err".to_string(),
                    }
                }),
                any::<u16>().prop_map(|_| {
                    DeletionStatistics::DeleteSkip {
                        key: "skip".to_string(),
                    }
                }),
                any::<u16>().prop_map(|_| {
                    DeletionStatistics::DeleteWarning {
                        key: "warn".to_string(),
                    }
                }),
            ],
            0..50,
        )
    }

    // -- Property 31: Progress Reporting --

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(50))]

        /// **Property 31: Progress Reporting**
        /// **Validates: Requirements 7.1, 7.3, 7.4**
        ///
        /// The show_indicator task MUST complete (not hang) for any sequence
        /// of DeletionStatistics events once the channel is closed.
        /// This validates requirements 7.1 (progress indicator) and 7.3 (summary).
        #[test]
        fn prop_indicator_completes_for_any_stats(
            stats in arb_stats_sequence(),
            show_progress in proptest::bool::ANY,
            show_result in proptest::bool::ANY,
        ) {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();

            rt.block_on(async {
                let (sender, receiver) = async_channel::unbounded();

                for s in &stats {
                    let stat = match s {
                        DeletionStatistics::DeleteComplete { key } => {
                            DeletionStatistics::DeleteComplete { key: key.clone() }
                        }
                        DeletionStatistics::DeleteBytes(b) => {
                            DeletionStatistics::DeleteBytes(*b)
                        }
                        DeletionStatistics::DeleteError { key } => {
                            DeletionStatistics::DeleteError { key: key.clone() }
                        }
                        DeletionStatistics::DeleteSkip { key } => {
                            DeletionStatistics::DeleteSkip { key: key.clone() }
                        }
                        DeletionStatistics::DeleteWarning { key } => {
                            DeletionStatistics::DeleteWarning { key: key.clone() }
                        }
                    };
                    sender.send(stat).await.unwrap();
                }

                drop(sender); // Close channel to trigger summary

                let handle = show_indicator(
                    receiver,
                    show_progress,
                    show_result,
                    false, // log_deletion_summary
                    false, // dry_run
                );

                // Must complete within 5 seconds
                tokio::time::timeout(Duration::from_secs(5), handle)
                    .await
                    .expect("indicator should complete within timeout")
                    .expect("indicator task should not panic");
            });
        }

        /// **Property 31: Progress Reporting (quiet mode)**
        /// **Validates: Requirements 7.4**
        ///
        /// When both show_progress and show_result are false (quiet mode),
        /// the indicator still processes stats correctly and completes.
        #[test]
        fn prop_indicator_quiet_mode_completes(
            stats in arb_stats_sequence(),
        ) {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();

            rt.block_on(async {
                let (sender, receiver) = async_channel::unbounded();

                for s in &stats {
                    let stat = match s {
                        DeletionStatistics::DeleteComplete { key } => {
                            DeletionStatistics::DeleteComplete { key: key.clone() }
                        }
                        DeletionStatistics::DeleteBytes(b) => {
                            DeletionStatistics::DeleteBytes(*b)
                        }
                        DeletionStatistics::DeleteError { key } => {
                            DeletionStatistics::DeleteError { key: key.clone() }
                        }
                        DeletionStatistics::DeleteSkip { key } => {
                            DeletionStatistics::DeleteSkip { key: key.clone() }
                        }
                        DeletionStatistics::DeleteWarning { key } => {
                            DeletionStatistics::DeleteWarning { key: key.clone() }
                        }
                    };
                    sender.send(stat).await.unwrap();
                }

                drop(sender);

                let handle = show_indicator(receiver, false, false, false, false);

                tokio::time::timeout(Duration::from_secs(5), handle)
                    .await
                    .expect("indicator should complete within timeout")
                    .expect("indicator task should not panic");
            });
        }

        /// **Property 31: Progress Reporting (dry-run)**
        /// **Validates: Requirements 7.1**
        ///
        /// In dry-run mode, the indicator still completes correctly.
        /// Throughput counters are zeroed in dry-run mode.
        #[test]
        fn prop_indicator_dry_run_completes(
            stats in arb_stats_sequence(),
        ) {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();

            rt.block_on(async {
                let (sender, receiver) = async_channel::unbounded();

                for s in &stats {
                    let stat = match s {
                        DeletionStatistics::DeleteComplete { key } => {
                            DeletionStatistics::DeleteComplete { key: key.clone() }
                        }
                        DeletionStatistics::DeleteBytes(b) => {
                            DeletionStatistics::DeleteBytes(*b)
                        }
                        DeletionStatistics::DeleteError { key } => {
                            DeletionStatistics::DeleteError { key: key.clone() }
                        }
                        DeletionStatistics::DeleteSkip { key } => {
                            DeletionStatistics::DeleteSkip { key: key.clone() }
                        }
                        DeletionStatistics::DeleteWarning { key } => {
                            DeletionStatistics::DeleteWarning { key: key.clone() }
                        }
                    };
                    sender.send(stat).await.unwrap();
                }

                drop(sender);

                let handle = show_indicator(
                    receiver,
                    false,
                    false,
                    false,
                    true, // dry_run = true
                );

                tokio::time::timeout(Duration::from_secs(5), handle)
                    .await
                    .expect("indicator should complete within timeout")
                    .expect("indicator task should not panic");
            });
        }
    }
}
