use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ExtractedFact {
    pub l0_abstract: String,
    pub l1_overview: String,
    pub l2_content: String,
    pub category: String,
    pub tags: Vec<String>,
    #[serde(skip)]
    pub source_text: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ExtractionResult {
    pub memories: Vec<ExtractedFact>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ReconcileDecision {
    pub action: String,
    pub fact_index: usize,
    #[serde(default)]
    pub match_index: Option<usize>,
    #[serde(default)]
    pub merged_content: Option<String>,
    #[serde(default)]
    pub context_label: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ReconcileResult {
    pub decisions: Vec<ReconcileDecision>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct BatchDedupResult {
    pub keep_indices: Vec<usize>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct IngestMessage {
    pub role: String,
    pub content: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct IngestRequest {
    pub messages: Vec<IngestMessage>,
    pub tenant_id: String,
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub entity_context: Option<String>,
    #[serde(default)]
    pub mode: IngestMode,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum IngestMode {
    #[default]
    Smart,
    Raw,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct IngestResponse {
    pub task_id: String,
    pub stored_count: usize,
}
