use std::sync::Arc;

use arrow_array::{RecordBatch, RecordBatchIterator, StringArray};
use arrow_schema::{DataType, Field, Schema};
use futures::TryStreamExt;
use lancedb::query::{ExecutableQuery, QueryBase};
use lancedb::Connection;
use serde::{Deserialize, Serialize};

use crate::domain::error::OmemError;
use crate::domain::space::{SharingEvent, Space};

const SPACES_TABLE: &str = "spaces";
const SHARING_EVENTS_TABLE: &str = "sharing_events";
const IMPORT_TASKS_TABLE: &str = "import_tasks";

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ImportTaskRecord {
    pub id: String,
    pub status: String,
    pub file_type: String,
    pub filename: String,
    pub agent_id: Option<String>,
    pub space_id: String,
    pub post_process: bool,

    #[serde(default = "default_strategy")]
    pub strategy: String,

    // Stage 1: Storage
    pub storage_total: usize,
    pub storage_stored: usize,
    pub storage_skipped: usize,

    // Stage 2: Extraction
    pub extraction_status: String,
    pub extraction_chunks: usize,
    pub extraction_facts: usize,
    pub extraction_progress: usize,

    // Stage 3: Reconciliation
    pub reconcile_status: String,
    pub reconcile_relations: usize,
    pub reconcile_merged: usize,
    pub reconcile_progress: usize,

    pub errors: Vec<String>,
    pub created_at: String,
    pub completed_at: Option<String>,
}

pub struct SpaceStore {
    spaces_db: Connection,
}

fn default_strategy() -> String {
    "auto".to_string()
}

impl SpaceStore {
    pub async fn new(system_uri: &str) -> Result<Self, OmemError> {
        let spaces_db = lancedb::connect(system_uri)
            .execute()
            .await
            .map_err(|e| OmemError::Storage(format!("space store connect failed: {e}")))?;
        Ok(Self { spaces_db })
    }

    pub async fn init_tables(&self) -> Result<(), OmemError> {
        let existing = self
            .spaces_db
            .table_names()
            .execute()
            .await
            .map_err(|e| OmemError::Storage(format!("failed to list tables: {e}")))?;

        if !existing.contains(&SPACES_TABLE.to_string()) {
            self.spaces_db
                .create_empty_table(SPACES_TABLE, Self::spaces_schema())
                .execute()
                .await
                .map_err(|e| OmemError::Storage(format!("failed to create spaces table: {e}")))?;
        }

        if !existing.contains(&SHARING_EVENTS_TABLE.to_string()) {
            self.spaces_db
                .create_empty_table(SHARING_EVENTS_TABLE, Self::events_schema())
                .execute()
                .await
                .map_err(|e| {
                    OmemError::Storage(format!("failed to create sharing_events table: {e}"))
                })?;
        }

        if !existing.contains(&IMPORT_TASKS_TABLE.to_string()) {
            self.spaces_db
                .create_empty_table(IMPORT_TASKS_TABLE, Self::import_tasks_schema())
                .execute()
                .await
                .map_err(|e| {
                    OmemError::Storage(format!("failed to create import_tasks table: {e}"))
                })?;
        }

        Ok(())
    }

    fn spaces_schema() -> Arc<Schema> {
        Arc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("data", DataType::Utf8, false),
            Field::new("space_type", DataType::Utf8, false),
            Field::new("owner_id", DataType::Utf8, false),
            Field::new("created_at", DataType::Utf8, false),
        ]))
    }

    fn events_schema() -> Arc<Schema> {
        Arc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("action", DataType::Utf8, false),
            Field::new("memory_id", DataType::Utf8, false),
            Field::new("from_space", DataType::Utf8, false),
            Field::new("to_space", DataType::Utf8, false),
            Field::new("user_id", DataType::Utf8, false),
            Field::new("agent_id", DataType::Utf8, false),
            Field::new("content_preview", DataType::Utf8, false),
            Field::new("timestamp", DataType::Utf8, false),
        ]))
    }

    fn import_tasks_schema() -> Arc<Schema> {
        Arc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("data", DataType::Utf8, false),
            Field::new("space_id", DataType::Utf8, false),
            Field::new("status", DataType::Utf8, false),
            Field::new("created_at", DataType::Utf8, false),
        ]))
    }

    async fn open_spaces_table(&self) -> Result<lancedb::table::Table, OmemError> {
        self.spaces_db
            .open_table(SPACES_TABLE)
            .execute()
            .await
            .map_err(|e| OmemError::Storage(format!("failed to open spaces table: {e}")))
    }

    async fn open_events_table(&self) -> Result<lancedb::table::Table, OmemError> {
        self.spaces_db
            .open_table(SHARING_EVENTS_TABLE)
            .execute()
            .await
            .map_err(|e| OmemError::Storage(format!("failed to open sharing_events table: {e}")))
    }

    async fn open_import_tasks_table(&self) -> Result<lancedb::table::Table, OmemError> {
        self.spaces_db
            .open_table(IMPORT_TASKS_TABLE)
            .execute()
            .await
            .map_err(|e| OmemError::Storage(format!("failed to open import_tasks table: {e}")))
    }

    pub async fn create_space(&self, space: &Space) -> Result<(), OmemError> {
        let data_json = serde_json::to_string(space)
            .map_err(|e| OmemError::Storage(format!("failed to serialize space: {e}")))?;

        let batch = RecordBatch::try_new(
            Self::spaces_schema(),
            vec![
                Arc::new(StringArray::from(vec![space.id.as_str()])),
                Arc::new(StringArray::from(vec![data_json.as_str()])),
                Arc::new(StringArray::from(vec![space.space_type.to_string().as_str()])),
                Arc::new(StringArray::from(vec![space.owner_id.as_str()])),
                Arc::new(StringArray::from(vec![space.created_at.as_str()])),
            ],
        )
        .map_err(|e| OmemError::Storage(format!("failed to build space batch: {e}")))?;

        let table = self.open_spaces_table().await?;
        let reader = RecordBatchIterator::new(vec![Ok(batch)], Self::spaces_schema());
        table
            .add(Box::new(reader) as Box<dyn arrow_array::RecordBatchReader + Send>)
            .execute()
            .await
            .map_err(|e| OmemError::Storage(format!("failed to insert space: {e}")))?;

        Ok(())
    }

    pub async fn get_space(&self, id: &str) -> Result<Option<Space>, OmemError> {
        let table = self.open_spaces_table().await?;
        let batches: Vec<RecordBatch> = table
            .query()
            .only_if(format!("id = '{}'", escape_sql(id)))
            .limit(1)
            .execute()
            .await
            .map_err(|e| OmemError::Storage(format!("space query failed: {e}")))?
            .try_collect()
            .await
            .map_err(|e| OmemError::Storage(format!("collect failed: {e}")))?;

        for batch in &batches {
            if batch.num_rows() > 0 {
                return Ok(Some(Self::row_to_space(batch, 0)?));
            }
        }
        Ok(None)
    }

    pub async fn list_spaces_for_user(&self, user_id: &str) -> Result<Vec<Space>, OmemError> {
        let table = self.open_spaces_table().await?;
        let batches: Vec<RecordBatch> = table
            .query()
            .only_if(format!("owner_id = '{}'", escape_sql(user_id)))
            .execute()
            .await
            .map_err(|e| OmemError::Storage(format!("list spaces query failed: {e}")))?
            .try_collect()
            .await
            .map_err(|e| OmemError::Storage(format!("collect failed: {e}")))?;

        let mut spaces = Vec::new();
        for batch in &batches {
            for i in 0..batch.num_rows() {
                spaces.push(Self::row_to_space(batch, i)?);
            }
        }
        Ok(spaces)
    }

    pub async fn update_space(&self, space: &Space) -> Result<(), OmemError> {
        let table = self.open_spaces_table().await?;
        table
            .delete(&format!("id = '{}'", escape_sql(&space.id)))
            .await
            .map_err(|e| OmemError::Storage(format!("delete for space update failed: {e}")))?;

        self.create_space(space).await
    }

    pub async fn delete_space(&self, id: &str) -> Result<(), OmemError> {
        let table = self.open_spaces_table().await?;
        table
            .delete(&format!("id = '{}'", escape_sql(id)))
            .await
            .map_err(|e| OmemError::Storage(format!("space delete failed: {e}")))?;
        Ok(())
    }

    pub async fn record_sharing_event(&self, event: &SharingEvent) -> Result<(), OmemError> {
        let batch = RecordBatch::try_new(
            Self::events_schema(),
            vec![
                Arc::new(StringArray::from(vec![event.id.as_str()])),
                Arc::new(StringArray::from(vec![event.action.to_string().as_str()])),
                Arc::new(StringArray::from(vec![event.memory_id.as_str()])),
                Arc::new(StringArray::from(vec![event.from_space.as_str()])),
                Arc::new(StringArray::from(vec![event.to_space.as_str()])),
                Arc::new(StringArray::from(vec![event.user_id.as_str()])),
                Arc::new(StringArray::from(vec![event.agent_id.as_str()])),
                Arc::new(StringArray::from(vec![event.content_preview.as_str()])),
                Arc::new(StringArray::from(vec![event.timestamp.as_str()])),
            ],
        )
        .map_err(|e| OmemError::Storage(format!("failed to build event batch: {e}")))?;

        let table = self.open_events_table().await?;
        let reader = RecordBatchIterator::new(vec![Ok(batch)], Self::events_schema());
        table
            .add(Box::new(reader) as Box<dyn arrow_array::RecordBatchReader + Send>)
            .execute()
            .await
            .map_err(|e| OmemError::Storage(format!("failed to insert sharing event: {e}")))?;

        Ok(())
    }

    pub async fn list_sharing_events(
        &self,
        space_id: &str,
        limit: usize,
    ) -> Result<Vec<SharingEvent>, OmemError> {
        let table = self.open_events_table().await?;
        let filter = format!(
            "from_space = '{}' OR to_space = '{}'",
            escape_sql(space_id),
            escape_sql(space_id)
        );
        let batches: Vec<RecordBatch> = table
            .query()
            .only_if(filter)
            .limit(limit)
            .execute()
            .await
            .map_err(|e| OmemError::Storage(format!("list events query failed: {e}")))?
            .try_collect()
            .await
            .map_err(|e| OmemError::Storage(format!("collect failed: {e}")))?;

        let mut events = Vec::new();
        for batch in &batches {
            for i in 0..batch.num_rows() {
                events.push(Self::row_to_event(batch, i)?);
            }
        }
        Ok(events)
    }

    pub async fn create_import_task(&self, task: &ImportTaskRecord) -> Result<(), OmemError> {
        let data_json = serde_json::to_string(task)
            .map_err(|e| OmemError::Storage(format!("failed to serialize import task: {e}")))?;

        let batch = RecordBatch::try_new(
            Self::import_tasks_schema(),
            vec![
                Arc::new(StringArray::from(vec![task.id.as_str()])),
                Arc::new(StringArray::from(vec![data_json.as_str()])),
                Arc::new(StringArray::from(vec![task.space_id.as_str()])),
                Arc::new(StringArray::from(vec![task.status.as_str()])),
                Arc::new(StringArray::from(vec![task.created_at.as_str()])),
            ],
        )
        .map_err(|e| OmemError::Storage(format!("failed to build import task batch: {e}")))?;

        let table = self.open_import_tasks_table().await?;
        let reader = RecordBatchIterator::new(vec![Ok(batch)], Self::import_tasks_schema());
        table
            .add(Box::new(reader) as Box<dyn arrow_array::RecordBatchReader + Send>)
            .execute()
            .await
            .map_err(|e| OmemError::Storage(format!("failed to insert import task: {e}")))?;

        Ok(())
    }

    pub async fn get_import_task(&self, id: &str) -> Result<Option<ImportTaskRecord>, OmemError> {
        let table = self.open_import_tasks_table().await?;
        let batches: Vec<RecordBatch> = table
            .query()
            .only_if(format!("id = '{}'", escape_sql(id)))
            .limit(1)
            .execute()
            .await
            .map_err(|e| OmemError::Storage(format!("import task query failed: {e}")))?
            .try_collect()
            .await
            .map_err(|e| OmemError::Storage(format!("collect failed: {e}")))?;

        for batch in &batches {
            if batch.num_rows() > 0 {
                return Ok(Some(Self::row_to_import_task(batch, 0)?));
            }
        }
        Ok(None)
    }

    pub async fn update_import_task(&self, task: &ImportTaskRecord) -> Result<(), OmemError> {
        let table = self.open_import_tasks_table().await?;
        table
            .delete(&format!("id = '{}'", escape_sql(&task.id)))
            .await
            .map_err(|e| OmemError::Storage(format!("delete for import task update failed: {e}")))?;

        self.create_import_task(task).await
    }

    pub async fn list_import_tasks(
        &self,
        space_id: &str,
        limit: usize,
    ) -> Result<Vec<ImportTaskRecord>, OmemError> {
        let table = self.open_import_tasks_table().await?;
        let batches: Vec<RecordBatch> = table
            .query()
            .only_if(format!("space_id = '{}'", escape_sql(space_id)))
            .limit(limit)
            .execute()
            .await
            .map_err(|e| OmemError::Storage(format!("list import tasks query failed: {e}")))?
            .try_collect()
            .await
            .map_err(|e| OmemError::Storage(format!("collect failed: {e}")))?;

        let mut tasks = Vec::new();
        for batch in &batches {
            for i in 0..batch.num_rows() {
                tasks.push(Self::row_to_import_task(batch, i)?);
            }
        }
        Ok(tasks)
    }

    pub async fn list_all_import_tasks(
        &self,
        limit: usize,
    ) -> Result<Vec<ImportTaskRecord>, OmemError> {
        let table = self.open_import_tasks_table().await?;
        let batches: Vec<RecordBatch> = table
            .query()
            .limit(limit)
            .execute()
            .await
            .map_err(|e| OmemError::Storage(format!("list all import tasks query failed: {e}")))?
            .try_collect()
            .await
            .map_err(|e| OmemError::Storage(format!("collect failed: {e}")))?;

        let mut tasks = Vec::new();
        for batch in &batches {
            for i in 0..batch.num_rows() {
                tasks.push(Self::row_to_import_task(batch, i)?);
            }
        }
        Ok(tasks)
    }

    fn row_to_space(batch: &RecordBatch, row: usize) -> Result<Space, OmemError> {
        let col = batch
            .column_by_name("data")
            .ok_or_else(|| OmemError::Storage("missing column: data".to_string()))?;
        let arr = col
            .as_any()
            .downcast_ref::<StringArray>()
            .ok_or_else(|| OmemError::Storage("column data is not Utf8".to_string()))?;
        let data_json = arr.value(row);
        serde_json::from_str(data_json)
            .map_err(|e| OmemError::Storage(format!("failed to parse space data: {e}")))
    }

    fn row_to_import_task(batch: &RecordBatch, row: usize) -> Result<ImportTaskRecord, OmemError> {
        let col = batch
            .column_by_name("data")
            .ok_or_else(|| OmemError::Storage("missing column: data".to_string()))?;
        let arr = col
            .as_any()
            .downcast_ref::<StringArray>()
            .ok_or_else(|| OmemError::Storage("column data is not Utf8".to_string()))?;
        let data_json = arr.value(row);
        serde_json::from_str(data_json)
            .map_err(|e| OmemError::Storage(format!("failed to parse import task data: {e}")))
    }

    pub async fn list_all_events(&self, limit: usize) -> Result<Vec<SharingEvent>, OmemError> {
        let table = self.open_events_table().await?;
        let batches: Vec<RecordBatch> = table
            .query()
            .limit(limit)
            .execute()
            .await
            .map_err(|e| OmemError::Storage(format!("list all events query failed: {e}")))?
            .try_collect()
            .await
            .map_err(|e| OmemError::Storage(format!("collect failed: {e}")))?;

        let mut events = Vec::new();
        for batch in &batches {
            for i in 0..batch.num_rows() {
                events.push(Self::row_to_event(batch, i)?);
            }
        }
        Ok(events)
    }

    fn row_to_event(batch: &RecordBatch, row: usize) -> Result<SharingEvent, OmemError> {
        let get_str = |name: &str| -> Result<String, OmemError> {
            let col = batch
                .column_by_name(name)
                .ok_or_else(|| OmemError::Storage(format!("missing column: {name}")))?;
            let arr = col
                .as_any()
                .downcast_ref::<StringArray>()
                .ok_or_else(|| OmemError::Storage(format!("column {name} is not Utf8")))?;
            Ok(arr.value(row).to_string())
        };

        let action_str = get_str("action")?;
        let action = serde_json::from_str(&format!("\"{action_str}\""))
            .map_err(|e| OmemError::Storage(format!("failed to parse sharing action: {e}")))?;

        Ok(SharingEvent {
            id: get_str("id")?,
            action,
            memory_id: get_str("memory_id")?,
            from_space: get_str("from_space")?,
            to_space: get_str("to_space")?,
            user_id: get_str("user_id")?,
            agent_id: get_str("agent_id")?,
            content_preview: get_str("content_preview")?,
            timestamp: get_str("timestamp")?,
        })
    }
}

fn escape_sql(s: &str) -> String {
    s.replace('\'', "''")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::space::{MemberRole, SharingAction, SpaceMember, SpaceType};
    use tempfile::TempDir;

    async fn setup() -> (SpaceStore, TempDir) {
        let dir = TempDir::new().expect("temp dir");
        let store = SpaceStore::new(dir.path().to_str().expect("path"))
            .await
            .expect("space store");
        store.init_tables().await.expect("init");
        (store, dir)
    }

    fn make_space(id: &str, name: &str, owner: &str) -> Space {
        Space {
            id: id.to_string(),
            space_type: SpaceType::Team,
            name: name.to_string(),
            owner_id: owner.to_string(),
            members: vec![SpaceMember {
                user_id: owner.to_string(),
                role: MemberRole::Admin,
                joined_at: "2025-01-01T00:00:00Z".to_string(),
            }],
            auto_share_rules: Vec::new(),
            created_at: "2025-01-01T00:00:00Z".to_string(),
            updated_at: "2025-01-01T00:00:00Z".to_string(),
        }
    }

    #[tokio::test]
    async fn test_space_store_crud() {
        let (store, _dir) = setup().await;

        let space = make_space("team:backend", "Backend Team", "user-001");
        store.create_space(&space).await.expect("create");

        let fetched = store.get_space("team:backend").await.expect("get");
        assert!(fetched.is_some());
        let fetched = fetched.expect("space exists");
        assert_eq!(fetched.id, "team:backend");
        assert_eq!(fetched.name, "Backend Team");
        assert_eq!(fetched.space_type, SpaceType::Team);
        assert_eq!(fetched.members.len(), 1);

        let spaces = store
            .list_spaces_for_user("user-001")
            .await
            .expect("list");
        assert_eq!(spaces.len(), 1);

        store
            .delete_space("team:backend")
            .await
            .expect("delete");

        let deleted = store.get_space("team:backend").await.expect("get after delete");
        assert!(deleted.is_none());
    }

    #[tokio::test]
    async fn test_space_store_update() {
        let (store, _dir) = setup().await;

        let mut space = make_space("team:fe", "Frontend", "user-001");
        store.create_space(&space).await.expect("create");

        space.name = "Frontend Team".to_string();
        space.updated_at = "2025-06-01T00:00:00Z".to_string();
        store.update_space(&space).await.expect("update");

        let fetched = store.get_space("team:fe").await.expect("get").expect("exists");
        assert_eq!(fetched.name, "Frontend Team");
    }

    #[tokio::test]
    async fn test_space_store_list_multiple_owners() {
        let (store, _dir) = setup().await;

        let s1 = make_space("personal:alice", "Alice", "alice");
        let s2 = make_space("personal:bob", "Bob", "bob");
        let s3 = make_space("team:shared", "Shared", "alice");

        store.create_space(&s1).await.expect("create s1");
        store.create_space(&s2).await.expect("create s2");
        store.create_space(&s3).await.expect("create s3");

        let alice_spaces = store.list_spaces_for_user("alice").await.expect("list alice");
        assert_eq!(alice_spaces.len(), 2);

        let bob_spaces = store.list_spaces_for_user("bob").await.expect("list bob");
        assert_eq!(bob_spaces.len(), 1);
    }

    #[tokio::test]
    async fn test_sharing_event_record() {
        let (store, _dir) = setup().await;

        let event = SharingEvent {
            id: "evt-001".to_string(),
            action: SharingAction::Share,
            memory_id: "mem-001".to_string(),
            from_space: "personal:alex".to_string(),
            to_space: "team:backend".to_string(),
            user_id: "user-001".to_string(),
            agent_id: "agent-001".to_string(),
            content_preview: "user prefers dark mode".to_string(),
            timestamp: "2025-01-01T00:00:00Z".to_string(),
        };

        store.record_sharing_event(&event).await.expect("record");

        let events = store
            .list_sharing_events("personal:alex", 100)
            .await
            .expect("list from_space");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].id, "evt-001");
        assert_eq!(events[0].action, SharingAction::Share);
        assert_eq!(events[0].from_space, "personal:alex");
        assert_eq!(events[0].to_space, "team:backend");

        let events_to = store
            .list_sharing_events("team:backend", 100)
            .await
            .expect("list to_space");
        assert_eq!(events_to.len(), 1);
    }

    #[tokio::test]
    async fn test_get_nonexistent_space() {
        let (store, _dir) = setup().await;
        let result = store.get_space("nonexistent").await.expect("query");
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_init_tables_idempotent() {
        let (store, _dir) = setup().await;
        store.init_tables().await.expect("second init should succeed");
    }
}
