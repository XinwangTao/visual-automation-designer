//! Execution event sinks.
//!
//! The executor used to talk to `tauri::AppHandle` directly. Headless entry
//! points (`vadctl`, `vad-mcp`) have no Tauri application, so event delivery is
//! abstracted behind this small trait. The GUI implements it with a Tauri
//! emitter; CLI callers use their own sink to collect evidence.

use std::sync::Arc;

use super::events::ExecutionEvent;

/// Receives execution events.
///
/// Implementations must be cheap and must not block for long: the executor
/// calls `emit` synchronously on its own task. A sink that needs to capture a
/// screenshot should keep it fast (single monitor capture) or queue the work.
pub trait EventSink: Send + Sync + 'static {
    fn emit(&self, event: &ExecutionEvent);
}

/// Sink that discards every event. Useful for tests and `--quiet` runs.
pub struct NullEventSink;

impl EventSink for NullEventSink {
    fn emit(&self, _event: &ExecutionEvent) {}
}

/// Convenience alias for a shared sink.
pub type SharedEventSink = Arc<dyn EventSink>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::BlockId;
    use std::sync::Mutex;

    struct CollectSink(Mutex<Vec<ExecutionEvent>>);

    impl EventSink for CollectSink {
        fn emit(&self, event: &ExecutionEvent) {
            self.0.lock().unwrap().push(event.clone());
        }
    }

    #[test]
    fn collects_events_in_order() {
        let sink = CollectSink(Mutex::new(Vec::new()));
        let id = BlockId::new();
        sink.emit(&ExecutionEvent::started());
        sink.emit(&ExecutionEvent::block_started(id.clone()));
        let events = sink.0.lock().unwrap();
        assert_eq!(events.len(), 2);
        assert!(matches!(events[1], ExecutionEvent::BlockStarted { .. }));
    }

    #[test]
    fn null_sink_is_noop() {
        NullEventSink.emit(&ExecutionEvent::started());
    }
}
