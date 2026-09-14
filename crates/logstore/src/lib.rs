pub mod memory;
pub use memory::InMemoryLogStore;

#[cfg(feature = "kafka")]
pub mod kafka;
#[cfg(feature = "kafka")]
pub use kafka::KafkaLogStore;
