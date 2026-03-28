use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use arrow_array::types::Float32Type;
use arrow_array::{
    Array, FixedSizeListArray, Float32Array, Int32Array, RecordBatch, RecordBatchIterator,
    StringArray,
};
use arrow_schema::{DataType, Field, Schema};
use futures::TryStreamExt;
use lancedb::index::scalar::FtsIndexBuilder;
use lancedb::index::Index;
use lancedb::query::{ExecutableQuery, QueryBase, Select};
use lancedb::table::Table;
use lancedb::Connection;

use crate::domain::category::Category;
use crate::domain::error::OmemError;
use crate::domain::memory::Memory;
use crate::domain::relation::MemoryRelation;
use crate::domain::space::Provenance;
use crate::domain::types::{MemoryState, MemoryType, Tier};

const VECTOR_DIM: i32 = 1024;
const TABLE_NAME: &str = "memories";

pub struct ListFilter {
    pub category: Option<String>,
    pub tier: Option<String>,
    pub tags: Option<Vec<String>>,
    pub memory_type: Option<String>,
    pub state: Option<String>,
    pub sort: String,
    pub order: String,
}

impl Default for ListFilter {
    fn default() -> Self {
        Self {
            category: None,
            tier: None,
            tags: None,
            memory_type: None,
            state: None,
            sort: "created_at".to_string(),
            order: "desc".to_string(),
        }
    }
}

pub struct LanceStore {
    db: Connection,
    table_name: String,
    fts_indexed: AtomicBool,
}

impl LanceStore {
    pub async fn new(uri: &str) -> Result<Self, OmemError> {
        let db = lancedb::connect(uri)
            .execute()
            .await
            .map_err(|e| OmemError::Storage(format!("failed to connect to LanceDB: {e}")))?;
        Ok(Self {
            db,
            table_name: TABLE_NAME.to_string(),
            fts_indexed: AtomicBool::new(false),
        })
    }

    pub async fn init_table(&self) -> Result<(), OmemError> {
        let existing = self
            .db
            .table_names()
            .execute()
            .await
            .map_err(|e| OmemError::Storage(format!("failed to list tables: {e}")))?;

        if existing.contains(&self.table_name) {
            return Ok(());
        }

        self.db
            .create_empty_table(&self.table_name, Self::schema())
            .execute()
            .await
            .map_err(|e| OmemError::Storage(format!("failed to create table: {e}")))?;

        Ok(())
    }

    fn schema() -> Arc<Schema> {
        Arc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("content", DataType::Utf8, false),
            Field::new("l0_abstract", DataType::Utf8, false),
            Field::new("l1_overview", DataType::Utf8, false),
            Field::new("l2_content", DataType::Utf8, false),
            Field::new(
                "vector",
                DataType::FixedSizeList(
                    Arc::new(Field::new("item", DataType::Float32, true)),
                    VECTOR_DIM,
                ),
                true,
            ),
            Field::new("category", DataType::Utf8, false),
            Field::new("memory_type", DataType::Utf8, false),
            Field::new("state", DataType::Utf8, false),
            Field::new("tier", DataType::Utf8, false),
            Field::new("importance", DataType::Float32, false),
            Field::new("confidence", DataType::Float32, false),
            Field::new("access_count", DataType::Int32, false),
            Field::new("tags", DataType::Utf8, false),
            Field::new("scope", DataType::Utf8, false),
            Field::new("agent_id", DataType::Utf8, true),
            Field::new("session_id", DataType::Utf8, true),
            Field::new("tenant_id", DataType::Utf8, false),
            Field::new("source", DataType::Utf8, true),
            Field::new("relations", DataType::Utf8, false),
            Field::new("superseded_by", DataType::Utf8, true),
            Field::new("invalidated_at", DataType::Utf8, true),
            Field::new("created_at", DataType::Utf8, false),
            Field::new("updated_at", DataType::Utf8, false),
            Field::new("last_accessed_at", DataType::Utf8, true),
            Field::new("space_id", DataType::Utf8, false),
            Field::new("visibility", DataType::Utf8, false),
            Field::new("owner_agent_id", DataType::Utf8, false),
            Field::new("provenance", DataType::Utf8, true),
        ]))
    }

    async fn open_table(&self) -> Result<Table, OmemError> {
        self.db
            .open_table(&self.table_name)
            .execute()
            .await
            .map_err(|e| OmemError::Storage(format!("failed to open table: {e}")))
    }

    fn memory_to_batch(memory: &Memory, vector: Option<&[f32]>) -> Result<RecordBatch, OmemError> {
        let tags_json = serde_json::to_string(&memory.tags)
            .map_err(|e| OmemError::Storage(format!("failed to serialize tags: {e}")))?;
        let relations_json = serde_json::to_string(&memory.relations)
            .map_err(|e| OmemError::Storage(format!("failed to serialize relations: {e}")))?;
        let provenance_json: Option<String> = memory
            .provenance
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(|e| OmemError::Storage(format!("failed to serialize provenance: {e}")))?;

        let vec_data: Vec<f32> = match vector {
            Some(v) => v.to_vec(),
            None => vec![0.0; VECTOR_DIM as usize],
        };

        let vector_array = FixedSizeListArray::from_iter_primitive::<Float32Type, _, _>(
            vec![Some(vec_data.into_iter().map(Some).collect::<Vec<_>>())],
            VECTOR_DIM,
        );

        RecordBatch::try_new(
            Self::schema(),
            vec![
                Arc::new(StringArray::from(vec![memory.id.as_str()])),
                Arc::new(StringArray::from(vec![memory.content.as_str()])),
                Arc::new(StringArray::from(vec![memory.l0_abstract.as_str()])),
                Arc::new(StringArray::from(vec![memory.l1_overview.as_str()])),
                Arc::new(StringArray::from(vec![memory.l2_content.as_str()])),
                Arc::new(vector_array),
                Arc::new(StringArray::from(vec![memory.category.to_string().as_str()])),
                Arc::new(StringArray::from(vec![memory.memory_type.to_string().as_str()])),
                Arc::new(StringArray::from(vec![memory.state.to_string().as_str()])),
                Arc::new(StringArray::from(vec![memory.tier.to_string().as_str()])),
                Arc::new(Float32Array::from(vec![memory.importance])),
                Arc::new(Float32Array::from(vec![memory.confidence])),
                Arc::new(Int32Array::from(vec![memory.access_count as i32])),
                Arc::new(StringArray::from(vec![tags_json.as_str()])),
                Arc::new(StringArray::from(vec![memory.scope.as_str()])),
                Arc::new(StringArray::from(vec![option_str(&memory.agent_id)])),
                Arc::new(StringArray::from(vec![option_str(&memory.session_id)])),
                Arc::new(StringArray::from(vec![memory.tenant_id.as_str()])),
                Arc::new(StringArray::from(vec![option_str(&memory.source)])),
                Arc::new(StringArray::from(vec![relations_json.as_str()])),
                Arc::new(StringArray::from(vec![option_str(&memory.superseded_by)])),
                Arc::new(StringArray::from(vec![option_str(&memory.invalidated_at)])),
                Arc::new(StringArray::from(vec![memory.created_at.as_str()])),
                Arc::new(StringArray::from(vec![memory.updated_at.as_str()])),
                Arc::new(StringArray::from(vec![option_str(
                    &memory.last_accessed_at,
                )])),
                Arc::new(StringArray::from(vec![memory.space_id.as_str()])),
                Arc::new(StringArray::from(vec![memory.visibility.as_str()])),
                Arc::new(StringArray::from(vec![memory.owner_agent_id.as_str()])),
                Arc::new(StringArray::from(vec![option_str(&provenance_json)])),
            ],
        )
        .map_err(|e| OmemError::Storage(format!("failed to build RecordBatch: {e}")))
    }

    fn batch_to_memories(batches: &[RecordBatch]) -> Result<Vec<Memory>, OmemError> {
        let mut memories = Vec::new();
        for batch in batches {
            for i in 0..batch.num_rows() {
                memories.push(Self::row_to_memory(batch, i)?);
            }
        }
        Ok(memories)
    }

    fn row_to_memory(batch: &RecordBatch, row: usize) -> Result<Memory, OmemError> {
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

        let get_opt_str = |name: &str| -> Result<Option<String>, OmemError> {
            let col = batch
                .column_by_name(name)
                .ok_or_else(|| OmemError::Storage(format!("missing column: {name}")))?;
            let arr = col
                .as_any()
                .downcast_ref::<StringArray>()
                .ok_or_else(|| OmemError::Storage(format!("column {name} is not Utf8")))?;
            if arr.is_null(row) {
                return Ok(None);
            }
            let val = arr.value(row);
            if val.is_empty() {
                Ok(None)
            } else {
                Ok(Some(val.to_string()))
            }
        };

        let get_str_or = |name: &str, default: &str| -> String {
            batch
                .column_by_name(name)
                .and_then(|col| col.as_any().downcast_ref::<StringArray>().map(|a| a.value(row).to_string()))
                .unwrap_or_else(|| default.to_string())
        };

        let get_f32 = |name: &str| -> Result<f32, OmemError> {
            let col = batch
                .column_by_name(name)
                .ok_or_else(|| OmemError::Storage(format!("missing column: {name}")))?;
            let arr = col
                .as_any()
                .downcast_ref::<Float32Array>()
                .ok_or_else(|| OmemError::Storage(format!("column {name} is not Float32")))?;
            Ok(arr.value(row))
        };

        let get_i32 = |name: &str| -> Result<i32, OmemError> {
            let col = batch
                .column_by_name(name)
                .ok_or_else(|| OmemError::Storage(format!("missing column: {name}")))?;
            let arr = col
                .as_any()
                .downcast_ref::<Int32Array>()
                .ok_or_else(|| OmemError::Storage(format!("column {name} is not Int32")))?;
            Ok(arr.value(row))
        };

        let tags_json = get_str("tags")?;
        let tags: Vec<String> = serde_json::from_str(&tags_json)
            .map_err(|e| OmemError::Storage(format!("failed to parse tags: {e}")))?;

        let relations_json = get_str("relations")?;
        let relations: Vec<MemoryRelation> = serde_json::from_str(&relations_json)
            .map_err(|e| OmemError::Storage(format!("failed to parse relations: {e}")))?;

        let category: Category = get_str("category")?
            .parse()
            .map_err(|e: String| OmemError::Storage(e))?;
        let memory_type: MemoryType = get_str("memory_type")?
            .parse()
            .map_err(|e: String| OmemError::Storage(e))?;
        let state: MemoryState = get_str("state")?
            .parse()
            .map_err(|e: String| OmemError::Storage(e))?;
        let tier: Tier = get_str("tier")?
            .parse()
            .map_err(|e: String| OmemError::Storage(e))?;

        let provenance_str = get_str_or("provenance", "");
        let provenance: Option<Provenance> = if provenance_str.is_empty() {
            None
        } else {
            serde_json::from_str(&provenance_str).ok()
        };

        Ok(Memory {
            id: get_str("id")?,
            content: get_str("content")?,
            l0_abstract: get_str("l0_abstract")?,
            l1_overview: get_str("l1_overview")?,
            l2_content: get_str("l2_content")?,
            category,
            memory_type,
            state,
            tier,
            importance: get_f32("importance")?,
            confidence: get_f32("confidence")?,
            access_count: get_i32("access_count")? as u32,
            tags,
            scope: get_str("scope")?,
            agent_id: get_opt_str("agent_id")?,
            session_id: get_opt_str("session_id")?,
            tenant_id: get_str("tenant_id")?,
            source: get_opt_str("source")?,
            relations,
            superseded_by: get_opt_str("superseded_by")?,
            invalidated_at: get_opt_str("invalidated_at")?,
            created_at: get_str("created_at")?,
            updated_at: get_str("updated_at")?,
            last_accessed_at: get_opt_str("last_accessed_at")?,
            space_id: get_str_or("space_id", ""),
            visibility: get_str_or("visibility", "global"),
            owner_agent_id: get_str_or("owner_agent_id", ""),
            provenance,
        })
    }

    fn extract_score(batch: &RecordBatch, row: usize) -> f32 {
        if let Some(col) = batch.column_by_name("_distance") {
            if let Some(arr) = col.as_any().downcast_ref::<Float32Array>() {
                let distance = arr.value(row);
                return 1.0 - distance;
            }
        }
        if let Some(col) = batch.column_by_name("_score") {
            if let Some(arr) = col.as_any().downcast_ref::<Float32Array>() {
                return arr.value(row);
            }
        }
        0.0
    }

    pub async fn list_all_active(&self) -> Result<Vec<Memory>, OmemError> {
        let table = self.open_table().await?;
        let batches: Vec<RecordBatch> = table
            .query()
            .only_if("state != 'deleted'")
            .execute()
            .await
            .map_err(|e| OmemError::Storage(format!("list all query failed: {e}")))?
            .try_collect()
            .await
            .map_err(|e| OmemError::Storage(format!("collect failed: {e}")))?;

        Self::batch_to_memories(&batches)
    }

    pub async fn create(
        &self,
        memory: &Memory,
        vector: Option<&[f32]>,
    ) -> Result<(), OmemError> {
        let batch = Self::memory_to_batch(memory, vector)?;
        let table = self.open_table().await?;
        let reader = RecordBatchIterator::new(vec![Ok(batch)], Self::schema());
        table
            .add(Box::new(reader) as Box<dyn arrow_array::RecordBatchReader + Send>)
            .execute()
            .await
            .map_err(|e| OmemError::Storage(format!("failed to insert memory: {e}")))?;

        // Auto-create FTS index after first successful write.
        // LanceDB requires data in the table before creating FTS indexes.
        if !self.fts_indexed.load(Ordering::Relaxed) {
            if let Err(e) = self.create_fts_index().await {
                tracing::warn!("Failed to create FTS index (will retry on next write): {e}");
            } else {
                self.fts_indexed.store(true, Ordering::Relaxed);
            }
        }

        Ok(())
    }

    pub async fn get_by_id(&self, id: &str) -> Result<Option<Memory>, OmemError> {
        let table = self.open_table().await?;
        let batches: Vec<RecordBatch> = table
            .query()
            .only_if(format!("id = '{}'", escape_sql(id)))
            .limit(1)
            .execute()
            .await
            .map_err(|e| OmemError::Storage(format!("query failed: {e}")))?
            .try_collect()
            .await
            .map_err(|e| OmemError::Storage(format!("collect failed: {e}")))?;

        let memories = Self::batch_to_memories(&batches)?;
        Ok(memories.into_iter().next())
    }

    pub async fn update(
        &self,
        memory: &Memory,
        vector: Option<&[f32]>,
    ) -> Result<(), OmemError> {
        let table = self.open_table().await?;
        table
            .delete(&format!("id = '{}'", escape_sql(&memory.id)))
            .await
            .map_err(|e| OmemError::Storage(format!("delete for update failed: {e}")))?;

        let batch = Self::memory_to_batch(memory, vector)?;
        let reader = RecordBatchIterator::new(vec![Ok(batch)], Self::schema());
        table
            .add(Box::new(reader) as Box<dyn arrow_array::RecordBatchReader + Send>)
            .execute()
            .await
            .map_err(|e| OmemError::Storage(format!("re-insert for update failed: {e}")))?;
        Ok(())
    }

    pub async fn soft_delete(&self, id: &str) -> Result<(), OmemError> {
        let memory = self
            .get_by_id(id)
            .await?
            .ok_or_else(|| OmemError::NotFound(format!("memory {id}")))?;

        let mut updated = memory;
        updated.state = MemoryState::Deleted;
        updated.updated_at = chrono::Utc::now().to_rfc3339();
        self.update(&updated, None).await
    }

    pub async fn list(
        &self,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<Memory>, OmemError> {
        let table = self.open_table().await?;
        let batches: Vec<RecordBatch> = table
            .query()
            .only_if("state != 'deleted'")
            .limit(limit + offset)
            .execute()
            .await
            .map_err(|e| OmemError::Storage(format!("list query failed: {e}")))?
            .try_collect()
            .await
            .map_err(|e| OmemError::Storage(format!("collect failed: {e}")))?;

        let all = Self::batch_to_memories(&batches)?;
        Ok(all.into_iter().skip(offset).take(limit).collect())
    }

    pub async fn vector_search(
        &self,
        query_vector: &[f32],
        limit: usize,
        min_score: f32,
        scope_filter: Option<&str>,
        visibility_filter: Option<&str>,
    ) -> Result<Vec<(Memory, f32)>, OmemError> {
        let table = self.open_table().await?;
        let mut query = table
            .query()
            .nearest_to(query_vector)
            .map_err(|e| OmemError::Storage(format!("vector query build failed: {e}")))?;

        query = query.limit(limit);

        let mut filter = "state != 'deleted'".to_string();
        if let Some(scope) = scope_filter {
            filter.push_str(&format!(" AND scope = '{}'", escape_sql(scope)));
        }
        if let Some(vis) = visibility_filter {
            filter.push_str(&format!(" AND ({vis})"));
        }
        query = query.only_if(filter);

        let batches: Vec<RecordBatch> = query
            .execute()
            .await
            .map_err(|e| OmemError::Storage(format!("vector search failed: {e}")))?
            .try_collect()
            .await
            .map_err(|e| OmemError::Storage(format!("collect failed: {e}")))?;

        let mut results = Vec::new();
        for batch in &batches {
            for i in 0..batch.num_rows() {
                let score = Self::extract_score(batch, i);
                if score >= min_score {
                    let memory = Self::row_to_memory(batch, i)?;
                    results.push((memory, score));
                }
            }
        }
        Ok(results)
    }

    pub async fn fts_search(
        &self,
        query: &str,
        limit: usize,
        scope_filter: Option<&str>,
        visibility_filter: Option<&str>,
    ) -> Result<Vec<(Memory, f32)>, OmemError> {
        let table = self.open_table().await?;

        let fts_query = lance_index::scalar::FullTextSearchQuery::new(query.to_string());

        let mut q = table
            .query()
            .full_text_search(fts_query)
            .select(Select::All)
            .limit(limit);

        let mut filter = "state != 'deleted'".to_string();
        if let Some(scope) = scope_filter {
            filter.push_str(&format!(" AND scope = '{}'", escape_sql(scope)));
        }
        if let Some(vis) = visibility_filter {
            filter.push_str(&format!(" AND ({vis})"));
        }
        q = q.postfilter().only_if(filter);

        let batches: Vec<RecordBatch> = q
            .execute()
            .await
            .map_err(|e| OmemError::Storage(format!("FTS search failed: {e}")))?
            .try_collect()
            .await
            .map_err(|e| OmemError::Storage(format!("collect failed: {e}")))?;

        let mut results = Vec::new();
        for batch in &batches {
            for i in 0..batch.num_rows() {
                let score = Self::extract_score(batch, i);
                let memory = Self::row_to_memory(batch, i)?;
                results.push((memory, score));
            }
        }
        Ok(results)
    }

    pub fn build_visibility_filter(
        &self,
        agent_id: &str,
        accessible_spaces: &[String],
    ) -> String {
        let mut conditions = vec!["state != 'deleted'".to_string()];

        let mut vis_conditions = vec!["visibility = 'global'".to_string()];

        if !agent_id.is_empty() {
            vis_conditions.push(format!(
                "(visibility = 'private' AND owner_agent_id = '{}')",
                agent_id.replace('\'', "''")
            ));
        }

        for space in accessible_spaces {
            vis_conditions.push(format!(
                "visibility = 'shared:{}'",
                space.replace('\'', "''")
            ));
        }

        conditions.push(format!("({})", vis_conditions.join(" OR ")));
        conditions.join(" AND ")
    }

    pub async fn create_vector_index(&self) -> Result<(), OmemError> {
        let table = self.open_table().await?;
        table
            .create_index(
                &["vector"],
                Index::IvfHnswSq(
                    lancedb::index::vector::IvfHnswSqIndexBuilder::default()
                        .distance_type(lancedb::DistanceType::Cosine),
                ),
            )
            .execute()
            .await
            .map_err(|e| OmemError::Storage(format!("failed to create vector index: {e}")))?;
        Ok(())
    }

    pub async fn create_fts_index(&self) -> Result<(), OmemError> {
        let table = self.open_table().await?;
        table
            .create_index(&["content"], Index::FTS(FtsIndexBuilder::default()))
            .execute()
            .await
            .map_err(|e| OmemError::Storage(format!("failed to create FTS index on content: {e}")))?;
        table
            .create_index(&["l0_abstract"], Index::FTS(FtsIndexBuilder::default()))
            .execute()
            .await
            .map_err(|e| {
                OmemError::Storage(format!("failed to create FTS index on l0_abstract: {e}"))
            })?;
        Ok(())
    }

    pub async fn list_filtered(
        &self,
        filter: &ListFilter,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<Memory>, OmemError> {
        let table = self.open_table().await?;
        let where_clause = Self::build_where_clause(filter);

        let batches: Vec<RecordBatch> = table
            .query()
            .only_if(&where_clause)
            .execute()
            .await
            .map_err(|e| OmemError::Storage(format!("list_filtered query failed: {e}")))?
            .try_collect()
            .await
            .map_err(|e| OmemError::Storage(format!("collect failed: {e}")))?;

        let mut memories = Self::batch_to_memories(&batches)?;

        // Sort in Rust (LanceDB query builder doesn't support ORDER BY)
        match filter.sort.as_str() {
            "importance" => memories.sort_by(|a, b| {
                a.importance
                    .partial_cmp(&b.importance)
                    .unwrap_or(std::cmp::Ordering::Equal)
            }),
            "access_count" => memories.sort_by_key(|m| m.access_count),
            "updated_at" => memories.sort_by(|a, b| a.updated_at.cmp(&b.updated_at)),
            _ => memories.sort_by(|a, b| a.created_at.cmp(&b.created_at)),
        }
        if filter.order == "desc" {
            memories.reverse();
        }

        Ok(memories.into_iter().skip(offset).take(limit).collect())
    }

    pub async fn count_filtered(&self, filter: &ListFilter) -> Result<usize, OmemError> {
        let table = self.open_table().await?;
        let where_clause = Self::build_where_clause(filter);

        let count = table
            .count_rows(Some(where_clause))
            .await
            .map_err(|e| OmemError::Storage(format!("count failed: {e}")))?;

        Ok(count)
    }

    /// Find memories whose provenance.shared_from_memory matches the given original memory ID.
    /// Used by the unshare handler to locate shared copies in a target space.
    pub async fn find_by_provenance_source(
        &self,
        source_memory_id: &str,
    ) -> Result<Vec<Memory>, OmemError> {
        let table = self.open_table().await?;
        let filter = format!(
            "state != 'deleted' AND provenance LIKE '%\"shared_from_memory\":\"{}\"%'",
            escape_sql(source_memory_id)
        );
        let batches: Vec<RecordBatch> = table
            .query()
            .only_if(filter)
            .execute()
            .await
            .map_err(|e| OmemError::Storage(format!("provenance query failed: {e}")))?
            .try_collect()
            .await
            .map_err(|e| OmemError::Storage(format!("collect failed: {e}")))?;

        Self::batch_to_memories(&batches)
    }

    fn build_where_clause(filter: &ListFilter) -> String {
        let mut conditions = Vec::new();

        match &filter.state {
            Some(s) => conditions.push(format!("state = '{}'", escape_sql(s))),
            None => conditions.push("state != 'deleted'".to_string()),
        }

        if let Some(ref cat) = filter.category {
            conditions.push(format!("category = '{}'", escape_sql(cat)));
        }
        if let Some(ref t) = filter.tier {
            conditions.push(format!("tier = '{}'", escape_sql(t)));
        }
        if let Some(ref mt) = filter.memory_type {
            conditions.push(format!("memory_type = '{}'", escape_sql(mt)));
        }
        if let Some(ref tags) = filter.tags {
            for tag in tags {
                let escaped = escape_sql(tag);
                conditions.push(format!(
                    "(tags LIKE '%\"{}\"%')",
                    escaped
                ));
            }
        }

        if conditions.is_empty() {
            "true".to_string()
        } else {
            conditions.join(" AND ")
        }
    }
}

fn option_str(opt: &Option<String>) -> Option<&str> {
    opt.as_deref()
}

fn escape_sql(s: &str) -> String {
    s.replace('\'', "''")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn setup() -> (LanceStore, TempDir) {
        let dir = TempDir::new().expect("failed to create temp dir");
        let store = LanceStore::new(dir.path().to_str().unwrap())
            .await
            .expect("failed to create store");
        store.init_table().await.expect("failed to init table");
        (store, dir)
    }

    fn make_memory(tenant: &str, content: &str) -> Memory {
        Memory::new(content, Category::Preferences, MemoryType::Insight, tenant)
    }

    #[tokio::test]
    async fn test_create_and_get_by_id() {
        let (store, _dir) = setup().await;
        let mem = make_memory("t-001", "user prefers dark mode");

        store.create(&mem, None).await.unwrap();

        let fetched = store.get_by_id(&mem.id).await.unwrap();
        assert!(fetched.is_some());
        let fetched = fetched.unwrap();
        assert_eq!(fetched.id, mem.id);
        assert_eq!(fetched.content, "user prefers dark mode");
        assert_eq!(fetched.tenant_id, "t-001");
        assert_eq!(fetched.category, Category::Preferences);
        assert_eq!(fetched.memory_type, MemoryType::Insight);
        assert_eq!(fetched.state, MemoryState::Active);
        assert_eq!(fetched.tier, Tier::Peripheral);
        assert!((fetched.importance - 0.5).abs() < f32::EPSILON);
        assert!((fetched.confidence - 0.5).abs() < f32::EPSILON);
        assert_eq!(fetched.access_count, 0);
        assert_eq!(fetched.scope, "global");
    }

    #[tokio::test]
    async fn test_vector_search() {
        let (store, _dir) = setup().await;

        let mut v1 = vec![0.0f32; VECTOR_DIM as usize];
        v1[0] = 1.0;
        let mut v2 = vec![0.0f32; VECTOR_DIM as usize];
        v2[0] = 0.9;
        v2[1] = 0.1;
        let mut v3 = vec![0.0f32; VECTOR_DIM as usize];
        v3[1] = 1.0;

        let m1 = make_memory("t-001", "closest match");
        let m2 = make_memory("t-001", "second closest");
        let m3 = make_memory("t-001", "furthest match");

        store.create(&m1, Some(&v1)).await.unwrap();
        store.create(&m2, Some(&v2)).await.unwrap();
        store.create(&m3, Some(&v3)).await.unwrap();

        let mut query_vec = vec![0.0f32; VECTOR_DIM as usize];
        query_vec[0] = 1.0;

        let results = store
            .vector_search(&query_vec, 3, 0.0, None, None)
            .await
            .unwrap();

        assert!(!results.is_empty());
        assert_eq!(results[0].0.content, "closest match");
        if results.len() >= 2 {
            assert!(results[0].1 >= results[1].1);
        }
    }

    #[tokio::test]
    async fn test_fts_search() {
        let (store, _dir) = setup().await;

        let m1 = make_memory("t-001", "rust programming language is fast");
        let m2 = make_memory("t-001", "python is a popular scripting language");
        let m3 = make_memory("t-001", "the weather is sunny today");

        store.create(&m1, None).await.unwrap();
        store.create(&m2, None).await.unwrap();
        store.create(&m3, None).await.unwrap();

        store.create_fts_index().await.unwrap();

        let results = store.fts_search("programming language", 10, None, None).await.unwrap();

        assert!(!results.is_empty());
        let contents: Vec<&str> = results.iter().map(|(m, _)| m.content.as_str()).collect();
        assert!(contents.contains(&"rust programming language is fast"));
    }

    #[tokio::test]
    async fn test_soft_delete() {
        let (store, _dir) = setup().await;
        let mem = make_memory("t-001", "to be deleted");

        store.create(&mem, None).await.unwrap();

        let before = store.get_by_id(&mem.id).await.unwrap();
        assert!(before.is_some());
        assert_eq!(before.unwrap().state, MemoryState::Active);

        store.soft_delete(&mem.id).await.unwrap();

        let after = store.get_by_id(&mem.id).await.unwrap();
        assert!(after.is_some());
        assert_eq!(after.unwrap().state, MemoryState::Deleted);
    }

    #[tokio::test]
    async fn test_list_with_pagination() {
        let (store, _dir) = setup().await;

        for i in 0..5 {
            let mem = make_memory("t-001", &format!("memory {i}"));
            store.create(&mem, None).await.unwrap();
        }

        let page1 = store.list(2, 0).await.unwrap();
        assert_eq!(page1.len(), 2);

        let page2 = store.list(2, 2).await.unwrap();
        assert_eq!(page2.len(), 2);

        let page3 = store.list(2, 4).await.unwrap();
        assert_eq!(page3.len(), 1);
    }

    #[tokio::test]
    async fn test_multi_tenant_isolation() {
        let (store_a, _dir_a) = setup().await;
        let (store_b, _dir_b) = setup().await;

        let mut va = vec![0.0f32; VECTOR_DIM as usize];
        va[0] = 1.0;
        let mut vb = vec![0.0f32; VECTOR_DIM as usize];
        vb[0] = 1.0;

        let mem_a = make_memory("tenant_A", "secret data for A");
        let mem_b = make_memory("tenant_B", "secret data for B");

        store_a.create(&mem_a, Some(&va)).await.unwrap();
        store_b.create(&mem_b, Some(&vb)).await.unwrap();

        let list_a = store_a.list(100, 0).await.unwrap();
        assert_eq!(list_a.len(), 1);
        assert_eq!(list_a[0].tenant_id, "tenant_A");

        let list_b = store_b.list(100, 0).await.unwrap();
        assert_eq!(list_b.len(), 1);
        assert_eq!(list_b[0].tenant_id, "tenant_B");
    }

    #[tokio::test]
    async fn test_list_filtered_by_category() {
        let (store, _dir) = setup().await;

        let m1 = Memory::new("dark mode pref", Category::Preferences, MemoryType::Insight, "t-001");
        let m2 = Memory::new("another pref", Category::Preferences, MemoryType::Insight, "t-001");
        let m3 = Memory::new("meeting happened", Category::Events, MemoryType::Session, "t-001");

        store.create(&m1, None).await.unwrap();
        store.create(&m2, None).await.unwrap();
        store.create(&m3, None).await.unwrap();

        let filter = ListFilter {
            category: Some("preferences".to_string()),
            ..Default::default()
        };
        let results = store.list_filtered(&filter, 100, 0).await.unwrap();
        assert_eq!(results.len(), 2);

        let filter_events = ListFilter {
            category: Some("events".to_string()),
            ..Default::default()
        };
        let results = store.list_filtered(&filter_events, 100, 0).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].content, "meeting happened");
    }

    #[tokio::test]
    async fn test_list_filtered_by_tier() {
        let (store, _dir) = setup().await;

        let mut m1 = make_memory("t-001", "core memory");
        m1.tier = Tier::Core;
        let mut m2 = make_memory("t-001", "working memory");
        m2.tier = Tier::Working;
        let m3 = make_memory("t-001", "peripheral memory");

        store.create(&m1, None).await.unwrap();
        store.create(&m2, None).await.unwrap();
        store.create(&m3, None).await.unwrap();

        let filter = ListFilter {
            tier: Some("core".to_string()),
            ..Default::default()
        };
        let results = store.list_filtered(&filter, 100, 0).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].content, "core memory");
    }

    #[tokio::test]
    async fn test_list_filtered_sort_by_importance() {
        let (store, _dir) = setup().await;

        let mut m1 = make_memory("t-001", "low importance");
        m1.importance = 0.2;
        let mut m2 = make_memory("t-001", "high importance");
        m2.importance = 0.9;
        let mut m3 = make_memory("t-001", "mid importance");
        m3.importance = 0.5;

        store.create(&m1, None).await.unwrap();
        store.create(&m2, None).await.unwrap();
        store.create(&m3, None).await.unwrap();

        let filter = ListFilter {
            sort: "importance".to_string(),
            order: "desc".to_string(),
            ..Default::default()
        };
        let results = store.list_filtered(&filter, 100, 0).await.unwrap();
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].content, "high importance");
        assert_eq!(results[1].content, "mid importance");
        assert_eq!(results[2].content, "low importance");
    }

    #[tokio::test]
    async fn test_count_filtered() {
        let (store, _dir) = setup().await;

        for i in 0..5 {
            let mem = make_memory("t-001", &format!("memory {i}"));
            store.create(&mem, None).await.unwrap();
        }

        let filter = ListFilter::default();
        let count = store.count_filtered(&filter).await.unwrap();
        assert_eq!(count, 5);

        let limited = store.list_filtered(&filter, 2, 0).await.unwrap();
        assert_eq!(limited.len(), 2);
    }

    #[test]
    fn test_visibility_filter_global() {
        let store = tokio::runtime::Runtime::new().unwrap().block_on(async {
            let dir = TempDir::new().unwrap();
            LanceStore::new(dir.path().to_str().unwrap()).await.unwrap()
        });
        let result = store.build_visibility_filter("", &[]);
        assert!(result.contains("visibility = 'global'"));
        assert!(result.contains("state != 'deleted'"));
        assert!(!result.contains("private"));
    }

    #[test]
    fn test_visibility_filter_private() {
        let store = tokio::runtime::Runtime::new().unwrap().block_on(async {
            let dir = TempDir::new().unwrap();
            LanceStore::new(dir.path().to_str().unwrap()).await.unwrap()
        });
        let result = store.build_visibility_filter("agent-1", &[]);
        assert!(result.contains("visibility = 'global'"));
        assert!(result.contains("visibility = 'private' AND owner_agent_id = 'agent-1'"));
    }

    #[test]
    fn test_visibility_filter_shared() {
        let store = tokio::runtime::Runtime::new().unwrap().block_on(async {
            let dir = TempDir::new().unwrap();
            LanceStore::new(dir.path().to_str().unwrap()).await.unwrap()
        });
        let spaces = vec!["team:backend".to_string(), "org:acme".to_string()];
        let result = store.build_visibility_filter("agent-1", &spaces);
        assert!(result.contains("visibility = 'global'"));
        assert!(result.contains("visibility = 'private' AND owner_agent_id = 'agent-1'"));
        assert!(result.contains("visibility = 'shared:team:backend'"));
        assert!(result.contains("visibility = 'shared:org:acme'"));
    }

    #[test]
    fn test_visibility_filter_escapes_sql() {
        let store = tokio::runtime::Runtime::new().unwrap().block_on(async {
            let dir = TempDir::new().unwrap();
            LanceStore::new(dir.path().to_str().unwrap()).await.unwrap()
        });
        let result = store.build_visibility_filter("agent'inject", &["space'bad".to_string()]);
        assert!(result.contains("agent''inject"));
        assert!(result.contains("space''bad"));
    }
}
