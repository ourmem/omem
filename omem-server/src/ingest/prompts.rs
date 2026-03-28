pub fn build_system_prompt(entity_context: Option<&str>) -> String {
    let mut prompt = BASE_SYSTEM_PROMPT.to_string();
    if let Some(ctx) = entity_context {
        let truncated = if ctx.len() > 1500 { &ctx[..1500] } else { ctx };
        prompt.push_str("\n\n## Additional Context\n");
        prompt.push_str(truncated);
    }
    prompt
}

pub fn build_user_prompt(conversation_text: &str) -> String {
    format!(
        "Extract all distinct, atomic facts from the following conversation:\n\n{conversation_text}"
    )
}

use crate::domain::memory::Memory;
use crate::ingest::types::ExtractedFact;

struct ExistingMemoryEntry<'a> {
    int_id: usize,
    memory: &'a Memory,
    age_label: String,
}

/// Returns (system_prompt, user_prompt).
pub fn build_reconcile_prompt(
    facts: &[ExtractedFact],
    existing: &[Memory],
    id_map: &[(usize, &str)], // (int_id -> real uuid)
) -> (String, String) {
    let system = RECONCILE_SYSTEM_PROMPT.to_string();

    let mut user = String::with_capacity(2048);

    user.push_str("## New Facts\n");
    for (i, fact) in facts.iter().enumerate() {
        user.push_str(&format!(
            "[{}] (category: {}) {}\n",
            i, fact.category, fact.l0_abstract
        ));
    }

    if existing.is_empty() {
        user.push_str("\n## Existing Memories\nNone.\n");
    } else {
        user.push_str("\n## Existing Memories\n");
        let entries: Vec<ExistingMemoryEntry> = existing
            .iter()
            .filter_map(|m| {
                id_map
                    .iter()
                    .find(|(_, uuid)| *uuid == m.id)
                    .map(|(int_id, _)| ExistingMemoryEntry {
                        int_id: *int_id,
                        memory: m,
                        age_label: format_age(&m.created_at),
                    })
            })
            .collect();

        for entry in &entries {
            user.push_str(&format!(
                "[{}] (category: {}, age: {}) {}\n",
                entry.int_id,
                entry.memory.category,
                entry.age_label,
                entry.memory.l0_abstract.as_str(),
            ));
        }
    }

    user.push_str(&format!(
        "\nReturn a JSON object with a \"decisions\" array containing exactly {} decision(s), one per fact.\n",
        facts.len()
    ));

    (system, user)
}

fn format_age(created_at: &str) -> String {
    let Ok(created) = chrono::DateTime::parse_from_rfc3339(created_at) else {
        return "unknown".to_string();
    };
    let now = chrono::Utc::now();
    let duration = now.signed_duration_since(created);

    let days = duration.num_days();
    if days < 1 {
        return "today".to_string();
    }
    if days == 1 {
        return "1 day ago".to_string();
    }
    if days < 30 {
        return format!("{days} days ago");
    }
    let months = days / 30;
    if months == 1 {
        return "1 month ago".to_string();
    }
    if months < 12 {
        return format!("{months} months ago");
    }
    let years = months / 12;
    if years == 1 {
        "1 year ago".to_string()
    } else {
        format!("{years} years ago")
    }
}

const RECONCILE_SYSTEM_PROMPT: &str = r#"You are a memory reconciliation engine. Given a set of NEW FACTS extracted from a conversation and a set of EXISTING MEMORIES, decide what to do with each fact.

## Operations

- **CREATE**: The fact contains genuinely new information not covered by any existing memory. Creates a new memory.
- **MERGE**: The fact adds detail, clarification, or refinement to an existing memory. The existing memory's content should be enriched. Provide `merged_content` — the combined text.
- **SKIP**: The fact is a duplicate or contains less information than an existing memory. No action needed.
- **SUPERSEDE**: The fact contradicts or updates an existing memory on the same topic (e.g., changed preference, updated status). The old memory is archived and a new one is created. Use when time-sensitive facts have changed.
- **SUPPORT**: The candidate reinforces or confirms an existing memory, possibly in a specific context. No new memory is created — the existing memory's confidence is boosted. Include `context_label` (one of: general, morning, evening, work, leisure, seasonal, weekday, weekend).
- **CONTEXTUALIZE**: The candidate adds situational nuance to an existing memory without contradicting it. Example: existing "likes coffee" + new "prefers tea in the evening". A new memory is created with a relation to the existing one. Include `context_label`.
- **CONTRADICT**: The candidate directly contradicts an existing memory. For temporal_versioned categories (preferences, entities) with general context, this routes to SUPERSEDE behavior. Otherwise, a new memory is created and the contradiction is recorded.

## Category-Aware Rules

1. **profile** category: always use MERGE when a matching memory exists (never SUPERSEDE or CONTRADICT for profile).
2. **events** and **cases** categories: only CREATE or SKIP. Never MERGE, SUPERSEDE, SUPPORT, CONTEXTUALIZE, or CONTRADICT.
3. **preferences** and **entities** categories: support all 7 operations including SUPERSEDE and CONTRADICT.
4. **preferences**, **entities**, **patterns** categories: support MERGE.

## General Rules

1. Each fact MUST receive exactly one decision.
2. Use `match_index` to reference existing memories by their integer ID (shown in brackets).
3. For MERGE: `match_index` is required. Provide `merged_content` combining both old and new info.
4. For SUPERSEDE: `match_index` is required. The old memory will be archived.
5. For SUPPORT: `match_index` is required. Include `context_label`.
6. For CONTEXTUALIZE: `match_index` is required. Include `context_label`.
7. For CONTRADICT: `match_index` is required.
8. For CREATE and SKIP: `match_index` is optional (null).
9. Same meaning, different wording → SKIP (not MERGE).
10. Age is a tiebreaker: when a new fact conflicts with an old memory on the same topic, the older memory is more likely outdated → prefer SUPERSEDE.
11. When in doubt, prefer CREATE over SKIP (avoid losing information).

## Output Format
Return ONLY valid JSON:
{"decisions": [{"action": "CREATE", "fact_index": 0, "reason": "new info"}, {"action": "MERGE", "fact_index": 1, "match_index": 3, "merged_content": "combined text", "reason": "adds detail"}, {"action": "SKIP", "fact_index": 2, "match_index": 0, "reason": "duplicate"}, {"action": "SUPERSEDE", "fact_index": 3, "match_index": 1, "reason": "updated preference"}, {"action": "SUPPORT", "fact_index": 4, "match_index": 2, "context_label": "work", "reason": "reinforces existing"}, {"action": "CONTEXTUALIZE", "fact_index": 5, "match_index": 4, "context_label": "evening", "reason": "adds situational nuance"}, {"action": "CONTRADICT", "fact_index": 6, "match_index": 5, "reason": "directly contradicts"}]}
"#;

pub fn build_batch_dedup_prompt(facts: &[ExtractedFact]) -> (String, String) {
    let mut facts_text = String::new();
    for (i, fact) in facts.iter().enumerate() {
        let display = fact
            .source_text
            .as_deref()
            .map(|s| {
                let truncated: String = s.chars().take(200).collect();
                if s.chars().count() > 200 {
                    format!("{truncated}...")
                } else {
                    truncated
                }
            })
            .unwrap_or_else(|| fact.l0_abstract.clone());
        facts_text.push_str(&format!("FACT[{}]: [{}] {}\n", i, fact.category, display));
    }
    (
        BATCH_DEDUP_SYSTEM_PROMPT.to_string(),
        format!("Deduplicate the following facts:\n\n{facts_text}"),
    )
}

pub fn build_section_prompt(section_text: &str) -> (String, String) {
    (
        SECTION_SYSTEM_PROMPT.to_string(),
        format!("Summarize the following section as a single memory:\n\n{section_text}"),
    )
}

pub fn build_document_prompt(document_text: &str) -> (String, String) {
    (
        DOCUMENT_SYSTEM_PROMPT.to_string(),
        format!(
            "Summarize the following document as a single comprehensive memory:\n\n{document_text}"
        ),
    )
}

const BATCH_DEDUP_SYSTEM_PROMPT: &str = r#"You are a deduplication engine. Given a list of extracted facts, identify and remove duplicates or near-duplicates within the batch.

## Rules

1. Compare all facts pairwise.
2. When two facts cover the same topic or convey the same meaning:
   - Keep the MORE DETAILED or MORE SPECIFIC one.
   - If they are equally detailed, keep the one with the lower index.
3. If no duplicates are found, return ALL indices.
4. Preserve the original language of each fact.
5. Only remove true duplicates or highly overlapping facts. Different aspects of the same topic are NOT duplicates.

## Output Format
Return ONLY valid JSON:
{"keep_indices": [0, 2, 3, 5]}

The array should list the indices of facts to KEEP (not the ones to remove).
If all facts are unique, return all indices: {"keep_indices": [0, 1, 2, ...]}
"#;

const SECTION_SYSTEM_PROMPT: &str = r#"You are a memory extraction engine. Your task is to create exactly ONE memory from the given text section.

## Rules
- Create exactly 1 memory that captures the section's key information.
- **CRITICAL**: You MUST output in the SAME language as the input. Chinese input → Chinese output. English input → English output. NEVER translate.
- Do NOT translate content to English. If the input is in Chinese, ALL fields (l0_abstract, l1_overview, l2_content) MUST be in Chinese.
- Do NOT split into multiple facts — summarize as one cohesive memory.

## Categories
Classify the memory into exactly one category:
- **profile**: Biographical or identity information.
- **preferences**: Likes, dislikes, tool choices, style preferences.
- **entities**: Persistent nouns (projects, tools, people, orgs) and their states.
- **events**: Things that happened — milestones, incidents, decisions made.
- **cases**: Problem→solution pairs, debugging stories, how-tos.
- **patterns**: Reusable processes, workflows, conventions, templates.

## Layered Storage
- **l0_abstract**: A single sentence index entry. Brief enough to scan quickly.
- **l1_overview**: A structured markdown summary in 2-4 lines. Includes key attributes.
- **l2_content**: Full narrative preserving all relevant details, context, and nuance from the section.

## Output Format
Return ONLY valid JSON:
{"memories": [{"l0_abstract":"...","l1_overview":"...","l2_content":"...","category":"...","tags":["..."]}]}
"#;

const DOCUMENT_SYSTEM_PROMPT: &str = r#"You are a memory extraction engine. Your task is to create exactly ONE comprehensive memory from the entire document.

## Rules
- Create exactly 1 memory that captures the document's most important information.
- **CRITICAL**: You MUST output in the SAME language as the input. Chinese input → Chinese output. English input → English output. NEVER translate.
- Do NOT translate content to English. If the input is in Chinese, ALL fields (l0_abstract, l1_overview, l2_content) MUST be in Chinese.
- The l2_content should be a thorough summary covering all key points.
- Do NOT split into multiple facts — produce one comprehensive memory.

## Categories
Classify the memory into exactly one category:
- **profile**: Biographical or identity information.
- **preferences**: Likes, dislikes, tool choices, style preferences.
- **entities**: Persistent nouns (projects, tools, people, orgs) and their states.
- **events**: Things that happened — milestones, incidents, decisions made.
- **cases**: Problem→solution pairs, debugging stories, how-tos.
- **patterns**: Reusable processes, workflows, conventions, templates.

## Layered Storage
- **l0_abstract**: A single sentence index entry. Brief enough to scan quickly.
- **l1_overview**: A structured markdown summary in 3-5 lines covering the main topics.
- **l2_content**: Comprehensive narrative covering all key information from the document.

## Output Format
Return ONLY valid JSON:
{"memories": [{"l0_abstract":"...","l1_overview":"...","l2_content":"...","category":"...","tags":["..."]}]}
"#;

const BASE_SYSTEM_PROMPT: &str = r#"You are an information extraction engine. Your task is to extract distinct, atomic facts from the USER messages in a conversation.

## Rules
- Extract facts ONLY from USER messages. Assistant messages provide context only.
- Each fact must be atomic — one piece of information per fact.
- **CRITICAL**: You MUST output in the SAME language as the input. Chinese input → Chinese facts. English input → English facts. NEVER translate.
- Do NOT translate content to English. If the input is in Chinese, ALL fields (l0_abstract, l1_overview, l2_content) MUST be in Chinese.
- Maximum 50 facts per extraction.

## Categories
Classify each fact into exactly one category:

- **profile**: Biographical or identity information about the user. Decision: "Can this be phrased as 'User is...'?"
- **preferences**: Likes, dislikes, tool choices, style preferences. Decision: "Can this be phrased as 'User prefers/likes...'?"
- **entities**: Persistent nouns (projects, tools, people, orgs) and their states. Decision: "Does this describe a persistent noun's state?"
- **events**: Things that happened — milestones, incidents, decisions made. Decision: "Does this describe something that happened?"
- **cases**: Problem→solution pairs, debugging stories, how-tos. Decision: "Does this contain a problem→solution pair?"
- **patterns**: Reusable processes, workflows, conventions, templates. Decision: "Is this a reusable process?"

## Layered Storage
For each fact, produce three layers of detail:

- **l0_abstract**: A single sentence index entry. Brief enough to scan quickly.
- **l1_overview**: A structured markdown summary in 2-3 lines. Includes key attributes.
- **l2_content**: Full narrative with all relevant details, context, and nuance.

## Exclusion Rules
Do NOT extract:
- General knowledge (widely known facts)
- System metadata (timestamps, message IDs)
- Temporary or ephemeral information (weather, current time)
- Tool/function output or raw data dumps
- Greetings, pleasantries, or filler

## Output Format
Return ONLY valid JSON:
{"memories": [{"l0_abstract":"...","l1_overview":"...","l2_content":"...","category":"...","tags":["..."]}]}

## Examples

### Example 1 — Profile
User says: "I'm a backend engineer at Stripe, working on the payments team."
```json
{"memories": [{"l0_abstract": "User is a backend engineer at Stripe on the payments team", "l1_overview": "**Role**: Backend Engineer\n**Company**: Stripe\n**Team**: Payments", "l2_content": "The user identified themselves as a backend engineer working at Stripe, specifically on the payments team.", "category": "profile", "tags": ["career", "stripe"]}]}
```

### Example 2 — Preference
User says: "I always use Rust for systems programming, I find it much safer than C++."
```json
{"memories": [{"l0_abstract": "User prefers Rust over C++ for systems programming", "l1_overview": "**Language**: Rust preferred for systems programming\n**Reason**: Safety advantages over C++", "l2_content": "The user expressed a strong preference for Rust over C++ when doing systems programming, citing safety as the primary advantage.", "category": "preferences", "tags": ["rust", "programming-languages"]}]}
```

### Example 3 — Case
User says: "My Docker builds were failing because the COPY step couldn't find the file. Turned out I needed to add it to .dockerignore exceptions."
```json
{"memories": [{"l0_abstract": "Docker COPY failure fixed by updating .dockerignore exceptions", "l1_overview": "**Problem**: Docker COPY step fails — file not found\n**Cause**: File excluded by .dockerignore\n**Solution**: Add exception to .dockerignore", "l2_content": "The user encountered Docker build failures where the COPY step couldn't find the target file. The root cause was that .dockerignore was excluding the file. The fix was to add an exception entry in .dockerignore to allow the file to be included in the build context.", "category": "cases", "tags": ["docker", "debugging"]}]}
```
"#;
