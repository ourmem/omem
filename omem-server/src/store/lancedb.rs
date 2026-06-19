use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use arrow_array::types::Float32Type;
use arrow_array::{
    Array, FixedSizeListArray, Float32Array, Int32Array, RecordBatch, RecordBatchIterator,
    StringArray, UInt64Array,
};
use arrow_schema::{DataType, Field, Schema};
use futures::TryStreamExt;
use lancedb::index::scalar::FtsIndexBuilder;
use lancedb::index::Index;
use lancedb::query::{ExecutableQuery, QueryBase, Select};
use lancedb::table::{NewColumnTransform, Table};
use lancedb::Connection;

use crate::domain::category::Category;
use crate::domain::error::OmemError;
use crate::domain::memory::Memory;
use crate::domain::relation::MemoryRelation;
use crate::domain::space::Provenance;
use crate::domain::types::{MemoryState, MemoryType, Tier};

pub const DEFAULT_VECTOR_DIM: i32 = 1024;
const TABLE_NAME: &str = "memories";

/// Default state filter for search/list operations.
/// Excludes deleted (soft-deleted) and superseded (replaced by another memory).
const DEFAULT_STATE_FILTER: &str = "state NOT IN ('deleted', 'superseded')";

/// Confidence multiplier applied to a memory that supersedes existing ones.
/// A replacement fact starts at reduced confidence until reinforced by further
/// access (the frequency/decay scoring naturally lifts it over time).
pub const SUPERSEDE_CONFIDENCE_PENALTY: f32 = 0.8;

/// State filter that includes superseded memories.
/// Use when an explicit caller wants to see historical/replaced entries.
const STATE_FILTER_KEEPING_SUPERSEDED: &str = "state != 'deleted'";

fn state_filter(include_superseded: bool) -> &'static str {
    if include_superseded {
        STATE_FILTER_KEEPING_SUPERSEDED
    } else {
        DEFAULT_STATE_FILTER
    }
}

pub struct ListFilter {
    pub category: Option<String>,
    pub tier: Option<String>,
    pub tags: Option<Vec<String>>,
    pub memory_type: Option<String>,
    pub state: Option<String>,
    pub include_superseded: bool,
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
            include_superseded: false,
            sort: "created_at".to_string(),
            order: "desc".to_string(),
        }
    }
}

pub struct LanceStore {
    db: Connection,
    table_name: String,
    vector_dim: i32,
    fts_indexed: AtomicBool,
}

impl LanceStore {
    /// Construct with the default 1024-dim vector schema (back-compat).
    pub async fn new(uri: &str) -> Result<Self, OmemError> {
        Self::with_dim(uri, DEFAULT_VECTOR_DIM).await
    }

    /// Construct with an explicit vector dimension. Must match the
    /// `dimensions()` reported by the active `EmbedService`.
    pub async fn with_dim(uri: &str, vector_dim: i32) -> Result<Self, OmemError> {
        if vector_dim <= 0 {
            return Err(OmemError::Storage(format!(
                "invalid vector_dim {vector_dim}: must be > 0"
            )));
        }
        let mut builder = lancedb::connect(uri);

        // For S3-compatible stores (e.g., Alibaba Cloud OSS), pass through
        // virtual-hosted style and endpoint configuration.
        if uri.starts_with("s3://") {
            if let Ok(val) = std::env::var("AWS_VIRTUAL_HOSTED_STYLE_REQUEST") {
                builder = builder.storage_option("aws_virtual_hosted_style_request", val);
            }
            if let Ok(val) = std::env::var("AWS_ENDPOINT_URL") {
                builder = builder.storage_option("aws_endpoint_url", val);
            } else if let Ok(val) = std::env::var("AWS_ENDPOINT") {
                builder = builder.storage_option("aws_endpoint_url", val);
            }
        }

        let db = builder
            .execute()
            .await
            .map_err(|e| OmemError::Storage(format!("failed to connect to LanceDB: {e}")))?;
        Ok(Self {
            db,
            table_name: TABLE_NAME.to_string(),
            vector_dim,
            fts_indexed: AtomicBool::new(false),
        })
    }

    /// Dimension of vectors stored in this table.
    pub fn vector_dim(&self) -> i32 {
        self.vector_dim
    }

    pub async fn init_table(&self) -> Result<(), OmemError> {
        let existing = self
            .db
            .table_names()
            .execute()
            .await
            .map_err(|e| OmemError::Storage(format!("failed to list tables: {e}")))?;

        if !existing.contains(&self.table_name) {
            self.db
                .create_empty_table(&self.table_name, self.schema())
                .execute()
                .await
                .map_err(|e| OmemError::Storage(format!("failed to create table: {e}")))?;
            return Ok(());
        }

        // Schema evolution: detect and add missing columns
        let table = self.open_table().await?;
        let current_schema = table
            .schema()
            .await
            .map_err(|e| OmemError::Storage(format!("failed to get table schema: {e}")))?;
        let expected_schema = self.schema();

        // Validate that the existing table's vector column matches our configured dim,
        // so a model swap that changes dimensions fails loudly at startup rather than
        // panicking deep in Arrow on the first write/read.
        if let Ok(vector_field) = current_schema.field_with_name("vector") {
            if let DataType::FixedSizeList(_, existing_dim) = vector_field.data_type() {
                if *existing_dim != self.vector_dim {
                    return Err(OmemError::Storage(format!(
                        "vector dim mismatch: table has {} but embedder produces {}; \
                         wipe the table or switch back to the matching embedding model",
                        existing_dim, self.vector_dim
                    )));
                }
            }
        }

        let missing_fields: Vec<Field> = expected_schema
            .fields()
            .iter()
            .filter(|f| current_schema.field_with_name(f.name()).is_err())
            .map(|f| f.as_ref().clone())
            .collect();

        if !missing_fields.is_empty() {
            let missing_schema = Arc::new(Schema::new(missing_fields));
            table
                .add_columns(NewColumnTransform::AllNulls(missing_schema), None)
                .await
                .map_err(|e| OmemError::Storage(format!("failed to add missing columns: {e}")))?;
        }

        Ok(())
    }

    fn schema(&self) -> Arc<Schema> {
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
                    self.vector_dim,
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
            Field::new("version", DataType::UInt64, true),
            Field::new("provenance_source_id", DataType::Utf8, true),
        ]))
    }

    async fn open_table(&self) -> Result<Table, OmemError> {
        self.db
            .open_table(&self.table_name)
            .execute()
            .await
            .map_err(|e| OmemError::Storage(format!("failed to open table: {e}")))
    }

    fn memory_to_batch(
        &self,
        memory: &Memory,
        vector: Option<&[f32]>,
    ) -> Result<RecordBatch, OmemError> {
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
            Some(v) => {
                if v.len() != self.vector_dim as usize {
                    return Err(OmemError::Storage(format!(
                        "embedding length {} does not match table vector_dim {}",
                        v.len(),
                        self.vector_dim
                    )));
                }
                v.to_vec()
            }
            None => vec![0.0; self.vector_dim as usize],
        };

        let vector_array = FixedSizeListArray::from_iter_primitive::<Float32Type, _, _>(
            vec![Some(vec_data.into_iter().map(Some).collect::<Vec<_>>())],
            self.vector_dim,
        );

        let provenance_source_id: Option<&str> = memory
            .provenance
            .as_ref()
            .map(|p| p.shared_from_memory.as_str());

        RecordBatch::try_new(
            self.schema(),
            vec![
                Arc::new(StringArray::from(vec![memory.id.as_str()])),
                Arc::new(StringArray::from(vec![memory.content.as_str()])),
                Arc::new(StringArray::from(vec![memory.l0_abstract.as_str()])),
                Arc::new(StringArray::from(vec![memory.l1_overview.as_str()])),
                Arc::new(StringArray::from(vec![memory.l2_content.as_str()])),
                Arc::new(vector_array),
                Arc::new(StringArray::from(vec![memory
                    .category
                    .to_string()
                    .as_str()])),
                Arc::new(StringArray::from(vec![memory
                    .memory_type
                    .to_string()
                    .as_str()])),
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
                Arc::new(UInt64Array::from(vec![memory.version])),
                Arc::new(StringArray::from(vec![provenance_source_id])),
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
                .and_then(|col| {
                    col.as_any()
                        .downcast_ref::<StringArray>()
                        .map(|a| a.value(row).to_string())
                })
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

        let version: Option<u64> = batch
            .column_by_name("version")
            .and_then(|col| col.as_any().downcast_ref::<UInt64Array>())
            .and_then(|arr| {
                if arr.is_null(row) {
                    None
                } else {
                    Some(arr.value(row))
                }
            });

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
            version,
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

    /// Lists all memories that are neither deleted nor superseded.
    /// Internal use; external lists should go through `list` or `list_filtered`.
    pub async fn list_all_active(&self) -> Result<Vec<Memory>, OmemError> {
        let table = self.open_table().await?;
        let batches: Vec<RecordBatch> = table
            .query()
            .only_if(DEFAULT_STATE_FILTER)
            .execute()
            .await
            .map_err(|e| OmemError::Storage(format!("list all query failed: {e}")))?
            .try_collect()
            .await
            .map_err(|e| OmemError::Storage(format!("collect failed: {e}")))?;

        Self::batch_to_memories(&batches)
    }

    pub async fn create(&self, memory: &Memory, vector: Option<&[f32]>) -> Result<(), OmemError> {
        let batch = self.memory_to_batch(memory, vector)?;
        let table = self.open_table().await?;
        let reader = RecordBatchIterator::new(vec![Ok(batch)], self.schema());
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

    /// Retrieve only the vector embedding for a memory by its ID.
    /// Returns `Ok(None)` if the memory is not found or has been deleted.
    pub async fn get_vector_by_id(&self, id: &str) -> Result<Option<Vec<f32>>, OmemError> {
        let table = self.open_table().await?;
        let batches: Vec<RecordBatch> = table
            .query()
            .only_if(format!(
                "id = '{}' AND {}",
                escape_sql(id),
                DEFAULT_STATE_FILTER
            ))
            .limit(1)
            .execute()
            .await
            .map_err(|e| OmemError::Storage(format!("vector query failed: {e}")))?
            .try_collect()
            .await
            .map_err(|e| OmemError::Storage(format!("collect failed: {e}")))?;

        if batches.is_empty() || batches[0].num_rows() == 0 {
            return Ok(None);
        }

        let batch = &batches[0];
        let col = batch
            .column_by_name("vector")
            .ok_or_else(|| OmemError::Storage("missing vector column".to_string()))?;
        let fsl = col
            .as_any()
            .downcast_ref::<FixedSizeListArray>()
            .ok_or_else(|| OmemError::Storage("vector column is not FixedSizeList".to_string()))?;
        let inner = fsl.value(0);
        let float_arr = inner
            .as_any()
            .downcast_ref::<Float32Array>()
            .ok_or_else(|| OmemError::Storage("vector inner is not Float32".to_string()))?;
        Ok(Some(float_arr.values().to_vec()))
    }

    pub async fn update(&self, memory: &Memory, vector: Option<&[f32]>) -> Result<(), OmemError> {
        // Auto-increment version on every update
        let mut mem = memory.clone();
        mem.version = Some(mem.version.unwrap_or(0) + 1);
        mem.updated_at = chrono::Utc::now().to_rfc3339();

        // Preserve the existing embedding on metadata-only updates. When the
        // caller passes `vector == None` (a state/tags/supersede change with no
        // content edit), `memory_to_batch` writes a zero vector, which combined
        // with `merge_insert.when_matched_update_all()` silently wipes the row's
        // embedding and makes it invisible to vector search. Reuse the stored
        // vector in that case.
        let preserved: Option<Vec<f32>> = if vector.is_none() {
            self.get_vector_by_id(&mem.id).await?
        } else {
            None
        };
        let vector = vector.or(preserved.as_deref());

        let table = self.open_table().await?;
        let batch = self.memory_to_batch(&mem, vector)?;
        let reader = RecordBatchIterator::new(vec![Ok(batch)], self.schema());

        // Atomic upsert keyed on `id`: in a single committed transaction,
        // replace the existing row (or insert it if it is somehow absent).
        //
        // This replaces a previous delete-then-add sequence, which was NOT
        // atomic: if the operation was interrupted between the delete and the
        // re-insert — e.g. the HTTP request was cancelled, so the handler
        // future was dropped at the `.await` — the row was deleted but never
        // restored, silently losing the memory. `merge_insert` commits as one
        // transaction, so an interrupted update either fully applies or not at
        // all; it can never leave the row deleted-without-replacement.
        let mut merge = table.merge_insert(&["id"]);
        merge.when_matched_update_all(None);
        merge.when_not_matched_insert_all();
        merge
            .execute(Box::new(reader) as Box<dyn arrow_array::RecordBatchReader + Send>)
            .await
            .map_err(|e| OmemError::Storage(format!("merge-insert for update failed: {e}")))?;
        Ok(())
    }

    /// Atomically replace `old_ids` with a new memory.
    ///
    /// Semantics:
    /// 1. Validate that every old_id exists and is not already superseded.
    ///    If any fail, return `Err(OmemError::Validation(...))` listing them;
    ///    no writes happen.
    /// 2. Insert the new memory (with its vector).
    /// 3. For each old, set `state = Superseded`, `superseded_by = new.id`,
    ///    `invalidated_at = now`.
    ///
    /// Lance has no native multi-row transactions, so step 3 is best-effort
    /// sequential. If a per-old update fails, the new memory remains
    /// (consolidated content is preserved) but the chain is partial — the
    /// surfaced error names the IDs that failed so callers can retry.
    pub async fn supersede_batch(
        &self,
        new: &Memory,
        new_vector: Option<&[f32]>,
        old_ids: &[String],
    ) -> Result<(), OmemError> {
        if old_ids.is_empty() {
            return self.create(new, new_vector).await;
        }

        let mut missing = Vec::new();
        let mut already = Vec::new();
        let mut olds: Vec<Memory> = Vec::with_capacity(old_ids.len());
        for id in old_ids {
            match self.get_by_id(id).await? {
                None => missing.push(id.clone()),
                Some(m) => {
                    if matches!(m.state, MemoryState::Superseded) {
                        already.push(id.clone());
                    } else {
                        olds.push(m);
                    }
                }
            }
        }
        if !missing.is_empty() || !already.is_empty() {
            let mut parts = Vec::new();
            if !missing.is_empty() {
                parts.push(format!("missing: [{}]", missing.join(", ")));
            }
            if !already.is_empty() {
                parts.push(format!("already superseded: [{}]", already.join(", ")));
            }
            return Err(OmemError::Validation(format!(
                "supersede precheck failed — {}",
                parts.join("; ")
            )));
        }

        let mut penalised = new.clone();
        penalised.confidence = (new.confidence * SUPERSEDE_CONFIDENCE_PENALTY).clamp(0.0, 1.0);
        self.create(&penalised, new_vector).await?;

        let now = chrono::Utc::now().to_rfc3339();
        let mut update_failures = Vec::new();
        for mut m in olds {
            m.state = MemoryState::Superseded;
            m.superseded_by = Some(new.id.clone());
            m.invalidated_at = Some(now.clone());
            m.updated_at = now.clone();
            if let Err(e) = self.update(&m, None).await {
                update_failures.push(format!("{}: {e}", m.id));
            }
        }
        if !update_failures.is_empty() {
            return Err(OmemError::Storage(format!(
                "new memory {} created, but failed to mark superseded: [{}]",
                new.id,
                update_failures.join("; ")
            )));
        }

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
        include_superseded: bool,
    ) -> Result<Vec<Memory>, OmemError> {
        let table = self.open_table().await?;
        let batches: Vec<RecordBatch> = table
            .query()
            .only_if(state_filter(include_superseded))
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
        include_superseded: bool,
    ) -> Result<Vec<(Memory, f32)>, OmemError> {
        let table = self.open_table().await?;
        let mut query = table
            .query()
            .nearest_to(query_vector)
            .map_err(|e| OmemError::Storage(format!("vector query build failed: {e}")))?;

        query = query.limit(limit);

        let mut filter = state_filter(include_superseded).to_string();
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
        include_superseded: bool,
    ) -> Result<Vec<(Memory, f32)>, OmemError> {
        let table = self.open_table().await?;

        let fts_query = lance_index::scalar::FullTextSearchQuery::new(query.to_string());

        let mut q = table
            .query()
            .full_text_search(fts_query)
            .select(Select::All)
            .limit(limit);

        let mut filter = state_filter(include_superseded).to_string();
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

    pub fn build_visibility_filter(&self, agent_id: &str, accessible_spaces: &[String]) -> String {
        let mut conditions = vec![DEFAULT_STATE_FILTER.to_string()];

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
            .map_err(|e| {
                OmemError::Storage(format!("failed to create FTS index on content: {e}"))
            })?;
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

        let schema = table
            .schema()
            .await
            .map_err(|e| OmemError::Storage(format!("schema check failed: {e}")))?;
        if schema.field_with_name("provenance_source_id").is_err() {
            return Ok(vec![]);
        }

        let filter = format!(
            "{} AND provenance_source_id = '{}'",
            DEFAULT_STATE_FILTER,
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

    pub async fn batch_soft_delete(&self, filter: &str) -> Result<usize, OmemError> {
        let table = self.open_table().await?;
        // Allow batch deletion of already-superseded memories too (cleanup paths
        // may want to garbage-collect old replaced fragments).
        let full_filter = format!("{} AND {}", filter, STATE_FILTER_KEEPING_SUPERSEDED);
        let batches: Vec<RecordBatch> = table
            .query()
            .only_if(&full_filter)
            .execute()
            .await
            .map_err(|e| OmemError::Storage(format!("batch_soft_delete query failed: {e}")))?
            .try_collect()
            .await
            .map_err(|e| OmemError::Storage(format!("collect failed: {e}")))?;

        let memories = Self::batch_to_memories(&batches)?;
        let count = memories.len();

        for mem in memories {
            self.soft_delete(&mem.id).await?;
        }
        Ok(count)
    }

    pub async fn count_by_filter(&self, filter: &str) -> Result<usize, OmemError> {
        let table = self.open_table().await?;
        let full_filter = format!("{} AND {}", filter, DEFAULT_STATE_FILTER);
        let count = table
            .count_rows(Some(full_filter))
            .await
            .map_err(|e| OmemError::Storage(format!("count_by_filter failed: {e}")))?;
        Ok(count)
    }

    pub async fn delete_all(&self) -> Result<usize, OmemError> {
        let all = self.list_all_active().await?;
        let count = all.len();
        for mem in all {
            self.soft_delete(&mem.id).await?;
        }
        Ok(count)
    }

    fn build_where_clause(filter: &ListFilter) -> String {
        let mut conditions = Vec::new();

        match &filter.state {
            Some(s) => conditions.push(format!("state = '{}'", escape_sql(s))),
            None => conditions.push(state_filter(filter.include_superseded).to_string()),
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
                conditions.push(format!("(tags LIKE '%\"{}\"%')", escaped));
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
    async fn test_metadata_only_update_preserves_vector() {
        // Regression: a metadata-only update (caller passes vector = None) must
        // NOT wipe the row's embedding. Previously `memory_to_batch` wrote a
        // zero vector for the None case and `merge_insert.when_matched_update_all`
        // committed it over the real embedding, making the memory invisible to
        // vector search.
        let (store, _dir) = setup().await;
        let mut mem = make_memory("t-001", "memory with an embedding");
        let mut v = vec![0.0f32; DEFAULT_VECTOR_DIM as usize];
        v[0] = 0.5;
        v[1] = 0.25;
        store.create(&mem, Some(&v)).await.unwrap();

        let stored = store
            .get_vector_by_id(&mem.id)
            .await
            .unwrap()
            .expect("vector present after create");
        assert_eq!(stored, v);

        // Metadata-only change: edit tags, request no re-embed (vector = None).
        mem.tags = vec!["touched".to_string()];
        store.update(&mem, None).await.unwrap();

        let after = store
            .get_vector_by_id(&mem.id)
            .await
            .unwrap()
            .expect("vector present after metadata-only update");
        assert_eq!(after, v, "metadata-only update wiped the embedding");
        assert!(
            after.iter().any(|&x| x != 0.0),
            "embedding was zeroed by a metadata-only update"
        );
    }

    #[tokio::test]
    async fn test_with_dim_stores_correct_dimension() {
        // 384 is a common smaller-model dim (e.g. all-MiniLM, bge-small).
        let dir = TempDir::new().expect("temp dir");
        let store = LanceStore::with_dim(dir.path().to_str().unwrap(), 384)
            .await
            .expect("with_dim should construct");
        store.init_table().await.expect("init_table");
        assert_eq!(store.vector_dim(), 384);

        // Store + retrieve a memory with a 384-dim vector — should round-trip.
        let mem = make_memory("t-384", "tiny embedding test");
        let v = vec![0.1f32; 384];
        store
            .create(&mem, Some(&v))
            .await
            .expect("create with 384-dim vector");
        let fetched = store.get_by_id(&mem.id).await.unwrap().expect("memory");
        assert_eq!(fetched.id, mem.id);
    }

    #[tokio::test]
    async fn test_with_dim_rejects_wrong_length_vector() {
        let dir = TempDir::new().expect("temp dir");
        let store = LanceStore::with_dim(dir.path().to_str().unwrap(), 384)
            .await
            .expect("with_dim");
        store.init_table().await.expect("init_table");

        let mem = make_memory("t-bad", "wrong-dim test");
        let v = vec![0.1f32; 768]; // wrong size for a 384-dim table
        let err = store
            .create(&mem, Some(&v))
            .await
            .expect_err("should reject");
        let msg = format!("{err:?}");
        assert!(msg.contains("does not match"), "unexpected error: {msg}");
    }

    #[tokio::test]
    async fn test_init_table_rejects_dim_mismatch_on_reopen() {
        // Create a table at 384 dim, drop, then try to reopen at 1024 dim.
        let dir = TempDir::new().expect("temp dir");
        let uri = dir.path().to_str().unwrap().to_string();
        {
            let store = LanceStore::with_dim(&uri, 384).await.expect("with_dim 384");
            store.init_table().await.expect("init_table");
        }
        let store = LanceStore::with_dim(&uri, 1024)
            .await
            .expect("with_dim 1024");
        let err = store
            .init_table()
            .await
            .expect_err("dim mismatch should error");
        let msg = format!("{err:?}");
        assert!(
            msg.contains("dim mismatch") || msg.contains("vector dim"),
            "unexpected error: {msg}"
        );
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

        let mut v1 = vec![0.0f32; DEFAULT_VECTOR_DIM as usize];
        v1[0] = 1.0;
        let mut v2 = vec![0.0f32; DEFAULT_VECTOR_DIM as usize];
        v2[0] = 0.9;
        v2[1] = 0.1;
        let mut v3 = vec![0.0f32; DEFAULT_VECTOR_DIM as usize];
        v3[1] = 1.0;

        let m1 = make_memory("t-001", "closest match");
        let m2 = make_memory("t-001", "second closest");
        let m3 = make_memory("t-001", "furthest match");

        store.create(&m1, Some(&v1)).await.unwrap();
        store.create(&m2, Some(&v2)).await.unwrap();
        store.create(&m3, Some(&v3)).await.unwrap();

        let mut query_vec = vec![0.0f32; DEFAULT_VECTOR_DIM as usize];
        query_vec[0] = 1.0;

        let results = store
            .vector_search(&query_vec, 3, 0.0, None, None, false)
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

        let results = store
            .fts_search("programming language", 10, None, None, false)
            .await
            .unwrap();

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
    async fn test_update_replaces_row_atomically() {
        // Regression: update() used to delete-then-add, which could lose the
        // row if interrupted between the two ops. It now upserts via
        // merge_insert, so the row is always replaced in place — exactly one
        // copy, never zero.
        let (store, _dir) = setup().await;
        let mem = make_memory("t-upd", "original content");
        store.create(&mem, None).await.unwrap();
        assert_eq!(store.list(100, 0, false).await.unwrap().len(), 1);

        let mut fetched = store.get_by_id(&mem.id).await.unwrap().expect("created");
        let v0 = fetched.version.unwrap_or(0);
        fetched.content = "updated content".to_string();
        fetched.l2_content = "updated content".to_string();
        store.update(&fetched, None).await.unwrap();

        // Exactly one row remains (no duplicate, no loss) with new content + bumped version.
        let all = store.list(100, 0, false).await.unwrap();
        assert_eq!(all.len(), 1, "update must not duplicate or drop the row");
        let after = store
            .get_by_id(&mem.id)
            .await
            .unwrap()
            .expect("still present");
        assert_eq!(after.content, "updated content");
        assert!(after.version.unwrap_or(0) > v0, "version should increment");
    }

    #[tokio::test]
    async fn test_update_upserts_when_row_absent() {
        // merge_insert's when_not_matched_insert_all means update() inserts the
        // row if it doesn't exist yet, rather than silently no-op'ing.
        let (store, _dir) = setup().await;
        let mem = make_memory("t-ups", "inserted via update");
        store.update(&mem, None).await.unwrap();
        let fetched = store.get_by_id(&mem.id).await.unwrap();
        assert!(
            fetched.is_some(),
            "update should upsert when the row is absent"
        );
        assert_eq!(fetched.unwrap().content, "inserted via update");
    }

    #[tokio::test]
    async fn test_list_with_pagination() {
        let (store, _dir) = setup().await;

        for i in 0..5 {
            let mem = make_memory("t-001", &format!("memory {i}"));
            store.create(&mem, None).await.unwrap();
        }

        let page1 = store.list(2, 0, false).await.unwrap();
        assert_eq!(page1.len(), 2);

        let page2 = store.list(2, 2, false).await.unwrap();
        assert_eq!(page2.len(), 2);

        let page3 = store.list(2, 4, false).await.unwrap();
        assert_eq!(page3.len(), 1);
    }

    #[tokio::test]
    async fn test_multi_tenant_isolation() {
        let (store_a, _dir_a) = setup().await;
        let (store_b, _dir_b) = setup().await;

        let mut va = vec![0.0f32; DEFAULT_VECTOR_DIM as usize];
        va[0] = 1.0;
        let mut vb = vec![0.0f32; DEFAULT_VECTOR_DIM as usize];
        vb[0] = 1.0;

        let mem_a = make_memory("tenant_A", "secret data for A");
        let mem_b = make_memory("tenant_B", "secret data for B");

        store_a.create(&mem_a, Some(&va)).await.unwrap();
        store_b.create(&mem_b, Some(&vb)).await.unwrap();

        let list_a = store_a.list(100, 0, false).await.unwrap();
        assert_eq!(list_a.len(), 1);
        assert_eq!(list_a[0].tenant_id, "tenant_A");

        let list_b = store_b.list(100, 0, false).await.unwrap();
        assert_eq!(list_b.len(), 1);
        assert_eq!(list_b[0].tenant_id, "tenant_B");
    }

    #[tokio::test]
    async fn test_list_filtered_by_category() {
        let (store, _dir) = setup().await;

        let m1 = Memory::new(
            "dark mode pref",
            Category::Preferences,
            MemoryType::Insight,
            "t-001",
        );
        let m2 = Memory::new(
            "another pref",
            Category::Preferences,
            MemoryType::Insight,
            "t-001",
        );
        let m3 = Memory::new(
            "meeting happened",
            Category::Events,
            MemoryType::Session,
            "t-001",
        );

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
        assert!(result.contains("state NOT IN ('deleted', 'superseded')"));
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

    #[tokio::test]
    async fn test_schema_evolution_adds_missing_columns() {
        let dir = TempDir::new().unwrap();
        let store = LanceStore::new(dir.path().to_str().unwrap()).await.unwrap();

        let old_schema = Arc::new(Schema::new(
            store
                .schema()
                .fields()
                .iter()
                .filter(|f| f.name() != "version" && f.name() != "provenance_source_id")
                .cloned()
                .collect::<Vec<_>>(),
        ));
        assert_eq!(old_schema.fields().len(), 29);

        store
            .db
            .create_empty_table(&store.table_name, old_schema)
            .execute()
            .await
            .unwrap();

        let table_before = store.open_table().await.unwrap();
        let schema_before = table_before.schema().await.unwrap();
        assert!(schema_before.field_with_name("version").is_err());
        assert!(schema_before
            .field_with_name("provenance_source_id")
            .is_err());

        store.init_table().await.unwrap();

        let table_after = store.open_table().await.unwrap();
        let schema_after = table_after.schema().await.unwrap();
        assert!(schema_after.field_with_name("version").is_ok());
        assert!(schema_after.field_with_name("provenance_source_id").is_ok());
        assert_eq!(schema_after.fields().len(), 31);
    }

    #[tokio::test]
    async fn test_init_table_idempotent() {
        let dir = TempDir::new().unwrap();
        let store = LanceStore::new(dir.path().to_str().unwrap()).await.unwrap();

        store.init_table().await.unwrap();

        let table = store.open_table().await.unwrap();
        let schema = table.schema().await.unwrap();
        let col_count = schema.fields().len();

        store.init_table().await.unwrap();

        let table2 = store.open_table().await.unwrap();
        let schema2 = table2.schema().await.unwrap();
        assert_eq!(schema2.fields().len(), col_count);
    }

    #[tokio::test]
    async fn test_find_by_provenance_source_missing_column() {
        let dir = TempDir::new().unwrap();
        let store = LanceStore::new(dir.path().to_str().unwrap()).await.unwrap();

        let old_schema = Arc::new(Schema::new(
            store
                .schema()
                .fields()
                .iter()
                .filter(|f| f.name() != "provenance_source_id")
                .cloned()
                .collect::<Vec<_>>(),
        ));
        store
            .db
            .create_empty_table(&store.table_name, old_schema)
            .execute()
            .await
            .unwrap();

        let result = store.find_by_provenance_source("some-id").await.unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_supersede_batch_marks_old_as_superseded() {
        let (store, _dir) = setup().await;
        let v = vec![0.1f32; DEFAULT_VECTOR_DIM as usize];

        let old1 = make_memory("t-001", "fragment 1 of 3");
        let old2 = make_memory("t-001", "fragment 2 of 3");
        let old3 = make_memory("t-001", "fragment 3 of 3");
        store.create(&old1, Some(&v)).await.unwrap();
        store.create(&old2, Some(&v)).await.unwrap();
        store.create(&old3, Some(&v)).await.unwrap();

        let new = make_memory("t-001", "consolidated content");
        let old_ids = vec![old1.id.clone(), old2.id.clone(), old3.id.clone()];
        store
            .supersede_batch(&new, Some(&v), &old_ids)
            .await
            .expect("supersede should succeed");

        for id in &old_ids {
            let fetched = store
                .get_by_id(id)
                .await
                .unwrap()
                .expect("old still exists");
            assert!(matches!(fetched.state, MemoryState::Superseded));
            assert_eq!(fetched.superseded_by.as_deref(), Some(new.id.as_str()));
            assert!(fetched.invalidated_at.is_some());
        }

        let new_fetched = store.get_by_id(&new.id).await.unwrap().expect("new exists");
        assert!(matches!(new_fetched.state, MemoryState::Active));
    }

    #[tokio::test]
    async fn test_supersede_batch_rejects_missing_id() {
        let (store, _dir) = setup().await;
        let v = vec![0.1f32; DEFAULT_VECTOR_DIM as usize];

        let real = make_memory("t-001", "existing memory");
        store.create(&real, Some(&v)).await.unwrap();

        let new = make_memory("t-001", "consolidated");
        let old_ids = vec![real.id.clone(), "ghost-id-does-not-exist".to_string()];
        let err = store
            .supersede_batch(&new, Some(&v), &old_ids)
            .await
            .expect_err("missing id should reject");

        let msg = format!("{err:?}");
        assert!(
            msg.contains("missing"),
            "error should mention missing: {msg}"
        );
        assert!(
            msg.contains("ghost-id-does-not-exist"),
            "error should list the ghost id: {msg}"
        );

        // No write happened — original memory unchanged, new memory not created.
        let fetched = store
            .get_by_id(&real.id)
            .await
            .unwrap()
            .expect("still there");
        assert!(matches!(fetched.state, MemoryState::Active));
        assert!(store.get_by_id(&new.id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_supersede_batch_rejects_already_superseded() {
        let (store, _dir) = setup().await;
        let v = vec![0.1f32; DEFAULT_VECTOR_DIM as usize];

        let old = make_memory("t-001", "original");
        store.create(&old, Some(&v)).await.unwrap();
        let first_new = make_memory("t-001", "first consolidation");
        store
            .supersede_batch(&first_new, Some(&v), &[old.id.clone()])
            .await
            .unwrap();

        // Trying to supersede `old` again should reject.
        let second_new = make_memory("t-001", "second attempt");
        let err = store
            .supersede_batch(&second_new, Some(&v), &[old.id.clone()])
            .await
            .expect_err("already-superseded should reject");

        let msg = format!("{err:?}");
        assert!(
            msg.contains("already superseded"),
            "error should mention already-superseded: {msg}"
        );
        assert!(
            store.get_by_id(&second_new.id).await.unwrap().is_none(),
            "second_new should NOT have been created"
        );
    }

    #[tokio::test]
    async fn test_default_state_filter_excludes_superseded() {
        let (store, _dir) = setup().await;
        let v = vec![0.1f32; DEFAULT_VECTOR_DIM as usize];

        let old = make_memory("t-001", "to be superseded");
        let alive = make_memory("t-001", "still active");
        store.create(&old, Some(&v)).await.unwrap();
        store.create(&alive, Some(&v)).await.unwrap();
        let new = make_memory("t-001", "replacement");
        store
            .supersede_batch(&new, Some(&v), &[old.id.clone()])
            .await
            .unwrap();

        // Default list excludes the superseded `old`.
        let listed = store.list(100, 0, false).await.unwrap();
        let ids: Vec<&str> = listed.iter().map(|m| m.id.as_str()).collect();
        assert!(!ids.contains(&old.id.as_str()), "old should be hidden");
        assert!(ids.contains(&alive.id.as_str()));
        assert!(ids.contains(&new.id.as_str()));

        // include_superseded=true surfaces it.
        let listed_with = store.list(100, 0, true).await.unwrap();
        let ids_with: Vec<&str> = listed_with.iter().map(|m| m.id.as_str()).collect();
        assert!(ids_with.contains(&old.id.as_str()));

        // get_by_id always returns regardless of state (history preserved).
        let direct = store.get_by_id(&old.id).await.unwrap();
        assert!(direct.is_some(), "get_by_id should still return superseded");
    }

    #[tokio::test]
    async fn test_supersede_applies_confidence_penalty() {
        let (store, _dir) = setup().await;
        let v = vec![0.1f32; DEFAULT_VECTOR_DIM as usize];

        let old = make_memory("t-001", "original fact");
        store.create(&old, Some(&v)).await.unwrap();

        let mut new = make_memory("t-001", "replacement fact");
        new.confidence = 0.5;

        store
            .supersede_batch(&new, Some(&v), &[old.id.clone()])
            .await
            .unwrap();

        let stored = store.get_by_id(&new.id).await.unwrap().unwrap();
        let expected = 0.5 * SUPERSEDE_CONFIDENCE_PENALTY;
        assert!(
            (stored.confidence - expected).abs() < f32::EPSILON,
            "confidence should be {expected}, got {}",
            stored.confidence
        );
    }

    #[tokio::test]
    async fn test_supersede_no_penalty_without_old_ids() {
        let (store, _dir) = setup().await;

        let mut mem = make_memory("t-001", "new fact, no replacement");
        mem.confidence = 0.5;
        store.supersede_batch(&mem, None, &[]).await.unwrap();

        let stored = store.get_by_id(&mem.id).await.unwrap().unwrap();
        assert!(
            (stored.confidence - 0.5).abs() < f32::EPSILON,
            "confidence should be unchanged at 0.5 when no old_ids, got {}",
            stored.confidence
        );
    }
}
