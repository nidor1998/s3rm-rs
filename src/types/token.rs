/// A cancellation token used to signal pipeline shutdown.
///
/// This is a type alias for [`tokio_util::sync::CancellationToken`]. Pass the
/// token to [`DeletionPipeline::new`](crate::DeletionPipeline::new) and call
/// [`cancel()`](tokio_util::sync::CancellationToken::cancel) on it to request
/// graceful shutdown of a running pipeline (e.g., in a Ctrl+C handler).
pub type PipelineCancellationToken = tokio_util::sync::CancellationToken;

/// Create a new [`PipelineCancellationToken`].
///
/// # Example
///
/// ```
/// use s3rm_rs::create_pipeline_cancellation_token;
///
/// let token = create_pipeline_cancellation_token();
/// assert!(!token.is_cancelled());
///
/// // Cancel the token (e.g., from a signal handler)
/// token.cancel();
/// assert!(token.is_cancelled());
/// ```
pub fn create_pipeline_cancellation_token() -> PipelineCancellationToken {
    tokio_util::sync::CancellationToken::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_cancellation_token() {
        create_pipeline_cancellation_token();
    }
}
