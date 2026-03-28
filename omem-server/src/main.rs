use std::sync::Arc;

use tracing_subscriber::{fmt, EnvFilter};

use omem_server::api::{build_router, AppState};
use omem_server::config::OmemConfig;
use omem_server::embed::{create_embed_service, EmbedService};
use omem_server::llm::{create_llm_service, LlmService};
use omem_server::store::{SpaceStore, StoreManager, TenantStore};

fn init_tracing(config: &OmemConfig) {
    let filter = EnvFilter::try_from_env("RUST_LOG")
        .unwrap_or_else(|_| EnvFilter::new(&config.log_level));

    fmt()
        .json()
        .with_env_filter(filter)
        .with_target(true)
        .with_thread_ids(true)
        .init();
}

#[tokio::main]
async fn main() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    let config = OmemConfig::from_env();
    init_tracing(&config);

    tracing::info!(
        port = config.port,
        embed_provider = %config.embed_provider,
        llm_provider = %config.llm_provider,
        llm_model = %config.llm_model,
        "starting omem-server"
    );

    let base_uri = config.store_uri();
    let store_manager = Arc::new(StoreManager::new(&base_uri));

    let system_uri = format!("{}/_system", base_uri);
    let tenant_store = Arc::new(
        TenantStore::new(&system_uri)
            .await
            .expect("failed to create TenantStore"),
    );
    tenant_store.init_table().await.expect("failed to init tenants table");

    let space_store = Arc::new(
        SpaceStore::new(&system_uri)
            .await
            .expect("failed to create SpaceStore"),
    );
    space_store.init_tables().await.expect("failed to init spaces tables");

    let embed: Arc<dyn EmbedService> = Arc::from(
        create_embed_service(&config)
            .await
            .expect("failed to create embed service"),
    );

    let llm: Arc<dyn LlmService> = Arc::from(
        create_llm_service(&config)
            .await
            .expect("failed to create LLM service"),
    );

    let state = Arc::new(AppState {
        store_manager,
        tenant_store,
        space_store,
        embed,
        llm,
        config: config.clone(),
        import_semaphore: Arc::new(tokio::sync::Semaphore::new(3)),
        reconcile_semaphore: Arc::new(tokio::sync::Semaphore::new(1)),
    });

    let app = build_router(state);

    let addr = format!("0.0.0.0:{}", config.port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("failed to bind TCP listener");

    tracing::info!(%addr, "listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("server error");
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    tracing::info!("shutdown signal received");
}
