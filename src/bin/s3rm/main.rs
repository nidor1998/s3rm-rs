use anyhow::Result;
use clap::{CommandFactory, Parser};
use clap_complete::generate;
use tracing::{debug, error, trace};

use s3rm_rs::callback::user_defined_event_callback::UserDefinedEventCallback;
use s3rm_rs::callback::user_defined_filter_callback::UserDefinedFilterCallback;
use s3rm_rs::config::Config;
use s3rm_rs::types::event_callback::EventType;
use s3rm_rs::{CLIArgs, DeletionPipeline, create_pipeline_cancellation_token, is_cancelled_error};

mod ctrl_c_handler;
pub mod indicator;
#[cfg(test)]
mod indicator_properties;
mod tracing_init;
pub mod ui_config;

const EXIT_CODE_WARNING: i32 = 3;

/// s3rm - Extremely fast Amazon S3 object deletion tool.
///
/// This binary is a thin wrapper over the s3rm-rs library.
/// All core functionality is implemented in the library crate.
#[cfg_attr(coverage_nightly, coverage(off))]
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

#[cfg_attr(coverage_nightly, coverage(off))]
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

async fn run(mut config: Config) -> Result<()> {
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

    #[allow(unused_assignments)]
    let mut has_warning = false;

    {
        let cancellation_token = create_pipeline_cancellation_token();

        ctrl_c_handler::spawn_ctrl_c_handler(cancellation_token.clone());

        let start_time = tokio::time::Instant::now();
        debug!("deletion pipeline start.");

        let mut pipeline = DeletionPipeline::new(config.clone(), cancellation_token).await;
        let indicator_join_handle = indicator::show_indicator(
            pipeline.get_stats_receiver(),
            ui_config::is_progress_indicator_needed(&config),
            ui_config::is_show_result_needed(&config),
            true, // log_deletion_summary
            config.dry_run,
        );

        pipeline.run().await;
        indicator_join_handle.await?;

        let duration_sec = format!("{:.3}", start_time.elapsed().as_secs_f32());

        if pipeline.has_error() {
            let errors = pipeline.get_errors_and_consume().unwrap();
            for err in &errors {
                if is_cancelled_error(err) {
                    debug!("deletion cancelled by user.");
                    return Ok(());
                }
                error!("{}", err);
            }
            error!(duration_sec = duration_sec, "s3rm failed.");
            return Err(anyhow::anyhow!("s3rm failed."));
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
}
