//! User-defined (Lua callback) filter stage.
//!
//! Adapted from s3sync's `pipeline/user_defined_filter.rs`.
//! Delegates filtering decisions to a registered filter callback via `FilterManager`.
//!
//! Unlike the simple filters (regex, size, time), this filter:
//! - Calls into the Lua VM (or Rust callback) for each object via `FilterManager`
//! - Cancels the pipeline on callback errors
//! - Uses `tokio::select!` to watch for cancellation
//! - Triggers event callbacks when objects are filtered

use anyhow::{Result, anyhow};
use tracing::{debug, error, info};

use crate::stage::{SendResult, Stage};
use crate::types::DeletionStatistics;
use crate::types::event_callback::{EventData, EventType};

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
                            // If a filter callback is registered, execute it
                            let need_delete = if self.base.config.filter_manager.is_callback_registered() {
                                let result = self.base.config.filter_manager.execute_filter(&object).await;

                                if let Err(e) = result {
                                    let error_msg = e.to_string();
                                    // Trigger error event if event callback is registered
                                    if self.base.config.event_manager.is_callback_registered() {
                                        let mut event_data = EventData::new(EventType::PIPELINE_ERROR);
                                        event_data.message = Some(format!("User defined filter error: {error_msg}"));
                                        self.base.config.event_manager.trigger_event(event_data).await;
                                    }

                                    self.base.cancellation_token.cancel();
                                    error!("user defined filter worker cancelled with error: {}", e);
                                    return Err(anyhow!("user defined filter worker cancelled with error: {}", e));
                                }

                                result?
                            } else {
                                // No filter callback registered, pass all objects through
                                true
                            };

                            if !need_delete {
                                // Trigger filtered event if event callback is registered
                                if self.base.config.event_manager.is_callback_registered() {
                                    let mut event_data = EventData::new(EventType::DELETE_FILTERED);
                                    event_data.key = Some(object.key().to_string());
                                    event_data.version_id = object.version_id().map(|v| v.to_string());
                                    event_data.size = Some(object.size() as u64);
                                    event_data.message = Some("Object filtered by user defined filter".to_string());
                                    self.base.config.event_manager.trigger_event(event_data).await;
                                }

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
    async fn passes_objects_through_when_no_callback() {
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

    #[tokio::test]
    async fn filters_with_registered_true_callback() {
        use crate::callback::filter_manager::FilterManager;
        use crate::types::filter_callback::FilterCallback;
        use anyhow::Result;
        use async_trait::async_trait;

        struct PassAllFilter;

        #[async_trait]
        impl FilterCallback for PassAllFilter {
            async fn filter(&mut self, _object: &S3Object) -> Result<bool> {
                Ok(true)
            }
        }

        init_dummy_tracing_subscriber();

        let (sender, receiver) = async_channel::bounded::<S3Object>(1000);
        let cancellation_token = token::create_pipeline_cancellation_token();
        let (mut base, next_stage_receiver) =
            create_base_helper(receiver, cancellation_token.clone()).await;

        // Register a filter that passes everything
        let mut manager = FilterManager::new();
        manager.register_callback(PassAllFilter);
        base.config.filter_manager = manager;

        let filter = UserDefinedFilter::new(base);
        let object = S3Object::NotVersioning(Object::builder().key("pass-me").build());

        sender.send(object).await.unwrap();
        sender.close();

        filter.filter().await.unwrap();

        let received = next_stage_receiver.recv().await.unwrap();
        assert_eq!(received.key(), "pass-me");
    }

    #[tokio::test]
    async fn filters_with_registered_false_callback() {
        use crate::callback::filter_manager::FilterManager;
        use crate::types::filter_callback::FilterCallback;
        use anyhow::Result;
        use async_trait::async_trait;

        struct RejectAllFilter;

        #[async_trait]
        impl FilterCallback for RejectAllFilter {
            async fn filter(&mut self, _object: &S3Object) -> Result<bool> {
                Ok(false)
            }
        }

        init_dummy_tracing_subscriber();

        let (sender, receiver) = async_channel::bounded::<S3Object>(1000);
        let cancellation_token = token::create_pipeline_cancellation_token();
        let (mut base, next_stage_receiver) =
            create_base_helper(receiver, cancellation_token.clone()).await;

        let mut manager = FilterManager::new();
        manager.register_callback(RejectAllFilter);
        base.config.filter_manager = manager;

        let filter = UserDefinedFilter::new(base);
        let object = S3Object::NotVersioning(Object::builder().key("reject-me").build());

        sender.send(object).await.unwrap();
        sender.close();

        filter.filter().await.unwrap();

        // Object should be filtered out, not forwarded
        assert!(next_stage_receiver.try_recv().is_err());
    }

    #[tokio::test]
    async fn filter_error_cancels_pipeline() {
        use crate::callback::filter_manager::FilterManager;
        use crate::types::filter_callback::FilterCallback;
        use anyhow::Result;
        use async_trait::async_trait;

        struct ErrorFilter;

        #[async_trait]
        impl FilterCallback for ErrorFilter {
            async fn filter(&mut self, _object: &S3Object) -> Result<bool> {
                Err(anyhow::anyhow!("filter error"))
            }
        }

        init_dummy_tracing_subscriber();

        let (sender, receiver) = async_channel::bounded::<S3Object>(1000);
        let cancellation_token = token::create_pipeline_cancellation_token();
        let (mut base, _next_stage_receiver) =
            create_base_helper(receiver, cancellation_token.clone()).await;

        let mut manager = FilterManager::new();
        manager.register_callback(ErrorFilter);
        base.config.filter_manager = manager;

        let filter = UserDefinedFilter::new(base);
        let object = S3Object::NotVersioning(Object::builder().key("error-me").build());

        sender.send(object).await.unwrap();
        sender.close();

        let result = filter.filter().await;
        assert!(result.is_err());
        assert!(cancellation_token.is_cancelled());
    }
}
