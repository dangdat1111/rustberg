use async_trait::async_trait;

use crate::error::RustbergResult;
use crate::model::{Catalog, LogRecord, Namespace, OptimizeJob, OptimizeJobType, TableIdent, TableMeta};

/// Persistence boundary implemented by `rustberg-metastore` (PostgreSQL).
/// The REST catalog handlers in `rustberg-catalog` only ever talk to this
/// trait, so the backing store (Postgres today, anything else later) is
/// swappable without touching HTTP handler code.
#[async_trait]
pub trait MetadataStore: Send + Sync {
    async fn create_catalog(&self, catalog: &Catalog) -> RustbergResult<()>;
    async fn get_catalog(&self, name: &str) -> RustbergResult<Catalog>;
    async fn list_catalogs(&self) -> RustbergResult<Vec<Catalog>>;

    async fn create_namespace(&self, ns: &Namespace) -> RustbergResult<()>;
    async fn list_namespaces(&self, catalog: &str) -> RustbergResult<Vec<Namespace>>;
    async fn drop_namespace(&self, ns: &Namespace) -> RustbergResult<()>;

    async fn create_table(&self, table: &TableMeta) -> RustbergResult<()>;
    async fn get_table(&self, ident: &TableIdent) -> RustbergResult<TableMeta>;
    async fn list_tables(&self, ns: &Namespace) -> RustbergResult<Vec<TableIdent>>;
    async fn drop_table(&self, ident: &TableIdent) -> RustbergResult<()>;

    /// Optimistic commit: succeeds only if the stored snapshot id still
    /// matches `expected_snapshot_id`, mirroring Iceberg's
    /// compare-and-swap catalog commit protocol.
    async fn commit_table(
        &self,
        ident: &TableIdent,
        expected_snapshot_id: Option<i64>,
        new_metadata_location: String,
        new_snapshot_id: i64,
    ) -> RustbergResult<()>;

    async fn enqueue_optimize_job(&self, job: &OptimizeJob) -> RustbergResult<()>;
    async fn next_pending_optimize_job(&self) -> RustbergResult<Option<OptimizeJob>>;
    async fn update_optimize_job(&self, job: &OptimizeJob) -> RustbergResult<()>;
}

/// Self-optimizing execution boundary implemented by `rustberg-optimizer`.
/// One call plans+executes one job; the worker loop that pulls jobs off
/// the metastore queue lives in the `api` crate's background task.
#[async_trait]
pub trait Optimizer: Send + Sync {
    async fn plan(&self, table: &TableMeta, job_type: OptimizeJobType) -> RustbergResult<OptimizeJob>;
    async fn execute(&self, job: &OptimizeJob) -> RustbergResult<OptimizeJob>;
}

/// Change-log streaming boundary implemented by `rustberg-logstore`.
#[async_trait]
pub trait LogStore: Send + Sync {
    async fn publish(&self, record: LogRecord) -> RustbergResult<()>;
}
