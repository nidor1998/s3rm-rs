//! Terminator stage for the deletion pipeline.
//!
//! Reused from s3sync's `pipeline/terminator.rs` with no modifications.
//! The Terminator consumes all remaining objects from the final pipeline
//! stage, draining the channel and allowing upstream stages to complete.

use async_channel::Receiver;
use tracing::debug;

/// Terminal stage that drains the final pipeline output channel.
///
/// Generic over the item type `T` (typically `S3Object`). Simply receives
/// all items until the channel closes, ensuring upstream stages can finish
/// their work without blocking on a full channel.
#[derive(Debug)]
pub struct Terminator<T> {
    receiver: Receiver<T>,
}

impl<T> Terminator<T> {
    pub fn new(receiver: Receiver<T>) -> Self {
        Self { receiver }
    }

    /// Consume all items from the receiver until the channel closes.
    pub async fn terminate(&self) {
        debug!("terminator has started.");
        while self.receiver.recv().await.is_ok() {}
        debug!("terminator has been completed.");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn terminates_when_channel_closed() {
        let (sender, receiver) = async_channel::bounded::<String>(10);

        sender.send("a".to_string()).await.unwrap();
        sender.send("b".to_string()).await.unwrap();
        sender.close();

        let terminator = Terminator::new(receiver);
        terminator.terminate().await;
        // Should complete without hanging
    }

    #[tokio::test]
    async fn terminates_empty_channel() {
        let (sender, receiver) = async_channel::bounded::<u32>(10);
        sender.close();

        let terminator = Terminator::new(receiver);
        terminator.terminate().await;
    }

    #[tokio::test]
    async fn terminates_after_sender_dropped() {
        let (sender, receiver) = async_channel::bounded::<i32>(10);
        sender.send(42).await.unwrap();
        drop(sender);

        let terminator = Terminator::new(receiver);
        terminator.terminate().await;
    }
}
