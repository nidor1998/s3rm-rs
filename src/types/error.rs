use anyhow::Error;
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum S3rmError {
    #[error("cancelled")]
    Cancelled,
}

pub fn is_cancelled_error(e: &Error) -> bool {
    if let Some(err) = e.downcast_ref::<S3rmError>() {
        return *err == S3rmError::Cancelled;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::anyhow;

    #[test]
    fn is_cancelled_error_test() {
        assert!(is_cancelled_error(&anyhow!(S3rmError::Cancelled)));
    }
}
