//! `rustberg-catalog` — the REST catalog service. Implements a practical
//! subset of the Iceberg REST Catalog spec (namespaces, tables, optimistic
//! commit) plus an endpoint to enqueue self-optimizing jobs. This is the
//! "Catalog service" box in the architecture diagram; it depends only on
//! the `MetadataStore` and `Optimizer` traits from `rustberg-core`, never on
//! a concrete backend, so `rustberg-api` can wire in Postgres (or anything
//! else) without this crate changing.

mod dto;
mod http_error;

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use chrono::Utc;

use rustberg_core::model::{Namespace, TableIdent, TableMeta};
use rustberg_core::traits::{MetadataStore, Optimizer};

use dto::*;
use http_error::ApiError;

#[derive(Clone)]
pub struct AppState {
    pub store: Arc<dyn MetadataStore>,
    pub optimizer: Arc<dyn Optimizer>,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/v1/catalogs", post(create_catalog).get(list_catalogs))
        .route(
            "/v1/:catalog/namespaces",
            post(create_namespace).get(list_namespaces),
        )
        .route("/v1/:catalog/namespaces/:ns", delete(drop_namespace))
        .route(
            "/v1/:catalog/namespaces/:ns/tables",
            post(create_table).get(list_tables),
        )
        .route(
            "/v1/:catalog/namespaces/:ns/tables/:table",
            get(load_table).delete(drop_table),
        )
        .route(
            "/v1/:catalog/namespaces/:ns/tables/:table/commit",
            post(commit_table),
        )
        .route(
            "/v1/:catalog/namespaces/:ns/tables/:table/optimize",
            post(trigger_optimize),
        )
        .with_state(state)
}

// ---- catalogs -----------------------------------------------------------

async fn create_catalog(
    State(state): State<AppState>,
    Json(req): Json<CreateCatalogRequest>,
) -> Result<Json<()>, ApiError> {
    state.store.create_catalog(&req.into()).await?;
    Ok(Json(()))
}

async fn list_catalogs(State(state): State<AppState>) -> Result<Json<Vec<String>>, ApiError> {
    let catalogs = state.store.list_catalogs().await?;
    Ok(Json(catalogs.into_iter().map(|c| c.name).collect()))
}

// ---- namespaces -----------------------------------------------------------

async fn create_namespace(
    State(state): State<AppState>,
    Path(catalog): Path<String>,
    Json(req): Json<CreateNamespaceRequest>,
) -> Result<Json<NamespaceResponse>, ApiError> {
    let ns = Namespace {
        catalog,
        levels: req.levels,
    };
    state.store.create_namespace(&ns).await?;
    Ok(Json(ns.into()))
}

async fn list_namespaces(
    State(state): State<AppState>,
    Path(catalog): Path<String>,
) -> Result<Json<Vec<NamespaceResponse>>, ApiError> {
    let namespaces = state.store.list_namespaces(&catalog).await?;
    Ok(Json(namespaces.into_iter().map(Into::into).collect()))
}

async fn drop_namespace(
    State(state): State<AppState>,
    Path((catalog, ns)): Path<(String, String)>,
) -> Result<(), ApiError> {
    let namespace = Namespace {
        catalog,
        levels: split_ns(&ns),
    };
    state.store.drop_namespace(&namespace).await?;
    Ok(())
}

// ---- tables -----------------------------------------------------------

async fn create_table(
    State(state): State<AppState>,
    Path((catalog, ns)): Path<(String, String)>,
    Json(req): Json<CreateTableRequest>,
) -> Result<Json<TableResponse>, ApiError> {
    let now = Utc::now();
    let ident = TableIdent {
        catalog,
        namespace: split_ns(&ns),
        name: req.name,
    };
    // A real implementation delegates to iceberg-rust here to write the
    // initial metadata.json under `req.location` and returns its path;
    // we stand in a deterministic placeholder so the catalog contract is
    // exercisable end to end today.
    let metadata_location = format!("{}/metadata/v1.metadata.json", req.location.trim_end_matches('/'));
    let table = TableMeta {
        ident,
        format: req.format,
        location: req.location,
        metadata_location,
        current_snapshot_id: None,
        properties: req.properties,
        created_at: now,
        updated_at: now,
    };
    state.store.create_table(&table).await?;
    Ok(Json(table.into()))
}

async fn list_tables(
    State(state): State<AppState>,
    Path((catalog, ns)): Path<(String, String)>,
) -> Result<Json<Vec<String>>, ApiError> {
    let namespace = Namespace {
        catalog,
        levels: split_ns(&ns),
    };
    let tables = state.store.list_tables(&namespace).await?;
    Ok(Json(tables.into_iter().map(|t| t.name).collect()))
}

async fn load_table(
    State(state): State<AppState>,
    Path((catalog, ns, table)): Path<(String, String, String)>,
) -> Result<Json<TableResponse>, ApiError> {
    let ident = TableIdent {
        catalog,
        namespace: split_ns(&ns),
        name: table,
    };
    let table = state.store.get_table(&ident).await?;
    Ok(Json(table.into()))
}

async fn drop_table(
    State(state): State<AppState>,
    Path((catalog, ns, table)): Path<(String, String, String)>,
) -> Result<(), ApiError> {
    let ident = TableIdent {
        catalog,
        namespace: split_ns(&ns),
        name: table,
    };
    state.store.drop_table(&ident).await?;
    Ok(())
}

async fn commit_table(
    State(state): State<AppState>,
    Path((catalog, ns, table)): Path<(String, String, String)>,
    Json(req): Json<CommitTableRequest>,
) -> Result<Json<TableResponse>, ApiError> {
    let ident = TableIdent {
        catalog,
        namespace: split_ns(&ns),
        name: table,
    };
    state
        .store
        .commit_table(
            &ident,
            req.expected_snapshot_id,
            req.new_metadata_location,
            req.new_snapshot_id,
        )
        .await?;
    let updated = state.store.get_table(&ident).await?;
    Ok(Json(updated.into()))
}

async fn trigger_optimize(
    State(state): State<AppState>,
    Path((catalog, ns, table)): Path<(String, String, String)>,
    Json(req): Json<TriggerOptimizeRequest>,
) -> Result<Json<uuid::Uuid>, ApiError> {
    let ident = TableIdent {
        catalog,
        namespace: split_ns(&ns),
        name: table,
    };
    let table_meta = state.store.get_table(&ident).await?;
    let job = state.optimizer.plan(&table_meta, req.job_type).await?;
    state.store.enqueue_optimize_job(&job).await?;
    Ok(Json(job.id))
}

/// Namespace levels travel over HTTP as a single dot-joined path segment
/// (matching the Iceberg REST spec's encoding), e.g. `prod.sales`.
fn split_ns(ns: &str) -> Vec<String> {
    ns.split('.').map(str::to_string).collect()
}
