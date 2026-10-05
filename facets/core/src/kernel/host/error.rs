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

    #[error("table `{0}` belongs to the kernel and cannot be used as a model")]
    ReservedTable(String),

    #[error("database error: {0}")]
    Db(#[from] surrealdb::Error),

    #[error("not implemented: {0}")]
    NotImplemented(String),

    #[error("{0}")]
    Message(String),

    /// The caller's role does not allow it (a record or field rule).
    #[error("not allowed: {0}")]
    Denied(String),
}

impl From<crate::data_model::SchemaError> for HostError {
    fn from(error: crate::data_model::SchemaError) -> Self {
        Self::InvalidPayload(error.to_string())
    }
}

impl HostError {
    pub fn to_json(&self) -> JsonValue {
        serde_json::json!({
            "error": self.to_string(),
            "kind": match self {
                Self::Capability(_) => "capability",
                Self::UnknownCommand(_) => "unknown_command",
                Self::InvalidPayload(_) => "invalid_payload",
                Self::ModelDenied(_) | Self::ModelPermission(_, _) | Self::ReservedTable(_) => "model",
                Self::Db(_) => "db",
                Self::NotImplemented(_) => "not_implemented",
                Self::Message(_) => "error",
                Self::Denied(_) => "denied",
            }
        })
    }
}

impl From<crate::data_model::QueryError> for HostError {
    fn from(error: crate::data_model::QueryError) -> Self {
        Self::InvalidPayload(error.to_string())
    }
}
