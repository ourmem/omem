use std::sync::Arc;

use crate::domain::category::Category;
use crate::domain::error::OmemError;
use crate::embed::EmbedService;
use crate::store::LanceStore;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AdmissionPreset {
    Balanced,
    Conservative,
    HighRecall,
}

impl AdmissionPreset {
    fn reject_threshold(self) -> f32 {
        match self {
            Self::Balanced => 0.45,
            Self::Conservative => 0.52,
            Self::HighRecall => 0.34,
        }
    }

    fn admit_threshold(self) -> f32 {
        match self {
            Self::Balanced => 0.60,
            Self::Conservative => 0.68,
            Self::HighRecall => 0.52,
        }
    }
}

const W_UTILITY: f32 = 0.1;
const W_CONFIDENCE: f32 = 0.1;
const W_NOVELTY: f32 = 0.1;
const W_RECENCY: f32 = 0.1;
const W_TYPE_PRIOR: f32 = 0.6;

#[derive(Debug, Clone)]
pub struct AdmissionResult {
    pub admitted: bool,
    pub score: f32,
    pub hint: String,
    pub audit: AdmissionAudit,
}

#[derive(Debug, Clone)]
pub struct AdmissionAudit {
    pub utility: f32,
    pub confidence: f32,
    pub novelty: f32,
    pub recency: f32,
    pub type_prior: f32,
    pub composite: f32,
    pub max_similarity: f32,
    pub reason: String,
}

pub struct AdmissionControl {
    preset: AdmissionPreset,
    embed: Arc<dyn EmbedService>,
    store: Arc<LanceStore>,
}

impl AdmissionControl {
    pub fn new(
        preset: AdmissionPreset,
        embed: Arc<dyn EmbedService>,
        store: Arc<LanceStore>,
    ) -> Self {
        Self {
            preset,
            embed,
            store,
        }
    }

    pub async fn evaluate(
        &self,
        text: &str,
        category: &Category,
        conversation: Option<&str>,
    ) -> Result<AdmissionResult, OmemError> {
        let utility = score_utility(text);

        let confidence = match conversation {
            Some(conv) => jaccard_similarity(text, conv),
            None => 0.5,
        };

        let embeddings = self.embed.embed(&[text.to_string()]).await?;
        let query_vec = embeddings
            .into_iter()
            .next()
            .ok_or_else(|| OmemError::Internal("embedding returned empty result".to_string()))?;

        let search_results = self
            .store
            .vector_search(&query_vec, 5, 0.0, None, None)
            .await
            .unwrap_or_default();

        let max_similarity = search_results
            .iter()
            .map(|(_, score)| *score)
            .fold(0.0_f32, f32::max);

        let novelty = 1.0 - max_similarity;
        let recency = compute_recency(&search_results);
        let type_prior = category_prior(category);

        let composite = W_UTILITY * utility
            + W_CONFIDENCE * confidence
            + W_NOVELTY * novelty
            + W_RECENCY * recency
            + W_TYPE_PRIOR * type_prior;

        let reject_th = self.preset.reject_threshold();
        let admit_th = self.preset.admit_threshold();

        let (admitted, hint) = if composite < reject_th {
            (false, "reject".to_string())
        } else if composite >= admit_th && max_similarity < 0.55 {
            (true, "add".to_string())
        } else {
            (true, "update_or_merge".to_string())
        };

        let reason = format!(
            "composite={composite:.3} (u={utility:.2} c={confidence:.2} n={novelty:.2} r={recency:.2} tp={type_prior:.2}) max_sim={max_similarity:.3} -> {hint}"
        );

        Ok(AdmissionResult {
            admitted,
            score: composite,
            hint,
            audit: AdmissionAudit {
                utility,
                confidence,
                novelty,
                recency,
                type_prior,
                composite,
                max_similarity,
                reason,
            },
        })
    }
}

fn score_utility(text: &str) -> f32 {
    let trimmed = text.trim();
    let len = trimmed.len();

    if len <= 5 {
        return 0.2;
    }
    if len <= 20 {
        return 0.3;
    }

    let mut score: f32 = 0.7;

    if trimmed.chars().any(|c| c.is_ascii_digit()) {
        score = score.max(0.85);
    }

    let words: Vec<&str> = trimmed.split_whitespace().collect();
    let has_proper_nouns = words
        .iter()
        .skip(1)
        .any(|w| w.chars().next().map(|c| c.is_uppercase()).unwrap_or(false));
    if has_proper_nouns {
        score = score.max(0.8);
    }

    if len > 60 {
        score = score.max(0.75);
    }

    score
}

fn jaccard_similarity(a: &str, b: &str) -> f32 {
    let words_a: std::collections::HashSet<&str> = a
        .split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()))
        .filter(|w| !w.is_empty())
        .collect();
    let words_b: std::collections::HashSet<&str> = b
        .split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()))
        .filter(|w| !w.is_empty())
        .collect();

    if words_a.is_empty() && words_b.is_empty() {
        return 0.0;
    }

    let intersection = words_a.intersection(&words_b).count() as f32;
    let union = words_a.union(&words_b).count() as f32;

    if union == 0.0 {
        return 0.0;
    }

    intersection / union
}

fn category_prior(cat: &Category) -> f32 {
    match cat {
        Category::Profile => 0.95,
        Category::Preferences => 0.90,
        Category::Entities => 0.75,
        Category::Events => 0.45,
        Category::Cases => 0.80,
        Category::Patterns => 0.85,
    }
}

/// `1.0 - exp(-gap_days / 30.0)` where gap_days = days since most similar memory.
fn compute_recency(results: &[(crate::domain::memory::Memory, f32)]) -> f32 {
    if results.is_empty() {
        return 1.0;
    }

    let best = results
        .iter()
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

    let Some((mem, _)) = best else {
        return 1.0;
    };

    let gap_days = chrono::Utc::now()
        .signed_duration_since(
            chrono::DateTime::parse_from_rfc3339(&mem.updated_at)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now()),
        )
        .num_days()
        .max(0) as f64;

    (1.0 - (-gap_days / 30.0_f64).exp()) as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::memory::Memory;
    use crate::domain::types::MemoryType;
    use tempfile::TempDir;

    struct MockEmbed {
        vector: Vec<f32>,
    }

    impl MockEmbed {
        fn new(vector: Vec<f32>) -> Self {
            Self { vector }
        }
    }

    #[async_trait::async_trait]
    impl EmbedService for MockEmbed {
        async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, OmemError> {
            Ok(texts.iter().map(|_| self.vector.clone()).collect())
        }
        fn dimensions(&self) -> usize {
            self.vector.len()
        }
    }

    async fn setup() -> (Arc<LanceStore>, TempDir) {
        let dir = TempDir::new().expect("temp dir");
        let store = LanceStore::new(dir.path().to_str().expect("path"))
            .await
            .expect("store");
        store.init_table().await.expect("init");
        (Arc::new(store), dir)
    }

    #[tokio::test]
    async fn test_balanced_rejects_low_quality() {
        let (store, _dir) = setup().await;
        let vec = vec![1.0; 1024];
        let embed = Arc::new(MockEmbed::new(vec.clone()));

        let existing = Memory::new(
            "some existing fact",
            Category::Events,
            MemoryType::Insight,
            "t",
        );
        store.create(&existing, Some(&vec)).await.expect("create");

        let ctrl = AdmissionControl::new(AdmissionPreset::Balanced, embed, store);

        let result = ctrl
            .evaluate("ok", &Category::Events, None)
            .await
            .expect("eval");

        assert!(!result.admitted);
        assert_eq!(result.hint, "reject");
        assert!(result.score < 0.45);
    }

    #[tokio::test]
    async fn test_balanced_admits_high_quality() {
        let (store, _dir) = setup().await;
        let embed = Arc::new(MockEmbed::new(vec![0.0; 1024]));
        let ctrl = AdmissionControl::new(AdmissionPreset::Balanced, embed, store);

        let result = ctrl
            .evaluate(
                "User is a senior backend engineer at Stripe working on payment infrastructure",
                &Category::Profile,
                Some("I work as a senior backend engineer at Stripe on payment infrastructure"),
            )
            .await
            .expect("eval");

        assert!(result.admitted);
        assert!(result.score >= 0.60);
    }

    #[tokio::test]
    async fn test_type_prior_dominates() {
        let (store, _dir) = setup().await;
        let embed = Arc::new(MockEmbed::new(vec![0.0; 1024]));
        let ctrl = AdmissionControl::new(AdmissionPreset::Balanced, embed, store);

        let result = ctrl
            .evaluate("User lives in San Francisco", &Category::Profile, None)
            .await
            .expect("eval");

        assert!(result.audit.type_prior >= 0.94);
        assert!(result.admitted);
    }

    #[tokio::test]
    async fn test_events_low_prior() {
        let (store, _dir) = setup().await;
        let embed = Arc::new(MockEmbed::new(vec![0.0; 1024]));
        let ctrl = AdmissionControl::new(AdmissionPreset::Balanced, embed, store);

        let result = ctrl
            .evaluate("something happened", &Category::Events, None)
            .await
            .expect("eval");

        assert!(result.audit.type_prior < 0.50);
    }

    #[tokio::test]
    async fn test_conservative_stricter() {
        let (store, _dir) = setup().await;
        let embed = Arc::new(MockEmbed::new(vec![0.0; 1024]));

        let text = "A deployment event happened";
        let cat = Category::Events;

        let balanced =
            AdmissionControl::new(AdmissionPreset::Balanced, embed.clone(), store.clone());
        let conservative = AdmissionControl::new(AdmissionPreset::Conservative, embed, store);

        let r_balanced = balanced.evaluate(text, &cat, None).await.expect("eval");
        let r_conservative = conservative.evaluate(text, &cat, None).await.expect("eval");

        assert!(
            AdmissionPreset::Conservative.reject_threshold()
                > AdmissionPreset::Balanced.reject_threshold()
        );
        assert!(
            AdmissionPreset::Conservative.admit_threshold()
                > AdmissionPreset::Balanced.admit_threshold()
        );
        assert!((r_balanced.score - r_conservative.score).abs() < f32::EPSILON);
    }

    #[tokio::test]
    async fn test_audit_record_complete() {
        let (store, _dir) = setup().await;
        let embed = Arc::new(MockEmbed::new(vec![0.0; 1024]));
        let ctrl = AdmissionControl::new(AdmissionPreset::Balanced, embed, store);

        let result = ctrl
            .evaluate(
                "User prefers dark mode in all IDEs",
                &Category::Preferences,
                Some("I always use dark mode"),
            )
            .await
            .expect("eval");

        let a = &result.audit;
        assert!(a.utility >= 0.0 && a.utility <= 1.0);
        assert!(a.confidence >= 0.0 && a.confidence <= 1.0);
        assert!(a.novelty >= 0.0 && a.novelty <= 1.0);
        assert!(a.recency >= 0.0 && a.recency <= 1.0);
        assert!(a.type_prior >= 0.0 && a.type_prior <= 1.0);
        assert!(a.composite >= 0.0);
        assert!(a.max_similarity >= 0.0);
        assert!(!a.reason.is_empty());
    }

    #[test]
    fn test_score_utility_short() {
        assert!((score_utility("hi") - 0.2).abs() < f32::EPSILON);
        assert!((score_utility("ok") - 0.2).abs() < f32::EPSILON);
    }

    #[test]
    fn test_score_utility_medium() {
        assert!((score_utility("hello world test") - 0.3).abs() < f32::EPSILON);
    }

    #[test]
    fn test_score_utility_with_numbers() {
        let s = score_utility("User deployed v2.0 to production last Friday");
        assert!(s >= 0.85);
    }

    #[test]
    fn test_score_utility_with_proper_nouns() {
        let s = score_utility("User works at Stripe on payments");
        assert!(s >= 0.8);
    }

    #[test]
    fn test_jaccard_identical() {
        let s = jaccard_similarity("hello world", "hello world");
        assert!((s - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_jaccard_disjoint() {
        let s = jaccard_similarity("hello world", "foo bar");
        assert!((s - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_jaccard_partial() {
        let s = jaccard_similarity("hello world foo", "hello world bar");
        assert!((s - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn test_category_prior_values() {
        assert!((category_prior(&Category::Profile) - 0.95).abs() < f32::EPSILON);
        assert!((category_prior(&Category::Preferences) - 0.90).abs() < f32::EPSILON);
        assert!((category_prior(&Category::Entities) - 0.75).abs() < f32::EPSILON);
        assert!((category_prior(&Category::Events) - 0.45).abs() < f32::EPSILON);
        assert!((category_prior(&Category::Cases) - 0.80).abs() < f32::EPSILON);
        assert!((category_prior(&Category::Patterns) - 0.85).abs() < f32::EPSILON);
    }

    #[test]
    fn test_compute_recency_empty() {
        let r = compute_recency(&[]);
        assert!((r - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_compute_recency_recent() {
        let mut mem = Memory::new("test", Category::Profile, MemoryType::Insight, "t");
        mem.updated_at = chrono::Utc::now().to_rfc3339();
        let r = compute_recency(&[(mem, 0.9)]);
        assert!(r < 0.05);
    }
}
