// Property-based tests for event callback invocation.
//
// **Property 32: Event Callback Invocation**
// For any registered event callback (Lua or Rust), the tool should invoke
// the callback for progress updates, errors, and completion events with
// structured event data including event type, object key, status, timestamps,
// and error information when applicable.
// **Validates: Requirements 7.6, 7.7**

#[cfg(test)]
mod tests {
    use crate::callback::event_manager::EventManager;
    use crate::types::event_callback::{EventCallback, EventData, EventType};
    use async_trait::async_trait;
    use proptest::prelude::*;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    /// A test callback that collects all received events.
    struct CollectingCallback {
        events: Arc<Mutex<Vec<EventData>>>,
    }

    impl CollectingCallback {
        fn new() -> (Self, Arc<Mutex<Vec<EventData>>>) {
            let events = Arc::new(Mutex::new(Vec::new()));
            (
                Self {
                    events: events.clone(),
                },
                events,
            )
        }
    }

    #[async_trait]
    impl EventCallback for CollectingCallback {
        async fn on_event(&mut self, event_data: EventData) {
            self.events.lock().await.push(event_data);
        }
    }

    /// Generate a sequence of event types that a typical pipeline might produce.
    fn arb_event_sequence() -> impl Strategy<Value = Vec<(EventType, Option<String>, Option<u64>)>>
    {
        prop::collection::vec(
            prop_oneof![
                Just((EventType::PIPELINE_START, None, None)),
                "[a-z]{1,20}".prop_map(|key| (EventType::DELETE_COMPLETE, Some(key), Some(1024))),
                "[a-z]{1,20}".prop_map(|key| (EventType::DELETE_FAILED, Some(key), None)),
                "[a-z]{1,20}".prop_map(|key| (EventType::DELETE_FILTERED, Some(key), None)),
                "[a-z]{1,20}".prop_map(|key| (EventType::DELETE_WARNING, Some(key), None)),
                Just((EventType::PIPELINE_END, None, None)),
            ],
            1..30,
        )
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(50))]

        /// **Property 32: Event Callback Invocation**
        /// **Validates: Requirements 7.6, 7.7**
        ///
        /// For any registered event callback with ALL_EVENTS flag, every
        /// triggered event is delivered to the callback with correct event_type.
        #[test]
        fn prop_all_events_callback_receives_every_event(
            events in arb_event_sequence(),
        ) {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();

            rt.block_on(async {
                let mut manager = EventManager::new();
                let (callback, collected) = CollectingCallback::new();
                manager.register_callback(EventType::ALL_EVENTS, callback, false);

                // Count how many PIPELINE_END events we'll send
                // (each PIPELINE_END also generates a STATS_REPORT)
                let pipeline_end_count = events.iter()
                    .filter(|(et, _, _)| *et == EventType::PIPELINE_END)
                    .count();

                for (event_type, key, size) in &events {
                    let mut event_data = EventData::new(*event_type);
                    event_data.key = key.clone();
                    event_data.size = *size;
                    manager.trigger_event(event_data).await;
                }

                let received = collected.lock().await;
                // Each event + one STATS_REPORT per PIPELINE_END
                let expected_count = events.len() + pipeline_end_count;
                assert_eq!(received.len(), expected_count,
                    "Expected {} events but got {}",
                    expected_count, received.len());
            });
        }

        /// **Property 32: Event Callback Invocation (structured data)**
        /// **Validates: Requirements 7.7**
        ///
        /// Event callbacks receive structured event data including event type,
        /// object key, size, and error information when applicable.
        #[test]
        fn prop_callback_receives_structured_data(
            key in "[a-z0-9/]{1,50}",
            size in 0u64..1_000_000u64,
        ) {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();

            rt.block_on(async {
                let mut manager = EventManager::new();
                let (callback, collected) = CollectingCallback::new();
                manager.register_callback(EventType::ALL_EVENTS, callback, false);

                let mut event_data = EventData::new(EventType::DELETE_COMPLETE);
                event_data.key = Some(key.clone());
                event_data.size = Some(size);
                event_data.version_id = Some("v1".to_string());
                manager.trigger_event(event_data).await;

                let received = collected.lock().await;
                assert_eq!(received.len(), 1);
                assert_eq!(received[0].event_type, EventType::DELETE_COMPLETE);
                assert_eq!(received[0].key.as_deref(), Some(key.as_str()));
                assert_eq!(received[0].size, Some(size));
                assert_eq!(received[0].version_id.as_deref(), Some("v1"));
            });
        }

        /// **Property 32: Event Callback Invocation (flag filtering)**
        /// **Validates: Requirements 7.6**
        ///
        /// When a callback is registered with specific event flags, only
        /// events matching those flags are delivered.
        #[test]
        fn prop_event_flag_filtering(
            events in arb_event_sequence(),
        ) {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();

            rt.block_on(async {
                let mut manager = EventManager::new();
                let (callback, collected) = CollectingCallback::new();
                // Only register for DELETE_COMPLETE
                manager.register_callback(EventType::DELETE_COMPLETE, callback, false);

                for (event_type, key, size) in &events {
                    let mut event_data = EventData::new(*event_type);
                    event_data.key = key.clone();
                    event_data.size = *size;
                    manager.trigger_event(event_data).await;
                }

                let received = collected.lock().await;
                let expected_count = events.iter()
                    .filter(|(et, _, _)| *et == EventType::DELETE_COMPLETE)
                    .count();
                assert_eq!(received.len(), expected_count);

                for event in received.iter() {
                    assert_eq!(event.event_type, EventType::DELETE_COMPLETE);
                }
            });
        }

        /// **Property 32: Event Callback Invocation (dry_run propagation)**
        /// **Validates: Requirements 7.6**
        ///
        /// When dry_run is set on the EventManager, all dispatched events
        /// have dry_run=true.
        #[test]
        fn prop_dry_run_propagated_to_events(
            events in arb_event_sequence(),
            dry_run in proptest::bool::ANY,
        ) {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();

            rt.block_on(async {
                let mut manager = EventManager::new();
                let (callback, collected) = CollectingCallback::new();
                manager.register_callback(EventType::ALL_EVENTS, callback, dry_run);

                for (event_type, key, size) in &events {
                    let mut event_data = EventData::new(*event_type);
                    event_data.key = key.clone();
                    event_data.size = *size;
                    manager.trigger_event(event_data).await;
                }

                let received = collected.lock().await;
                for event in received.iter() {
                    assert_eq!(event.dry_run, dry_run,
                        "Expected dry_run={} but got dry_run={}",
                        dry_run, event.dry_run);
                }
            });
        }
    }
}
