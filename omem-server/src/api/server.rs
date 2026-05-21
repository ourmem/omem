use std::sync::Arc;

use tokio::sync::Semaphore;

use crate::config::OmemConfig;
use crate::embed::EmbedService;
use crate::llm::LlmService;
use crate::store::{SpaceStore, StoreManager, TenantStore};

pub struct AppState {
    pub store_manager: Arc<StoreManager>,
    pub tenant_store: Arc<TenantStore>,
    pub space_store: Arc<SpaceStore>,
    pub embed: Arc<dyn EmbedService>,
    pub llm: Arc<dyn LlmService>,
    pub config: OmemConfig,
    pub import_semaphore: Arc<Semaphore>,
    pub reconcile_semaphore: Arc<Semaphore>,
    pub share_rate_limiter: Arc<crate::api::rate_limit::RateLimiter>,
}

/// Map tenant_id to their personal Space ID.
/// All CRUD operations go through the personal space by default.
pub fn personal_space_id(tenant_id: &str) -> String {
    format!("personal/{tenant_id}")
}

/// Normalize a space ID: convert legacy colon-separated format to slash format.
/// e.g. "team:abc" → "team/abc", "org:xyz" → "org/xyz"
/// Already-slash IDs are returned unchanged.
pub fn normalize_space_id(space_id: &str) -> String {
    // Only convert the first colon after known prefixes (team, org, personal)
    if space_id.starts_with("team:")
        || space_id.starts_with("org:")
        || space_id.starts_with("personal:")
    {
        space_id.replacen(':', "/", 1)
    } else {
        space_id.to_string()
    }
}
