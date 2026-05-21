use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use super::lancedb::{LanceStore, DEFAULT_VECTOR_DIM};
use crate::domain::error::OmemError;
use crate::domain::space::{MemberRole, Space, SpaceType};

const DEFAULT_MAX_CACHED: usize = 1000;

struct CacheEntry {
    store: Arc<LanceStore>,
    last_accessed: std::time::Instant,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AccessLevel {
    ReadWrite,
    ReadOnly,
}

#[derive(Clone)]
pub struct AccessibleStore {
    pub store: Arc<LanceStore>,
    pub space_id: String,
    pub weight: f32,
    pub access: AccessLevel,
}

pub struct StoreManager {
    base_uri: String,
    vector_dim: i32,
    cache: Mutex<HashMap<String, CacheEntry>>,
    max_cached: usize,
}

impl StoreManager {
    /// Construct with the default 1024-dim vector schema (back-compat for callers
    /// that don't yet pass an embed-derived dim).
    pub fn new(base_uri: &str) -> Self {
        Self::with_vector_dim(base_uri, DEFAULT_VECTOR_DIM)
    }

    pub fn with_vector_dim(base_uri: &str, vector_dim: i32) -> Self {
        Self {
            base_uri: base_uri.trim_end_matches('/').to_string(),
            vector_dim,
            cache: Mutex::new(HashMap::new()),
            max_cached: DEFAULT_MAX_CACHED,
        }
    }

    #[cfg(test)]
    pub fn with_max_cached(base_uri: &str, max_cached: usize) -> Self {
        Self {
            base_uri: base_uri.trim_end_matches('/').to_string(),
            vector_dim: DEFAULT_VECTOR_DIM,
            cache: Mutex::new(HashMap::new()),
            max_cached,
        }
    }

    pub fn vector_dim(&self) -> i32 {
        self.vector_dim
    }

    pub async fn get_store(&self, tenant_id: &str) -> Result<Arc<LanceStore>, OmemError> {
        let mut cache = self.cache.lock().await;

        if let Some(entry) = cache.get_mut(tenant_id) {
            entry.last_accessed = std::time::Instant::now();
            return Ok(entry.store.clone());
        }

        if cache.len() >= self.max_cached {
            let oldest_key = cache
                .iter()
                .min_by_key(|(_, v)| v.last_accessed)
                .map(|(k, _)| k.clone());
            if let Some(key) = oldest_key {
                tracing::debug!(tenant_id = %key, "evicting LRU tenant store");
                cache.remove(&key);
            }
        }

        let uri = format!("{}/{}", self.base_uri, tenant_id);
        let store = LanceStore::with_dim(&uri, self.vector_dim).await?;
        store.init_table().await?;
        let store = Arc::new(store);

        cache.insert(
            tenant_id.to_string(),
            CacheEntry {
                store: store.clone(),
                last_accessed: std::time::Instant::now(),
            },
        );

        Ok(store)
    }

    /// Weight by space type: personal=1.0 > team=0.8 > org=0.6.
    /// Used for multi-space search result ranking.
    pub async fn get_accessible_stores(
        &self,
        user_id: &str,
        spaces: &[Space],
    ) -> Result<Vec<AccessibleStore>, OmemError> {
        let mut stores = Vec::new();
        for space in spaces {
            let store = self.get_store(&space.id).await?;
            let weight = match space.space_type {
                SpaceType::Personal => 1.0,
                SpaceType::Team => 0.8,
                SpaceType::Organization => 0.6,
            };
            let access = Self::determine_access(user_id, space);
            stores.push(AccessibleStore {
                store,
                space_id: space.id.clone(),
                weight,
                access,
            });
        }
        Ok(stores)
    }

    fn determine_access(user_id: &str, space: &Space) -> AccessLevel {
        if space.owner_id == user_id {
            return AccessLevel::ReadWrite;
        }
        for member in &space.members {
            if member.user_id == user_id {
                return match member.role {
                    MemberRole::Admin | MemberRole::Member => AccessLevel::ReadWrite,
                    MemberRole::Reader => AccessLevel::ReadOnly,
                };
            }
        }
        AccessLevel::ReadOnly
    }

    pub async fn cache_size(&self) -> usize {
        self.cache.lock().await.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::category::Category;
    use crate::domain::memory::Memory;
    use crate::domain::types::MemoryType;
    use tempfile::TempDir;

    fn make_memory(tenant: &str, content: &str) -> Memory {
        Memory::new(content, Category::Preferences, MemoryType::Insight, tenant)
    }

    #[tokio::test]
    async fn test_store_manager_creates_tenant_store() {
        let dir = TempDir::new().expect("temp dir");
        let manager = StoreManager::new(dir.path().to_str().expect("path"));

        let store = manager.get_store("tenant-1").await.expect("get store");

        let mem = make_memory("tenant-1", "hello from tenant 1");
        store.create(&mem, None).await.expect("create");

        let fetched = store.get_by_id(&mem.id).await.expect("get");
        assert!(fetched.is_some());
        assert_eq!(fetched.expect("exists").content, "hello from tenant 1");
    }

    #[tokio::test]
    async fn test_store_manager_caches() {
        let dir = TempDir::new().expect("temp dir");
        let manager = StoreManager::new(dir.path().to_str().expect("path"));

        let store1 = manager.get_store("tenant-1").await.expect("get store");
        let store2 = manager
            .get_store("tenant-1")
            .await
            .expect("get store again");

        assert!(Arc::ptr_eq(&store1, &store2));
        assert_eq!(manager.cache_size().await, 1);
    }

    #[tokio::test]
    async fn test_store_manager_lru_eviction() {
        let dir = TempDir::new().expect("temp dir");
        let manager = StoreManager::with_max_cached(dir.path().to_str().expect("path"), 2);

        let _s1 = manager.get_store("t1").await.expect("t1");
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        let _s2 = manager.get_store("t2").await.expect("t2");
        assert_eq!(manager.cache_size().await, 2);

        let _s1_again = manager.get_store("t1").await.expect("t1 again");
        assert_eq!(manager.cache_size().await, 2);

        let _s3 = manager.get_store("t3").await.expect("t3");
        assert_eq!(manager.cache_size().await, 2);

        let s2_new = manager.get_store("t2").await.expect("t2 recreated");
        assert!(!Arc::ptr_eq(&_s2, &s2_new));
    }

    #[tokio::test]
    async fn test_tenant_isolation() {
        let dir = TempDir::new().expect("temp dir");
        let manager = StoreManager::new(dir.path().to_str().expect("path"));

        let store_a = manager.get_store("tenant-A").await.expect("store A");
        let store_b = manager.get_store("tenant-B").await.expect("store B");

        let mem_a = make_memory("tenant-A", "secret data for A");
        store_a.create(&mem_a, None).await.expect("create in A");

        let mem_b = make_memory("tenant-B", "secret data for B");
        store_b.create(&mem_b, None).await.expect("create in B");

        let list_a = store_a.list(100, 0, false).await.expect("list A");
        assert_eq!(list_a.len(), 1);
        assert_eq!(list_a[0].content, "secret data for A");

        let list_b = store_b.list(100, 0, false).await.expect("list B");
        assert_eq!(list_b.len(), 1);
        assert_eq!(list_b[0].content, "secret data for B");
    }
}
