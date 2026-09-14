//! `rustberg-metastore` — PostgreSQL-backed implementation of
//! `rustberg_core::traits::MetadataStore`. This is the tier-3 "Metadata
//! store" box in the architecture diagram: everything above it (catalog
//! REST handlers, optimizer worker) only depends on the trait, never on
//! `sqlx` or SQL directly.

use async_trait::async_trait;
use sqlx::postgres::{PgPool, PgPoolOptions};
use sqlx::FromRow;

use rustberg_core::error::{RustbergError, RustbergResult};
use rustberg_core::model::{
    Catalog, Namespace, OptimizeJob, OptimizeJobStatus, OptimizeJobType, TableFormat, TableIdent,
    TableMeta,
};
use rustberg_core::traits::MetadataStore;

pub struct PgMetadataStore {
    pool: PgPool,
}

impl PgMetadataStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Connects and runs pending migrations from `../../migrations`.
    pub async fn connect(database_url: &str) -> RustbergResult<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(10)
            .connect(database_url)
            .await
            .map_err(|e| RustbergError::Storage(e.to_string()))?;

        sqlx::migrate!("../../migrations")
            .run(&pool)
            .await
            .map_err(|e| RustbergError::Storage(e.to_string()))?;

        Ok(Self::new(pool))
    }
}

// ---- row <-> domain model plumbing -----------------------------------

fn format_to_str(f: TableFormat) -> &'static str {
    match f {
        TableFormat::Iceberg => "iceberg",
        TableFormat::MixedIceberg => "mixed_iceberg",
        TableFormat::MixedHive => "mixed_hive",
    }
}

fn format_from_str(s: &str) -> RustbergResult<TableFormat> {
    match s {
        "iceberg" => Ok(TableFormat::Iceberg),
        "mixed_iceberg" => Ok(TableFormat::MixedIceberg),
        "mixed_hive" => Ok(TableFormat::MixedHive),
        other => Err(RustbergError::Internal(format!("unknown table format in db: {other}"))),
    }
}

fn job_type_to_str(t: OptimizeJobType) -> &'static str {
    match t {
        OptimizeJobType::MinorCompaction => "minor_compaction",
        OptimizeJobType::MajorCompaction => "major_compaction",
        OptimizeJobType::FullCompaction => "full_compaction",
        OptimizeJobType::Expiration => "expiration",
    }
}

fn job_type_from_str(s: &str) -> RustbergResult<OptimizeJobType> {
    match s {
        "minor_compaction" => Ok(OptimizeJobType::MinorCompaction),
        "major_compaction" => Ok(OptimizeJobType::MajorCompaction),
        "full_compaction" => Ok(OptimizeJobType::FullCompaction),
        "expiration" => Ok(OptimizeJobType::Expiration),
        other => Err(RustbergError::Internal(format!("unknown job type in db: {other}"))),
    }
}

fn job_status_to_str(s: OptimizeJobStatus) -> &'static str {
    match s {
        OptimizeJobStatus::Pending => "pending",
        OptimizeJobStatus::Planning => "planning",
        OptimizeJobStatus::Executing => "executing",
        OptimizeJobStatus::Committing => "committing",
        OptimizeJobStatus::Succeeded => "succeeded",
        OptimizeJobStatus::Failed => "failed",
    }
}

fn job_status_from_str(s: &str) -> RustbergResult<OptimizeJobStatus> {
    match s {
        "pending" => Ok(OptimizeJobStatus::Pending),
        "planning" => Ok(OptimizeJobStatus::Planning),
        "executing" => Ok(OptimizeJobStatus::Executing),
        "committing" => Ok(OptimizeJobStatus::Committing),
        "succeeded" => Ok(OptimizeJobStatus::Succeeded),
        "failed" => Ok(OptimizeJobStatus::Failed),
        other => Err(RustbergError::Internal(format!("unknown job status in db: {other}"))),
    }
}

#[derive(FromRow)]
struct CatalogRow {
    name: String,
    format: String,
    warehouse: String,
    properties: serde_json::Value,
}

impl TryFrom<CatalogRow> for Catalog {
    type Error = RustbergError;
    fn try_from(r: CatalogRow) -> RustbergResult<Catalog> {
        Ok(Catalog {
            name: r.name,
            format: format_from_str(&r.format)?,
            warehouse: r.warehouse,
            properties: r.properties,
        })
    }
}

#[derive(FromRow)]
struct TableRow {
    catalog: String,
    namespace: Vec<String>,
    name: String,
    format: String,
    location: String,
    metadata_location: String,
    current_snapshot_id: Option<i64>,
    properties: serde_json::Value,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

impl TryFrom<TableRow> for TableMeta {
    type Error = RustbergError;
    fn try_from(r: TableRow) -> RustbergResult<TableMeta> {
        Ok(TableMeta {
            ident: TableIdent {
                catalog: r.catalog,
                namespace: r.namespace,
                name: r.name,
            },
            format: format_from_str(&r.format)?,
            location: r.location,
            metadata_location: r.metadata_location,
            current_snapshot_id: r.current_snapshot_id,
            properties: r.properties,
            created_at: r.created_at,
            updated_at: r.updated_at,
        })
    }
}

#[derive(FromRow)]
struct OptimizeJobRow {
    id: uuid::Uuid,
    catalog: String,
    namespace: Vec<String>,
    table_name: String,
    job_type: String,
    status: String,
    input_files: i32,
    output_files: Option<i32>,
    error: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

impl TryFrom<OptimizeJobRow> for OptimizeJob {
    type Error = RustbergError;
    fn try_from(r: OptimizeJobRow) -> RustbergResult<OptimizeJob> {
        Ok(OptimizeJob {
            id: r.id,
            table: TableIdent {
                catalog: r.catalog,
                namespace: r.namespace,
                name: r.table_name,
            },
            job_type: job_type_from_str(&r.job_type)?,
            status: job_status_from_str(&r.status)?,
            input_files: r.input_files,
            output_files: r.output_files,
            error: r.error,
            created_at: r.created_at,
            updated_at: r.updated_at,
        })
    }
}

// ---- trait impl --------------------------------------------------------

#[async_trait]
impl MetadataStore for PgMetadataStore {
    async fn create_catalog(&self, catalog: &Catalog) -> RustbergResult<()> {
        sqlx::query("INSERT INTO catalogs (name, format, warehouse, properties) VALUES ($1, $2, $3, $4)")
            .bind(&catalog.name)
            .bind(format_to_str(catalog.format))
            .bind(&catalog.warehouse)
            .bind(&catalog.properties)
            .execute(&self.pool)
            .await
            .map_err(|e| RustbergError::Storage(e.to_string()))?;
        Ok(())
    }

    async fn get_catalog(&self, name: &str) -> RustbergResult<Catalog> {
        let row = sqlx::query_as::<_, CatalogRow>("SELECT * FROM catalogs WHERE name = $1")
            .bind(name)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| RustbergError::Storage(e.to_string()))?
            .ok_or_else(|| RustbergError::CatalogNotFound(name.to_string()))?;
        row.try_into()
    }

    async fn list_catalogs(&self) -> RustbergResult<Vec<Catalog>> {
        let rows = sqlx::query_as::<_, CatalogRow>("SELECT * FROM catalogs ORDER BY name")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| RustbergError::Storage(e.to_string()))?;
        rows.into_iter().map(TryInto::try_into).collect()
    }

    async fn create_namespace(&self, ns: &Namespace) -> RustbergResult<()> {
        sqlx::query("INSERT INTO namespaces (catalog, levels) VALUES ($1, $2)")
            .bind(&ns.catalog)
            .bind(&ns.levels)
            .execute(&self.pool)
            .await
            .map_err(|e| RustbergError::Storage(e.to_string()))?;
        Ok(())
    }

    async fn list_namespaces(&self, catalog: &str) -> RustbergResult<Vec<Namespace>> {
        let rows: Vec<(Vec<String>,)> =
            sqlx::query_as("SELECT levels FROM namespaces WHERE catalog = $1 ORDER BY levels")
                .bind(catalog)
                .fetch_all(&self.pool)
                .await
                .map_err(|e| RustbergError::Storage(e.to_string()))?;

        Ok(rows
            .into_iter()
            .map(|(levels,)| Namespace {
                catalog: catalog.to_string(),
                levels,
            })
            .collect())
    }

    async fn drop_namespace(&self, ns: &Namespace) -> RustbergResult<()> {
        sqlx::query("DELETE FROM namespaces WHERE catalog = $1 AND levels = $2")
            .bind(&ns.catalog)
            .bind(&ns.levels)
            .execute(&self.pool)
            .await
            .map_err(|e| RustbergError::Storage(e.to_string()))?;
        Ok(())
    }

    async fn create_table(&self, table: &TableMeta) -> RustbergResult<()> {
        sqlx::query(
            "INSERT INTO tables
                (catalog, namespace, name, format, location, metadata_location,
                 current_snapshot_id, properties, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
        )
        .bind(&table.ident.catalog)
        .bind(&table.ident.namespace)
        .bind(&table.ident.name)
        .bind(format_to_str(table.format))
        .bind(&table.location)
        .bind(&table.metadata_location)
        .bind(table.current_snapshot_id)
        .bind(&table.properties)
        .bind(table.created_at)
        .bind(table.updated_at)
        .execute(&self.pool)
        .await
        .map_err(|e| RustbergError::Storage(e.to_string()))?;
        Ok(())
    }

    async fn get_table(&self, ident: &TableIdent) -> RustbergResult<TableMeta> {
        let row = sqlx::query_as::<_, TableRow>(
            "SELECT * FROM tables WHERE catalog = $1 AND namespace = $2 AND name = $3",
        )
        .bind(&ident.catalog)
        .bind(&ident.namespace)
        .bind(&ident.name)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RustbergError::Storage(e.to_string()))?
        .ok_or_else(|| RustbergError::TableNotFound(ident.to_string()))?;
        row.try_into()
    }

    async fn list_tables(&self, ns: &Namespace) -> RustbergResult<Vec<TableIdent>> {
        let rows: Vec<(String,)> =
            sqlx::query_as("SELECT name FROM tables WHERE catalog = $1 AND namespace = $2 ORDER BY name")
                .bind(&ns.catalog)
                .bind(&ns.levels)
                .fetch_all(&self.pool)
                .await
                .map_err(|e| RustbergError::Storage(e.to_string()))?;

        Ok(rows
            .into_iter()
            .map(|(name,)| TableIdent {
                catalog: ns.catalog.clone(),
                namespace: ns.levels.clone(),
                name,
            })
            .collect())
    }

    async fn drop_table(&self, ident: &TableIdent) -> RustbergResult<()> {
        sqlx::query("DELETE FROM tables WHERE catalog = $1 AND namespace = $2 AND name = $3")
            .bind(&ident.catalog)
            .bind(&ident.namespace)
            .bind(&ident.name)
            .execute(&self.pool)
            .await
            .map_err(|e| RustbergError::Storage(e.to_string()))?;
        Ok(())
    }

    async fn commit_table(
        &self,
        ident: &TableIdent,
        expected_snapshot_id: Option<i64>,
        new_metadata_location: String,
        new_snapshot_id: i64,
    ) -> RustbergResult<()> {
        // Optimistic concurrency: the UPDATE only matches a row when the
        // stored snapshot id is exactly what the caller expected, giving
        // us Iceberg's compare-and-swap commit semantics without a
        // separate lock table.
        let result = sqlx::query(
            "UPDATE tables
                SET metadata_location = $1, current_snapshot_id = $2, updated_at = now()
              WHERE catalog = $3 AND namespace = $4 AND name = $5
                AND current_snapshot_id IS NOT DISTINCT FROM $6",
        )
        .bind(&new_metadata_location)
        .bind(new_snapshot_id)
        .bind(&ident.catalog)
        .bind(&ident.namespace)
        .bind(&ident.name)
        .bind(expected_snapshot_id)
        .execute(&self.pool)
        .await
        .map_err(|e| RustbergError::Storage(e.to_string()))?;

        if result.rows_affected() == 0 {
            let current = self.get_table(ident).await?;
            return Err(RustbergError::CommitConflict {
                table: ident.to_string(),
                expected: expected_snapshot_id,
                found: current.current_snapshot_id,
            });
        }
        Ok(())
    }

    async fn enqueue_optimize_job(&self, job: &OptimizeJob) -> RustbergResult<()> {
        sqlx::query(
            "INSERT INTO optimize_jobs
                (id, catalog, namespace, table_name, job_type, status,
                 input_files, output_files, error, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
        )
        .bind(job.id)
        .bind(&job.table.catalog)
        .bind(&job.table.namespace)
        .bind(&job.table.name)
        .bind(job_type_to_str(job.job_type))
        .bind(job_status_to_str(job.status))
        .bind(job.input_files)
        .bind(job.output_files)
        .bind(&job.error)
        .bind(job.created_at)
        .bind(job.updated_at)
        .execute(&self.pool)
        .await
        .map_err(|e| RustbergError::Storage(e.to_string()))?;
        Ok(())
    }

    async fn next_pending_optimize_job(&self) -> RustbergResult<Option<OptimizeJob>> {
        // SKIP LOCKED lets multiple optimizer workers poll the same queue
        // without stepping on each other.
        let row = sqlx::query_as::<_, OptimizeJobRow>(
            "SELECT * FROM optimize_jobs
              WHERE status = 'pending'
              ORDER BY created_at
              FOR UPDATE SKIP LOCKED
              LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RustbergError::Storage(e.to_string()))?;

        row.map(TryInto::try_into).transpose()
    }

    async fn update_optimize_job(&self, job: &OptimizeJob) -> RustbergResult<()> {
        sqlx::query(
            "UPDATE optimize_jobs
                SET status = $1, output_files = $2, error = $3, updated_at = now()
              WHERE id = $4",
        )
        .bind(job_status_to_str(job.status))
        .bind(job.output_files)
        .bind(&job.error)
        .bind(job.id)
        .execute(&self.pool)
        .await
        .map_err(|e| RustbergError::Storage(e.to_string()))?;
        Ok(())
    }
}
