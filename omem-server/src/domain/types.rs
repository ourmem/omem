use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum MemoryType {
    Pinned,
    Insight,
    Session,
}

impl MemoryType {
    pub fn is_pinned(&self) -> bool {
        matches!(self, Self::Pinned)
    }
}

impl fmt::Display for MemoryType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pinned => write!(f, "pinned"),
            Self::Insight => write!(f, "insight"),
            Self::Session => write!(f, "session"),
        }
    }
}

impl FromStr for MemoryType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "pinned" => Ok(Self::Pinned),
            "insight" => Ok(Self::Insight),
            "session" => Ok(Self::Session),
            _ => Err(format!("unknown memory type: {s}")),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum MemoryState {
    Active,
    Archived,
    Deleted,
    Superseded,
}

impl fmt::Display for MemoryState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Active => write!(f, "active"),
            Self::Archived => write!(f, "archived"),
            Self::Deleted => write!(f, "deleted"),
            Self::Superseded => write!(f, "superseded"),
        }
    }
}

impl FromStr for MemoryState {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "active" => Ok(Self::Active),
            "archived" => Ok(Self::Archived),
            "deleted" => Ok(Self::Deleted),
            "superseded" => Ok(Self::Superseded),
            _ => Err(format!("unknown memory state: {s}")),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    Core,
    Working,
    Peripheral,
}

impl fmt::Display for Tier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Core => write!(f, "core"),
            Self::Working => write!(f, "working"),
            Self::Peripheral => write!(f, "peripheral"),
        }
    }
}

impl FromStr for Tier {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "core" => Ok(Self::Core),
            "working" => Ok(Self::Working),
            "peripheral" => Ok(Self::Peripheral),
            _ => Err(format!("unknown tier: {s}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_type_display_lowercase() {
        assert_eq!(MemoryType::Pinned.to_string(), "pinned");
        assert_eq!(MemoryType::Insight.to_string(), "insight");
        assert_eq!(MemoryType::Session.to_string(), "session");
    }

    #[test]
    fn memory_type_from_str() {
        assert_eq!("pinned".parse::<MemoryType>().unwrap(), MemoryType::Pinned);
        assert_eq!(
            "INSIGHT".parse::<MemoryType>().unwrap(),
            MemoryType::Insight
        );
        assert!("unknown".parse::<MemoryType>().is_err());
    }

    #[test]
    fn memory_type_is_pinned() {
        assert!(MemoryType::Pinned.is_pinned());
        assert!(!MemoryType::Insight.is_pinned());
        assert!(!MemoryType::Session.is_pinned());
    }

    #[test]
    fn memory_type_serde_roundtrip() {
        let mt = MemoryType::Insight;
        let json = serde_json::to_string(&mt).unwrap();
        assert_eq!(json, "\"insight\"");
        let parsed: MemoryType = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, mt);
    }

    #[test]
    fn memory_state_display_lowercase() {
        assert_eq!(MemoryState::Active.to_string(), "active");
        assert_eq!(MemoryState::Archived.to_string(), "archived");
        assert_eq!(MemoryState::Deleted.to_string(), "deleted");
    }

    #[test]
    fn memory_state_serde_roundtrip() {
        let ms = MemoryState::Archived;
        let json = serde_json::to_string(&ms).unwrap();
        assert_eq!(json, "\"archived\"");
        let parsed: MemoryState = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, ms);
    }

    #[test]
    fn tier_display_lowercase() {
        assert_eq!(Tier::Core.to_string(), "core");
        assert_eq!(Tier::Working.to_string(), "working");
        assert_eq!(Tier::Peripheral.to_string(), "peripheral");
    }

    #[test]
    fn tier_serde_roundtrip() {
        let t = Tier::Working;
        let json = serde_json::to_string(&t).unwrap();
        assert_eq!(json, "\"working\"");
        let parsed: Tier = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, t);
    }

    #[test]
    fn tier_from_str() {
        assert_eq!("core".parse::<Tier>().unwrap(), Tier::Core);
        assert_eq!("PERIPHERAL".parse::<Tier>().unwrap(), Tier::Peripheral);
        assert!("invalid".parse::<Tier>().is_err());
    }
}
