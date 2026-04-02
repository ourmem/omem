pub mod error;
pub mod handlers;
pub mod middleware;
pub mod router;
pub mod server;

pub use router::build_router;
pub use server::AppState;

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    use crate::api::{build_router, AppState};
    use crate::config::OmemConfig;
    use crate::domain::error::OmemError;
    use crate::embed::EmbedService;
    use crate::llm::LlmService;
    use crate::store::{SpaceStore, StoreManager, TenantStore};

    struct TestEmbedder;

    #[async_trait::async_trait]
    impl EmbedService for TestEmbedder {
        async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, OmemError> {
            Ok(texts
                .iter()
                .map(|_| {
                    let mut v = vec![0.0f32; 1024];
                    v[0] = 1.0;
                    v
                })
                .collect())
        }
        fn dimensions(&self) -> usize {
            1024
        }
    }

    struct TestLlm;

    #[async_trait::async_trait]
    impl LlmService for TestLlm {
        async fn complete_text(&self, _system: &str, _user: &str) -> Result<String, OmemError> {
            Ok(r#"{"memories":[]}"#.to_string())
        }
    }

    async fn setup_app() -> (axum::Router, tempfile::TempDir) {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let uri = dir.path().to_str().expect("path");

        let store_manager = Arc::new(StoreManager::new(uri));

        let system_uri = format!("{}/_system", uri);
        let tenant_store = Arc::new(TenantStore::new(&system_uri).await.expect("tenant store"));
        tenant_store.init_table().await.expect("init tenants");

        let space_store = Arc::new(SpaceStore::new(&system_uri).await.expect("space store"));
        space_store.init_tables().await.expect("init spaces");

        let embed: Arc<dyn EmbedService> = Arc::new(TestEmbedder);
        let llm: Arc<dyn LlmService> = Arc::new(TestLlm);

        let state = Arc::new(AppState {
            store_manager,
            tenant_store,
            space_store,
            embed,
            llm,
            config: OmemConfig::default(),
            import_semaphore: Arc::new(tokio::sync::Semaphore::new(3)),
            reconcile_semaphore: Arc::new(tokio::sync::Semaphore::new(1)),
        });

        (build_router(state), dir)
    }

    async fn create_test_tenant(app: &axum::Router) -> String {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/tenants")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"name":"test-workspace"}"#))
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);

        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        json["api_key"].as_str().expect("api_key").to_string()
    }

    #[tokio::test]
    async fn test_health_returns_ok() {
        let (app, _dir) = setup_app().await;

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);

        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(json["status"], "ok");
    }

    #[tokio::test]
    async fn test_create_tenant() {
        let (app, _dir) = setup_app().await;

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/tenants")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"name":"my-workspace"}"#))
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);

        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert!(json["id"].as_str().is_some());
        assert!(json["api_key"].as_str().is_some());
        assert_eq!(json["id"], json["api_key"]);
        assert_eq!(json["status"], "active");
    }

    #[tokio::test]
    async fn test_auth_required() {
        let (app, _dir) = setup_app().await;

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/memories")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(json["error"]["code"], "unauthorized");
    }

    #[tokio::test]
    async fn test_invalid_api_key_returns_401() {
        let (app, _dir) = setup_app().await;

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/memories")
                    .header("x-api-key", "nonexistent-key")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_create_direct_memory() {
        let (app, _dir) = setup_app().await;
        let api_key = create_test_tenant(&app).await;

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/memories")
                    .header("content-type", "application/json")
                    .header("x-api-key", &api_key)
                    .body(Body::from(
                        r#"{"content":"user prefers dark mode","tags":["preference"],"source":"manual"}"#,
                    ))
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::CREATED);

        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(json["content"], "user prefers dark mode");
        assert!(json["id"].as_str().is_some());
    }

    #[tokio::test]
    async fn test_ingest_messages() {
        let (app, _dir) = setup_app().await;
        let api_key = create_test_tenant(&app).await;

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/memories")
                    .header("content-type", "application/json")
                    .header("x-api-key", &api_key)
                    .body(Body::from(
                        r#"{"messages":[{"role":"user","content":"I like Rust"},{"role":"assistant","content":"Noted!"}],"mode":"raw"}"#,
                    ))
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::ACCEPTED);

        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert!(json["task_id"].as_str().is_some());
        assert_eq!(json["stored_count"], 2);
    }

    #[tokio::test]
    async fn test_create_and_get_memory() {
        let (app, _dir) = setup_app().await;
        let api_key = create_test_tenant(&app).await;

        let create_resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/memories")
                    .header("content-type", "application/json")
                    .header("x-api-key", &api_key)
                    .body(Body::from(r#"{"content":"test memory"}"#))
                    .expect("request"),
            )
            .await
            .expect("response");

        let bytes = create_resp
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        let created: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        let memory_id = created["id"].as_str().expect("id");

        let get_resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/v1/memories/{memory_id}"))
                    .header("x-api-key", &api_key)
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(get_resp.status(), StatusCode::OK);

        let bytes = get_resp
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        let fetched: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(fetched["id"], memory_id);
        assert_eq!(fetched["content"], "test memory");
    }

    #[tokio::test]
    async fn test_list_memories() {
        let (app, _dir) = setup_app().await;
        let api_key = create_test_tenant(&app).await;

        for i in 0..3 {
            app.clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/v1/memories")
                        .header("content-type", "application/json")
                        .header("x-api-key", &api_key)
                        .body(Body::from(format!(r#"{{"content":"memory {i}"}}"#)))
                        .expect("request"),
                )
                .await
                .expect("response");
        }

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/v1/memories?limit=10&offset=0")
                    .header("x-api-key", &api_key)
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);

        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(json["memories"].as_array().expect("array").len(), 3);
    }

    #[tokio::test]
    async fn test_delete_memory() {
        let (app, _dir) = setup_app().await;
        let api_key = create_test_tenant(&app).await;

        let create_resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/memories")
                    .header("content-type", "application/json")
                    .header("x-api-key", &api_key)
                    .body(Body::from(r#"{"content":"to delete"}"#))
                    .expect("request"),
            )
            .await
            .expect("response");

        let bytes = create_resp
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        let created: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        let memory_id = created["id"].as_str().expect("id");

        let delete_resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!("/v1/memories/{memory_id}"))
                    .header("x-api-key", &api_key)
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(delete_resp.status(), StatusCode::OK);

        let bytes = delete_resp
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(json["status"], "deleted");
    }

    #[tokio::test]
    async fn test_update_memory() {
        let (app, _dir) = setup_app().await;
        let api_key = create_test_tenant(&app).await;

        let create_resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/memories")
                    .header("content-type", "application/json")
                    .header("x-api-key", &api_key)
                    .body(Body::from(r#"{"content":"original"}"#))
                    .expect("request"),
            )
            .await
            .expect("response");

        let bytes = create_resp
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        let created: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        let memory_id = created["id"].as_str().expect("id");

        let update_resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri(format!("/v1/memories/{memory_id}"))
                    .header("content-type", "application/json")
                    .header("x-api-key", &api_key)
                    .body(Body::from(
                        r#"{"content":"updated","tags":["new-tag"]}"#,
                    ))
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(update_resp.status(), StatusCode::OK);

        let bytes = update_resp
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(json["content"], "updated");
        assert_eq!(json["tags"][0], "new-tag");
    }

    #[tokio::test]
    async fn test_search_memories() {
        let (app, _dir) = setup_app().await;
        let api_key = create_test_tenant(&app).await;

        app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/memories")
                    .header("content-type", "application/json")
                    .header("x-api-key", &api_key)
                    .body(Body::from(r#"{"content":"rust programming language"}"#))
                    .expect("request"),
            )
            .await
            .expect("response");

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/v1/memories/search?q=rust&limit=10")
                    .header("x-api-key", &api_key)
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);

        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert!(json["results"].as_array().is_some());
    }

    #[tokio::test]
    async fn test_profile_returns_empty() {
        let (app, _dir) = setup_app().await;
        let api_key = create_test_tenant(&app).await;

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/v1/profile")
                    .header("x-api-key", &api_key)
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);

        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert!(json["static_facts"].as_array().expect("array").is_empty());
        assert!(json["dynamic_context"].as_array().expect("array").is_empty());
    }

    #[tokio::test]
    async fn test_tenant_without_name_returns_400() {
        let (app, _dir) = setup_app().await;

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/tenants")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"name":""}"#))
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_memory_not_found_returns_404() {
        let (app, _dir) = setup_app().await;
        let api_key = create_test_tenant(&app).await;

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/v1/memories/nonexistent-id")
                    .header("x-api-key", &api_key)
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_cors_headers() {
        let (app, _dir) = setup_app().await;

        let response = app
            .oneshot(
                Request::builder()
                    .method("OPTIONS")
                    .uri("/health")
                    .header("origin", "http://example.com")
                    .header("access-control-request-method", "GET")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert!(response.headers().contains_key("access-control-allow-origin"));
        assert_eq!(
            response.headers().get("access-control-allow-origin").unwrap(),
            "*"
        );
    }

    #[tokio::test]
    async fn test_list_memories_total_count() {
        let (app, _dir) = setup_app().await;
        let api_key = create_test_tenant(&app).await;

        for i in 0..5 {
            app.clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/v1/memories")
                        .header("content-type", "application/json")
                        .header("x-api-key", &api_key)
                        .body(Body::from(format!(r#"{{"content":"memory {i}"}}"#)))
                        .expect("request"),
                )
                .await
                .expect("response");
        }

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/v1/memories?limit=2&offset=0")
                    .header("x-api-key", &api_key)
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);

        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(json["total_count"].as_u64().expect("total_count"), 5);
        assert_eq!(json["memories"].as_array().expect("array").len(), 2);
        assert_eq!(json["limit"].as_u64().expect("limit"), 2);
    }

    #[tokio::test]
    async fn test_list_memories_with_sort() {
        let (app, _dir) = setup_app().await;
        let api_key = create_test_tenant(&app).await;

        for i in 0..3 {
            app.clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/v1/memories")
                        .header("content-type", "application/json")
                        .header("x-api-key", &api_key)
                        .body(Body::from(format!(r#"{{"content":"memory {i}"}}"#)))
                        .expect("request"),
                )
                .await
                .expect("response");
        }

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/v1/memories?sort=created_at&order=asc")
                    .header("x-api-key", &api_key)
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);

        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        let memories = json["memories"].as_array().expect("array");
        assert_eq!(memories.len(), 3);
        assert_eq!(json["total_count"].as_u64().expect("total_count"), 3);
    }

    #[tokio::test]
    async fn test_list_memories_backward_compatible() {
        let (app, _dir) = setup_app().await;
        let api_key = create_test_tenant(&app).await;

        app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/memories")
                    .header("content-type", "application/json")
                    .header("x-api-key", &api_key)
                    .body(Body::from(r#"{"content":"test"}"#))
                    .expect("request"),
            )
            .await
            .expect("response");

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/v1/memories")
                    .header("x-api-key", &api_key)
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);

        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert!(json["memories"].as_array().is_some());
        assert!(json["total_count"].as_u64().is_some());
        assert!(json["limit"].as_u64().is_some());
        assert!(json["offset"].as_u64().is_some());
    }

    #[tokio::test]
    async fn test_config_returns_decay_params() {
        let (app, _dir) = setup_app().await;
        let api_key = create_test_tenant(&app).await;

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/v1/stats/config")
                    .header("x-api-key", &api_key)
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);

        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert!(json["decay"]["half_life_days"].as_f64().is_some());
        assert!(json["decay"]["tiers"]["core"]["beta"].as_f64().is_some());
        assert!(json["promotion"].is_object());
        assert!(json["demotion"].is_object());
        assert!(json["categories"].as_array().is_some());
        assert!(json["tiers"].as_array().is_some());
        assert!(json["relation_types"].as_array().is_some());
    }

    #[tokio::test]
    async fn test_tags_basic() {
        let (app, _dir) = setup_app().await;
        let api_key = create_test_tenant(&app).await;

        for tag_set in [r#"["rust","coding"]"#, r#"["rust","web"]"#, r#"["web"]"#] {
            app.clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/v1/memories")
                        .header("content-type", "application/json")
                        .header("x-api-key", &api_key)
                        .body(Body::from(format!(
                            r#"{{"content":"test","tags":{tag_set}}}"#
                        )))
                        .expect("request"),
                )
                .await
                .expect("response");
        }

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/v1/stats/tags")
                    .header("x-api-key", &api_key)
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);

        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        let tags = json["tags"].as_array().expect("tags array");
        assert!(!tags.is_empty());
        assert_eq!(json["total_tag_usages"].as_u64().expect("usages"), 5);

        let first = &tags[0];
        let top_count = first["count"].as_u64().expect("count");
        assert!(top_count >= 2);
    }

    #[tokio::test]
    async fn test_decay_curve() {
        let (app, _dir) = setup_app().await;
        let api_key = create_test_tenant(&app).await;

        let create_resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/memories")
                    .header("content-type", "application/json")
                    .header("x-api-key", &api_key)
                    .body(Body::from(r#"{"content":"decay test memory"}"#))
                    .expect("request"),
            )
            .await
            .expect("response");

        let bytes = create_resp
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        let created: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        let memory_id = created["id"].as_str().expect("id");

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!(
                        "/v1/stats/decay?memory_id={memory_id}&points=30"
                    ))
                    .header("x-api-key", &api_key)
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);

        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(json["memory_id"], memory_id);
        let curve = json["decay_curve"].as_array().expect("decay_curve");
        assert_eq!(curve.len(), 30);
        assert!(json["decay_params"]["beta"].as_f64().is_some());
        assert!(json["current_strength"].as_f64().is_some());
    }

    #[tokio::test]
    async fn test_decay_404() {
        let (app, _dir) = setup_app().await;
        let api_key = create_test_tenant(&app).await;

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/v1/stats/decay?memory_id=nonexistent-id")
                    .header("x-api-key", &api_key)
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_relations_basic() {
        let (app, _dir) = setup_app().await;
        let api_key = create_test_tenant(&app).await;

        let create_resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/memories")
                    .header("content-type", "application/json")
                    .header("x-api-key", &api_key)
                    .body(Body::from(r#"{"content":"target memory"}"#))
                    .expect("request"),
            )
            .await
            .expect("response");

        let bytes = create_resp
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        let target: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        let target_id = target["id"].as_str().expect("id");

        let create_resp2 = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/memories")
                    .header("content-type", "application/json")
                    .header("x-api-key", &api_key)
                    .body(Body::from(format!(
                        r#"{{"content":"source memory","relations":[{{"relation_type":"supports","target_id":"{target_id}"}}]}}"#
                    )))
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(create_resp2.status(), StatusCode::CREATED);

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/v1/stats/relations")
                    .header("x-api-key", &api_key)
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);

        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert!(json["edges"].as_array().is_some());
        assert!(json["nodes"].as_array().is_some());
        assert!(json["total_nodes"].as_u64().is_some());
        assert!(json["total_edges"].as_u64().is_some());
    }

    #[tokio::test]
    async fn test_relations_empty() {
        let (app, _dir) = setup_app().await;
        let api_key = create_test_tenant(&app).await;

        app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/memories")
                    .header("content-type", "application/json")
                    .header("x-api-key", &api_key)
                    .body(Body::from(r#"{"content":"no relations"}"#))
                    .expect("request"),
            )
            .await
            .expect("response");

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/v1/stats/relations")
                    .header("x-api-key", &api_key)
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);

        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(json["nodes"].as_array().expect("nodes").len(), 0);
        assert_eq!(json["edges"].as_array().expect("edges").len(), 0);
    }

    #[tokio::test]
    async fn test_create_space() {
        let (app, _dir) = setup_app().await;
        let api_key = create_test_tenant(&app).await;

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/spaces")
                    .header("content-type", "application/json")
                    .header("x-api-key", &api_key)
                    .body(Body::from(
                        r#"{"name":"Backend Team","space_type":"team"}"#,
                    ))
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::CREATED);

        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(json["name"], "Backend Team");
        assert_eq!(json["space_type"], "team");
        assert!(json["id"].as_str().expect("id").starts_with("team/"));
        assert_eq!(json["owner_id"], api_key);
        assert_eq!(json["members"].as_array().expect("members").len(), 1);
    }

    #[tokio::test]
    async fn test_list_spaces() {
        let (app, _dir) = setup_app().await;
        let api_key = create_test_tenant(&app).await;

        app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/spaces")
                    .header("content-type", "application/json")
                    .header("x-api-key", &api_key)
                    .body(Body::from(
                        r#"{"name":"Team A","space_type":"team"}"#,
                    ))
                    .expect("request"),
            )
            .await
            .expect("response");

        app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/spaces")
                    .header("content-type", "application/json")
                    .header("x-api-key", &api_key)
                    .body(Body::from(
                        r#"{"name":"Team B","space_type":"team"}"#,
                    ))
                    .expect("request"),
            )
            .await
            .expect("response");

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/v1/spaces")
                    .header("x-api-key", &api_key)
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);

        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        let spaces = json.as_array().expect("array");
        assert_eq!(spaces.len(), 3);
    }

    #[tokio::test]
    async fn test_add_member() {
        let (app, _dir) = setup_app().await;
        let api_key = create_test_tenant(&app).await;

        let create_resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/spaces")
                    .header("content-type", "application/json")
                    .header("x-api-key", &api_key)
                    .body(Body::from(
                        r#"{"name":"Team Space","space_type":"team"}"#,
                    ))
                    .expect("request"),
            )
            .await
            .expect("response");

        let bytes = create_resp
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        let space: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        let space_id = space["id"].as_str().expect("id");

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/v1/spaces/{space_id}/members"))
                    .header("content-type", "application/json")
                    .header("x-api-key", &api_key)
                    .body(Body::from(
                        r#"{"user_id":"bob","role":"member"}"#,
                    ))
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);

        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        let members = json["members"].as_array().expect("members");
        assert_eq!(members.len(), 2);
        assert!(members.iter().any(|m| m["user_id"] == "bob"));
    }

    #[tokio::test]
    async fn test_remove_member() {
        let (app, _dir) = setup_app().await;
        let api_key = create_test_tenant(&app).await;

        let create_resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/spaces")
                    .header("content-type", "application/json")
                    .header("x-api-key", &api_key)
                    .body(Body::from(
                        r#"{"name":"Team","space_type":"team","members":[{"user_id":"alice","role":"member"}]}"#,
                    ))
                    .expect("request"),
            )
            .await
            .expect("response");

        let bytes = create_resp
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        let space: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        let space_id = space["id"].as_str().expect("id");
        assert_eq!(space["members"].as_array().expect("members").len(), 2);

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!("/v1/spaces/{space_id}/members/alice"))
                    .header("x-api-key", &api_key)
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);

        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        let members = json["members"].as_array().expect("members");
        assert_eq!(members.len(), 1);
        assert!(!members.iter().any(|m| m["user_id"] == "alice"));
    }

    #[tokio::test]
    async fn test_tenant_auto_creates_personal_space() {
        let (app, _dir) = setup_app().await;
        let api_key = create_test_tenant(&app).await;

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/v1/spaces")
                    .header("x-api-key", &api_key)
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);

        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        let spaces = json.as_array().expect("array");
        assert_eq!(spaces.len(), 1);
        assert!(spaces[0]["id"]
            .as_str()
            .expect("id")
            .starts_with("personal/"));
        assert_eq!(spaces[0]["space_type"], "personal");
    }

    #[tokio::test]
    async fn test_multi_space_search() {
        let (app, _dir) = setup_app().await;
        let api_key = create_test_tenant(&app).await;

        app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/memories")
                    .header("content-type", "application/json")
                    .header("x-api-key", &api_key)
                    .body(Body::from(r#"{"content":"user prefers dark mode"}"#))
                    .expect("request"),
            )
            .await
            .expect("response");

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/v1/memories/search?q=dark+mode&limit=10")
                    .header("x-api-key", &api_key)
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);

        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert!(json["results"].as_array().is_some());
    }

    #[tokio::test]
    async fn test_delete_space() {
        let (app, _dir) = setup_app().await;
        let api_key = create_test_tenant(&app).await;

        let create_resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/spaces")
                    .header("content-type", "application/json")
                    .header("x-api-key", &api_key)
                    .body(Body::from(
                        r#"{"name":"Temp","space_type":"team"}"#,
                    ))
                    .expect("request"),
            )
            .await
            .expect("response");

        let bytes = create_resp
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        let space: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        let space_id = space["id"].as_str().expect("id");

        let del_resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!("/v1/spaces/{space_id}"))
                    .header("x-api-key", &api_key)
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(del_resp.status(), StatusCode::OK);

        let get_resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/v1/spaces/{space_id}"))
                    .header("x-api-key", &api_key)
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(get_resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_update_member_role() {
        let (app, _dir) = setup_app().await;
        let api_key = create_test_tenant(&app).await;

        let create_resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/spaces")
                    .header("content-type", "application/json")
                    .header("x-api-key", &api_key)
                    .body(Body::from(
                        r#"{"name":"Team","space_type":"team","members":[{"user_id":"carol","role":"reader"}]}"#,
                    ))
                    .expect("request"),
            )
            .await
            .expect("response");

        let bytes = create_resp
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        let space: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        let space_id = space["id"].as_str().expect("id");

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri(format!("/v1/spaces/{space_id}/members/carol"))
                    .header("content-type", "application/json")
                    .header("x-api-key", &api_key)
                    .body(Body::from(r#"{"role":"admin"}"#))
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);

        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        let carol = json["members"]
            .as_array()
            .expect("members")
            .iter()
            .find(|m| m["user_id"] == "carol")
            .expect("carol");
        assert_eq!(carol["role"], "admin");
    }

    fn build_multipart(fields: &[(&str, &str)], file: Option<(&str, &str)>) -> (String, Vec<u8>) {
        let boundary = "----TestBoundary7MA4YWxkTrZu0gW";
        let mut body = Vec::new();
        for (name, value) in fields {
            body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
            body.extend_from_slice(
                format!("Content-Disposition: form-data; name=\"{name}\"\r\n\r\n").as_bytes(),
            );
            body.extend_from_slice(format!("{value}\r\n").as_bytes());
        }
        if let Some((filename, content)) = file {
            body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
            body.extend_from_slice(
                format!(
                    "Content-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\nContent-Type: application/octet-stream\r\n\r\n"
                )
                .as_bytes(),
            );
            body.extend_from_slice(content.as_bytes());
            body.extend_from_slice(b"\r\n");
        }
        body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
        let ct = format!("multipart/form-data; boundary={boundary}");
        (ct, body)
    }

    #[tokio::test]
    async fn test_force_import() {
        let (app, _dir) = setup_app().await;
        let api_key = create_test_tenant(&app).await;

        let file_content = "hello world test data for force import";

        // --- First import: should succeed ---
        let (ct, body) = build_multipart(
            &[("file_type", "memory"), ("post_process", "false")],
            Some(("test.txt", file_content)),
        );
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/imports")
                    .header("content-type", &ct)
                    .header("x-api-key", &api_key)
                    .body(Body::from(body))
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = resp.into_body().collect().await.expect("body").to_bytes();
        let first: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        let first_id = first["id"].as_str().expect("first import id").to_string();

        // --- Second import (same content, no force): should be rejected ---
        let (ct, body) = build_multipart(
            &[("file_type", "memory"), ("post_process", "false")],
            Some(("test.txt", file_content)),
        );
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/imports")
                    .header("content-type", &ct)
                    .header("x-api-key", &api_key)
                    .body(Body::from(body))
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let bytes = resp.into_body().collect().await.expect("body").to_bytes();
        let err: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        let msg = err["error"]["message"].as_str().unwrap_or("");
        assert!(msg.contains("duplicate"), "expected duplicate error, got: {msg}");

        // --- Third import (same content, force=true): should succeed ---
        let (ct, body) = build_multipart(
            &[
                ("file_type", "memory"),
                ("post_process", "false"),
                ("force", "true"),
            ],
            Some(("test.txt", file_content)),
        );
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/imports")
                    .header("content-type", &ct)
                    .header("x-api-key", &api_key)
                    .body(Body::from(body))
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = resp.into_body().collect().await.expect("body").to_bytes();
        let forced: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        let forced_id = forced["id"].as_str().expect("forced import id").to_string();
        assert_ne!(first_id, forced_id, "force import should create new task");
    }

    #[tokio::test]
    async fn test_force_import_rollback() {
        let (app, _dir) = setup_app().await;
        let api_key = create_test_tenant(&app).await;

        let file_content = "rollback test data for force import";

        // --- First normal import ---
        let (ct, body) = build_multipart(
            &[("file_type", "memory"), ("post_process", "false")],
            Some(("rollback.txt", file_content)),
        );
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/imports")
                    .header("content-type", &ct)
                    .header("x-api-key", &api_key)
                    .body(Body::from(body))
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = resp.into_body().collect().await.expect("body").to_bytes();
        let first: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        let first_id = first["id"].as_str().expect("id").to_string();

        // --- Force import same content ---
        let (ct, body) = build_multipart(
            &[
                ("file_type", "memory"),
                ("post_process", "false"),
                ("force", "true"),
            ],
            Some(("rollback.txt", file_content)),
        );
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/imports")
                    .header("content-type", &ct)
                    .header("x-api-key", &api_key)
                    .body(Body::from(body))
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = resp.into_body().collect().await.expect("body").to_bytes();
        let forced: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        let forced_id = forced["id"].as_str().expect("id").to_string();

        // rollback only the forced import
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/v1/imports/{forced_id}/rollback"))
                    .header("x-api-key", &api_key)
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = resp.into_body().collect().await.expect("body").to_bytes();
        let rb: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(rb["import_status"], "rolled_back");

        // --- First import should still be intact ---
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/v1/imports/{first_id}"))
                    .header("x-api-key", &api_key)
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = resp.into_body().collect().await.expect("body").to_bytes();
        let original: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(original["status"], "completed");
    }
}
