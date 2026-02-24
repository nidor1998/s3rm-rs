// Progress indicator adapted from s3sync's `bin/s3sync/cli/indicator.rs`.
//
// Displays real-time deletion progress using indicatif and moving averages.
// Reads DeletionStatistics from an async channel and updates a progress bar.

use std::io;
use std::io::Write;

use async_channel::Receiver;
use indicatif::{HumanBytes, HumanCount, HumanDuration, ProgressBar, ProgressStyle};
use s3rm_rs::types::DeletionStatistics;
use simple_moving_average::{SMA, SumTreeSMA};
use tokio::task::JoinHandle;
use tokio::time::Instant;
use tracing::info;

/// Summary returned by [`show_indicator`] after the stats channel closes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndicatorSummary {
    pub total_delete_count: u64,
    pub total_delete_bytes: u64,
    pub total_error_count: u64,
    pub total_skip_count: u64,
    pub total_warning_count: u64,
}

/// Moving average window in seconds (samples).
const MOVING_AVERAGE_PERIOD_SECS: usize = 10;

/// How often (in seconds) to refresh the progress display.
const REFRESH_INTERVAL: f32 = 1.0;

/// Spawn a background task that reads deletion statistics from the channel
/// and displays progress using indicatif.
///
/// # Arguments
/// - `stats_receiver` - Channel receiver for `DeletionStatistics` events
/// - `show_progress` - Whether to display the live-updating progress line
/// - `show_result` - Whether to display the final summary line
/// - `dry_run` - Whether we're in dry-run mode (suppresses throughput in display)
///
/// The task runs until `stats_receiver` is closed (all senders dropped).
/// Returns a `JoinHandle` that should be awaited after the pipeline finishes.
pub fn show_indicator(
    stats_receiver: Receiver<DeletionStatistics>,
    show_progress: bool,
    show_result: bool,
    dry_run: bool,
) -> JoinHandle<IndicatorSummary> {
    let progress_style = ProgressStyle::with_template("{wide_msg}").unwrap();
    let progress_text = ProgressBar::new(0);
    progress_text.set_style(progress_style);

    tokio::spawn(async move {
        let start_time = Instant::now();

        let mut ma_deleted_count = SumTreeSMA::<_, u64, MOVING_AVERAGE_PERIOD_SECS>::new();

        let mut total_delete_count: u64 = 0;
        let mut total_delete_bytes: u64 = 0;
        let mut total_error_count: u64 = 0;
        let mut total_skip_count: u64 = 0;
        let mut total_warning_count: u64 = 0;

        loop {
            let mut period_count: u64 = 0;

            let period = Instant::now();
            loop {
                while let Ok(stats) = stats_receiver.try_recv() {
                    match stats {
                        DeletionStatistics::DeleteComplete { .. } => {
                            period_count += 1;
                            total_delete_count += 1;
                        }
                        DeletionStatistics::DeleteBytes(size) => {
                            total_delete_bytes += size;
                        }
                        DeletionStatistics::DeleteError { .. } => {
                            total_error_count += 1;
                        }
                        DeletionStatistics::DeleteSkip { .. } => {
                            total_skip_count += 1;
                        }
                        DeletionStatistics::DeleteWarning { .. } => {
                            total_warning_count += 1;
                        }
                    }
                }

                if REFRESH_INTERVAL < period.elapsed().as_secs_f32() {
                    break;
                }

                if stats_receiver.is_closed() {
                    // --- FINAL SUMMARY ---
                    let elapsed = start_time.elapsed();
                    let elapsed_secs_f64 = elapsed.as_secs_f64();

                    let mut objects_per_sec = (total_delete_count as f64 / elapsed_secs_f64) as u64;

                    if elapsed_secs_f64 < REFRESH_INTERVAL as f64 {
                        objects_per_sec = total_delete_count;
                    }
                    if dry_run {
                        objects_per_sec = 0;
                    }

                    info!(
                        message = "deletion summary",
                        deleted_bytes = total_delete_bytes,
                        deleted_objects = total_delete_count,
                        deleted_objects_per_sec = objects_per_sec,
                        skipped = total_skip_count,
                        error = total_error_count,
                        warning = total_warning_count,
                        duration_sec = elapsed_secs_f64,
                    );

                    if show_result {
                        progress_text.set_style(ProgressStyle::with_template("{msg}").unwrap());

                        progress_text.finish_with_message(format!(
                            "deleted {:>3} objects | {:>3} objects/sec,  skipped {} objects,  error {} objects, warning {} objects,  deleted {:>3},  duration {}",
                            total_delete_count,
                            HumanCount(objects_per_sec),
                            total_skip_count,
                            total_error_count,
                            total_warning_count,
                            HumanBytes(total_delete_bytes),
                            HumanDuration(elapsed),
                        ));

                        println!();
                        io::stdout().flush().unwrap()
                    }

                    return IndicatorSummary {
                        total_delete_count,
                        total_delete_bytes,
                        total_error_count,
                        total_skip_count,
                        total_warning_count,
                    };
                }

                tokio::time::sleep(std::time::Duration::from_secs_f32(0.05)).await;
            }

            if !dry_run {
                ma_deleted_count.add_sample(period_count);
            }

            if show_progress {
                progress_text.set_message(format!(
                    "deleted {:>3} objects | {:>3} objects/sec,  skipped {} objects,  error {} objects, warning {} objects,  deleted {:>3}",
                    total_delete_count,
                    HumanCount(ma_deleted_count.get_average()),
                    total_skip_count,
                    total_error_count,
                    total_warning_count,
                    HumanBytes(total_delete_bytes),
                ));
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn show_indicator_empty_channel_completes() {
        let (sender, receiver) = async_channel::unbounded();
        drop(sender); // Close channel immediately

        let handle = show_indicator(receiver, false, false, false);
        let summary = tokio::time::timeout(std::time::Duration::from_secs(5), handle)
            .await
            .expect("indicator should complete within timeout")
            .expect("indicator task should not panic");

        assert_eq!(summary.total_delete_count, 0);
        assert_eq!(summary.total_delete_bytes, 0);
        assert_eq!(summary.total_error_count, 0);
        assert_eq!(summary.total_skip_count, 0);
        assert_eq!(summary.total_warning_count, 0);
    }

    #[tokio::test]
    async fn show_indicator_with_stats_completes() {
        let (sender, receiver) = async_channel::unbounded();

        // Send some stats
        sender
            .send(DeletionStatistics::DeleteComplete {
                key: "test/obj1".to_string(),
            })
            .await
            .unwrap();
        sender
            .send(DeletionStatistics::DeleteBytes(1024))
            .await
            .unwrap();
        sender
            .send(DeletionStatistics::DeleteError {
                key: "test/obj2".to_string(),
            })
            .await
            .unwrap();
        sender
            .send(DeletionStatistics::DeleteSkip {
                key: "test/obj3".to_string(),
            })
            .await
            .unwrap();
        sender
            .send(DeletionStatistics::DeleteWarning {
                key: "test/obj4".to_string(),
            })
            .await
            .unwrap();

        drop(sender); // Close channel

        let handle = show_indicator(receiver, false, false, false);
        let summary = tokio::time::timeout(std::time::Duration::from_secs(5), handle)
            .await
            .expect("indicator should complete within timeout")
            .expect("indicator task should not panic");

        assert_eq!(summary.total_delete_count, 1);
        assert_eq!(summary.total_delete_bytes, 1024);
        assert_eq!(summary.total_error_count, 1);
        assert_eq!(summary.total_skip_count, 1);
        assert_eq!(summary.total_warning_count, 1);
    }

    #[tokio::test]
    async fn show_indicator_dry_run_mode() {
        let (sender, receiver) = async_channel::unbounded();

        sender
            .send(DeletionStatistics::DeleteComplete {
                key: "test/obj1".to_string(),
            })
            .await
            .unwrap();
        sender
            .send(DeletionStatistics::DeleteBytes(2048))
            .await
            .unwrap();

        drop(sender);

        let handle = show_indicator(receiver, false, false, true);
        let summary = tokio::time::timeout(std::time::Duration::from_secs(5), handle)
            .await
            .expect("indicator should complete within timeout")
            .expect("indicator task should not panic");

        assert_eq!(summary.total_delete_count, 1);
        assert_eq!(summary.total_delete_bytes, 2048);
    }

    #[tokio::test]
    async fn show_indicator_with_logging() {
        let (sender, receiver) = async_channel::unbounded();

        sender
            .send(DeletionStatistics::DeleteComplete {
                key: "test/obj1".to_string(),
            })
            .await
            .unwrap();
        sender
            .send(DeletionStatistics::DeleteBytes(512))
            .await
            .unwrap();

        drop(sender);

        let handle = show_indicator(receiver, false, false, false);
        tokio::time::timeout(std::time::Duration::from_secs(5), handle)
            .await
            .expect("indicator should complete within timeout")
            .expect("indicator task should not panic");
    }
}
