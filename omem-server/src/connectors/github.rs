use std::sync::Arc;

use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use crate::domain::category::Category;
use crate::domain::error::OmemError;
use crate::domain::memory::Memory;
use crate::domain::types::MemoryType;
use crate::embed::EmbedService;
use crate::store::LanceStore;

type HmacSha256 = Hmac<Sha256>;

pub struct GitHubConnector {
    client: reqwest::Client,
    pub store: Arc<LanceStore>,
    pub embed: Arc<dyn EmbedService>,
    pub webhook_secret: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ConnectRequest {
    pub access_token: String,
    pub repo: String,
    pub webhook_url: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ConnectResponse {
    pub status: String,
    pub repo: String,
    pub webhook_id: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookPayload {
    pub action: Option<String>,
    pub r#ref: Option<String>,
    pub commits: Option<Vec<CommitPayload>>,
    pub issue: Option<IssuePayload>,
    pub comment: Option<CommentPayload>,
    pub pull_request: Option<PullRequestPayload>,
    pub review: Option<ReviewPayload>,
    pub repository: Option<RepositoryPayload>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitPayload {
    pub id: Option<String>,
    pub message: Option<String>,
    pub added: Option<Vec<String>>,
    pub modified: Option<Vec<String>>,
    pub removed: Option<Vec<String>>,
    pub author: Option<AuthorPayload>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorPayload {
    pub name: Option<String>,
    pub email: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssuePayload {
    pub number: Option<u64>,
    pub title: Option<String>,
    pub body: Option<String>,
    pub state: Option<String>,
    pub user: Option<UserPayload>,
    pub labels: Option<Vec<LabelPayload>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserPayload {
    pub login: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabelPayload {
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommentPayload {
    pub body: Option<String>,
    pub user: Option<UserPayload>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullRequestPayload {
    pub number: Option<u64>,
    pub title: Option<String>,
    pub body: Option<String>,
    pub state: Option<String>,
    pub user: Option<UserPayload>,
    pub head: Option<BranchPayload>,
    pub base: Option<BranchPayload>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchPayload {
    pub r#ref: Option<String>,
    pub sha: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewPayload {
    pub body: Option<String>,
    pub state: Option<String>,
    pub user: Option<UserPayload>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositoryPayload {
    pub full_name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct WebhookResult {
    pub event_type: String,
    pub memories_created: usize,
}

impl GitHubConnector {
    pub fn new(
        store: Arc<LanceStore>,
        embed: Arc<dyn EmbedService>,
        webhook_secret: Option<String>,
    ) -> Self {
        Self {
            client: reqwest::Client::new(),
            store,
            embed,
            webhook_secret,
        }
    }

    pub fn verify_signature(&self, payload: &[u8], signature: &str) -> Result<(), OmemError> {
        let secret = self
            .webhook_secret
            .as_ref()
            .ok_or_else(|| OmemError::Validation("webhook secret not configured".to_string()))?;

        let sig_hex = signature
            .strip_prefix("sha256=")
            .ok_or_else(|| OmemError::Validation("invalid signature format".to_string()))?;

        let expected = hex::decode(sig_hex)
            .map_err(|e| OmemError::Validation(format!("invalid hex in signature: {e}")))?;

        let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
            .map_err(|e| OmemError::Internal(format!("HMAC init failed: {e}")))?;
        mac.update(payload);

        mac.verify_slice(&expected)
            .map_err(|_| OmemError::Unauthorized("webhook signature mismatch".to_string()))
    }

    pub fn process_webhook(
        &self,
        event_type: &str,
        payload: &WebhookPayload,
        tenant_id: &str,
    ) -> Result<Vec<Memory>, OmemError> {
        let repo_name = payload
            .repository
            .as_ref()
            .and_then(|r| r.full_name.clone())
            .unwrap_or_else(|| "unknown".to_string());

        match event_type {
            "push" => self.handle_push(payload, tenant_id, &repo_name),
            "issues" | "issue_comment" => self.handle_issue(payload, tenant_id, &repo_name),
            "pull_request" | "pull_request_review" => {
                self.handle_pull_request(payload, tenant_id, &repo_name)
            }
            _ => Ok(Vec::new()),
        }
    }

    fn handle_push(
        &self,
        payload: &WebhookPayload,
        tenant_id: &str,
        repo_name: &str,
    ) -> Result<Vec<Memory>, OmemError> {
        let commits = match &payload.commits {
            Some(c) => c,
            None => return Ok(Vec::new()),
        };

        let mut memories = Vec::new();

        for commit in commits {
            let message = commit.message.as_deref().unwrap_or("");
            let author = commit
                .author
                .as_ref()
                .and_then(|a| a.name.clone())
                .unwrap_or_else(|| "unknown".to_string());
            let commit_id = commit.id.as_deref().unwrap_or("unknown");

            let all_files: Vec<String> = [
                commit.added.as_deref().unwrap_or(&[]),
                commit.modified.as_deref().unwrap_or(&[]),
            ]
            .concat();

            let content = format!(
                "Commit {short_id} by {author}: {message}\nChanged files: {files}",
                short_id = &commit_id[..commit_id.len().min(8)],
                files = all_files.join(", ")
            );

            let mut memory =
                Memory::new(&content, Category::Events, MemoryType::Session, tenant_id);
            memory.tags = vec![
                format!("repo:{repo_name}"),
                "github:push".to_string(),
                format!("commit:{}", &commit_id[..commit_id.len().min(8)]),
            ];
            memory.source = Some(format!("github:{repo_name}"));

            for file in &all_files {
                if let Some(lang) = Self::detect_language_from_filename(file) {
                    let code_content = format!("[File changed: {file} in commit {commit_id}]");
                    let mut file_memory = Memory::new(
                        &code_content,
                        Category::Entities,
                        MemoryType::Session,
                        tenant_id,
                    );
                    file_memory.tags = vec![
                        format!("repo:{repo_name}"),
                        "github:file_change".to_string(),
                        format!("language:{lang}"),
                        format!("file:{file}"),
                    ];
                    file_memory.source = Some(format!("github:{repo_name}"));
                    memories.push(file_memory);
                }
            }

            memories.push(memory);
        }

        Ok(memories)
    }

    fn handle_issue(
        &self,
        payload: &WebhookPayload,
        tenant_id: &str,
        repo_name: &str,
    ) -> Result<Vec<Memory>, OmemError> {
        let mut memories = Vec::new();

        if let Some(issue) = &payload.issue {
            let title = issue.title.as_deref().unwrap_or("Untitled");
            let body = issue.body.as_deref().unwrap_or("");
            let number = issue.number.unwrap_or(0);
            let author = issue
                .user
                .as_ref()
                .and_then(|u| u.login.clone())
                .unwrap_or_else(|| "unknown".to_string());

            let content = format!("Issue #{number}: {title}\nBy: {author}\n\n{body}");

            let mut memory =
                Memory::new(&content, Category::Events, MemoryType::Session, tenant_id);
            memory.tags = vec![
                format!("repo:{repo_name}"),
                "github:issue".to_string(),
                format!("issue:{number}"),
            ];
            memory.source = Some(format!("github:{repo_name}"));
            memories.push(memory);
        }

        if let Some(comment) = &payload.comment {
            let body = comment.body.as_deref().unwrap_or("");
            let author = comment
                .user
                .as_ref()
                .and_then(|u| u.login.clone())
                .unwrap_or_else(|| "unknown".to_string());
            let issue_number = payload.issue.as_ref().and_then(|i| i.number).unwrap_or(0);

            let content = format!("Comment on issue #{issue_number} by {author}:\n{body}");

            let mut memory =
                Memory::new(&content, Category::Events, MemoryType::Session, tenant_id);
            memory.tags = vec![
                format!("repo:{repo_name}"),
                "github:issue_comment".to_string(),
                format!("issue:{issue_number}"),
            ];
            memory.source = Some(format!("github:{repo_name}"));
            memories.push(memory);
        }

        Ok(memories)
    }

    fn handle_pull_request(
        &self,
        payload: &WebhookPayload,
        tenant_id: &str,
        repo_name: &str,
    ) -> Result<Vec<Memory>, OmemError> {
        let mut memories = Vec::new();

        if let Some(pr) = &payload.pull_request {
            let title = pr.title.as_deref().unwrap_or("Untitled");
            let body = pr.body.as_deref().unwrap_or("");
            let number = pr.number.unwrap_or(0);
            let author = pr
                .user
                .as_ref()
                .and_then(|u| u.login.clone())
                .unwrap_or_else(|| "unknown".to_string());
            let head_ref = pr
                .head
                .as_ref()
                .and_then(|h| h.r#ref.clone())
                .unwrap_or_default();
            let base_ref = pr
                .base
                .as_ref()
                .and_then(|b| b.r#ref.clone())
                .unwrap_or_default();

            let content = format!(
                "PR #{number}: {title}\nBy: {author}\nBranch: {head_ref} → {base_ref}\n\n{body}"
            );

            let mut memory =
                Memory::new(&content, Category::Events, MemoryType::Session, tenant_id);
            memory.tags = vec![
                format!("repo:{repo_name}"),
                "github:pull_request".to_string(),
                format!("pr:{number}"),
            ];
            memory.source = Some(format!("github:{repo_name}"));
            memories.push(memory);
        }

        if let Some(review) = &payload.review {
            let body = review.body.as_deref().unwrap_or("");
            let state = review.state.as_deref().unwrap_or("unknown");
            let author = review
                .user
                .as_ref()
                .and_then(|u| u.login.clone())
                .unwrap_or_else(|| "unknown".to_string());
            let pr_number = payload
                .pull_request
                .as_ref()
                .and_then(|p| p.number)
                .unwrap_or(0);

            if !body.is_empty() {
                let content = format!("Review on PR #{pr_number} by {author} ({state}):\n{body}");

                let mut memory =
                    Memory::new(&content, Category::Events, MemoryType::Session, tenant_id);
                memory.tags = vec![
                    format!("repo:{repo_name}"),
                    "github:review".to_string(),
                    format!("pr:{pr_number}"),
                ];
                memory.source = Some(format!("github:{repo_name}"));
                memories.push(memory);
            }
        }

        Ok(memories)
    }

    pub async fn register_webhook(
        &self,
        access_token: &str,
        repo: &str,
        webhook_url: &str,
    ) -> Result<ConnectResponse, OmemError> {
        let url = format!("https://api.github.com/repos/{repo}/hooks");

        let body = serde_json::json!({
            "name": "web",
            "active": true,
            "events": ["push", "issues", "issue_comment", "pull_request", "pull_request_review"],
            "config": {
                "url": webhook_url,
                "content_type": "json",
                "secret": self.webhook_secret.as_deref().unwrap_or(""),
            }
        });

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {access_token}"))
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", "omem-server")
            .json(&body)
            .send()
            .await
            .map_err(|e| OmemError::Internal(format!("GitHub API request failed: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response
                .text()
                .await
                .unwrap_or_else(|_| "unknown error".to_string());
            return Err(OmemError::Internal(format!(
                "GitHub API returned {status}: {text}"
            )));
        }

        let resp_json: serde_json::Value = response
            .json()
            .await
            .map_err(|e| OmemError::Internal(format!("failed to parse GitHub response: {e}")))?;

        Ok(ConnectResponse {
            status: "connected".to_string(),
            repo: repo.to_string(),
            webhook_id: resp_json["id"].as_u64(),
        })
    }

    fn detect_language_from_filename(filename: &str) -> Option<String> {
        let ext = filename.rsplit('.').next()?;
        match ext.to_lowercase().as_str() {
            "rs" => Some("rust".to_string()),
            "py" => Some("python".to_string()),
            "js" | "mjs" => Some("javascript".to_string()),
            "ts" | "mts" => Some("typescript".to_string()),
            "go" => Some("go".to_string()),
            _ => None,
        }
    }

    pub async fn store_memories(&self, memories: Vec<Memory>) -> Result<usize, OmemError> {
        let count = memories.len();
        for memory in &memories {
            let vectors = self
                .embed
                .embed(std::slice::from_ref(&memory.content))
                .await
                .map_err(|e| OmemError::Embedding(format!("failed to embed: {e}")))?;
            let vector = vectors.into_iter().next();
            self.store.create(memory, vector.as_deref()).await?;
        }
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn make_connector() -> (GitHubConnector, tempfile::TempDir) {
        let dir = tempfile::TempDir::new().unwrap();
        let store = Arc::new(LanceStore::new(dir.path().to_str().unwrap()).await.unwrap());
        store.init_table().await.unwrap();

        let connector = GitHubConnector {
            client: reqwest::Client::new(),
            store,
            embed: Arc::new(crate::embed::NoopEmbedder::new(1024)),
            webhook_secret: Some("test-secret".to_string()),
        };
        (connector, dir)
    }

    #[tokio::test]
    async fn test_verify_signature_valid() {
        let (connector, _dir) = make_connector().await;
        let payload = b"test payload";

        let mut mac = HmacSha256::new_from_slice(b"test-secret").unwrap();
        mac.update(payload);
        let result = mac.finalize();
        let sig = format!("sha256={}", hex::encode(result.into_bytes()));

        assert!(connector.verify_signature(payload, &sig).is_ok());
    }

    #[tokio::test]
    async fn test_verify_signature_invalid() {
        let (connector, _dir) = make_connector().await;
        let payload = b"test payload";
        let bad_sig = "sha256=0000000000000000000000000000000000000000000000000000000000000000";

        let result = connector.verify_signature(payload, bad_sig);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_verify_signature_bad_format() {
        let (connector, _dir) = make_connector().await;
        let result = connector.verify_signature(b"data", "invalid-format");
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_handle_push_event() {
        let (connector, _dir) = make_connector().await;
        let payload = WebhookPayload {
            action: None,
            r#ref: Some("refs/heads/main".to_string()),
            commits: Some(vec![CommitPayload {
                id: Some("abc12345".to_string()),
                message: Some("fix: resolve bug in parser".to_string()),
                added: Some(vec!["src/parser.rs".to_string()]),
                modified: Some(vec!["src/main.rs".to_string()]),
                removed: None,
                author: Some(AuthorPayload {
                    name: Some("dev".to_string()),
                    email: Some("dev@example.com".to_string()),
                }),
            }]),
            issue: None,
            comment: None,
            pull_request: None,
            review: None,
            repository: Some(RepositoryPayload {
                full_name: Some("owner/repo".to_string()),
            }),
        };

        let memories = connector
            .process_webhook("push", &payload, "t-001")
            .unwrap();
        assert!(!memories.is_empty());

        let commit_mem = memories
            .iter()
            .find(|m| m.tags.contains(&"github:push".to_string()));
        assert!(commit_mem.is_some());
        let cm = commit_mem.unwrap();
        assert!(cm.content.contains("fix: resolve bug in parser"));
        assert!(cm.tags.contains(&"repo:owner/repo".to_string()));
    }

    #[tokio::test]
    async fn test_handle_issue_event() {
        let (connector, _dir) = make_connector().await;
        let payload = WebhookPayload {
            action: Some("opened".to_string()),
            r#ref: None,
            commits: None,
            issue: Some(IssuePayload {
                number: Some(42),
                title: Some("Bug: crash on startup".to_string()),
                body: Some("The app crashes when...".to_string()),
                state: Some("open".to_string()),
                user: Some(UserPayload {
                    login: Some("reporter".to_string()),
                }),
                labels: None,
            }),
            comment: None,
            pull_request: None,
            review: None,
            repository: Some(RepositoryPayload {
                full_name: Some("owner/repo".to_string()),
            }),
        };

        let memories = connector
            .process_webhook("issues", &payload, "t-001")
            .unwrap();
        assert_eq!(memories.len(), 1);
        assert!(memories[0].content.contains("Issue #42"));
        assert!(memories[0].content.contains("Bug: crash on startup"));
        assert!(memories[0].tags.contains(&"github:issue".to_string()));
    }

    #[tokio::test]
    async fn test_handle_issue_comment_event() {
        let (connector, _dir) = make_connector().await;
        let payload = WebhookPayload {
            action: Some("created".to_string()),
            r#ref: None,
            commits: None,
            issue: Some(IssuePayload {
                number: Some(42),
                title: Some("Bug".to_string()),
                body: None,
                state: None,
                user: None,
                labels: None,
            }),
            comment: Some(CommentPayload {
                body: Some("I can reproduce this".to_string()),
                user: Some(UserPayload {
                    login: Some("helper".to_string()),
                }),
            }),
            pull_request: None,
            review: None,
            repository: Some(RepositoryPayload {
                full_name: Some("owner/repo".to_string()),
            }),
        };

        let memories = connector
            .process_webhook("issue_comment", &payload, "t-001")
            .unwrap();
        assert_eq!(memories.len(), 2);
        let comment_mem = memories
            .iter()
            .find(|m| m.tags.contains(&"github:issue_comment".to_string()));
        assert!(comment_mem.is_some());
        assert!(comment_mem
            .unwrap()
            .content
            .contains("I can reproduce this"));
    }

    #[tokio::test]
    async fn test_handle_pull_request_event() {
        let (connector, _dir) = make_connector().await;
        let payload = WebhookPayload {
            action: Some("opened".to_string()),
            r#ref: None,
            commits: None,
            issue: None,
            comment: None,
            pull_request: Some(PullRequestPayload {
                number: Some(10),
                title: Some("Add new feature".to_string()),
                body: Some("This PR adds...".to_string()),
                state: Some("open".to_string()),
                user: Some(UserPayload {
                    login: Some("dev".to_string()),
                }),
                head: Some(BranchPayload {
                    r#ref: Some("feature-branch".to_string()),
                    sha: None,
                }),
                base: Some(BranchPayload {
                    r#ref: Some("main".to_string()),
                    sha: None,
                }),
            }),
            review: None,
            repository: Some(RepositoryPayload {
                full_name: Some("owner/repo".to_string()),
            }),
        };

        let memories = connector
            .process_webhook("pull_request", &payload, "t-001")
            .unwrap();
        assert_eq!(memories.len(), 1);
        assert!(memories[0].content.contains("PR #10"));
        assert!(memories[0].content.contains("Add new feature"));
        assert!(memories[0]
            .tags
            .contains(&"github:pull_request".to_string()));
    }

    #[tokio::test]
    async fn test_handle_pr_review_event() {
        let (connector, _dir) = make_connector().await;
        let payload = WebhookPayload {
            action: Some("submitted".to_string()),
            r#ref: None,
            commits: None,
            issue: None,
            comment: None,
            pull_request: Some(PullRequestPayload {
                number: Some(10),
                title: Some("Feature".to_string()),
                body: None,
                state: None,
                user: None,
                head: None,
                base: None,
            }),
            review: Some(ReviewPayload {
                body: Some("LGTM!".to_string()),
                state: Some("approved".to_string()),
                user: Some(UserPayload {
                    login: Some("reviewer".to_string()),
                }),
            }),
            repository: Some(RepositoryPayload {
                full_name: Some("owner/repo".to_string()),
            }),
        };

        let memories = connector
            .process_webhook("pull_request_review", &payload, "t-001")
            .unwrap();
        assert!(memories.len() >= 2);
        let review_mem = memories
            .iter()
            .find(|m| m.tags.contains(&"github:review".to_string()));
        assert!(review_mem.is_some());
        assert!(review_mem.unwrap().content.contains("LGTM!"));
    }

    #[tokio::test]
    async fn test_unknown_event_returns_empty() {
        let (connector, _dir) = make_connector().await;
        let payload = WebhookPayload {
            action: None,
            r#ref: None,
            commits: None,
            issue: None,
            comment: None,
            pull_request: None,
            review: None,
            repository: None,
        };

        let memories = connector
            .process_webhook("unknown_event", &payload, "t-001")
            .unwrap();
        assert!(memories.is_empty());
    }
}
