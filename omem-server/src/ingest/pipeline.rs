use std::sync::Arc;

use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::domain::category::Category;
use crate::domain::error::OmemError;
use crate::embed::EmbedService;
use crate::ingest::admission::{AdmissionControl, AdmissionPreset};
use crate::ingest::extractor::FactExtractor;
use crate::ingest::noise::NoiseFilter;
use crate::ingest::privacy::{is_fully_private, strip_private_content};
use crate::ingest::reconciler::Reconciler;
use crate::ingest::session::{SessionMessage, SessionStore};
use crate::ingest::types::{IngestMessage, IngestMode, IngestRequest, IngestResponse};
use crate::llm::LlmService;
use crate::store::LanceStore;

const BYTE_BUDGET: usize = 200_000;
const MESSAGE_BUDGET: usize = 20;

pub struct IngestPipeline {
    extractor: Arc<FactExtractor>,
    reconciler: Arc<Reconciler>,
    session_store: Arc<SessionStore>,
    noise_filter: Arc<tokio::sync::Mutex<NoiseFilter>>,
    admission: Arc<AdmissionControl>,
    embed: Arc<dyn EmbedService>,
}

impl IngestPipeline {
    pub fn new(
        store: Arc<LanceStore>,
        session_store: Arc<SessionStore>,
        embed: Arc<dyn EmbedService>,
        llm: Arc<dyn LlmService>,
    ) -> Self {
        let extractor = Arc::new(FactExtractor::new(llm.clone()));
        let reconciler = Arc::new(Reconciler::new(llm.clone(), store.clone(), embed.clone()));
        let noise_filter = Arc::new(tokio::sync::Mutex::new(NoiseFilter::new(Vec::new())));
        let admission = Arc::new(AdmissionControl::new(
            AdmissionPreset::Balanced,
            embed.clone(),
            store,
        ));
        Self {
            extractor,
            reconciler,
            session_store,
            noise_filter,
            admission,
            embed,
        }
    }

    pub async fn ingest(&self, request: IngestRequest) -> Result<IngestResponse, OmemError> {
        if request.messages.is_empty() {
            return Err(OmemError::Validation("no messages provided".to_string()));
        }

        let task_id = Uuid::new_v4().to_string();
        let session_id = request
            .session_id
            .clone()
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let agent_id = request.agent_id.clone().unwrap_or_default();

        let session_messages: Vec<SessionMessage> = request
            .messages
            .iter()
            .map(|m| SessionMessage::new(&session_id, &agent_id, &m.role, &m.content, vec![]))
            .collect();

        let stored_count = self.session_store.bulk_create(&session_messages).await?;

        if matches!(request.mode, IngestMode::Raw) {
            return Ok(IngestResponse {
                task_id,
                stored_count,
            });
        }

        let selected = select_messages(&request.messages);
        let extractor = self.extractor.clone();
        let reconciler = self.reconciler.clone();
        let noise_filter = self.noise_filter.clone();
        let admission = self.admission.clone();
        let embed = self.embed.clone();
        let entity_context = request.entity_context.clone();
        let tenant_id = request.tenant_id.clone();
        let bg_task_id = task_id.clone();

        tokio::spawn(async move {
            info!(task_id = %bg_task_id, message_count = selected.len(), "slow path: starting extraction");

            let sanitized: Vec<IngestMessage> = selected
                .iter()
                .map(|m| IngestMessage {
                    role: m.role.clone(),
                    content: strip_private_content(&m.content),
                })
                .filter(|m| !is_fully_private(&m.content))
                .collect();

            if sanitized.is_empty() {
                info!(task_id = %bg_task_id, "all messages fully private, skipping");
                return;
            }

            let facts = match extractor
                .extract(&sanitized, entity_context.as_deref())
                .await
            {
                Ok(f) => f,
                Err(e) => {
                    error!(error = %e, task_id = %bg_task_id, "extraction failed — raw mode fallback");
                    return;
                }
            };

            if facts.is_empty() {
                info!(task_id = %bg_task_id, "no facts extracted");
                return;
            }

            let mut noise_guard = noise_filter.lock().await;
            let mut clean_facts = Vec::new();
            for fact in &facts {
                let fact_vector = match embed.embed(std::slice::from_ref(&fact.l0_abstract)).await {
                    Ok(vecs) => vecs.into_iter().next(),
                    Err(_) => None,
                };
                if noise_guard.is_noise(&fact.l0_abstract, fact_vector.as_deref()) {
                    debug!(fact = %fact.l0_abstract, "filtered as noise");
                    if let Some(vec) = fact_vector {
                        noise_guard.learn_noise(vec);
                    }
                    continue;
                }
                clean_facts.push(fact.clone());
            }
            drop(noise_guard);

            if clean_facts.is_empty() {
                info!(task_id = %bg_task_id, "all facts filtered as noise");
                return;
            }

            let conversation_text: String = sanitized
                .iter()
                .map(|m| format!("{}: {}", m.role, m.content))
                .collect::<Vec<_>>()
                .join("\n");

            let mut admitted_facts = Vec::new();
            for fact in &clean_facts {
                let category = fact
                    .category
                    .parse::<Category>()
                    .unwrap_or(Category::Events);

                let result = admission
                    .evaluate(&fact.l0_abstract, &category, Some(&conversation_text))
                    .await;

                match result {
                    Ok(ar) if ar.admitted => {
                        debug!(
                            fact = %fact.l0_abstract,
                            score = ar.score,
                            hint = %ar.hint,
                            "admitted"
                        );
                        admitted_facts.push(fact.clone());
                    }
                    Ok(ar) => {
                        debug!(
                            fact = %fact.l0_abstract,
                            score = ar.score,
                            "rejected by admission"
                        );
                    }
                    Err(e) => {
                        warn!(error = %e, "admission error, admitting by default");
                        admitted_facts.push(fact.clone());
                    }
                }
            }

            if admitted_facts.is_empty() {
                info!(task_id = %bg_task_id, "all facts rejected by admission control");
                return;
            }

            match reconciler.reconcile(&admitted_facts, &tenant_id).await {
                Ok(memories) => {
                    info!(
                        task_id = %bg_task_id,
                        fact_count = admitted_facts.len(),
                        memory_count = memories.len(),
                        "reconciliation complete"
                    );
                }
                Err(e) => {
                    error!(error = %e, task_id = %bg_task_id, "reconciliation failed");
                }
            }
        });

        Ok(IngestResponse {
            task_id,
            stored_count,
        })
    }
}

fn select_messages(messages: &[IngestMessage]) -> Vec<IngestMessage> {
    let mut selected = Vec::new();
    let mut total_bytes = 0;

    for msg in messages.iter().rev() {
        if selected.len() >= MESSAGE_BUDGET {
            break;
        }
        let msg_bytes = msg.content.len();
        if total_bytes + msg_bytes > BYTE_BUDGET && !selected.is_empty() {
            break;
        }
        total_bytes += msg_bytes;
        selected.push(msg.clone());
    }

    selected.reverse();
    selected
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingest::session::SessionStore;
    use crate::store::LanceStore;
    use std::sync::Mutex;
    use tempfile::TempDir;

    struct MockEmbed;

    #[async_trait::async_trait]
    impl EmbedService for MockEmbed {
        async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, OmemError> {
            Ok(texts.iter().map(|_| vec![0.0; 1024]).collect())
        }
        fn dimensions(&self) -> usize {
            1024
        }
    }

    struct TrackingLlm {
        call_count: Mutex<u32>,
        response: String,
    }

    impl TrackingLlm {
        fn new(response: &str) -> Self {
            Self {
                call_count: Mutex::new(0),
                response: response.to_string(),
            }
        }

        fn calls(&self) -> u32 {
            *self.call_count.lock().expect("lock")
        }
    }

    #[async_trait::async_trait]
    impl LlmService for TrackingLlm {
        async fn complete_text(&self, _system: &str, _user: &str) -> Result<String, OmemError> {
            *self.call_count.lock().expect("lock") += 1;
            Ok(self.response.clone())
        }
    }

    struct FailingLlm;

    #[async_trait::async_trait]
    impl LlmService for FailingLlm {
        async fn complete_text(&self, _system: &str, _user: &str) -> Result<String, OmemError> {
            Err(OmemError::Llm("service unavailable".to_string()))
        }
    }

    async fn setup_pipeline(
        llm: Arc<dyn LlmService>,
    ) -> (IngestPipeline, Arc<SessionStore>, TempDir) {
        let dir = TempDir::new().expect("temp dir");
        let path = dir.path().to_str().expect("path");

        let store = Arc::new(LanceStore::new(path).await.expect("store"));
        store.init_table().await.expect("init memories");

        let session_store = Arc::new(SessionStore::new(path).await.expect("session store"));
        session_store.init_table().await.expect("init sessions");

        let embed: Arc<dyn EmbedService> = Arc::new(MockEmbed);
        let pipeline = IngestPipeline::new(store, session_store.clone(), embed, llm);

        (pipeline, session_store, dir)
    }

    fn make_request(messages: Vec<(&str, &str)>, mode: IngestMode) -> IngestRequest {
        IngestRequest {
            messages: messages
                .into_iter()
                .map(|(role, content)| IngestMessage {
                    role: role.to_string(),
                    content: content.to_string(),
                })
                .collect(),
            tenant_id: "t-001".to_string(),
            agent_id: Some("agent-1".to_string()),
            session_id: Some("sess-test".to_string()),
            entity_context: None,
            mode,
        }
    }

    #[tokio::test]
    async fn test_fast_path_stores_sessions() {
        let llm = Arc::new(TrackingLlm::new(r#"{"memories":[]}"#));
        let (pipeline, session_store, _dir) = setup_pipeline(llm).await;

        let request = make_request(
            vec![
                ("user", "I prefer dark mode"),
                ("assistant", "Noted!"),
                ("user", "Also use Rust"),
            ],
            IngestMode::Smart,
        );

        let response = pipeline.ingest(request).await.expect("ingest");
        assert_eq!(response.stored_count, 3);
        assert!(!response.task_id.is_empty());

        let count = session_store
            .count_by_session("sess-test")
            .await
            .expect("count");
        assert_eq!(count, 3);
    }

    #[tokio::test]
    async fn test_content_hash_dedup_via_pipeline() {
        let llm = Arc::new(TrackingLlm::new(r#"{"memories":[]}"#));
        let (pipeline, session_store, _dir) = setup_pipeline(llm).await;

        let request = make_request(
            vec![("user", "hello"), ("assistant", "hi")],
            IngestMode::Raw,
        );

        let r1 = pipeline.ingest(request).await.expect("first ingest");
        assert_eq!(r1.stored_count, 2);

        let dup_request = make_request(
            vec![("user", "hello"), ("assistant", "hi")],
            IngestMode::Raw,
        );

        let r2 = pipeline.ingest(dup_request).await.expect("second ingest");
        assert_eq!(r2.stored_count, 0);

        let count = session_store
            .count_by_session("sess-test")
            .await
            .expect("count");
        assert_eq!(count, 2);
    }

    #[test]
    fn test_message_budget() {
        let messages: Vec<IngestMessage> = (0..25)
            .map(|i| IngestMessage {
                role: "user".to_string(),
                content: format!("message {i}"),
            })
            .collect();

        let selected = select_messages(&messages);
        assert_eq!(selected.len(), MESSAGE_BUDGET);

        assert_eq!(selected[0].content, "message 5");
        assert_eq!(selected[MESSAGE_BUDGET - 1].content, "message 24");
    }

    #[test]
    fn test_message_budget_byte_limit() {
        let big_content = "x".repeat(100_000);
        let messages: Vec<IngestMessage> = (0..5)
            .map(|_| IngestMessage {
                role: "user".to_string(),
                content: big_content.clone(),
            })
            .collect();

        let selected = select_messages(&messages);
        assert_eq!(selected.len(), 2);
    }

    #[test]
    fn test_message_budget_always_includes_one() {
        let huge = "x".repeat(500_000);
        let messages = vec![IngestMessage {
            role: "user".to_string(),
            content: huge,
        }];

        let selected = select_messages(&messages);
        assert_eq!(selected.len(), 1);
    }

    #[test]
    fn test_message_budget_empty() {
        let selected = select_messages(&[]);
        assert!(selected.is_empty());
    }

    #[test]
    fn test_message_budget_preserves_order() {
        let messages: Vec<IngestMessage> = (0..5)
            .map(|i| IngestMessage {
                role: "user".to_string(),
                content: format!("msg-{i}"),
            })
            .collect();

        let selected = select_messages(&messages);
        assert_eq!(selected.len(), 5);
        for (i, msg) in selected.iter().enumerate() {
            assert_eq!(msg.content, format!("msg-{i}"));
        }
    }

    #[tokio::test]
    async fn test_raw_mode() {
        let llm = Arc::new(TrackingLlm::new(r#"{"memories":[]}"#));
        let llm_ref = llm.clone();
        let (pipeline, session_store, _dir) = setup_pipeline(llm as Arc<dyn LlmService>).await;

        let request = make_request(
            vec![("user", "remember this"), ("assistant", "ok")],
            IngestMode::Raw,
        );

        let response = pipeline.ingest(request).await.expect("ingest");
        assert_eq!(response.stored_count, 2);

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        assert_eq!(llm_ref.calls(), 0, "LLM should not be called in raw mode");

        let count = session_store
            .count_by_session("sess-test")
            .await
            .expect("count");
        assert_eq!(count, 2);
    }

    #[tokio::test]
    async fn test_graceful_degradation() {
        let llm: Arc<dyn LlmService> = Arc::new(FailingLlm);
        let (pipeline, session_store, _dir) = setup_pipeline(llm).await;

        let request = make_request(
            vec![("user", "important data"), ("assistant", "received")],
            IngestMode::Smart,
        );

        let response = pipeline
            .ingest(request)
            .await
            .expect("ingest should succeed despite LLM failure");
        assert_eq!(response.stored_count, 2);

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let count = session_store
            .count_by_session("sess-test")
            .await
            .expect("count");
        assert_eq!(count, 2);
    }

    #[tokio::test]
    async fn test_empty_messages_rejected() {
        let llm: Arc<dyn LlmService> = Arc::new(TrackingLlm::new(r#"{"memories":[]}"#));
        let (pipeline, _session_store, _dir) = setup_pipeline(llm).await;

        let request = IngestRequest {
            messages: vec![],
            tenant_id: "t-001".to_string(),
            agent_id: None,
            session_id: None,
            entity_context: None,
            mode: IngestMode::Smart,
        };

        let result = pipeline.ingest(request).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_auto_generated_session_id() {
        let llm: Arc<dyn LlmService> = Arc::new(TrackingLlm::new(r#"{"memories":[]}"#));
        let (pipeline, _session_store, _dir) = setup_pipeline(llm).await;

        let request = IngestRequest {
            messages: vec![IngestMessage {
                role: "user".to_string(),
                content: "test".to_string(),
            }],
            tenant_id: "t-001".to_string(),
            agent_id: None,
            session_id: None,
            entity_context: None,
            mode: IngestMode::Raw,
        };

        let response = pipeline.ingest(request).await.expect("ingest");
        assert_eq!(response.stored_count, 1);
    }
}
