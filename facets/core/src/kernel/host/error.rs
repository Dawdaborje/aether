use aether_security::capabilities::CapabilityError;
use serde_json::Value as JsonValue;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum HostError {
    #[error(transparent)]
    Capability(#[from] CapabilityError),

    #[error("unknown host command `{0}`")]
    UnknownCommand(String),

    #[error("invalid payload: {0}")]
    InvalidPayload(String),

    #[error("model `{0}` is not in this plugin's access_models")]
    ModelDenied(String),

    #[error("model `{0}` does not allow {1}")]
    ModelPermission(String, &'static str),

    #[error("raw SurQL rejected: {0}")]
    SurqlRejected(String),

    #[error("database error: {0}")]
    Db(#[from] surrealdb::Error),

    #[error("not implemented: {0}")]
    NotImplemented(String),

    #[error("{0}")]
    Message(String),
}

impl HostError {
    pub fn to_json(&self) -> JsonValue {
        serde_json::json!({
            "error": self.to_string(),
            "kind": match self {
                Self::Capability(_) => "capability",
                Self::UnknownCommand(_) => "unknown_command",
                Self::InvalidPayload(_) => "invalid_payload",
                Self::ModelDenied(_) | Self::ModelPermission(_, _) => "model",
                Self::SurqlRejected(_) => "surql",
                Self::Db(_) => "db",
                Self::NotImplemented(_) => "not_implemented",
                Self::Message(_) => "error",
            }
        })
    }
}
