//! User-defined (Lua callback) filter stage.
//!
//! Adapted from s3sync's `pipeline/user_defined_filter.rs`.
//! Delegates filtering decisions to a registered Lua filter callback.
//!
//! Unlike the simple filters (regex, size, time), this filter:
//! - Calls into the Lua VM (or Rust callback) for each object
//! - Cancels the pipeline on callback errors
//! - Uses `tokio::select!` to watch for cancellation
//!
//! The actual Lua VM and callback infrastructure will be implemented in Task 7.
//! This stage provides the pipeline integration point.

use anyhow::Result;
use tracing::{debug, info};

use crate::stage::{SendResult, Stage};
use crate::types::DeletionStatistics;

pub struct UserDefinedFilter {
    base: Stage,
}

impl UserDefinedFilter {
    pub fn new(base: Stage) -> Self {
        Self { base }
    }

    pub async fn filter(&self) -> Result<()> {
        debug!("user defined filter worker started.");
        self.receive_and_filter().await
    }

    async fn receive_and_filter(&self) -> Result<()> {
        loop {
            tokio::select! {
                recv_result = self.base.receiver.as_ref().unwrap().recv() => {
                    match recv_result {
                        Ok(object) => {
                            // TODO(Task 7): Integrate with Lua VM / FilterManager
                            // For now, the user-defined filter is a pass-through.
                            // When Task 7 implements the Lua integration, this will
                            // call filter_manager.execute_filter(&object).
                            //
                            // Placeholder: check if filter_callback_lua_script is configured.
                            // If no callback is configured, pass all objects through.
                            let need_delete = if self.base.config.filter_callback_lua_script.is_some() {
                                // When Lua is integrated (Task 7), this will call the Lua filter.
                                // For now, pass all objects through as a safe default.
                                true
                            } else {
                                // No user-defined filter configured, pass all objects through
                                true
                            };

                            if !need_delete {
                                self.base
                                    .send_stats(DeletionStatistics::DeleteSkip {
                                        key: object.key().to_string(),
                                    })
                                    .await;
                                continue;
                            }

                            if self.base.send(object).await? == SendResult::Closed {
                                return Ok(());
                            }
                        },
                        Err(_) => {
                            debug!("user defined filter worker has been completed.");
                            break;
                        }
                    }
                },
                _ = self.base.cancellation_token.cancelled() => {
                    info!("user defined filter worker has been cancelled.");
                    return Ok(());
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filters::tests::create_base_helper;
    use crate::types::S3Object;
    use crate::types::token;
    use aws_sdk_s3::types::Object;

    fn init_dummy_tracing_subscriber() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter("dummy=trace")
            .try_init();
    }

    #[tokio::test]
    async fn passes_objects_through() {
        init_dummy_tracing_subscriber();

        let (sender, receiver) = async_channel::bounded::<S3Object>(1000);
        let cancellation_token = token::create_pipeline_cancellation_token();
        let (base, next_stage_receiver) =
            create_base_helper(receiver, cancellation_token.clone()).await;

        let filter = UserDefinedFilter::new(base);
        let object = S3Object::NotVersioning(Object::builder().key("test-key").build());

        sender.send(object).await.unwrap();
        sender.close();

        filter.filter().await.unwrap();

        let received = next_stage_receiver.recv().await.unwrap();
        assert_eq!(received.key(), "test-key");
    }

    #[tokio::test]
    async fn handles_cancellation() {
        init_dummy_tracing_subscriber();

        let (_sender, receiver) = async_channel::bounded::<S3Object>(1000);
        let cancellation_token = token::create_pipeline_cancellation_token();
        let (base, next_stage_receiver) =
            create_base_helper(receiver, cancellation_token.clone()).await;

        let filter = UserDefinedFilter::new(base);

        cancellation_token.cancel();
        filter.filter().await.unwrap();

        assert!(next_stage_receiver.try_recv().is_err());
    }

    #[tokio::test]
    async fn handles_channel_close() {
        init_dummy_tracing_subscriber();

        let (sender, receiver) = async_channel::bounded::<S3Object>(1000);
        let cancellation_token = token::create_pipeline_cancellation_token();
        let (base, _next_stage_receiver) =
            create_base_helper(receiver, cancellation_token.clone()).await;

        let filter = UserDefinedFilter::new(base);

        // Close sender immediately (empty stream)
        sender.close();

        filter.filter().await.unwrap();
    }
}
