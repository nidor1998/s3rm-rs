// UI configuration helpers adapted from s3sync's `bin/s3sync/cli/ui_config.rs`.
//
// Determines whether to show the progress indicator and result summary
// based on Config settings (quiet mode, verbosity, JSON logging).

use s3rm_rs::config::Config;

/// Whether to show the live-updating progress indicator.
///
/// Returns `false` when:
/// - `show_no_progress` is set (quiet mode)
/// - Verbosity is above Warn (tracing takes over the terminal)
/// - JSON logging is enabled (progress text would corrupt JSON output)
pub fn is_progress_indicator_needed(config: &Config) -> bool {
    if config.show_no_progress {
        return false;
    }

    if config.tracing_config.is_none() {
        return true;
    }

    if log::Level::Warn < config.tracing_config.as_ref().unwrap().tracing_level {
        return false;
    }

    !config.tracing_config.as_ref().unwrap().json_tracing
}

/// Whether to show the final result summary line.
///
/// Returns `false` when:
/// - `show_no_progress` is set (quiet mode)
/// - JSON logging is enabled (result would corrupt JSON output)
pub fn is_show_result_needed(config: &Config) -> bool {
    if config.show_no_progress {
        return false;
    }

    if config.tracing_config.is_none() {
        return true;
    }

    !config.tracing_config.as_ref().unwrap().json_tracing
}

#[cfg(test)]
mod tests {
    use super::*;
    use s3rm_rs::config::TracingConfig;

    fn make_config(show_no_progress: bool, tracing_config: Option<TracingConfig>) -> Config {
        Config {
            target: s3rm_rs::types::StoragePath::S3 {
                bucket: "test-bucket".to_string(),
                prefix: "".to_string(),
            },
            show_no_progress,
            target_client_config: None,
            force_retry_config: s3rm_rs::config::ForceRetryConfig {
                force_retry_count: 3,
                force_retry_interval_milliseconds: 1000,
            },
            tracing_config,
            worker_size: 1,
            warn_as_error: false,
            dry_run: false,
            rate_limit_objects: None,
            max_parallel_listings: 1,
            object_listing_queue_size: 1000,
            max_parallel_listing_max_depth: 10,
            allow_parallel_listings_in_express_one_zone: false,
            filter_config: Default::default(),
            max_keys: 1000,
            auto_complete_shell: None,
            event_callback_lua_script: None,
            filter_callback_lua_script: None,
            allow_lua_os_library: false,
            allow_lua_unsafe_vm: false,
            lua_vm_memory_limit: 0,
            if_match: false,
            max_delete: None,
            filter_manager: Default::default(),
            event_manager: Default::default(),
            batch_size: 1000,
            delete_all_versions: false,
            force: false,
        }
    }

    #[test]
    fn progress_indicator_needed_default() {
        let config = make_config(false, None);
        assert!(is_progress_indicator_needed(&config));
    }

    #[test]
    fn progress_indicator_suppressed_by_quiet_mode() {
        let config = make_config(true, None);
        assert!(!is_progress_indicator_needed(&config));
    }

    #[test]
    fn progress_indicator_suppressed_by_json_tracing() {
        let config = make_config(
            false,
            Some(TracingConfig {
                tracing_level: log::Level::Warn,
                json_tracing: true,
                aws_sdk_tracing: false,
                span_events_tracing: false,
                disable_color_tracing: false,
            }),
        );
        assert!(!is_progress_indicator_needed(&config));
    }

    #[test]
    fn progress_indicator_suppressed_by_high_verbosity() {
        let config = make_config(
            false,
            Some(TracingConfig {
                tracing_level: log::Level::Info,
                json_tracing: false,
                aws_sdk_tracing: false,
                span_events_tracing: false,
                disable_color_tracing: false,
            }),
        );
        assert!(!is_progress_indicator_needed(&config));
    }

    #[test]
    fn progress_indicator_shown_at_warn_level() {
        let config = make_config(
            false,
            Some(TracingConfig {
                tracing_level: log::Level::Warn,
                json_tracing: false,
                aws_sdk_tracing: false,
                span_events_tracing: false,
                disable_color_tracing: false,
            }),
        );
        assert!(is_progress_indicator_needed(&config));
    }

    #[test]
    fn show_result_needed_default() {
        let config = make_config(false, None);
        assert!(is_show_result_needed(&config));
    }

    #[test]
    fn show_result_suppressed_by_quiet_mode() {
        let config = make_config(true, None);
        assert!(!is_show_result_needed(&config));
    }

    #[test]
    fn show_result_suppressed_by_json_tracing() {
        let config = make_config(
            false,
            Some(TracingConfig {
                tracing_level: log::Level::Warn,
                json_tracing: true,
                aws_sdk_tracing: false,
                span_events_tracing: false,
                disable_color_tracing: false,
            }),
        );
        assert!(!is_show_result_needed(&config));
    }

    #[test]
    fn show_result_shown_at_high_verbosity() {
        // Unlike progress indicator, result summary is shown at high verbosity
        let config = make_config(
            false,
            Some(TracingConfig {
                tracing_level: log::Level::Info,
                json_tracing: false,
                aws_sdk_tracing: false,
                span_events_tracing: false,
                disable_color_tracing: false,
            }),
        );
        assert!(is_show_result_needed(&config));
    }
}
