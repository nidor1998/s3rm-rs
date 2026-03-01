use anyhow::Result;
use clap::{CommandFactory, Parser};
use clap_complete::generate;
use tracing::{debug, error, trace};

use s3rm_rs::callback::user_defined_event_callback::UserDefinedEventCallback;
use s3rm_rs::callback::user_defined_filter_callback::UserDefinedFilterCallback;
use s3rm_rs::config::Config;
use s3rm_rs::types::event_callback::EventType;
use s3rm_rs::{
    CLIArgs, DeletionPipeline, create_pipeline_cancellation_token, exit_code_from_error,
    is_cancelled_error,
};

mod ctrl_c_handler;
pub mod indicator;
#[cfg(test)]
mod indicator_properties;
mod tracing_init;
pub mod ui_config;

const EXIT_CODE_WARNING: i32 = 3;
const EXIT_CODE_ABNORMAL_TERMINATION: i32 = 101;

/// s3rm - Fast Amazon S3 object deletion tool.
///
/// This binary is a thin wrapper over the s3rm-rs library.
/// All core functionality is implemented in the library crate.
#[tokio::main]
async fn main() -> Result<()> {
    let config = load_config_exit_if_err();

    if let Some(shell) = config.auto_complete_shell {
        generate(
            shell,
            &mut CLIArgs::command(),
            "s3rm",
            &mut std::io::stdout(),
        );

        return Ok(());
    }

    start_tracing_if_necessary(&config);

    trace!("config = {:?}", config);

    run(config).await
}

fn load_config_exit_if_err() -> Config {
    let config = Config::try_from(CLIArgs::parse());
    if let Err(error_message) = config {
        clap::Error::raw(clap::error::ErrorKind::ValueValidation, error_message).exit();
    }
    config.unwrap()
}

fn start_tracing_if_necessary(config: &Config) -> bool {
    if config.tracing_config.is_none() {
        return false;
    }

    tracing_init::init_tracing(config.tracing_config.as_ref().unwrap());
    true
}

fn register_user_defined_callbacks(config: &mut Config) {
    // Note: Each type of callback is registered only once.
    // The user-defined event callback is disabled by default.
    let mut user_defined_event_callback = UserDefinedEventCallback::new();
    // This is for testing purpose only.
    if config.test_user_defined_callback {
        user_defined_event_callback.enable = true;
    }
    if user_defined_event_callback.is_enabled() {
        // By default, the user-defined event callback notifies all events.
        // You can modify EventType::ALL_EVENTS to filter specific events
        config.event_manager.register_callback(
            EventType::ALL_EVENTS,
            user_defined_event_callback,
            config.dry_run,
        );
    }

    // The user-defined filter callback is disabled by default.
    // But you can modify the `UserDefinedFilterCallback` to enable it.
    // User-defined filter callback allows us to filter objects while listing them.
    let mut user_defined_filter_callback = UserDefinedFilterCallback::new();
    // This is for testing purpose only.
    if config.test_user_defined_callback {
        user_defined_filter_callback.enable = true;
    }
    if user_defined_filter_callback.is_enabled() {
        config
            .filter_manager
            .register_callback(user_defined_filter_callback);
    }
}

async fn run(mut config: Config) -> Result<()> {
    register_user_defined_callbacks(&mut config);

    #[allow(unused_assignments)]
    let mut has_warning = false;

    {
        let cancellation_token = create_pipeline_cancellation_token();

        ctrl_c_handler::spawn_ctrl_c_handler(cancellation_token.clone());

        let start_time = tokio::time::Instant::now();
        debug!("deletion pipeline start.");

        let mut pipeline = DeletionPipeline::new(config.clone(), cancellation_token).await;

        // Check prerequisites (confirmation prompt) before starting the indicator,
        // so the progress bar doesn't interfere with the prompt.
        if let Err(e) = pipeline.check_prerequisites().await {
            pipeline.close_stats_sender();
            if is_cancelled_error(&e) {
                println!("Deletion cancelled.");
                debug!("deletion cancelled by user.");
                return Ok(());
            }
            let code = exit_code_from_error(&e);
            error!("{}", e);
            std::process::exit(code);
        }

        let indicator_join_handle = indicator::show_indicator(
            pipeline.get_stats_receiver(),
            ui_config::is_progress_indicator_needed(&config),
            ui_config::is_show_result_needed(&config),
            config.dry_run,
        );

        pipeline.run().await;
        match indicator_join_handle.await {
            Ok(_summary) => {}
            Err(e) => {
                error!("indicator task panicked: {}", e);
                std::process::exit(EXIT_CODE_ABNORMAL_TERMINATION);
            }
        }

        let duration_sec = format!("{:.3}", start_time.elapsed().as_secs_f32());

        if pipeline.has_error() {
            if pipeline.has_panic() {
                error!(duration_sec = duration_sec, "s3rm abnormal termination.");
                std::process::exit(EXIT_CODE_ABNORMAL_TERMINATION);
            }
            let errors = pipeline.get_errors_and_consume().unwrap();
            // Use the highest exit code across all errors so that a severe
            // status (e.g. 3 for PartialFailure) is not downgraded by a
            // subsequent generic error (code 1).
            let mut code = 1;
            for err in &errors {
                if is_cancelled_error(err) {
                    debug!("deletion cancelled by user.");
                    return Ok(());
                }
                code = code.max(exit_code_from_error(err));
                error!("{}", err);
            }
            error!(duration_sec = duration_sec, "s3rm failed.");
            std::process::exit(code);
        }

        has_warning = pipeline.has_warning();

        debug!(duration_sec = duration_sec, "s3rm has been completed.");
    }

    if has_warning {
        std::process::exit(EXIT_CODE_WARNING);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_fork::rusty_fork_test;
    use s3rm_rs::config::args::parse_from_args;

    rusty_fork_test! {
        #[test]
        fn with_tracing() {
            let args = vec![
                "s3rm",
                "-v",
                "s3://test-bucket/prefix/",
            ];

            let config = Config::try_from(parse_from_args(args).unwrap()).unwrap();
            assert!(start_tracing_if_necessary(&config));
        }

        #[test]
        fn without_tracing() {
            let args = vec![
                "s3rm",
                "-qq",
                "s3://test-bucket/prefix/",
            ];

            let config = Config::try_from(parse_from_args(args).unwrap()).unwrap();
            assert!(!start_tracing_if_necessary(&config));
        }
    }

    #[test]
    fn register_user_defined_callbacks_enabled() {
        let args = vec!["s3rm", "-f", "s3://test-bucket/prefix/"];
        let mut config = Config::try_from(parse_from_args(args).unwrap()).unwrap();
        config.test_user_defined_callback = true;

        assert!(!config.event_manager.is_callback_registered());
        assert!(!config.filter_manager.is_callback_registered());

        register_user_defined_callbacks(&mut config);

        assert!(
            config.event_manager.is_callback_registered(),
            "Event callback should be registered when test_user_defined_callback is true"
        );
        assert!(
            config.filter_manager.is_callback_registered(),
            "Filter callback should be registered when test_user_defined_callback is true"
        );
    }

    #[test]
    fn register_user_defined_callbacks_disabled_by_default() {
        let args = vec!["s3rm", "-f", "s3://test-bucket/prefix/"];
        let mut config = Config::try_from(parse_from_args(args).unwrap()).unwrap();
        assert!(!config.test_user_defined_callback);

        register_user_defined_callbacks(&mut config);

        assert!(
            !config.event_manager.is_callback_registered(),
            "Event callback should NOT be registered by default"
        );
        assert!(
            !config.filter_manager.is_callback_registered(),
            "Filter callback should NOT be registered by default"
        );
    }

    // ===================================================================
    // Pipeline error tests
    // ===================================================================

    /// When the S3 endpoint is unreachable the pipeline records errors
    /// during the listing phase.
    #[tokio::test]
    async fn pipeline_run_errors_on_unreachable_endpoint() {
        let args = vec![
            "s3rm",
            "-f",
            "--target-access-key",
            "dummy",
            "--target-secret-access-key",
            "dummy",
            "--target-endpoint-url",
            "https://anything.invalid",
            "--aws-config-file",
            "./test_data/test_config/config",
            "--aws-shared-credentials-file",
            "./test_data/test_config/credentials",
            "--connect-timeout-milliseconds",
            "1",
            "--aws-max-attempts",
            "0",
            "s3://test-bucket/prefix/",
        ];

        let config = Config::try_from(parse_from_args(args).unwrap()).unwrap();
        let cancellation_token = create_pipeline_cancellation_token();
        let mut pipeline = DeletionPipeline::new(config, cancellation_token).await;

        pipeline.run().await;

        assert!(
            pipeline.has_error(),
            "Pipeline should have errors when S3 endpoint is unreachable"
        );
        assert_eq!(
            pipeline.get_deletion_stats().stats_deleted_objects,
            0,
            "No objects should be deleted when endpoint is unreachable"
        );
    }

    /// When --force is used with --dry-run and the endpoint is
    /// unreachable the pipeline still records listing errors.
    #[tokio::test]
    async fn pipeline_run_dry_run_errors_on_unreachable_endpoint() {
        let args = vec![
            "s3rm",
            "-f",
            "-d",
            "--target-access-key",
            "dummy",
            "--target-secret-access-key",
            "dummy",
            "--target-endpoint-url",
            "https://anything.invalid",
            "--aws-config-file",
            "./test_data/test_config/config",
            "--aws-shared-credentials-file",
            "./test_data/test_config/credentials",
            "--connect-timeout-milliseconds",
            "1",
            "--aws-max-attempts",
            "0",
            "s3://test-bucket/prefix/",
        ];

        let config = Config::try_from(parse_from_args(args).unwrap()).unwrap();
        assert!(config.dry_run);

        let cancellation_token = create_pipeline_cancellation_token();
        let mut pipeline = DeletionPipeline::new(config, cancellation_token).await;

        pipeline.run().await;

        assert!(
            pipeline.has_error(),
            "Pipeline should have errors even in dry-run when listing fails"
        );
    }
}
