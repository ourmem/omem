use std::sync::Arc;

use crate::domain::error::OmemError;
use crate::domain::types::MemoryState;
use crate::lifecycle::decay::parse_datetime;
use crate::store::lancedb::LanceStore;

pub struct AutoForgetter {
    store: Arc<LanceStore>,
}

impl AutoForgetter {
    pub fn new(store: Arc<LanceStore>) -> Self {
        Self { store }
    }

    pub fn detect_ttl(content: &str) -> Option<chrono::TimeDelta> {
        let lower = content.to_lowercase();

        if lower.contains("today") || lower.contains("今天") {
            return chrono::TimeDelta::try_days(2);
        }
        if lower.contains("tomorrow") || lower.contains("明天") {
            return chrono::TimeDelta::try_days(2);
        }
        if lower.contains("next week") || lower.contains("下周") {
            return chrono::TimeDelta::try_days(10);
        }
        if lower.contains("this month") || lower.contains("这个月") {
            return chrono::TimeDelta::try_days(35);
        }

        None
    }

    pub async fn cleanup_expired(&self) -> Result<usize, OmemError> {
        let memories = self.store.list(10000, 0).await?;
        let now = chrono::Utc::now();
        let mut deleted_count = 0;

        for memory in memories {
            if let Some(ttl) = Self::detect_ttl(&memory.content) {
                if let Some(created) = parse_datetime(&memory.created_at) {
                    if created + ttl < now {
                        self.store.soft_delete(&memory.id).await?;
                        deleted_count += 1;
                    }
                }
            }
        }

        Ok(deleted_count)
    }

    pub async fn archive_superseded(&self, max_age_days: u32) -> Result<usize, OmemError> {
        let memories = self.store.list(10000, 0).await?;
        let now = chrono::Utc::now();
        let max_age = chrono::TimeDelta::try_days(max_age_days as i64)
            .unwrap_or_else(chrono::TimeDelta::zero);
        let mut archived_count = 0;

        for memory in memories {
            if memory.superseded_by.is_some() {
                if let Some(created) = parse_datetime(&memory.created_at) {
                    if now - created > max_age {
                        let mut archived = memory;
                        archived.state = MemoryState::Archived;
                        archived.updated_at = now.to_rfc3339();
                        self.store.update(&archived, None).await?;
                        archived_count += 1;
                    }
                }
            }
        }

        Ok(archived_count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::category::Category;
    use crate::domain::memory::Memory;
    use crate::domain::types::MemoryType;
    use tempfile::TempDir;

    async fn setup() -> (Arc<LanceStore>, TempDir) {
        let dir = TempDir::new().expect("failed to create temp dir");
        let store = LanceStore::new(dir.path().to_str().expect("invalid path"))
            .await
            .expect("failed to create store");
        store.init_table().await.expect("failed to init table");
        (Arc::new(store), dir)
    }

    fn make_memory(tenant: &str, content: &str) -> Memory {
        Memory::new(content, Category::Preferences, MemoryType::Insight, tenant)
    }

    fn days_ago_str(n: i64) -> String {
        let delta = chrono::TimeDelta::try_days(n).unwrap_or_default();
        (chrono::Utc::now() - delta).to_rfc3339()
    }

    #[test]
    fn test_ttl_detection_tomorrow() {
        let ttl = AutoForgetter::detect_ttl("meeting tomorrow at 3pm");
        assert!(ttl.is_some());
        let days = ttl.expect("should have ttl").num_days();
        assert_eq!(days, 2);
    }

    #[test]
    fn test_ttl_detection_tomorrow_chinese() {
        let ttl = AutoForgetter::detect_ttl("明天下午三点开会");
        assert!(ttl.is_some());
        assert_eq!(ttl.expect("should have ttl").num_days(), 2);
    }

    #[test]
    fn test_ttl_detection_next_week() {
        let ttl = AutoForgetter::detect_ttl("exam next week on Friday");
        assert!(ttl.is_some());
        assert_eq!(ttl.expect("should have ttl").num_days(), 10);
    }

    #[test]
    fn test_ttl_detection_next_week_chinese() {
        let ttl = AutoForgetter::detect_ttl("下周五有考试");
        assert!(ttl.is_some());
        assert_eq!(ttl.expect("should have ttl").num_days(), 10);
    }

    #[test]
    fn test_ttl_detection_this_month() {
        let ttl = AutoForgetter::detect_ttl("deadline this month");
        assert!(ttl.is_some());
        assert_eq!(ttl.expect("should have ttl").num_days(), 35);
    }

    #[test]
    fn test_ttl_detection_today() {
        let ttl = AutoForgetter::detect_ttl("finish report today");
        assert!(ttl.is_some());
        assert_eq!(ttl.expect("should have ttl").num_days(), 2);
    }

    #[test]
    fn test_no_ttl_for_permanent() {
        assert!(AutoForgetter::detect_ttl("I like Rust").is_none());
        assert!(AutoForgetter::detect_ttl("my favorite color is blue").is_none());
        assert!(AutoForgetter::detect_ttl("").is_none());
    }

    #[tokio::test]
    async fn test_cleanup_removes_expired() {
        let (store, _dir) = setup().await;

        let dim = 1024;
        let v = vec![0.1f32; dim];

        let mut m_expired = make_memory("t-001", "meeting today at 3pm");
        m_expired.created_at = days_ago_str(5);
        m_expired.updated_at = m_expired.created_at.clone();
        store.create(&m_expired, Some(&v)).await.expect("create");

        let mut m_fresh = make_memory("t-001", "meeting today at 5pm");
        m_fresh.created_at = days_ago_str(0);
        m_fresh.updated_at = m_fresh.created_at.clone();
        store.create(&m_fresh, Some(&v)).await.expect("create");

        let mut m_permanent = make_memory("t-001", "I like Rust programming");
        m_permanent.created_at = days_ago_str(100);
        m_permanent.updated_at = m_permanent.created_at.clone();
        store.create(&m_permanent, Some(&v)).await.expect("create");

        let forgetter = AutoForgetter::new(store.clone());
        let count = forgetter.cleanup_expired().await.expect("cleanup");

        assert_eq!(
            count, 1,
            "only the expired 'today' memory (5 days old) should be deleted"
        );

        let remaining = store.list(100, 0).await.expect("list");
        assert_eq!(remaining.len(), 2);
    }

    #[tokio::test]
    async fn test_archive_superseded() {
        let (store, _dir) = setup().await;

        let dim = 1024;
        let v = vec![0.1f32; dim];

        let mut m_old_superseded = make_memory("t-001", "old preference A");
        m_old_superseded.superseded_by = Some("new-id".to_string());
        m_old_superseded.created_at = days_ago_str(60);
        m_old_superseded.updated_at = m_old_superseded.created_at.clone();
        store
            .create(&m_old_superseded, Some(&v))
            .await
            .expect("create");

        let mut m_recent_superseded = make_memory("t-001", "recent preference B");
        m_recent_superseded.superseded_by = Some("newer-id".to_string());
        m_recent_superseded.created_at = days_ago_str(5);
        m_recent_superseded.updated_at = m_recent_superseded.created_at.clone();
        store
            .create(&m_recent_superseded, Some(&v))
            .await
            .expect("create");

        let m_active = make_memory("t-001", "active preference C");
        store.create(&m_active, Some(&v)).await.expect("create");

        let forgetter = AutoForgetter::new(store.clone());
        let count = forgetter.archive_superseded(30).await.expect("archive");

        assert_eq!(
            count, 1,
            "only the old superseded memory should be archived"
        );

        let mem = store
            .get_by_id(&m_old_superseded.id)
            .await
            .expect("get")
            .expect("should exist");
        assert_eq!(mem.state, MemoryState::Archived);
    }

    #[tokio::test]
    async fn test_cleanup_no_expired() {
        let (store, _dir) = setup().await;

        let v = vec![0.1f32; 1024];
        let m = make_memory("t-001", "I like Rust");
        store.create(&m, Some(&v)).await.expect("create");

        let forgetter = AutoForgetter::new(store);
        let count = forgetter.cleanup_expired().await.expect("cleanup");
        assert_eq!(count, 0);
    }
}
