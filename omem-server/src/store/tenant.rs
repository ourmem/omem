use std::sync::Arc;

use arrow_array::{RecordBatch, RecordBatchIterator, StringArray};
use arrow_schema::{DataType, Field, Schema};
use futures::TryStreamExt;
use lancedb::query::{ExecutableQuery, QueryBase};
use lancedb::Connection;

use crate::domain::error::OmemError;
use crate::domain::tenant::{Tenant, TenantConfig, TenantStatus};

const TENANT_TABLE: &str = "tenants";

pub struct TenantStore {
    db: Connection,
}

impl TenantStore {
    pub async fn new(uri: &str) -> Result<Self, OmemError> {
        let db = lancedb::connect(uri)
            .execute()
            .await
            .map_err(|e| OmemError::Storage(format!("tenant store connect failed: {e}")))?;
        Ok(Self { db })
    }

    pub async fn init_table(&self) -> Result<(), OmemError> {
        let existing = self
            .db
            .table_names()
            .execute()
            .await
            .map_err(|e| OmemError::Storage(format!("failed to list tables: {e}")))?;

        if existing.contains(&TENANT_TABLE.to_string()) {
            return Ok(());
        }

        self.db
            .create_empty_table(TENANT_TABLE, Self::schema())
            .execute()
            .await
            .map_err(|e| OmemError::Storage(format!("failed to create tenants table: {e}")))?;

        Ok(())
    }

    fn schema() -> Arc<Schema> {
        Arc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("name", DataType::Utf8, false),
            Field::new("status", DataType::Utf8, false),
            Field::new("config", DataType::Utf8, false),
            Field::new("created_at", DataType::Utf8, false),
        ]))
    }

    async fn open_table(&self) -> Result<lancedb::table::Table, OmemError> {
        self.db
            .open_table(TENANT_TABLE)
            .execute()
            .await
            .map_err(|e| OmemError::Storage(format!("failed to open tenants table: {e}")))
    }

    pub async fn create(&self, tenant: &Tenant) -> Result<(), OmemError> {
        let config_json = serde_json::to_string(&tenant.config)
            .map_err(|e| OmemError::Storage(format!("failed to serialize tenant config: {e}")))?;
        let status_str = tenant.status.to_string();

        let batch = RecordBatch::try_new(
            Self::schema(),
            vec![
                Arc::new(StringArray::from(vec![tenant.id.as_str()])),
                Arc::new(StringArray::from(vec![tenant.name.as_str()])),
                Arc::new(StringArray::from(vec![status_str.as_str()])),
                Arc::new(StringArray::from(vec![config_json.as_str()])),
                Arc::new(StringArray::from(vec![tenant.created_at.as_str()])),
            ],
        )
        .map_err(|e| OmemError::Storage(format!("failed to build tenant batch: {e}")))?;

        let table = self.open_table().await?;
        let reader = RecordBatchIterator::new(vec![Ok(batch)], Self::schema());
        table
            .add(Box::new(reader) as Box<dyn arrow_array::RecordBatchReader + Send>)
            .execute()
            .await
            .map_err(|e| OmemError::Storage(format!("failed to insert tenant: {e}")))?;

        Ok(())
    }

    pub async fn get_by_id(&self, id: &str) -> Result<Option<Tenant>, OmemError> {
        let table = self.open_table().await?;
        let batches: Vec<RecordBatch> = table
            .query()
            .only_if(format!("id = '{}'", escape_sql(id)))
            .limit(1)
            .execute()
            .await
            .map_err(|e| OmemError::Storage(format!("tenant query failed: {e}")))?
            .try_collect()
            .await
            .map_err(|e| OmemError::Storage(format!("collect failed: {e}")))?;

        for batch in &batches {
            if batch.num_rows() > 0 {
                return Ok(Some(Self::row_to_tenant(batch, 0)?));
            }
        }
        Ok(None)
    }

    fn row_to_tenant(batch: &RecordBatch, row: usize) -> Result<Tenant, OmemError> {
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

        let status: TenantStatus = get_str("status")?
            .parse()
            .map_err(|e: String| OmemError::Storage(e))?;
        let config: TenantConfig = serde_json::from_str(&get_str("config")?)
            .map_err(|e| OmemError::Storage(format!("failed to parse tenant config: {e}")))?;

        Ok(Tenant {
            id: get_str("id")?,
            name: get_str("name")?,
            status,
            config,
            created_at: get_str("created_at")?,
        })
    }
}

fn escape_sql(s: &str) -> String {
    s.replace('\'', "''")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn setup() -> (TenantStore, TempDir) {
        let dir = TempDir::new().expect("temp dir");
        let store = TenantStore::new(dir.path().to_str().expect("path"))
            .await
            .expect("tenant store");
        store.init_table().await.expect("init");
        (store, dir)
    }

    #[tokio::test]
    async fn test_create_and_get_tenant() {
        let (store, _dir) = setup().await;

        let tenant = Tenant {
            id: "t-001".to_string(),
            name: "test-workspace".to_string(),
            status: TenantStatus::Active,
            config: TenantConfig::default(),
            created_at: chrono::Utc::now().to_rfc3339(),
        };

        store.create(&tenant).await.expect("create tenant");

        let fetched = store.get_by_id("t-001").await.expect("get tenant");
        assert!(fetched.is_some());
        let fetched = fetched.expect("tenant exists");
        assert_eq!(fetched.id, "t-001");
        assert_eq!(fetched.name, "test-workspace");
        assert_eq!(fetched.status, TenantStatus::Active);
    }

    #[tokio::test]
    async fn test_get_nonexistent_tenant() {
        let (store, _dir) = setup().await;
        let result = store.get_by_id("nonexistent").await.expect("query");
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_init_table_idempotent() {
        let (store, _dir) = setup().await;
        store
            .init_table()
            .await
            .expect("second init should succeed");
    }
}
