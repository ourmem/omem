use std::sync::Arc;

use regex::Regex;

use crate::domain::error::OmemError;
use crate::ingest::prompts;
use crate::ingest::types::{ExtractionResult, ExtractedFact, IngestMessage};
use crate::llm::{complete_json, LlmService};

const DEFAULT_MAX_FACTS: usize = 50;
const DEFAULT_MAX_INPUT_CHARS: usize = 8000;
const VALID_CATEGORIES: &[&str] = &[
    "profile",
    "preferences",
    "entities",
    "events",
    "cases",
    "patterns",
];

pub struct FactExtractor {
    llm: Arc<dyn LlmService>,
    max_facts: usize,
    pub(crate) max_input_chars: usize,
}

impl FactExtractor {
    pub fn new(llm: Arc<dyn LlmService>) -> Self {
        Self {
            llm,
            max_facts: DEFAULT_MAX_FACTS,
            max_input_chars: DEFAULT_MAX_INPUT_CHARS,
        }
    }

    pub async fn extract(
        &self,
        messages: &[IngestMessage],
        entity_context: Option<&str>,
    ) -> Result<Vec<ExtractedFact>, OmemError> {
        if messages.is_empty() {
            return Ok(Vec::new());
        }

        let conversation_text = self.format_messages(messages);
        let cleaned = strip_envelope_metadata(&conversation_text);

        if cleaned.trim().is_empty() {
            return Ok(Vec::new());
        }

        let system = prompts::build_system_prompt(entity_context);
        let user = prompts::build_user_prompt(&cleaned);

        let result: ExtractionResult = complete_json(self.llm.as_ref(), &system, &user).await?;

        let facts = result
            .memories
            .into_iter()
            .filter(|f| !f.l0_abstract.trim().is_empty())
            .map(|mut f| {
                f.category = normalize_category(&f.category);
                f
            })
            .take(self.max_facts)
            .collect();

        Ok(facts)
    }

    /// Extract using custom system/user prompts (for section/document modes).
    pub async fn extract_with_prompts(
        &self,
        system: &str,
        user: &str,
    ) -> Result<Vec<ExtractedFact>, OmemError> {
        let result: ExtractionResult = complete_json(self.llm.as_ref(), system, user).await?;

        let facts = result
            .memories
            .into_iter()
            .filter(|f| !f.l0_abstract.trim().is_empty())
            .map(|mut f| {
                f.category = normalize_category(&f.category);
                f
            })
            .take(self.max_facts)
            .collect();

        Ok(facts)
    }

    fn format_messages(&self, messages: &[IngestMessage]) -> String {
        let mut full_text = String::new();
        for msg in messages {
            full_text.push_str(&msg.role);
            full_text.push_str(": ");
            full_text.push_str(&msg.content);
            full_text.push('\n');
        }

        if full_text.len() > self.max_input_chars {
            let start = full_text.len() - self.max_input_chars;
            let boundary = full_text[start..]
                .find('\n')
                .map(|i| start + i + 1)
                .unwrap_or(start);
            let boundary = if boundary >= full_text.len() {
                start
            } else {
                boundary
            };
            return full_text[boundary..].to_string();
        }

        full_text
    }
}

fn normalize_category(raw: &str) -> String {
    let lower = raw.trim().to_lowercase();
    if VALID_CATEGORIES.contains(&lower.as_str()) {
        lower
    } else {
        "profile".to_string()
    }
}

/// Strips OpenClaw channel-injected platform metadata from conversation text.
/// Patterns removed:
///   - "System: [timestamp] Channel..." lines
///   - "Conversation info (untrusted metadata):" + JSON blocks
///   - "Sender (untrusted metadata):" + JSON blocks
pub fn strip_envelope_metadata(text: &str) -> String {
    let system_channel =
        Regex::new(r"(?m)^(?:\w+:\s*)?System:\s*\[.*?\]\s*Channel.*$")
            .expect("valid regex: system_channel");
    let result = system_channel.replace_all(text, "");

    let conv_info = Regex::new(
        r"(?ms)Conversation info \(untrusted metadata\):\s*\{.*?\}",
    )
    .expect("valid regex: conv_info");
    let result = conv_info.replace_all(&result, "");

    let sender_info =
        Regex::new(r"(?ms)Sender \(untrusted metadata\):\s*\{.*?\}")
            .expect("valid regex: sender_info");
    let result = sender_info.replace_all(&result, "");

    result.into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct MockLlm {
        response: Mutex<String>,
        captured_system: Mutex<Option<String>>,
        captured_user: Mutex<Option<String>>,
    }

    impl MockLlm {
        fn new(json_response: &str) -> Self {
            Self {
                response: Mutex::new(json_response.to_string()),
                captured_system: Mutex::new(None),
                captured_user: Mutex::new(None),
            }
        }

        fn captured_system(&self) -> Option<String> {
            self.captured_system.lock().expect("lock").clone()
        }

        fn captured_user(&self) -> Option<String> {
            self.captured_user.lock().expect("lock").clone()
        }
    }

    #[async_trait::async_trait]
    impl LlmService for MockLlm {
        async fn complete_text(&self, system: &str, user: &str) -> Result<String, OmemError> {
            *self.captured_system.lock().expect("lock") = Some(system.to_string());
            *self.captured_user.lock().expect("lock") = Some(user.to_string());
            Ok(self.response.lock().expect("lock").clone())
        }
    }

    fn msg(role: &str, content: &str) -> IngestMessage {
        IngestMessage {
            role: role.to_string(),
            content: content.to_string(),
        }
    }

    #[tokio::test]
    async fn extract_profile_fact() {
        let json = r#"{"memories":[{"l0_abstract":"User is a backend engineer at Stripe","l1_overview":"**Role**: Backend Engineer\n**Company**: Stripe","l2_content":"The user works at Stripe as a backend engineer.","category":"profile","tags":["career"]}]}"#;
        let llm = Arc::new(MockLlm::new(json));
        let extractor = FactExtractor::new(llm);

        let messages = vec![
            msg("user", "I'm a backend engineer at Stripe"),
            msg("assistant", "That's great!"),
        ];
        let facts = extractor.extract(&messages, None).await.expect("extract");

        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].category, "profile");
        assert!(facts[0].l0_abstract.contains("Stripe"));
    }

    #[tokio::test]
    async fn extract_preference_fact() {
        let json = r#"{"memories":[{"l0_abstract":"User prefers Rust over C++","l1_overview":"Prefers Rust for safety","l2_content":"User prefers Rust for systems programming.","category":"preferences","tags":["rust"]}]}"#;
        let llm = Arc::new(MockLlm::new(json));
        let extractor = FactExtractor::new(llm);

        let messages = vec![msg("user", "I prefer Rust over C++ for safety reasons")];
        let facts = extractor.extract(&messages, None).await.expect("extract");

        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].category, "preferences");
    }

    #[tokio::test]
    async fn extract_event_fact() {
        let json = r#"{"memories":[{"l0_abstract":"User deployed v2.0 to production last Friday","l1_overview":"**Event**: Production deployment\n**Version**: v2.0","l2_content":"User deployed v2.0 to production last Friday.","category":"events","tags":["deployment"]}]}"#;
        let llm = Arc::new(MockLlm::new(json));
        let extractor = FactExtractor::new(llm);

        let messages = vec![msg("user", "We deployed v2.0 to production last Friday")];
        let facts = extractor.extract(&messages, None).await.expect("extract");

        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].category, "events");
    }

    #[tokio::test]
    async fn extract_case_fact() {
        let json = r#"{"memories":[{"l0_abstract":"Docker COPY failure fixed by updating .dockerignore","l1_overview":"**Problem**: COPY fails\n**Solution**: Update .dockerignore","l2_content":"Docker builds failing because COPY step couldn't find file. Fixed by adding .dockerignore exception.","category":"cases","tags":["docker"]}]}"#;
        let llm = Arc::new(MockLlm::new(json));
        let extractor = FactExtractor::new(llm);

        let messages = vec![msg("user", "My Docker builds were failing because COPY couldn't find the file. I fixed it by updating .dockerignore.")];
        let facts = extractor.extract(&messages, None).await.expect("extract");

        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].category, "cases");
    }

    #[tokio::test]
    async fn empty_conversation_returns_empty() {
        let llm = Arc::new(MockLlm::new(r#"{"memories":[]}"#));
        let extractor = FactExtractor::new(llm);

        let facts = extractor.extract(&[], None).await.expect("extract");
        assert!(facts.is_empty());
    }

    #[tokio::test]
    async fn long_conversation_truncated() {
        let json = r#"{"memories":[{"l0_abstract":"Some fact","l1_overview":"Overview","l2_content":"Content","category":"profile","tags":[]}]}"#;
        let llm = Arc::new(MockLlm::new(json));
        let mut extractor = FactExtractor::new(llm.clone());
        extractor.max_input_chars = 100;

        let long_msg = "a".repeat(200);
        let messages = vec![msg("user", &long_msg)];
        let facts = extractor.extract(&messages, None).await.expect("extract");

        let captured = llm.captured_user().expect("captured");
        let conversation_part = captured.strip_prefix("Extract all distinct, atomic facts from the following conversation:\n\n").expect("prefix");
        assert!(conversation_part.len() <= 100);
        assert_eq!(facts.len(), 1);
    }

    #[tokio::test]
    async fn json_with_markdown_fences_parses() {
        let json = "```json\n{\"memories\":[{\"l0_abstract\":\"Fact\",\"l1_overview\":\"O\",\"l2_content\":\"C\",\"category\":\"profile\",\"tags\":[]}]}\n```";
        let llm = Arc::new(MockLlm::new(json));
        let extractor = FactExtractor::new(llm);

        let messages = vec![msg("user", "test input")];
        let facts = extractor.extract(&messages, None).await.expect("extract");
        assert_eq!(facts.len(), 1);
    }

    #[tokio::test]
    async fn entity_context_appended_to_prompt() {
        let json = r#"{"memories":[]}"#;
        let llm = Arc::new(MockLlm::new(json));
        let extractor = FactExtractor::new(llm.clone());

        let messages = vec![msg("user", "hello")];
        let ctx = "Focus on extracting project-related facts";
        extractor
            .extract(&messages, Some(ctx))
            .await
            .expect("extract");

        let system = llm.captured_system().expect("captured");
        assert!(system.contains("Additional Context"));
        assert!(system.contains(ctx));
    }

    #[tokio::test]
    async fn more_than_50_facts_truncated() {
        let mut memories = Vec::new();
        for i in 0..60 {
            memories.push(format!(
                r#"{{"l0_abstract":"Fact {i}","l1_overview":"O","l2_content":"C","category":"profile","tags":[]}}"#
            ));
        }
        let json = format!(r#"{{"memories":[{}]}}"#, memories.join(","));
        let llm = Arc::new(MockLlm::new(&json));
        let extractor = FactExtractor::new(llm);

        let messages = vec![msg("user", "lots of info")];
        let facts = extractor.extract(&messages, None).await.expect("extract");
        assert_eq!(facts.len(), 50);
    }

    #[tokio::test]
    async fn envelope_metadata_stripped() {
        let json = r#"{"memories":[{"l0_abstract":"User likes cats","l1_overview":"O","l2_content":"C","category":"preferences","tags":[]}]}"#;
        let llm = Arc::new(MockLlm::new(json));
        let extractor = FactExtractor::new(llm.clone());

        let messages = vec![
            msg("system", "System: [2024-01-01T00:00:00Z] Channel #general"),
            msg("user", "Conversation info (untrusted metadata):\n{\"platform\": \"slack\"}\nI love cats"),
        ];
        let facts = extractor.extract(&messages, None).await.expect("extract");

        let captured = llm.captured_user().expect("captured");
        assert!(!captured.contains("untrusted metadata"));
        assert!(!captured.contains("Channel #general"));
        assert!(captured.contains("I love cats"));
        assert_eq!(facts.len(), 1);
    }

    #[tokio::test]
    async fn empty_l0_abstract_filtered_out() {
        let json = r#"{"memories":[{"l0_abstract":"Valid fact","l1_overview":"O","l2_content":"C","category":"profile","tags":[]},{"l0_abstract":"","l1_overview":"O","l2_content":"C","category":"profile","tags":[]},{"l0_abstract":"  ","l1_overview":"O","l2_content":"C","category":"profile","tags":[]}]}"#;
        let llm = Arc::new(MockLlm::new(json));
        let extractor = FactExtractor::new(llm);

        let messages = vec![msg("user", "some input")];
        let facts = extractor.extract(&messages, None).await.expect("extract");
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].l0_abstract, "Valid fact");
    }

    #[tokio::test]
    async fn invalid_category_normalized_to_profile() {
        let json = r#"{"memories":[{"l0_abstract":"Fact","l1_overview":"O","l2_content":"C","category":"UNKNOWN_CAT","tags":[]}]}"#;
        let llm = Arc::new(MockLlm::new(json));
        let extractor = FactExtractor::new(llm);

        let messages = vec![msg("user", "test")];
        let facts = extractor.extract(&messages, None).await.expect("extract");
        assert_eq!(facts[0].category, "profile");
    }

    #[tokio::test]
    async fn category_case_insensitive_normalization() {
        let json = r#"{"memories":[{"l0_abstract":"Fact","l1_overview":"O","l2_content":"C","category":"PREFERENCES","tags":[]}]}"#;
        let llm = Arc::new(MockLlm::new(json));
        let extractor = FactExtractor::new(llm);

        let messages = vec![msg("user", "test")];
        let facts = extractor.extract(&messages, None).await.expect("extract");
        assert_eq!(facts[0].category, "preferences");
    }

    #[test]
    fn strip_envelope_system_channel_line() {
        let input = "System: [2024-01-01T00:00:00Z] Channel #general\nuser: hello";
        let result = strip_envelope_metadata(input);
        assert!(!result.contains("Channel #general"));
        assert!(result.contains("user: hello"));
    }

    #[test]
    fn strip_envelope_conversation_info_block() {
        let input = "Conversation info (untrusted metadata):\n{\"platform\": \"slack\", \"channel\": \"#dev\"}\nuser: hello";
        let result = strip_envelope_metadata(input);
        assert!(!result.contains("untrusted metadata"));
        assert!(!result.contains("slack"));
        assert!(result.contains("user: hello"));
    }

    #[test]
    fn strip_envelope_sender_info_block() {
        let input = "Sender (untrusted metadata):\n{\"name\": \"John\"}\nuser: hello";
        let result = strip_envelope_metadata(input);
        assert!(!result.contains("untrusted metadata"));
        assert!(!result.contains("John"));
        assert!(result.contains("user: hello"));
    }

    #[test]
    fn strip_envelope_preserves_clean_text() {
        let input = "user: I like Rust\nassistant: Great choice!";
        let result = strip_envelope_metadata(input);
        assert_eq!(result, input);
    }
}
