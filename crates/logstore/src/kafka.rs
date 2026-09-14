//! Kafka-backed LogStore, compiled only with `--features kafka` since it
//! links against `librdkafka` (a C library — needs `libsasl2-dev`,
//! `libssl-dev`, `cmake` on the build host). This is the Rust analogue of
//! Amoro's Kafka/Pulsar LogStore plugin.

use async_trait::async_trait;
use rdkafka::producer::{FutureProducer, FutureRecord};
use rdkafka::ClientConfig;
use std::time::Duration;

use rustberg_core::error::{RustbergError, RustbergResult};
use rustberg_core::model::LogRecord;
use rustberg_core::traits::LogStore;

pub struct KafkaLogStore {
    producer: FutureProducer,
    topic_prefix: String,
}

impl KafkaLogStore {
    pub fn new(brokers: &str, topic_prefix: impl Into<String>) -> RustbergResult<Self> {
        let producer: FutureProducer = ClientConfig::new()
            .set("bootstrap.servers", brokers)
            .set("message.timeout.ms", "5000")
            .create()
            .map_err(|e| RustbergError::LogStore(e.to_string()))?;

        Ok(Self {
            producer,
            topic_prefix: topic_prefix.into(),
        })
    }

    fn topic_for(&self, record: &LogRecord) -> String {
        format!(
            "{}.{}.{}.{}",
            self.topic_prefix,
            record.table.catalog,
            record.table.namespace.join("_"),
            record.table.name
        )
    }
}

#[async_trait]
impl LogStore for KafkaLogStore {
    async fn publish(&self, record: LogRecord) -> RustbergResult<()> {
        let topic = self.topic_for(&record);
        let payload = serde_json::to_vec(&record).map_err(|e| RustbergError::LogStore(e.to_string()))?;
        let key = record.table.name.clone();

        self.producer
            .send(
                FutureRecord::to(&topic).payload(&payload).key(&key),
                Duration::from_secs(5),
            )
            .await
            .map_err(|(e, _)| RustbergError::LogStore(e.to_string()))?;

        Ok(())
    }
}
