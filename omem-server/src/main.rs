use std::sync::Arc;

use tracing_subscriber::{fmt, EnvFilter};

use omem_server::api::{build_router, AppState};
use omem_server::config::OmemConfig;
use omem_server::embed::{create_embed_service, EmbedService};
use omem_server::lifecycle::consolidator::{ConsolidationConfig, Consolidator};
use omem_server::llm::{create_llm_service, LlmService};
use omem_server::store::{SpaceStore, StoreManager, TenantStore};

fn init_tracing(config: &OmemConfig) {
    let filter =
        EnvFilter::try_from_env("RUST_LOG").unwrap_or_else(|_| EnvFilter::new(&config.log_level));

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

    // Build the embedder first so we can size the Lance vector column to
    // exactly match what the model produces. Lets users plug in any embedding
    // model (384-, 768-, 1024-dim, etc.) without recompiling.
    let embed: Arc<dyn EmbedService> = Arc::from(
        create_embed_service(&config)
            .await
            .expect("failed to create embed service"),
    );
    let vector_dim: i32 = embed
        .dimensions()
        .try_into()
        .expect("embedding dim does not fit in i32");
    tracing::info!(vector_dim, "embedder reported vector dimension");

    let store_manager = Arc::new(StoreManager::with_vector_dim(&base_uri, vector_dim));

    let system_uri = format!("{}/_system", base_uri);
    let tenant_store = Arc::new(
        TenantStore::new(&system_uri)
            .await
            .expect("failed to create TenantStore"),
    );
    tenant_store
        .init_table()
        .await
        .expect("failed to init tenants table");

    let space_store = Arc::new(
        SpaceStore::new(&system_uri)
            .await
            .expect("failed to create SpaceStore"),
    );
    space_store
        .init_tables()
        .await
        .expect("failed to init spaces tables");

    let llm: Arc<dyn LlmService> = Arc::from(
        create_llm_service(&config)
            .await
            .expect("failed to create LLM service"),
    );

    let shutdown_tx =
        spawn_consolidation_cron(&config, &store_manager, &space_store, &llm, &embed);

    let state = Arc::new(AppState {
        store_manager,
        tenant_store,
        space_store,
        embed,
        llm,
        config: config.clone(),
        import_semaphore: Arc::new(tokio::sync::Semaphore::new(3)),
        reconcile_semaphore: Arc::new(tokio::sync::Semaphore::new(1)),
        share_rate_limiter: Arc::new(omem_server::api::rate_limit::RateLimiter::new(
            config.share_rate_per_min,
        )),
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

    if let Some(tx) = shutdown_tx {
        let _ = tx.send(true);
    }
}

fn spawn_consolidation_cron(
    config: &OmemConfig,
    store_manager: &Arc<StoreManager>,
    space_store: &Arc<SpaceStore>,
    llm: &Arc<dyn LlmService>,
    embed: &Arc<dyn EmbedService>,
) -> Option<tokio::sync::watch::Sender<bool>> {
    if !config.consolidation_enabled {
        return None;
    }

    let (tx, mut rx) = tokio::sync::watch::channel(false);
    let consolidator = Consolidator::new(
        Arc::clone(store_manager),
        Arc::clone(space_store),
        Arc::clone(llm),
        Arc::clone(embed),
        ConsolidationConfig {
            lookback_hours: config.consolidation_lookback_hours,
            ..ConsolidationConfig::default()
        },
    );
    let interval_secs = config.consolidation_interval_secs;

    tokio::spawn(async move {
        tracing::info!(interval_secs, "consolidation cron started");
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
        interval.tick().await;
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    if let Err(e) = consolidator.run_once().await {
                        tracing::warn!(error = %e, "consolidation cycle failed");
                    }
                }
                _ = rx.changed() => {
                    tracing::info!("consolidation cron shutting down");
                    break;
                }
            }
        }
    });

    Some(tx)
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
