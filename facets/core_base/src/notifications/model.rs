use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Who may see a notification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Audience {
    /// Every logged-in member of the organization.
    Members,
    /// Members and anonymous visitors.
    Everyone,
    /// Specific actors: user ids (`users:…`) and visitor ids (`visitors:…`).
    Actors { actors: Vec<String> },
}

impl Audience {
    /// The `audience_kind` column.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Members => "members",
            Self::Everyone => "everyone",
            Self::Actors { .. } => "actors",
        }
    }

    /// The `audience_actors` column.
    pub fn actors(&self) -> &[String] {
        match self {
            Self::Actors { actors } => actors,
            _ => &[],
        }
    }

    pub fn from_columns(kind: &str, actors: Vec<String>) -> Self {
        match kind {
            "everyone" => Self::Everyone,
            "actors" => Self::Actors { actors },
            _ => Self::Members,
        }
    }

    pub fn includes(&self, viewer: &Viewer) -> bool {
        match self {
            Self::Everyone => true,
            Self::Members => viewer.member,
            Self::Actors { actors } => viewer
                .actor
                .as_ref()
                .is_some_and(|actor| actors.iter().any(|candidate| candidate == actor)),
        }
    }
}

/// The person on the other end of a request: a member, a visitor with a cookie, or an
/// anonymous browser with no identity yet (who only sees what is for everyone).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Viewer {
    /// `users:…` or `visitors:…`; none for a browser without an identity.
    pub actor: Option<String>,
    /// A logged-in member of the organization.
    pub member: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    Info,
    Success,
    Warning,
    Error,
}

impl Level {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Success => "success",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "info" => Some(Self::Info),
            "success" => Some(Self::Success),
            "warning" => Some(Self::Warning),
            "error" => Some(Self::Error),
            _ => None,
        }
    }
}

/// What a sender provides; the kernel fills in the rest.
#[derive(Debug, Clone)]
pub struct NewNotification {
    /// `kernel`, or the plugin that sent it.
    pub source: String,
    pub level: Level,
    pub title: String,
    pub body: Option<String>,
    /// An app path opened when the notification is clicked.
    pub link: Option<String>,
    pub payload: Option<Value>,
    pub audience: Audience,
    /// Seconds until it stops being shown; none keeps it until retention removes it.
    pub expires_in_secs: Option<u64>,
}

impl NewNotification {
    /// A notification from the kernel itself, for the organization's members.
    pub fn kernel(level: Level, title: impl Into<String>, body: Option<String>) -> Self {
        Self {
            source: "kernel".to_string(),
            level,
            title: title.into(),
            body,
            link: None,
            payload: None,
            audience: Audience::Members,
            expires_in_secs: None,
        }
    }
}

/// A stored notification, as sent to browsers.
#[derive(Debug, Clone, Serialize)]
pub struct Notification {
    /// Time-ordered and unique; the event stream's `id`.
    pub id: String,
    pub source: String,
    pub level: Level,
    pub title: String,
    pub body: Option<String>,
    pub link: Option<String>,
    pub payload: Option<Value>,
    pub created_at: String,
    /// Whether the viewer has read it; only set when listing.
    pub read: bool,
    /// Who may see it. Not sent: the server decides.
    #[serde(skip)]
    pub audience: Audience,
}
