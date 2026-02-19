use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Context, Result, anyhow};
use async_channel::{Receiver, Sender};

use crate::config::Config;
use crate::storage::Storage;
use crate::types::token::PipelineCancellationToken;
use crate::types::{DeletionStatistics, S3Object};

/// Result of sending an object to the next stage.
#[derive(Debug, Clone, PartialEq)]
pub enum SendResult {
    Success,
    Closed,
}

/// Shared context passed to each pipeline stage.
///
/// Adapted from s3sync's Stage struct, simplified for deletion-only operations:
/// - Only a single `target` storage (no `source` needed since s3rm-rs doesn't sync)
/// - Channels connect stages: each stage reads from `receiver` and writes to `sender`
/// - The ObjectLister stage has no `receiver` (it's the pipeline entry point)
/// - The Terminator stage has no `sender` (it's the pipeline exit)
///
/// Each stage takes ownership of a `Stage`, consuming it during pipeline construction.
pub struct Stage {
    pub config: Config,
    pub target: Storage,
    pub receiver: Option<Receiver<S3Object>>,
    pub sender: Option<Sender<S3Object>>,
    pub cancellation_token: PipelineCancellationToken,
    pub has_warning: Arc<AtomicBool>,
}

impl Stage {
    pub fn new(
        config: Config,
        target: Storage,
        receiver: Option<Receiver<S3Object>>,
        sender: Option<Sender<S3Object>>,
        cancellation_token: PipelineCancellationToken,
        has_warning: Arc<AtomicBool>,
    ) -> Self {
        Self {
            config,
            target,
            receiver,
            sender,
            cancellation_token,
            has_warning,
        }
    }

    /// Send an object to the next stage via the sender channel.
    ///
    /// Returns `SendResult::Closed` if the downstream channel has been closed
    /// (e.g. due to cancellation), allowing the caller to exit gracefully.
    pub async fn send(&self, object: S3Object) -> Result<SendResult> {
        let result = self
            .sender
            .as_ref()
            .unwrap()
            .send(object)
            .await
            .context("async_channel::Sender::send() failed.");

        if let Err(e) = result {
            return if !self.is_channel_closed() {
                Err(anyhow!(e))
            } else {
                Ok(SendResult::Closed)
            };
        }

        Ok(SendResult::Success)
    }

    /// Check if the sender channel has been closed by the receiver.
    pub fn is_channel_closed(&self) -> bool {
        self.sender.as_ref().unwrap().is_closed()
    }

    /// Send a statistics event through the storage stats channel.
    pub async fn send_stats(&self, stats: DeletionStatistics) {
        let _ = self.target.get_stats_sender().send(stats).await;
    }

    /// Set the warning flag to indicate a non-fatal issue occurred.
    pub fn set_warning(&self) {
        self.has_warning.store(true, Ordering::SeqCst);
    }
}
