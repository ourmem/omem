use std::sync::Arc;

use axum::routing::{delete, get, post};
use axum::Router;
use tower_http::cors::{Any, CorsLayer};

use crate::api::handlers;
use crate::api::middleware::{auth_middleware, logging_middleware};
use crate::api::server::AppState;

pub fn build_router(state: Arc<AppState>) -> Router {
    let authed_routes = Router::new()
        .route("/v1/memories/search", get(handlers::search_memories))
        .route(
            "/v1/memories/batch-delete",
            post(handlers::batch_delete),
        )
        .route("/v1/memories/all", delete(handlers::delete_all_memories))
        .route(
            "/v1/memories/{id}",
            get(handlers::get_memory)
                .put(handlers::update_memory)
                .delete(handlers::delete_memory),
        )
        .route(
            "/v1/memories",
            get(handlers::list_memories).post(handlers::create_memory),
        )
        .route("/v1/profile", get(handlers::get_profile))
        .route("/v1/stats", get(handlers::get_stats))
        .route("/v1/stats/config", get(handlers::get_config))
        .route("/v1/stats/tags", get(handlers::get_tags))
        .route("/v1/stats/decay", get(handlers::get_decay))
        .route("/v1/stats/relations", get(handlers::get_relations))
        .route("/v1/stats/spaces", get(handlers::get_spaces_stats))
        .route("/v1/stats/sharing", get(handlers::get_sharing_stats))
        .route("/v1/stats/agents", get(handlers::get_agents_stats))
        .route("/v1/files", post(handlers::upload_file))
        .route("/v1/imports", post(handlers::create_import).get(handlers::list_imports))
        .route("/v1/imports/{id}", get(handlers::get_import))
        .route("/v1/imports/{id}/intelligence", post(handlers::trigger_intelligence))
        .route("/v1/imports/{id}/rollback", post(handlers::rollback_import))
        .route("/v1/imports/cross-reconcile", post(handlers::cross_reconcile))
        .route(
            "/v1/connectors/github/connect",
            post(handlers::github_connect),
        )
        .route(
            "/v1/spaces",
            get(handlers::list_spaces).post(handlers::create_space),
        )
        .route(
            "/v1/spaces/{id}",
            get(handlers::get_space)
                .put(handlers::update_space)
                .delete(handlers::delete_space),
        )
        .route("/v1/spaces/{id}/members", post(handlers::add_member))
        .route(
            "/v1/spaces/{id}/members/{user_id}",
            delete(handlers::remove_member).put(handlers::update_member_role),
        )
        .route(
            "/v1/memories/{id}/share",
            post(handlers::share_memory),
        )
        .route(
            "/v1/memories/{id}/pull",
            post(handlers::pull_memory),
        )
        .route(
            "/v1/memories/{id}/unshare",
            post(handlers::unshare_memory),
        )
        .route(
            "/v1/memories/batch-share",
            post(handlers::batch_share),
        )
        .route(
            "/v1/spaces/{id}/auto-share-rules",
            get(handlers::list_auto_share_rules).post(handlers::create_auto_share_rule),
        )
        .route(
            "/v1/spaces/{id}/auto-share-rules/{rule_id}",
            delete(handlers::delete_auto_share_rule),
        )
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ));

    let public_routes = Router::new()
        .route("/health", get(health))
        .route("/v1/tenants", post(handlers::create_tenant))
        .route(
            "/v1/connectors/github/webhook",
            post(handlers::github_webhook),
        );

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        .merge(authed_routes)
        .merge(public_routes)
        .layer(cors)
        .layer(axum::middleware::from_fn(logging_middleware))
        .with_state(state)
}

async fn health() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({"status": "ok"}))
}
