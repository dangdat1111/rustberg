use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Table formats Amoro manages. Native Iceberg is the primary target for
/// rustberg; the Mixed formats are Amoro-specific extensions on top of
/// Iceberg/Hive that rustberg does not implement yet (see README).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TableFormat {
    Iceberg,
    MixedIceberg,
    MixedHive,
}

/// A catalog groups namespaces/tables under one metastore + storage
/// configuration, equivalent to Amoro's `CatalogMeta`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Catalog {
    pub name: String,
    pub format: TableFormat,
    /// warehouse root, e.g. s3://bucket/warehouse or file:///data/warehouse
    pub warehouse: String,
    pub properties: serde_json::Value,
}

/// A dotted namespace path, e.g. ["prod", "sales"].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Namespace {
    pub catalog: String,
    pub levels: Vec<String>,
}

impl Namespace {
    pub fn joined(&self) -> String {
        self.levels.join(".")
    }
}

/// Fully-qualified table identifier: catalog.namespace.table
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TableIdent {
    pub catalog: String,
    pub namespace: Vec<String>,
    pub name: String,
}

impl std::fmt::Display for TableIdent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.catalog, self.namespace.join("."), self.name)
    }
}

/// Table metadata as tracked by the catalog service. Deliberately a thin
/// pointer, not a full copy of the Iceberg table metadata JSON — rustberg
/// delegates actual metadata-file parsing/writing to `iceberg-rust` once
/// that integration lands (see crates/catalog/src/iceberg.rs).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableMeta {
    pub ident: TableIdent,
    pub format: TableFormat,
    pub location: String,
    pub metadata_location: String,
    pub current_snapshot_id: Option<i64>,
    pub properties: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OptimizeJobType {
    MinorCompaction,
    MajorCompaction,
    FullCompaction,
    Expiration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OptimizeJobStatus {
    Pending,
    Planning,
    Executing,
    Committing,
    Succeeded,
    Failed,
}

/// A unit of self-optimizing work, equivalent to Amoro's `OptimizingTask`.
/// The optimizer engine plans these from table metadata (file count,
/// size skew, delete-file ratio) and executes them via DataFusion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizeJob {
    pub id: Uuid,
    pub table: TableIdent,
    pub job_type: OptimizeJobType,
    pub status: OptimizeJobStatus,
    pub input_files: i32,
    pub output_files: Option<i32>,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A change-log record shape flowing through the LogStore (Kafka), used
/// for millisecond-latency CDC-style reads ahead of the next Iceberg
/// commit — the Rust analogue of Amoro's LogStore records.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogRecord {
    pub table: TableIdent,
    pub op: LogOp,
    pub epoch_ms: i64,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogOp {
    Insert,
    Update,
    Delete,
}
