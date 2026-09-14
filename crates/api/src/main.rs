mod worker;

use std::sync::Arc;
use std::time::Duration;

use rustberg_catalog::AppState;
use rustberg_metastore::PgMetadataStore;
use rustberg_optimizer::NaiveOptimizer;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let database_url =
        std::env::var("DATABASE_URL").unwrap_or_else(|_| "postgres://rustberg:rustberg@localhost/rustberg".into());
    let listen_addr = std::env::var("LISTEN_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".into());

    let store: Arc<dyn rustberg_core::traits::MetadataStore> =
        Arc::new(PgMetadataStore::connect(&database_url).await?);
    let optimizer: Arc<dyn rustberg_core::traits::Optimizer> = Arc::new(NaiveOptimizer);

    // Background optimizer worker — polls the queue table so multiple
    // `rustberg-server` processes can run concurrently without duplicating
    // work (see worker::run doc comment).
    tokio::spawn(worker::run(store.clone(), optimizer.clone(), Duration::from_secs(2)));

    let state = AppState { store, optimizer };
    let app = rustberg_catalog::router(state);

    let listener = tokio::net::TcpListener::bind(&listen_addr).await?;
    tracing::info!(addr = %listen_addr, "rustberg-server listening");
    axum::serve(listener, app).await?;

    Ok(())
}
