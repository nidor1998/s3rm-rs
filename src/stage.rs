use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use async_channel::{Receiver, Sender};

use crate::config::Config;
use crate::storage::Storage;
use crate::types::S3Object;
use crate::types::token::PipelineCancellationToken;

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
}
