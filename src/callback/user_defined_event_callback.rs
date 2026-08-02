use std::io::Write;

use crate::types::event_callback::{EventCallback, EventData, EventType};
use async_trait::async_trait;

// This struct represents a user-defined event callback.
// It can be used to implement custom event handling logic, such as logging or monitoring.
pub struct UserDefinedEventCallback {
    pub enable: bool,
}

impl UserDefinedEventCallback {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        // Todo: If you need to enable the callback, set `enable` to `true`
        // Lua scripting event callback is disabled if UserDefinedEventCallback is enabled.
        Self { enable: false }
    }

    pub fn is_enabled(&self) -> bool {
        self.enable
    }
}

#[async_trait]
impl EventCallback for UserDefinedEventCallback {
    // If you want to implement a custom event callback, you can do so by modifying this function.
    // The callbacks are called serially, and the callback function MUST return immediately.
    // If a callback function takes a long time to execute, it may block a whole pipeline.
    async fn on_event(&mut self, event_data: EventData) {
        // Todo: Implement your custom event handling logic here.
        // eprintln! panics when stderr is a closed pipe (`s3rm ... 2>&1 |
        // head`); event reporting is best-effort, like the tracing output,
        // which already ignores a closed stderr.
        match event_data.event_type {
            EventType::PIPELINE_START => {
                let _ = writeln!(std::io::stderr(), "Pipeline started: {event_data:?}");
            }
            EventType::PIPELINE_END => {
                let _ = writeln!(std::io::stderr(), "Pipeline ended: {event_data:?}");
            }
            EventType::DELETE_COMPLETE => {
                let _ = writeln!(std::io::stderr(), "Delete complete: {event_data:?}");
            }
            EventType::DELETE_FAILED => {
                let _ = writeln!(std::io::stderr(), "Delete failed: {event_data:?}");
            }
            EventType::DELETE_FILTERED => {
                let _ = writeln!(std::io::stderr(), "Delete filtered: {event_data:?}");
            }
            EventType::PIPELINE_ERROR => {
                let _ = writeln!(std::io::stderr(), "Pipeline error: {event_data:?}");
            }
            EventType::DELETE_CANCEL => {
                let _ = writeln!(std::io::stderr(), "Delete cancelled: {event_data:?}");
            }
            EventType::STATS_REPORT => {
                let _ = writeln!(std::io::stderr(), "Stats report: {event_data:?}");
            }
            // Currently, all events are captured by above match arms,
            _ => {
                let _ = writeln!(std::io::stderr(), "Other events: {event_data:?}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_creates_disabled_callback() {
        let callback = UserDefinedEventCallback::new();
        assert!(!callback.enable);
        assert!(!callback.is_enabled());
    }

    #[test]
    fn enable_field_controls_is_enabled() {
        let mut callback = UserDefinedEventCallback::new();
        callback.enable = true;
        assert!(callback.is_enabled());
    }
}
