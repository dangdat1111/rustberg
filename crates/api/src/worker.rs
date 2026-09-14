use std::sync::Arc;
use std::time::Duration;

use rustberg_core::model::OptimizeJobStatus;
use rustberg_core::traits::{MetadataStore, Optimizer};

/// Polls the metastore's optimize-job queue and runs each job through the
/// optimizer, persisting the result. This is the process that turns the
/// "Optimizer engine" box in the architecture diagram into an actual
/// background worker; it's spawned once at startup in `main.rs` and can
/// be scaled out to multiple processes since `next_pending_optimize_job`
/// uses `FOR UPDATE SKIP LOCKED`.
pub async fn run(store: Arc<dyn MetadataStore>, optimizer: Arc<dyn Optimizer>, poll_interval: Duration) {
    loop {
        match store.next_pending_optimize_job().await {
            Ok(Some(mut job)) => {
                job.status = OptimizeJobStatus::Executing;
                if let Err(e) = store.update_optimize_job(&job).await {
                    tracing::error!(error = %e, "failed to mark job executing");
                    continue;
                }

                match optimizer.execute(&job).await {
                    Ok(result) => {
                        if let Err(e) = store.update_optimize_job(&result).await {
                            tracing::error!(error = %e, "failed to persist job result");
                        }
                    }
                    Err(e) => {
                        job.status = OptimizeJobStatus::Failed;
                        job.error = Some(e.to_string());
                        if let Err(e) = store.update_optimize_job(&job).await {
                            tracing::error!(error = %e, "failed to persist job failure");
                        }
                    }
                }
            }
            Ok(None) => tokio::time::sleep(poll_interval).await,
            Err(e) => {
                tracing::error!(error = %e, "failed to poll optimize job queue");
                tokio::time::sleep(poll_interval).await;
            }
        }
    }
}
