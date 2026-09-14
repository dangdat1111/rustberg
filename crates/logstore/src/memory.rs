use async_trait::async_trait;
use tokio::sync::broadcast;

use rustberg_core::error::RustbergResult;
use rustberg_core::model::LogRecord;
use rustberg_core::traits::LogStore;

/// In-process LogStore backed by a broadcast channel. Useful for local
/// dev/tests and for running the API server without a Kafka cluster —
/// swap in `kafka::KafkaLogStore` (behind the `kafka` feature) for
/// production, matching Amoro's own pluggable LogStore design.
pub struct InMemoryLogStore {
    tx: broadcast::Sender<LogRecord>,
}

impl InMemoryLogStore {
    pub fn new(capacity: usize) -> Self {
        let (tx, _rx) = broadcast::channel(capacity);
        Self { tx }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<LogRecord> {
        self.tx.subscribe()
    }
}

impl Default for InMemoryLogStore {
    fn default() -> Self {
        Self::new(1024)
    }
}

#[async_trait]
impl LogStore for InMemoryLogStore {
    async fn publish(&self, record: LogRecord) -> RustbergResult<()> {
        // No active subscribers is not an error for a log stream.
        let _ = self.tx.send(record);
        Ok(())
    }
}
