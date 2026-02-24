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
#[cfg_attr(coverage_nightly, coverage(off))]
impl EventCallback for UserDefinedEventCallback {
    // If you want to implement a custom event callback, you can do so by modifying this function.
    // The callbacks are called serially, and the callback function MUST return immediately.
    // If a callback function takes a long time to execute, it may block a whole pipeline.
    #[cfg_attr(coverage_nightly, coverage(off))]
    async fn on_event(&mut self, event_data: EventData) {
        // Todo: Implement your custom event handling logic here.
        match event_data.event_type {
            EventType::PIPELINE_START => {
                println!("Pipeline started: {event_data:?}");
            }
            EventType::PIPELINE_END => {
                println!("Pipeline ended: {event_data:?}");
            }
            EventType::DELETE_COMPLETE => {
                println!("Delete complete: {event_data:?}");
            }
            EventType::DELETE_FAILED => {
                println!("Delete failed: {event_data:?}");
            }
            EventType::DELETE_FILTERED => {
                println!("Delete filtered: {event_data:?}");
            }
            EventType::DELETE_WARNING => {
                println!("Delete warning: {event_data:?}");
            }
            EventType::PIPELINE_ERROR => {
                println!("Pipeline error: {event_data:?}");
            }
            EventType::DELETE_CANCEL => {
                println!("Delete cancelled: {event_data:?}");
            }
            EventType::STATS_REPORT => {
                println!("Stats report: {event_data:?}");
            }
            // Currently, all events are captured by above match arms,
            _ => {
                println!("Other events: {event_data:?}");
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
