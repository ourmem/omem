use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;

use tracing::warn;

use crate::domain::error::OmemError;
use crate::domain::memory::Memory;
use crate::lifecycle::decay::{DecayConfig, DecayEngine};
use crate::store::lancedb::LanceStore;

use super::reranker::Reranker;
use super::trace::{RetrievalTrace, StageTrace};

pub struct SearchRequest {
    pub query: String,
    pub query_vector: Option<Vec<f32>>,
    pub tenant_id: String,
    pub scope_filter: Option<String>,
    pub limit: Option<usize>,
    pub min_score: Option<f32>,
    pub include_trace: bool,
    pub tags_filter: Option<Vec<String>>,
    pub source_filter: Option<String>,
    pub agent_id_filter: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub memory: Memory,
    pub score: f32,
}

pub struct SearchResults {
    pub results: Vec<SearchResult>,
    pub trace: RetrievalTrace,
}

pub struct RetrievalPipeline {
    store: Arc<LanceStore>,
    reranker: Option<Reranker>,
    decay_engine: DecayEngine,
    pinned_boost: f32,
    rrf_k: f32,
    vector_weight: f32,
    bm25_weight: f32,
    min_score: f32,
    hard_cutoff: f32,
    default_limit: usize,
}

struct FusionEntry {
    memory: Memory,
    rrf_score: f32,
    bm25_score: f32,
    pre_rerank_score: f32,
}

impl RetrievalPipeline {
    pub fn new(store: Arc<LanceStore>) -> Self {
        Self {
            store,
            reranker: None,
            decay_engine: DecayEngine::new(DecayConfig::default()),
            pinned_boost: 1.5,
            rrf_k: 60.0,
            vector_weight: 0.7,
            bm25_weight: 0.3,
            min_score: 0.3,
            hard_cutoff: 0.005,
            default_limit: 20,
        }
    }

    pub fn with_pinned_boost(mut self, boost: f32) -> Self {
        self.pinned_boost = boost;
        self
    }

    pub fn with_rrf_k(mut self, k: f32) -> Self {
        self.rrf_k = k;
        self
    }

    pub fn with_weights(mut self, vector: f32, bm25: f32) -> Self {
        self.vector_weight = vector;
        self.bm25_weight = bm25;
        self
    }

    pub fn with_min_score(mut self, score: f32) -> Self {
        self.min_score = score;
        self
    }

    pub fn with_hard_cutoff(mut self, cutoff: f32) -> Self {
        self.hard_cutoff = cutoff;
        self
    }

    pub fn with_default_limit(mut self, limit: usize) -> Self {
        self.default_limit = limit;
        self
    }

    pub fn with_reranker(mut self, reranker: Reranker) -> Self {
        self.reranker = Some(reranker);
        self
    }

    pub fn with_decay_engine(mut self, engine: DecayEngine) -> Self {
        self.decay_engine = engine;
        self
    }

    pub async fn search(&self, request: &SearchRequest) -> Result<SearchResults, OmemError> {
        let pipeline_start = Instant::now();
        let mut trace = RetrievalTrace::new();
        let limit = request.limit.unwrap_or(self.default_limit);
        let min_score = request.min_score.unwrap_or(self.min_score);
        let fetch_limit = limit * 3;

        let (candidates, stage1) = self.stage_parallel_search(request, fetch_limit).await?;
        trace.add_stage(stage1);

        if candidates.vector.is_empty() && candidates.bm25.is_empty() {
            trace.finalize(0, pipeline_start.elapsed().as_millis() as u64);
            return Ok(SearchResults {
                results: Vec::new(),
                trace,
            });
        }

        let (fused, stage2) = self.stage_rrf_fusion(candidates);
        trace.add_stage(stage2);

        let (normalized_rrf, stage3) = Self::stage_rrf_normalize(fused);
        trace.add_stage(stage3);

        let (filtered, stage4) = Self::stage_min_score_filter(normalized_rrf, min_score);
        trace.add_stage(stage4);

        let (capped, stage5) = Self::stage_topk_cap(filtered, limit * 2);
        trace.add_stage(stage5);

        let (reranked, stage6) = self
            .stage_cross_encoder_rerank(capped, &request.query)
            .await;
        trace.add_stage(stage6);

        let (floored, stage7) = Self::stage_bm25_floor(reranked);
        trace.add_stage(stage7);

        let (decayed, stage8) = self.stage_decay_boost(floored);
        trace.add_stage(stage8);

        let (weighted, stage9) = Self::stage_importance_weight(decayed);
        trace.add_stage(stage9);

        let (normalized, stage10) = Self::stage_length_normalization(weighted);
        trace.add_stage(stage10);

        let (cutoff_results, stage11) = Self::stage_hard_cutoff(normalized, self.hard_cutoff);
        trace.add_stage(stage11);

        let (final_entries, stage12) = Self::stage_mmr_diversity(cutoff_results, limit);
        trace.add_stage(stage12);

        let results: Vec<SearchResult> = final_entries
            .into_iter()
            .map(|e| SearchResult {
                memory: e.memory,
                score: e.rrf_score,
            })
            .collect();

        let count = results.len();
        trace.finalize(count, pipeline_start.elapsed().as_millis() as u64);

        Ok(SearchResults { results, trace })
    }

    async fn stage_parallel_search(
        &self,
        request: &SearchRequest,
        fetch_limit: usize,
    ) -> Result<(ParallelResults, StageTrace), OmemError> {
        let stage_start = Instant::now();
        let scope = request.scope_filter.as_deref();

        let vector_fut = async {
            if let Some(ref qv) = request.query_vector {
                self.store
                    .vector_search(qv, fetch_limit, 0.0, scope, None)
                    .await
            } else {
                Ok(Vec::new())
            }
        };

        let bm25_fut = async {
            self.store
                .fts_search(&request.query, fetch_limit, scope, None)
                .await
        };

        let (vector_res, bm25_res) = tokio::join!(vector_fut, bm25_fut);

        let vector_results = match vector_res {
            Ok(v) => v,
            Err(e) => {
                warn!("vector search failed, falling back to BM25-only: {e}");
                Vec::new()
            }
        };

        let bm25_results = match bm25_res {
            Ok(v) => v,
            Err(e) => {
                warn!("BM25 search failed, falling back to vector-only: {e}");
                Vec::new()
            }
        };

        let total_count = vector_results.len() + bm25_results.len();
        let score_range = Self::compute_score_range(&vector_results, &bm25_results);

        let stage = StageTrace {
            name: "parallel_search".to_string(),
            input_count: 0,
            output_count: total_count,
            dropped_ids: Vec::new(),
            score_range,
            duration_ms: stage_start.elapsed().as_millis() as u64,
        };

        Ok((
            ParallelResults {
                vector: vector_results,
                bm25: bm25_results,
            },
            stage,
        ))
    }

    fn stage_rrf_fusion(&self, candidates: ParallelResults) -> (Vec<FusionEntry>, StageTrace) {
        let stage_start = Instant::now();
        let input_count = candidates.vector.len() + candidates.bm25.len();

        let mut score_map: HashMap<String, FusionEntry> = HashMap::new();

        for (rank, (memory, _score)) in candidates.vector.into_iter().enumerate() {
            let rrf = self.vector_weight / (self.rrf_k + (rank + 1) as f32);
            score_map
                .entry(memory.id.clone())
                .and_modify(|e| e.rrf_score += rrf)
                .or_insert(FusionEntry {
                    memory,
                    rrf_score: rrf,
                    bm25_score: 0.0,
                    pre_rerank_score: 0.0,
                });
        }

        for (rank, (memory, bm25_raw)) in candidates.bm25.into_iter().enumerate() {
            let rrf = self.bm25_weight / (self.rrf_k + (rank + 1) as f32);
            score_map
                .entry(memory.id.clone())
                .and_modify(|e| {
                    e.rrf_score += rrf;
                    e.bm25_score = bm25_raw;
                })
                .or_insert(FusionEntry {
                    memory,
                    rrf_score: rrf,
                    bm25_score: bm25_raw,
                    pre_rerank_score: 0.0,
                });
        }

        let mut fused: Vec<FusionEntry> = score_map.into_values().collect();

        for entry in &mut fused {
            if entry.memory.memory_type.is_pinned() {
                entry.rrf_score *= self.pinned_boost;
            }
        }

        let score_range = fusion_score_range(&fused);

        let stage = StageTrace {
            name: "rrf_fusion".to_string(),
            input_count,
            output_count: fused.len(),
            dropped_ids: Vec::new(),
            score_range,
            duration_ms: stage_start.elapsed().as_millis() as u64,
        };

        (fused, stage)
    }

    /// Normalize RRF scores to [0, 1] range so downstream thresholds (min_score, hard_cutoff) work correctly.
    /// RRF raw scores are tiny (max ~0.033 for K=60 with 2 legs), but thresholds expect [0, 1].
    /// - Multiple results: min-max normalization (best=1.0, worst=0.0)
    /// - Single result: scale by RRF_SCALE (40.0) and clamp to [0, 1]
    fn stage_rrf_normalize(mut entries: Vec<FusionEntry>) -> (Vec<FusionEntry>, StageTrace) {
        const RRF_SCALE: f32 = 40.0;
        let stage_start = Instant::now();
        let input_count = entries.len();

        if entries.len() > 1 {
            let max_score = entries
                .iter()
                .map(|e| e.rrf_score)
                .fold(f32::NEG_INFINITY, f32::max);
            let min_score = entries
                .iter()
                .map(|e| e.rrf_score)
                .fold(f32::INFINITY, f32::min);
            let range = max_score - min_score;
            if range > 0.0 {
                for entry in &mut entries {
                    entry.rrf_score = (entry.rrf_score - min_score) / range;
                }
            } else if max_score > 0.0 {
                for entry in &mut entries {
                    entry.rrf_score = 1.0;
                }
            }
        } else if entries.len() == 1 {
            entries[0].rrf_score = (entries[0].rrf_score * RRF_SCALE).min(1.0);
        }

        let score_range = fusion_score_range(&entries);

        let stage = StageTrace {
            name: "rrf_normalize".to_string(),
            input_count,
            output_count: entries.len(),
            dropped_ids: Vec::new(),
            score_range,
            duration_ms: stage_start.elapsed().as_millis() as u64,
        };

        (entries, stage)
    }

    fn stage_min_score_filter(
        entries: Vec<FusionEntry>,
        min_score: f32,
    ) -> (Vec<FusionEntry>, StageTrace) {
        let stage_start = Instant::now();
        let input_count = entries.len();

        let mut kept = Vec::new();
        let mut dropped_ids = Vec::new();

        for entry in entries {
            if entry.rrf_score >= min_score {
                kept.push(entry);
            } else {
                dropped_ids.push(entry.memory.id);
            }
        }

        let score_range = fusion_score_range(&kept);

        let stage = StageTrace {
            name: "min_score_filter".to_string(),
            input_count,
            output_count: kept.len(),
            dropped_ids,
            score_range,
            duration_ms: stage_start.elapsed().as_millis() as u64,
        };

        (kept, stage)
    }

    fn stage_topk_cap(
        mut entries: Vec<FusionEntry>,
        limit: usize,
    ) -> (Vec<FusionEntry>, StageTrace) {
        let stage_start = Instant::now();
        let input_count = entries.len();

        entries.sort_by(|a, b| {
            b.rrf_score
                .partial_cmp(&a.rrf_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let dropped_ids: Vec<String> = entries
            .iter()
            .skip(limit)
            .map(|e| e.memory.id.clone())
            .collect();

        entries.truncate(limit);

        let score_range = fusion_score_range(&entries);

        let stage = StageTrace {
            name: "topk_cap".to_string(),
            input_count,
            output_count: entries.len(),
            dropped_ids,
            score_range,
            duration_ms: stage_start.elapsed().as_millis() as u64,
        };

        (entries, stage)
    }

    async fn stage_cross_encoder_rerank(
        &self,
        mut entries: Vec<FusionEntry>,
        query: &str,
    ) -> (Vec<FusionEntry>, StageTrace) {
        let stage_start = Instant::now();
        let input_count = entries.len();

        for e in &mut entries {
            e.pre_rerank_score = e.rrf_score;
        }

        if let Some(ref reranker) = self.reranker {
            let docs: Vec<&str> = entries.iter().map(|e| e.memory.content.as_str()).collect();
            match reranker.rerank(query, &docs).await {
                Ok(scores) => {
                    for (entry, &rerank_score) in entries.iter_mut().zip(scores.iter()) {
                        entry.rrf_score = rerank_score * 0.6 + entry.pre_rerank_score * 0.4;
                    }
                }
                Err(e) => {
                    warn!("reranker failed, keeping original scores: {e}");
                }
            }
        }

        let score_range = fusion_score_range(&entries);

        let stage = StageTrace {
            name: "cross_encoder_rerank".to_string(),
            input_count,
            output_count: entries.len(),
            dropped_ids: Vec::new(),
            score_range,
            duration_ms: stage_start.elapsed().as_millis() as u64,
        };

        (entries, stage)
    }

    fn stage_bm25_floor(mut entries: Vec<FusionEntry>) -> (Vec<FusionEntry>, StageTrace) {
        let stage_start = Instant::now();
        let input_count = entries.len();

        for entry in &mut entries {
            if entry.bm25_score >= 0.75 {
                let floor = entry.pre_rerank_score * 0.95;
                if entry.rrf_score < floor {
                    entry.rrf_score = floor;
                }
            }
        }

        let score_range = fusion_score_range(&entries);

        let stage = StageTrace {
            name: "bm25_floor".to_string(),
            input_count,
            output_count: entries.len(),
            dropped_ids: Vec::new(),
            score_range,
            duration_ms: stage_start.elapsed().as_millis() as u64,
        };

        (entries, stage)
    }

    fn stage_decay_boost(&self, mut entries: Vec<FusionEntry>) -> (Vec<FusionEntry>, StageTrace) {
        let stage_start = Instant::now();
        let input_count = entries.len();

        for entry in &mut entries {
            entry.rrf_score = self
                .decay_engine
                .apply_search_boost(entry.rrf_score, &entry.memory);
        }

        let score_range = fusion_score_range(&entries);

        let stage = StageTrace {
            name: "decay_boost".to_string(),
            input_count,
            output_count: entries.len(),
            dropped_ids: Vec::new(),
            score_range,
            duration_ms: stage_start.elapsed().as_millis() as u64,
        };

        (entries, stage)
    }

    fn stage_importance_weight(mut entries: Vec<FusionEntry>) -> (Vec<FusionEntry>, StageTrace) {
        let stage_start = Instant::now();
        let input_count = entries.len();

        for entry in &mut entries {
            entry.rrf_score *= 0.7 + 0.3 * entry.memory.importance;
        }

        let score_range = fusion_score_range(&entries);

        let stage = StageTrace {
            name: "importance_weight".to_string(),
            input_count,
            output_count: entries.len(),
            dropped_ids: Vec::new(),
            score_range,
            duration_ms: stage_start.elapsed().as_millis() as u64,
        };

        (entries, stage)
    }

    fn stage_length_normalization(mut entries: Vec<FusionEntry>) -> (Vec<FusionEntry>, StageTrace) {
        let stage_start = Instant::now();
        let input_count = entries.len();

        for entry in &mut entries {
            let len_ratio = entry.memory.content.len() as f32 / 500.0;
            let log_val = if len_ratio > 0.0 {
                len_ratio.log2()
            } else {
                0.0
            };
            let denominator = (1.0 + log_val).max(1.0);
            entry.rrf_score /= denominator;
        }

        let score_range = fusion_score_range(&entries);

        let stage = StageTrace {
            name: "length_normalization".to_string(),
            input_count,
            output_count: entries.len(),
            dropped_ids: Vec::new(),
            score_range,
            duration_ms: stage_start.elapsed().as_millis() as u64,
        };

        (entries, stage)
    }

    fn stage_hard_cutoff(
        entries: Vec<FusionEntry>,
        threshold: f32,
    ) -> (Vec<FusionEntry>, StageTrace) {
        let stage_start = Instant::now();
        let input_count = entries.len();

        let mut kept = Vec::new();
        let mut dropped_ids = Vec::new();

        for entry in entries {
            if entry.rrf_score >= threshold {
                kept.push(entry);
            } else {
                dropped_ids.push(entry.memory.id);
            }
        }

        let score_range = fusion_score_range(&kept);

        let stage = StageTrace {
            name: "hard_cutoff".to_string(),
            input_count,
            output_count: kept.len(),
            dropped_ids,
            score_range,
            duration_ms: stage_start.elapsed().as_millis() as u64,
        };

        (kept, stage)
    }

    fn stage_mmr_diversity(
        mut entries: Vec<FusionEntry>,
        limit: usize,
    ) -> (Vec<FusionEntry>, StageTrace) {
        let stage_start = Instant::now();
        let input_count = entries.len();

        entries.sort_by(|a, b| {
            b.rrf_score
                .partial_cmp(&a.rrf_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        for i in 1..entries.len() {
            let mut too_similar = false;
            for j in 0..i {
                if content_jaccard(&entries[j].memory.content, &entries[i].memory.content) > 0.85 {
                    too_similar = true;
                    break;
                }
            }
            if too_similar {
                entries[i].rrf_score *= 0.5;
            }
        }

        entries.sort_by(|a, b| {
            b.rrf_score
                .partial_cmp(&a.rrf_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let dropped_ids: Vec<String> = entries
            .iter()
            .skip(limit)
            .map(|e| e.memory.id.clone())
            .collect();
        entries.truncate(limit);

        let score_range = fusion_score_range(&entries);

        let stage = StageTrace {
            name: "mmr_diversity".to_string(),
            input_count,
            output_count: entries.len(),
            dropped_ids,
            score_range,
            duration_ms: stage_start.elapsed().as_millis() as u64,
        };

        (entries, stage)
    }

    fn compute_score_range(
        vec_results: &[(Memory, f32)],
        bm25_results: &[(Memory, f32)],
    ) -> Option<(f32, f32)> {
        let all_scores: Vec<f32> = vec_results
            .iter()
            .chain(bm25_results.iter())
            .map(|(_, s)| *s)
            .collect();

        if all_scores.is_empty() {
            return None;
        }
        let min = all_scores.iter().copied().fold(f32::INFINITY, f32::min);
        let max = all_scores.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        Some((min, max))
    }
}

struct ParallelResults {
    vector: Vec<(Memory, f32)>,
    bm25: Vec<(Memory, f32)>,
}

fn fusion_score_range(entries: &[FusionEntry]) -> Option<(f32, f32)> {
    if entries.is_empty() {
        return None;
    }
    let min = entries
        .iter()
        .map(|e| e.rrf_score)
        .fold(f32::INFINITY, f32::min);
    let max = entries
        .iter()
        .map(|e| e.rrf_score)
        .fold(f32::NEG_INFINITY, f32::max);
    Some((min, max))
}

fn content_jaccard(a: &str, b: &str) -> f32 {
    let a_words: HashSet<&str> = a.split_whitespace().collect();
    let b_words: HashSet<&str> = b.split_whitespace().collect();
    let union_count = a_words.union(&b_words).count();
    if union_count == 0 {
        return 1.0;
    }
    let inter_count = a_words.intersection(&b_words).count();
    inter_count as f32 / union_count as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::category::Category;
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

    fn make_memory(tenant: &str, content: &str, mem_type: MemoryType) -> Memory {
        Memory::new(content, Category::Preferences, mem_type, tenant)
    }

    fn make_entry(content: &str, score: f32) -> FusionEntry {
        FusionEntry {
            memory: make_memory("t", content, MemoryType::Insight),
            rrf_score: score,
            bm25_score: 0.0,
            pre_rerank_score: 0.0,
        }
    }

    const DIM: usize = 1024;

    fn make_vector(primary: usize, value: f32) -> Vec<f32> {
        let mut v = vec![0.0f32; DIM];
        v[primary] = value;
        v
    }

    #[tokio::test]
    async fn test_hybrid_search() {
        let (store, _dir) = setup().await;

        let m1 = make_memory(
            "t-001",
            "rust programming language is fast",
            MemoryType::Insight,
        );
        let m2 = make_memory("t-001", "python scripting language", MemoryType::Insight);
        let m3 = make_memory("t-001", "the weather is sunny today", MemoryType::Insight);

        let v1 = make_vector(0, 1.0);
        let v2 = make_vector(0, 0.8);
        let v3 = make_vector(1, 1.0);

        store.create(&m1, Some(&v1)).await.expect("create m1");
        store.create(&m2, Some(&v2)).await.expect("create m2");
        store.create(&m3, Some(&v3)).await.expect("create m3");

        store.create_fts_index().await.expect("create fts index");

        let pipeline = RetrievalPipeline::new(store)
            .with_min_score(0.0)
            .with_hard_cutoff(0.0);

        let request = SearchRequest {
            query: "rust programming".to_string(),
            query_vector: Some(make_vector(0, 1.0)),
            tenant_id: "t-001".to_string(),
            scope_filter: None,
            limit: Some(10),
            min_score: Some(0.0),
            include_trace: true,
            tags_filter: None,
            source_filter: None,
            agent_id_filter: None,
        };

        let results = pipeline.search(&request).await.expect("search");
        assert!(!results.results.is_empty());
        assert_eq!(results.trace.stages.len(), 12);
    }

    #[tokio::test]
    async fn test_rrf_fusion_scoring() {
        let rrf_k = 60.0_f32;
        let vector_weight = 0.7_f32;
        let bm25_weight = 0.3_f32;

        let rank1_vector = vector_weight / (rrf_k + 1.0);
        let rank1_bm25 = bm25_weight / (rrf_k + 1.0);
        let rank2_vector = vector_weight / (rrf_k + 2.0);

        let combined = rank1_vector + rank1_bm25;
        assert!(
            combined > rank2_vector,
            "item in both legs should score higher than item in one leg"
        );

        let expected_combined = 1.0 / (rrf_k + 1.0);
        let diff = (combined - expected_combined).abs();
        assert!(
            diff < 1e-6,
            "combined should equal 1/(k+1) when weights sum to 1.0"
        );
    }

    #[tokio::test]
    async fn test_pinned_boost() {
        let (store, _dir) = setup().await;

        let m_pinned = make_memory("t-001", "important pinned fact", MemoryType::Pinned);
        let m_normal = make_memory("t-001", "important normal fact", MemoryType::Insight);

        let v = make_vector(0, 1.0);
        store
            .create(&m_pinned, Some(&v))
            .await
            .expect("create pinned");
        store
            .create(&m_normal, Some(&v))
            .await
            .expect("create normal");

        let pipeline = RetrievalPipeline::new(store)
            .with_min_score(0.0)
            .with_hard_cutoff(0.0);

        let request = SearchRequest {
            query: "important".to_string(),
            query_vector: Some(make_vector(0, 1.0)),
            tenant_id: "t-001".to_string(),
            scope_filter: None,
            limit: Some(10),
            min_score: Some(0.0),
            include_trace: false,
            tags_filter: None,
            source_filter: None,
            agent_id_filter: None,
        };

        let results = pipeline.search(&request).await.expect("search");
        assert!(!results.results.is_empty());

        let pinned_result = results
            .results
            .iter()
            .find(|r| r.memory.memory_type.is_pinned());
        let normal_result = results
            .results
            .iter()
            .find(|r| !r.memory.memory_type.is_pinned());

        if let (Some(p), Some(n)) = (pinned_result, normal_result) {
            assert!(
                p.score > n.score,
                "pinned ({}) should outscore normal ({})",
                p.score,
                n.score,
            );
        }
    }

    #[tokio::test]
    async fn test_min_score_filter() {
        let entries = vec![
            FusionEntry {
                memory: make_memory("t", "high", MemoryType::Insight),
                rrf_score: 0.8,
                bm25_score: 0.0,
                pre_rerank_score: 0.0,
            },
            FusionEntry {
                memory: make_memory("t", "low", MemoryType::Insight),
                rrf_score: 0.1,
                bm25_score: 0.0,
                pre_rerank_score: 0.0,
            },
            FusionEntry {
                memory: make_memory("t", "mid", MemoryType::Insight),
                rrf_score: 0.5,
                bm25_score: 0.0,
                pre_rerank_score: 0.0,
            },
        ];

        let (kept, stage) = RetrievalPipeline::stage_min_score_filter(entries, 0.3);
        assert_eq!(kept.len(), 2);
        assert_eq!(stage.dropped_ids.len(), 1);
        assert_eq!(stage.name, "min_score_filter");
        assert!(kept.iter().all(|e| e.rrf_score >= 0.3));
    }

    #[tokio::test]
    async fn test_fallback_vector_only() {
        let (store, _dir) = setup().await;

        let m1 = make_memory("t-001", "vector only content", MemoryType::Insight);
        let v1 = make_vector(0, 1.0);
        store.create(&m1, Some(&v1)).await.expect("create m1");

        let pipeline = RetrievalPipeline::new(store)
            .with_min_score(0.0)
            .with_hard_cutoff(0.0);

        let request = SearchRequest {
            query: "nonexistent fts query".to_string(),
            query_vector: Some(make_vector(0, 1.0)),
            tenant_id: "t-001".to_string(),
            scope_filter: None,
            limit: Some(10),
            min_score: Some(0.0),
            include_trace: true,
            tags_filter: None,
            source_filter: None,
            agent_id_filter: None,
        };

        let results = pipeline
            .search(&request)
            .await
            .expect("search should succeed even without FTS index");
        assert!(!results.results.is_empty());
    }

    #[tokio::test]
    async fn test_trace_output() {
        let (store, _dir) = setup().await;

        let m1 = make_memory("t-001", "trace test content", MemoryType::Insight);
        store
            .create(&m1, Some(&make_vector(0, 1.0)))
            .await
            .expect("create");

        let pipeline = RetrievalPipeline::new(store)
            .with_min_score(0.0)
            .with_hard_cutoff(0.0);

        let request = SearchRequest {
            query: "trace".to_string(),
            query_vector: Some(make_vector(0, 1.0)),
            tenant_id: "t-001".to_string(),
            scope_filter: None,
            limit: Some(10),
            min_score: Some(0.0),
            include_trace: true,
            tags_filter: None,
            source_filter: None,
            agent_id_filter: None,
        };

        let results = pipeline.search(&request).await.expect("search");
        let trace = &results.trace;

        assert_eq!(trace.stages.len(), 12);
        assert_eq!(trace.stages[0].name, "parallel_search");
        assert_eq!(trace.stages[1].name, "rrf_fusion");
        assert_eq!(trace.stages[2].name, "rrf_normalize");
        assert_eq!(trace.stages[3].name, "min_score_filter");
        assert_eq!(trace.stages[4].name, "topk_cap");
        assert_eq!(trace.stages[5].name, "cross_encoder_rerank");
        assert_eq!(trace.stages[6].name, "bm25_floor");
        assert_eq!(trace.stages[7].name, "decay_boost");
        assert_eq!(trace.stages[8].name, "importance_weight");
        assert_eq!(trace.stages[9].name, "length_normalization");
        assert_eq!(trace.stages[10].name, "hard_cutoff");
        assert_eq!(trace.stages[11].name, "mmr_diversity");

        let display = trace.to_string();
        assert!(display.contains("Retrieval Trace"));
        assert!(display.contains("parallel_search"));
        assert!(display.contains("mmr_diversity"));
    }

    #[tokio::test]
    async fn test_empty_results() {
        let (store, _dir) = setup().await;

        let pipeline = RetrievalPipeline::new(store);

        let request = SearchRequest {
            query: "nothing matches".to_string(),
            query_vector: Some(make_vector(0, 1.0)),
            tenant_id: "t-001".to_string(),
            scope_filter: None,
            limit: Some(10),
            min_score: None,
            include_trace: false,
            tags_filter: None,
            source_filter: None,
            agent_id_filter: None,
        };

        let results = pipeline
            .search(&request)
            .await
            .expect("search should not error on empty");
        assert!(results.results.is_empty());
        assert_eq!(results.trace.final_count, 0);
    }

    #[test]
    fn test_rerank_blends_scores() {
        let original_score = 0.5_f32;
        let rerank_score = 0.9_f32;
        let blended: f32 = rerank_score * 0.6 + original_score * 0.4;

        assert!((blended - 0.74_f32).abs() < 0.001_f32);

        let low_rerank = 0.1_f32;
        let blended_low = low_rerank * 0.6 + original_score * 0.4;
        assert!(blended > blended_low);
    }

    #[test]
    fn test_bm25_floor_preserved() {
        let mut entries = vec![
            FusionEntry {
                memory: make_memory("t", "JIRA-1234 ticket", MemoryType::Insight),
                rrf_score: 0.3,
                bm25_score: 0.9,
                pre_rerank_score: 0.8,
            },
            FusionEntry {
                memory: make_memory("t", "some other result", MemoryType::Insight),
                rrf_score: 0.3,
                bm25_score: 0.2,
                pre_rerank_score: 0.8,
            },
        ];

        entries[0].pre_rerank_score = 0.8;
        entries[1].pre_rerank_score = 0.8;

        let (result, _stage) = RetrievalPipeline::stage_bm25_floor(entries);

        let high_bm25 = &result[0];
        assert!(
            high_bm25.rrf_score >= 0.8 * 0.95 - f32::EPSILON,
            "high BM25 result should be floored to 95% of pre-rerank: got {}",
            high_bm25.rrf_score
        );

        let low_bm25 = &result[1];
        assert!(
            (low_bm25.rrf_score - 0.3).abs() < f32::EPSILON,
            "low BM25 result should be unchanged: got {}",
            low_bm25.rrf_score
        );
    }

    #[test]
    fn test_decay_boost_applied() {
        let engine = DecayEngine::new(DecayConfig {
            floor_peripheral: 0.0,
            ..DecayConfig::default()
        });

        let mut recent = make_memory("t", "recent memory", MemoryType::Insight);
        recent.access_count = 10;
        recent.importance = 0.8;
        recent.confidence = 0.8;

        let delta_old = chrono::TimeDelta::try_days(180).unwrap_or_default();
        let mut old = make_memory("t", "old memory", MemoryType::Insight);
        old.access_count = 1;
        old.importance = 0.3;
        old.confidence = 0.3;
        old.created_at = (chrono::Utc::now() - delta_old).to_rfc3339();
        old.last_accessed_at = None;

        let recent_boost = engine.apply_search_boost(1.0, &recent);
        let old_boost = engine.apply_search_boost(1.0, &old);

        assert!(
            recent_boost > old_boost,
            "recent memory ({recent_boost}) should get higher boost than old ({old_boost})"
        );
    }

    #[test]
    fn test_length_normalization() {
        let short_content = "short text";
        let long_content = "a ".repeat(1000);

        let short_entries = vec![make_entry(short_content, 1.0)];
        let (short_result, _) = RetrievalPipeline::stage_length_normalization(short_entries);
        let short_score = short_result[0].rrf_score;

        let long_entries = vec![make_entry(&long_content, 1.0)];
        let (long_result, _) = RetrievalPipeline::stage_length_normalization(long_entries);
        let long_score = long_result[0].rrf_score;

        assert!(
            short_score > long_score,
            "short ({short_score}) should score higher than long ({long_score})"
        );

        assert!(
            (short_score - 1.0).abs() < f32::EPSILON,
            "short content should not be penalized: got {short_score}"
        );
    }

    #[test]
    fn test_mmr_diversity() {
        let entries = vec![
            make_entry(
                "the quick brown fox jumps over the lazy dog and rests in the meadow today",
                0.9,
            ),
            make_entry(
                "the quick brown fox jumps over the lazy cat and rests in the meadow today",
                0.85,
            ),
            make_entry("completely different content about rust programming", 0.8),
        ];

        let (result, stage) = RetrievalPipeline::stage_mmr_diversity(entries, 10);
        assert_eq!(result.len(), 3);
        assert_eq!(stage.name, "mmr_diversity");

        let fox_dog = result.iter().find(|e| e.memory.content.contains("dog"));
        let fox_cat = result.iter().find(|e| e.memory.content.contains("cat"));
        let rust = result.iter().find(|e| e.memory.content.contains("rust"));

        if let (Some(dog), Some(cat), Some(r)) = (fox_dog, fox_cat, rust) {
            assert!(
                dog.rrf_score > cat.rrf_score,
                "near-duplicate should be penalized: dog={}, cat={}",
                dog.rrf_score,
                cat.rrf_score
            );
            assert!(
                r.rrf_score > cat.rrf_score,
                "unique result should rank above penalized duplicate: rust={}, cat={}",
                r.rrf_score,
                cat.rrf_score
            );
        }
    }

    #[test]
    fn test_hard_cutoff() {
        let entries = vec![
            make_entry("high", 0.8),
            make_entry("mid", 0.4),
            make_entry("low", 0.2),
            make_entry("very low", 0.1),
        ];

        let (kept, stage) = RetrievalPipeline::stage_hard_cutoff(entries, 0.35);
        assert_eq!(kept.len(), 2);
        assert_eq!(stage.dropped_ids.len(), 2);
        assert!(kept.iter().all(|e| e.rrf_score >= 0.35));
    }

    #[test]
    fn test_content_jaccard() {
        let a = "the quick brown fox";
        let b = "the quick brown fox";
        assert!((content_jaccard(a, b) - 1.0).abs() < f32::EPSILON);

        let c = "completely different words here";
        assert!(content_jaccard(a, c) < 0.2);

        assert!((content_jaccard("", "") - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_rrf_normalize_multiple_results() {
        let entries = vec![
            make_entry("best", 0.033),
            make_entry("mid", 0.020),
            make_entry("worst", 0.010),
        ];

        let (result, stage) = RetrievalPipeline::stage_rrf_normalize(entries);
        assert_eq!(stage.name, "rrf_normalize");
        assert_eq!(result.len(), 3);

        let best = result.iter().find(|e| e.memory.content == "best").unwrap();
        let worst = result.iter().find(|e| e.memory.content == "worst").unwrap();
        let mid = result.iter().find(|e| e.memory.content == "mid").unwrap();

        assert!(
            (best.rrf_score - 1.0).abs() < 1e-6,
            "best should be 1.0, got {}",
            best.rrf_score
        );
        assert!(
            (worst.rrf_score - 0.0).abs() < 1e-6,
            "worst should be 0.0, got {}",
            worst.rrf_score
        );
        assert!(
            mid.rrf_score > 0.0 && mid.rrf_score < 1.0,
            "mid should be between 0 and 1, got {}",
            mid.rrf_score
        );
    }

    #[test]
    fn test_rrf_normalize_single_result() {
        let entries = vec![make_entry("only", 0.016)];

        let (result, _) = RetrievalPipeline::stage_rrf_normalize(entries);
        assert_eq!(result.len(), 1);
        let score = result[0].rrf_score;
        assert!(
            (score - 0.64).abs() < 1e-4,
            "0.016 * 40 = 0.64, got {score}"
        );
    }

    #[test]
    fn test_rrf_normalize_single_result_clamped() {
        let entries = vec![make_entry("high", 0.05)];

        let (result, _) = RetrievalPipeline::stage_rrf_normalize(entries);
        assert!(
            (result[0].rrf_score - 1.0).abs() < 1e-6,
            "should clamp to 1.0, got {}",
            result[0].rrf_score
        );
    }

    #[test]
    fn test_rrf_normalize_equal_scores() {
        let entries = vec![make_entry("a", 0.016), make_entry("b", 0.016)];

        let (result, _) = RetrievalPipeline::stage_rrf_normalize(entries);
        assert!((result[0].rrf_score - 1.0).abs() < 1e-6);
        assert!((result[1].rrf_score - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_rrf_normalize_empty() {
        let entries: Vec<FusionEntry> = vec![];
        let (result, _) = RetrievalPipeline::stage_rrf_normalize(entries);
        assert!(result.is_empty());
    }
}
