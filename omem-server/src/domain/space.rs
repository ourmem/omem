use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SpaceType {
    Personal,
    Team,
    Organization,
}

impl fmt::Display for SpaceType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SpaceType::Personal => write!(f, "personal"),
            SpaceType::Team => write!(f, "team"),
            SpaceType::Organization => write!(f, "organization"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum MemberRole {
    Admin,
    Member,
    Reader,
}

impl fmt::Display for MemberRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MemberRole::Admin => write!(f, "admin"),
            MemberRole::Member => write!(f, "member"),
            MemberRole::Reader => write!(f, "reader"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpaceMember {
    pub user_id: String,
    pub role: MemberRole,
    pub joined_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoShareRule {
    pub id: String,
    pub source_space: String,
    pub categories: Vec<String>,
    pub tags: Vec<String>,
    pub min_importance: f32,
    pub require_approval: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Space {
    /// Format: "personal:alex", "team:backend", "org:acme"
    pub id: String,
    pub space_type: SpaceType,
    pub name: String,
    pub owner_id: String,
    pub members: Vec<SpaceMember>,
    pub auto_share_rules: Vec<AutoShareRule>,
    pub created_at: String,
    pub updated_at: String,
}

impl Default for Space {
    fn default() -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            id: String::new(),
            space_type: SpaceType::Personal,
            name: String::new(),
            owner_id: String::new(),
            members: Vec::new(),
            auto_share_rules: Vec::new(),
            created_at: now.clone(),
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provenance {
    pub shared_from_space: String,
    pub shared_from_memory: String,
    pub shared_by_user: String,
    pub shared_by_agent: String,
    pub shared_at: String,
    pub original_created_at: String,
    #[serde(default)]
    pub source_version: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SharingAction {
    Share,
    Pull,
    Unshare,
    BatchShare,
    Reshare,
}

impl fmt::Display for SharingAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SharingAction::Share => write!(f, "share"),
            SharingAction::Pull => write!(f, "pull"),
            SharingAction::Unshare => write!(f, "unshare"),
            SharingAction::BatchShare => write!(f, "batch_share"),
            SharingAction::Reshare => write!(f, "reshare"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharingEvent {
    pub id: String,
    pub action: SharingAction,
    pub memory_id: String,
    pub from_space: String,
    pub to_space: String,
    pub user_id: String,
    pub agent_id: String,
    pub content_preview: String,
    pub timestamp: String,
}

/// A share that matched an auto-share rule with `require_approval = true`,
/// awaiting a human decision. Held in a queue (no notifications) until an
/// approver with write access to `target_space` approves or rejects it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingShare {
    pub id: String,
    pub source_space: String,
    pub source_memory: String,
    pub target_space: String,
    pub rule_id: String,
    pub requested_by_user: String,
    pub requested_by_agent: String,
    pub content_preview: String,
    pub created_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_space_type_display() {
        assert_eq!(SpaceType::Personal.to_string(), "personal");
        assert_eq!(SpaceType::Team.to_string(), "team");
        assert_eq!(SpaceType::Organization.to_string(), "organization");
    }

    #[test]
    fn test_member_role_display() {
        assert_eq!(MemberRole::Admin.to_string(), "admin");
        assert_eq!(MemberRole::Member.to_string(), "member");
        assert_eq!(MemberRole::Reader.to_string(), "reader");
    }

    #[test]
    fn test_sharing_action_display() {
        assert_eq!(SharingAction::Share.to_string(), "share");
        assert_eq!(SharingAction::Pull.to_string(), "pull");
        assert_eq!(SharingAction::Unshare.to_string(), "unshare");
        assert_eq!(SharingAction::BatchShare.to_string(), "batch_share");
    }

    #[test]
    fn test_space_default() {
        let space = Space::default();
        assert!(space.id.is_empty());
        assert_eq!(space.space_type, SpaceType::Personal);
        assert!(space.name.is_empty());
        assert!(space.owner_id.is_empty());
        assert!(space.members.is_empty());
        assert!(space.auto_share_rules.is_empty());
        assert!(!space.created_at.is_empty());
        assert_eq!(space.created_at, space.updated_at);
    }

    #[test]
    fn test_space_serialization_roundtrip() {
        let space = Space {
            id: "team:backend".to_string(),
            space_type: SpaceType::Team,
            name: "Backend Team".to_string(),
            owner_id: "user-001".to_string(),
            members: vec![SpaceMember {
                user_id: "user-001".to_string(),
                role: MemberRole::Admin,
                joined_at: "2025-01-01T00:00:00Z".to_string(),
            }],
            auto_share_rules: vec![AutoShareRule {
                id: "rule-1".to_string(),
                source_space: "personal:alex".to_string(),
                categories: vec!["cases".to_string()],
                tags: vec!["architecture".to_string()],
                min_importance: 0.7,
                require_approval: true,
                created_at: "2025-01-01T00:00:00Z".to_string(),
            }],
            created_at: "2025-01-01T00:00:00Z".to_string(),
            updated_at: "2025-01-02T00:00:00Z".to_string(),
        };

        let json = serde_json::to_string(&space).expect("serialize");
        let parsed: Space = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(parsed.id, "team:backend");
        assert_eq!(parsed.space_type, SpaceType::Team);
        assert_eq!(parsed.name, "Backend Team");
        assert_eq!(parsed.members.len(), 1);
        assert_eq!(parsed.members[0].role, MemberRole::Admin);
        assert_eq!(parsed.auto_share_rules.len(), 1);
        assert!((parsed.auto_share_rules[0].min_importance - 0.7).abs() < f32::EPSILON);
    }

    #[test]
    fn test_sharing_event_serialization_roundtrip() {
        let event = SharingEvent {
            id: "evt-001".to_string(),
            action: SharingAction::Share,
            memory_id: "mem-001".to_string(),
            from_space: "personal:alex".to_string(),
            to_space: "team:backend".to_string(),
            user_id: "user-001".to_string(),
            agent_id: "agent-001".to_string(),
            content_preview: "user prefers dark mode".to_string(),
            timestamp: "2025-01-01T00:00:00Z".to_string(),
        };

        let json = serde_json::to_string(&event).expect("serialize");
        let parsed: SharingEvent = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(parsed.id, "evt-001");
        assert_eq!(parsed.action, SharingAction::Share);
        assert_eq!(parsed.from_space, "personal:alex");
        assert_eq!(parsed.to_space, "team:backend");
    }

    #[test]
    fn test_provenance_serialization_roundtrip() {
        let prov = Provenance {
            shared_from_space: "personal:alex".to_string(),
            shared_from_memory: "mem-original".to_string(),
            shared_by_user: "user-001".to_string(),
            shared_by_agent: "agent-001".to_string(),
            shared_at: "2025-01-01T00:00:00Z".to_string(),
            original_created_at: "2024-12-01T00:00:00Z".to_string(),
            source_version: Some(3),
        };

        let json = serde_json::to_string(&prov).expect("serialize");
        let parsed: Provenance = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(parsed.shared_from_space, "personal:alex");
        assert_eq!(parsed.shared_from_memory, "mem-original");
        assert_eq!(parsed.original_created_at, "2024-12-01T00:00:00Z");
    }

    #[test]
    fn test_space_type_serde_snake_case() {
        let json = serde_json::to_string(&SpaceType::Organization).expect("serialize");
        assert_eq!(json, "\"organization\"");

        let parsed: SpaceType = serde_json::from_str("\"personal\"").expect("deserialize");
        assert_eq!(parsed, SpaceType::Personal);
    }

    #[test]
    fn test_sharing_action_serde_snake_case() {
        let json = serde_json::to_string(&SharingAction::BatchShare).expect("serialize");
        assert_eq!(json, "\"batch_share\"");

        let parsed: SharingAction = serde_json::from_str("\"pull\"").expect("deserialize");
        assert_eq!(parsed, SharingAction::Pull);
    }
}
