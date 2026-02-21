// Ctrl+C signal handler adapted from s3sync's `bin/s3sync/cli/ctrl_c_handler/mod.rs`.
//
// Uses tokio::select! to wait for either pipeline cancellation or Ctrl+C signal.

use s3rm_rs::PipelineCancellationToken;
use tokio::task::JoinHandle;
use tokio::{select, signal};
use tracing::{debug, warn};

pub fn spawn_ctrl_c_handler(cancellation_token: PipelineCancellationToken) -> JoinHandle<()> {
    tokio::spawn(async move {
        select! {
            _ = cancellation_token.cancelled() => {
                debug!("cancellation_token canceled.")
            }
            _ = signal::ctrl_c() => {
                warn!("ctrl-c received, shutting down.");
                cancellation_token.cancel();
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    use once_cell::sync::Lazy;
    use s3rm_rs::create_pipeline_cancellation_token;
    use std::sync::Arc;
    use tokio::sync::Semaphore;

    static SEMAPHORE: Lazy<Arc<Semaphore>> = Lazy::new(|| Arc::new(Semaphore::new(1)));

    #[tokio::test]
    async fn ctrl_c_handler_handles_cancellation_token() {
        let _semaphore = SEMAPHORE.clone().acquire_owned().await.unwrap();

        let cancellation_token = create_pipeline_cancellation_token();

        let join_handle = spawn_ctrl_c_handler(cancellation_token.clone());
        cancellation_token.cancel();

        join_handle.await.unwrap();

        assert!(cancellation_token.is_cancelled());
    }
}
