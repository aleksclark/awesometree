use serde::{Deserialize, Serialize};

/// AWM WorkSession lifecycle states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum WorkSessionState {
    #[default]
    Proposed,
    Open,
    Paused,
    Closed,
    Aborted,
}

impl WorkSessionState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Proposed => "proposed",
            Self::Open => "open",
            Self::Paused => "paused",
            Self::Closed => "closed",
            Self::Aborted => "aborted",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "proposed" => Some(Self::Proposed),
            "open" => Some(Self::Open),
            "paused" => Some(Self::Paused),
            "closed" => Some(Self::Closed),
            "aborted" => Some(Self::Aborted),
            _ => None,
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Closed | Self::Aborted)
    }

    pub fn can_transition_to(self, to: Self) -> bool {
        if self == to {
            return true;
        }
        matches!(
            (self, to),
            (Self::Proposed, Self::Open)
                | (Self::Proposed, Self::Aborted)
                | (Self::Open, Self::Paused)
                | (Self::Open, Self::Closed)
                | (Self::Open, Self::Aborted)
                | (Self::Paused, Self::Open)
                | (Self::Paused, Self::Closed)
                | (Self::Paused, Self::Aborted)
        )
    }
}

impl std::fmt::Display for WorkSessionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
