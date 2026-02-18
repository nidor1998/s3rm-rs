use anyhow::Error;
use thiserror::Error;

/// Application-level error types for s3rm-rs.
///
/// These represent errors that occur during pipeline operations,
/// configuration, and user interaction. Adapted from s3sync's error
/// pattern with additional deletion-specific variants.
///
/// ## Exit Codes
///
/// Each variant maps to an exit code (via `exit_code()`):
/// - 0: Non-error conditions (Cancelled, DryRun)
/// - 1: General errors (AwsSdk, LuaScript, Io, Pipeline)
/// - 2: Configuration errors (InvalidConfig, InvalidUri, InvalidRegex)
/// - 3: Partial failure (some objects deleted, some failed)
#[derive(Error, Debug, PartialEq)]
pub enum S3rmError {
    /// AWS SDK error (may be retryable based on error type).
    #[error("AWS SDK error: {0}")]
    AwsSdk(String),

    /// Configuration error (non-retryable).
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    /// Invalid S3 URI format.
    #[error("Invalid S3 URI: {0}")]
    InvalidUri(String),

    /// Invalid regex pattern.
    #[error("Invalid regex pattern: {0}")]
    InvalidRegex(String),

    /// Lua script error.
    #[error("Lua script error: {0}")]
    LuaScript(String),

    /// I/O error.
    #[error("I/O error: {0}")]
    Io(String),

    /// Operation cancelled by user.
    #[error("Operation cancelled by user")]
    Cancelled,

    /// Dry-run mode — no deletions performed.
    #[error("Dry-run mode - no deletions performed")]
    DryRun,

    /// Partial failure during deletion.
    #[error("Partial failure: {deleted} deleted, {failed} failed")]
    PartialFailure { deleted: u64, failed: u64 },

    /// General pipeline error.
    #[error("Pipeline error: {0}")]
    Pipeline(String),
}

impl S3rmError {
    /// Get the appropriate process exit code for this error.
    ///
    /// Exit codes follow s3sync conventions:
    /// - 0: Success or non-error conditions (Cancelled, DryRun)
    /// - 1: General error (AwsSdk, LuaScript, Io, Pipeline)
    /// - 2: Invalid arguments/configuration (InvalidConfig, InvalidUri, InvalidRegex)
    /// - 3: Partial failure (some objects deleted, some failed)
    pub fn exit_code(&self) -> i32 {
        match self {
            S3rmError::Cancelled | S3rmError::DryRun => 0,
            S3rmError::InvalidConfig(_)
            | S3rmError::InvalidUri(_)
            | S3rmError::InvalidRegex(_) => 2,
            S3rmError::PartialFailure { .. } => 3,
            _ => 1,
        }
    }

    /// Check if this error is retryable.
    ///
    /// Only AWS SDK errors are considered retryable at the pipeline level.
    /// The actual retry decision is delegated to the AWS SDK's retry policy.
    pub fn is_retryable(&self) -> bool {
        matches!(self, S3rmError::AwsSdk(_))
    }
}

/// Check if an anyhow::Error wraps a cancellation error.
///
/// Reused from s3sync's is_cancelled_error pattern for pipeline
/// cancellation detection via anyhow downcast.
pub fn is_cancelled_error(e: &Error) -> bool {
    if let Some(err) = e.downcast_ref::<S3rmError>() {
        return *err == S3rmError::Cancelled;
    }
    false
}

/// Check if an anyhow::Error wraps a dry-run error.
pub fn is_dry_run_error(e: &Error) -> bool {
    if let Some(err) = e.downcast_ref::<S3rmError>() {
        return *err == S3rmError::DryRun;
    }
    false
}

/// Extract the exit code from an anyhow::Error, defaulting to 1.
pub fn exit_code_from_error(e: &Error) -> i32 {
    if let Some(err) = e.downcast_ref::<S3rmError>() {
        return err.exit_code();
    }
    1
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::anyhow;

    // --- is_cancelled_error tests (existing from Task 2) ---

    #[test]
    fn is_cancelled_error_test() {
        assert!(is_cancelled_error(&anyhow!(S3rmError::Cancelled)));
    }

    #[test]
    fn is_cancelled_error_false_for_other_errors() {
        assert!(!is_cancelled_error(&anyhow!(S3rmError::Pipeline(
            "test".to_string()
        ))));
        assert!(!is_cancelled_error(&anyhow!("generic error")));
    }

    // --- is_dry_run_error tests ---

    #[test]
    fn is_dry_run_error_test() {
        assert!(is_dry_run_error(&anyhow!(S3rmError::DryRun)));
    }

    #[test]
    fn is_dry_run_error_false_for_other_errors() {
        assert!(!is_dry_run_error(&anyhow!(S3rmError::Cancelled)));
    }

    // --- exit_code tests ---

    #[test]
    fn exit_code_cancelled() {
        assert_eq!(S3rmError::Cancelled.exit_code(), 0);
    }

    #[test]
    fn exit_code_dry_run() {
        assert_eq!(S3rmError::DryRun.exit_code(), 0);
    }

    #[test]
    fn exit_code_invalid_config() {
        assert_eq!(
            S3rmError::InvalidConfig("bad".to_string()).exit_code(),
            2
        );
    }

    #[test]
    fn exit_code_invalid_uri() {
        assert_eq!(
            S3rmError::InvalidUri("bad://uri".to_string()).exit_code(),
            2
        );
    }

    #[test]
    fn exit_code_invalid_regex() {
        assert_eq!(
            S3rmError::InvalidRegex("[invalid".to_string()).exit_code(),
            2
        );
    }

    #[test]
    fn exit_code_partial_failure() {
        assert_eq!(
            S3rmError::PartialFailure {
                deleted: 90,
                failed: 10
            }
            .exit_code(),
            3
        );
    }

    #[test]
    fn exit_code_aws_sdk() {
        assert_eq!(
            S3rmError::AwsSdk("service error".to_string()).exit_code(),
            1
        );
    }

    #[test]
    fn exit_code_lua_script() {
        assert_eq!(
            S3rmError::LuaScript("runtime error".to_string()).exit_code(),
            1
        );
    }

    #[test]
    fn exit_code_io() {
        assert_eq!(
            S3rmError::Io("file not found".to_string()).exit_code(),
            1
        );
    }

    #[test]
    fn exit_code_pipeline() {
        assert_eq!(
            S3rmError::Pipeline("stage failed".to_string()).exit_code(),
            1
        );
    }

    // --- is_retryable tests ---

    #[test]
    fn is_retryable_aws_sdk() {
        assert!(S3rmError::AwsSdk("throttled".to_string()).is_retryable());
    }

    #[test]
    fn is_retryable_non_retryable_errors() {
        assert!(!S3rmError::InvalidConfig("bad".to_string()).is_retryable());
        assert!(!S3rmError::InvalidUri("bad".to_string()).is_retryable());
        assert!(!S3rmError::InvalidRegex("bad".to_string()).is_retryable());
        assert!(!S3rmError::LuaScript("bad".to_string()).is_retryable());
        assert!(!S3rmError::Io("bad".to_string()).is_retryable());
        assert!(!S3rmError::Cancelled.is_retryable());
        assert!(!S3rmError::DryRun.is_retryable());
        assert!(
            !S3rmError::PartialFailure {
                deleted: 1,
                failed: 1
            }
            .is_retryable()
        );
        assert!(!S3rmError::Pipeline("bad".to_string()).is_retryable());
    }

    // --- Display tests ---

    #[test]
    fn error_display_messages() {
        assert_eq!(
            S3rmError::AwsSdk("timeout".to_string()).to_string(),
            "AWS SDK error: timeout"
        );
        assert_eq!(
            S3rmError::InvalidConfig("missing region".to_string()).to_string(),
            "Invalid configuration: missing region"
        );
        assert_eq!(
            S3rmError::InvalidUri("bad://".to_string()).to_string(),
            "Invalid S3 URI: bad://"
        );
        assert_eq!(
            S3rmError::InvalidRegex("[".to_string()).to_string(),
            "Invalid regex pattern: ["
        );
        assert_eq!(
            S3rmError::LuaScript("nil access".to_string()).to_string(),
            "Lua script error: nil access"
        );
        assert_eq!(
            S3rmError::Io("permission denied".to_string()).to_string(),
            "I/O error: permission denied"
        );
        assert_eq!(
            S3rmError::Cancelled.to_string(),
            "Operation cancelled by user"
        );
        assert_eq!(
            S3rmError::DryRun.to_string(),
            "Dry-run mode - no deletions performed"
        );
        assert_eq!(
            S3rmError::PartialFailure {
                deleted: 95,
                failed: 5
            }
            .to_string(),
            "Partial failure: 95 deleted, 5 failed"
        );
        assert_eq!(
            S3rmError::Pipeline("channel closed".to_string()).to_string(),
            "Pipeline error: channel closed"
        );
    }

    // --- exit_code_from_error tests ---

    #[test]
    fn exit_code_from_anyhow_s3rm_error() {
        assert_eq!(exit_code_from_error(&anyhow!(S3rmError::Cancelled)), 0);
        assert_eq!(
            exit_code_from_error(&anyhow!(S3rmError::InvalidConfig("x".to_string()))),
            2
        );
        assert_eq!(
            exit_code_from_error(&anyhow!(S3rmError::PartialFailure {
                deleted: 1,
                failed: 1
            })),
            3
        );
        assert_eq!(
            exit_code_from_error(&anyhow!(S3rmError::Pipeline("x".to_string()))),
            1
        );
    }

    #[test]
    fn exit_code_from_generic_anyhow_error() {
        assert_eq!(exit_code_from_error(&anyhow!("unknown error")), 1);
    }
}
