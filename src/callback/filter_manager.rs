//! Filter callback manager.
//!
//! Adapted from s3sync's `callback/filter_manager.rs`.
//! Wraps a single `FilterCallback` trait object for use by the
//! `UserDefinedFilter` pipeline stage.

use std::fmt;
use std::sync::Arc;

use anyhow::Result;
use tokio::sync::Mutex;

use crate::types::S3Object;
use crate::types::filter_callback::FilterCallback;

/// Manages a registered filter callback.
///
/// Holds an optional `FilterCallback` behind `Arc<Mutex<...>>` so it can
/// be shared across pipeline stages and called asynchronously.
#[derive(Clone)]
pub struct FilterManager {
    callback: Option<Arc<Mutex<Box<dyn FilterCallback + Send + Sync>>>>,
}

impl Default for FilterManager {
    fn default() -> Self {
        Self::new()
    }
}

impl FilterManager {
    pub fn new() -> Self {
        Self { callback: None }
    }

    /// Register a filter callback implementation.
    pub fn register_callback<T: FilterCallback + Send + Sync + 'static>(&mut self, callback: T) {
        self.callback = Some(Arc::new(Mutex::new(Box::new(callback))));
    }

    /// Returns true if a filter callback has been registered.
    pub fn is_callback_registered(&self) -> bool {
        self.callback.is_some()
    }

    /// Execute the registered filter callback on the given object.
    ///
    /// # Panics
    /// Panics if no callback has been registered. Check `is_callback_registered()` first.
    pub async fn execute_filter(&self, object: &S3Object) -> Result<bool> {
        if let Some(callback) = &self.callback {
            callback.lock().await.filter(object).await
        } else {
            panic!("Filter callback is not registered");
        }
    }
}

impl fmt::Debug for FilterManager {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FilterManager")
            .field("callback_registered", &self.callback.is_some())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    struct AlwaysTrueFilter;

    #[async_trait]
    impl FilterCallback for AlwaysTrueFilter {
        async fn filter(&mut self, _object: &S3Object) -> Result<bool> {
            Ok(true)
        }
    }

    struct AlwaysFalseFilter;

    #[async_trait]
    impl FilterCallback for AlwaysFalseFilter {
        async fn filter(&mut self, _object: &S3Object) -> Result<bool> {
            Ok(false)
        }
    }

    #[tokio::test]
    async fn new_manager_has_no_callback() {
        let manager = FilterManager::new();
        assert!(!manager.is_callback_registered());
    }

    #[tokio::test]
    async fn default_manager_has_no_callback() {
        let manager = FilterManager::default();
        assert!(!manager.is_callback_registered());
    }

    #[tokio::test]
    async fn register_and_execute_true_filter() {
        let mut manager = FilterManager::new();
        manager.register_callback(AlwaysTrueFilter);
        assert!(manager.is_callback_registered());

        let object =
            S3Object::NotVersioning(aws_sdk_s3::types::Object::builder().key("test").build());
        let result = manager.execute_filter(&object).await.unwrap();
        assert!(result);
    }

    #[tokio::test]
    async fn register_and_execute_false_filter() {
        let mut manager = FilterManager::new();
        manager.register_callback(AlwaysFalseFilter);

        let object =
            S3Object::NotVersioning(aws_sdk_s3::types::Object::builder().key("test").build());
        let result = manager.execute_filter(&object).await.unwrap();
        assert!(!result);
    }

    #[tokio::test]
    #[should_panic(expected = "Filter callback is not registered")]
    async fn execute_without_registration_panics() {
        let manager = FilterManager::new();
        let object =
            S3Object::NotVersioning(aws_sdk_s3::types::Object::builder().key("test").build());
        let _ = manager.execute_filter(&object).await;
    }

    #[test]
    fn debug_format() {
        let manager = FilterManager::new();
        let debug = format!("{manager:?}");
        assert!(debug.contains("callback_registered: false"));
    }
}
