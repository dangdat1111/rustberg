use serde::{Deserialize, Serialize};

use rustberg_core::model::{Catalog, Namespace, OptimizeJobType, TableFormat, TableIdent, TableMeta};

#[derive(Deserialize)]
pub struct CreateCatalogRequest {
    pub name: String,
    pub format: TableFormat,
    pub warehouse: String,
    #[serde(default)]
    pub properties: serde_json::Value,
}

impl From<CreateCatalogRequest> for Catalog {
    fn from(r: CreateCatalogRequest) -> Self {
        Catalog {
            name: r.name,
            format: r.format,
            warehouse: r.warehouse,
            properties: r.properties,
        }
    }
}

#[derive(Deserialize)]
pub struct CreateNamespaceRequest {
    pub levels: Vec<String>,
}

#[derive(Serialize)]
pub struct NamespaceResponse {
    pub namespace: Vec<String>,
}

impl From<Namespace> for NamespaceResponse {
    fn from(n: Namespace) -> Self {
        Self { namespace: n.levels }
    }
}

#[derive(Deserialize)]
pub struct CreateTableRequest {
    pub name: String,
    pub format: TableFormat,
    pub location: String,
    #[serde(default)]
    pub properties: serde_json::Value,
}

#[derive(Serialize)]
pub struct TableResponse {
    pub identifier: TableIdentDto,
    pub format: TableFormat,
    pub location: String,
    pub metadata_location: String,
    pub current_snapshot_id: Option<i64>,
}

#[derive(Serialize)]
pub struct TableIdentDto {
    pub catalog: String,
    pub namespace: Vec<String>,
    pub name: String,
}

impl From<TableMeta> for TableResponse {
    fn from(t: TableMeta) -> Self {
        Self {
            identifier: TableIdentDto {
                catalog: t.ident.catalog,
                namespace: t.ident.namespace,
                name: t.ident.name,
            },
            format: t.format,
            location: t.location,
            metadata_location: t.metadata_location,
            current_snapshot_id: t.current_snapshot_id,
        }
    }
}

#[derive(Deserialize)]
pub struct CommitTableRequest {
    /// The snapshot id the client last read, for optimistic concurrency.
    pub expected_snapshot_id: Option<i64>,
    pub new_metadata_location: String,
    pub new_snapshot_id: i64,
}

#[derive(Deserialize)]
pub struct TriggerOptimizeRequest {
    pub job_type: OptimizeJobType,
}
