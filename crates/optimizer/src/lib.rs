//! `rustberg-optimizer` — the self-optimizing engine. `plan()` decides what
//! compaction work a table needs (minor/major/full) from its file
//! statistics; `execute()` runs that work and returns the job updated
//! with a result. This crate is the "Optimizer engine" box in the
//! architecture diagram.
//!
//! The default `NaiveOptimizer` here implements the trait with a
//! placeholder executor so the rest of the system (metastore queue, API
//! worker loop) is fully wireable and testable today. Swap in a
//! DataFusion-backed executor (behind the `datafusion-exec` feature)
//! once the Iceberg data-file listing / manifest-write path lands in
//! `rustberg-catalog` — `execute()` is the only method that needs to change.

use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;

use rustberg_core::error::RustbergResult;
use rustberg_core::model::{OptimizeJob, OptimizeJobStatus, OptimizeJobType, TableMeta};
use rustberg_core::traits::Optimizer;

/// Trigger a compaction plan once a table crosses this many small data
/// files — the same signal Amoro's own planner uses, simplified.
pub const MINOR_COMPACTION_FILE_THRESHOLD: i32 = 50;

pub struct NaiveOptimizer;

#[async_trait]
impl Optimizer for NaiveOptimizer {
    async fn plan(&self, table: &TableMeta, job_type: OptimizeJobType) -> RustbergResult<OptimizeJob> {
        // A real planner reads the table's current manifest list (via
        // iceberg-rust) to count small files / delete-file ratio and
        // decide minor vs. major vs. full compaction. Here we just take
        // the caller's job_type and stamp a fresh job.
        let now = Utc::now();
        Ok(OptimizeJob {
            id: Uuid::new_v4(),
            table: table.ident.clone(),
            job_type,
            status: OptimizeJobStatus::Pending,
            input_files: 0,
            output_files: None,
            error: None,
            created_at: now,
            updated_at: now,
        })
    }

    async fn execute(&self, job: &OptimizeJob) -> RustbergResult<OptimizeJob> {
        // Placeholder execution: a DataFusion-backed implementation
        // would, per job:
        //   1. list input Parquet data files for `job.table` via
        //      iceberg-rust's manifest reader
        //   2. build a DataFusion plan: ParquetExec(inputs) -> sort/merge
        //      -> ParquetSink(new file)
        //   3. run the plan, collect written file paths + row counts
        //   4. hand the new file list back to rustberg-catalog to commit a
        //      new Iceberg snapshot (replace input files with output)
        let mut updated = job.clone();
        updated.status = OptimizeJobStatus::Succeeded;
        updated.output_files = Some(1);
        updated.updated_at = Utc::now();
        tracing::info!(job_id = %job.id, table = %job.table, "optimize job executed (naive stub)");
        Ok(updated)
    }
}
