use anyhow::Result;
use async_trait::async_trait;

use crate::types::S3Object;
use crate::types::filter_callback::FilterCallback;

// This struct represents a user-defined filter callback.
// It can be used to implement custom filtering logic for objects while listing them.
// This callback is invoked while listing objects in the source.
// So this callback is preferred to be used for filtering objects based on basic properties like key, size, etc.
pub struct UserDefinedFilterCallback {
    pub enable: bool,
}

impl UserDefinedFilterCallback {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        // Todo: If you need to enable the callback, set `enable` to `true`
        // Lua scripting filter callback is disabled if UserDefinedFilterCallback is enabled.
        Self { enable: false }
    }

    pub fn is_enabled(&self) -> bool {
        self.enable
    }
}

#[async_trait]
impl FilterCallback for UserDefinedFilterCallback {
    // If you want to implement a custom filter callback, you can do so by modifying this function.
    // The callbacks are called serially, and the callback function MUST return immediately.
    // If a callback function takes a long time to execute, it may block a whole pipeline.
    // This callback is invoked while listing objects.
    // This function should return false if the object should be filtered out (not deleted)
    // and true if the object should be deleted.
    // If an error occurs, it should be handled gracefully, and the function should return an error, and the pipeline will be cancelled.
    async fn filter(&mut self, _object: &S3Object) -> Result<bool> {
        let _key: &str = _object.key();
        // Todo: Implement your custom filtering logic here.
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_creates_disabled_callback() {
        let callback = UserDefinedFilterCallback::new();
        assert!(!callback.enable);
        assert!(!callback.is_enabled());
    }

    #[test]
    fn enable_field_controls_is_enabled() {
        let mut callback = UserDefinedFilterCallback::new();
        callback.enable = true;
        assert!(callback.is_enabled());
    }
}
