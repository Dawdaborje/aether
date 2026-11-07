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

    /// A value the model itself refuses (wrong type, too long, not an option, missing). Reads like
    /// `InvalidPayload` everywhere, but its text is written for people ("task.due: must be a date
    /// like 2026-10-03"), so a script's caller is shown it, where a script's own mistake stays internal.
    #[error("invalid payload: {0}")]
    Refused(String),

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
        use crate::data_model::SchemaError;
        match error {
            // A field the model does not have is the plugin author's mistake, not the person's.
            SchemaError::UnknownField { .. } => Self::InvalidPayload(error.to_string()),
            SchemaError::InvalidValue { .. } | SchemaError::Required { .. } => {
                Self::Refused(error.to_string())
            }
        }
    }
}

impl HostError {
    /// The message a person may be shown when this is why their request failed, without the
    /// `invalid payload:` prefix a program would want.
    pub fn for_the_person(&self) -> Option<String> {
        match self {
            Self::Denied(_) => Some(self.to_string()),
            Self::Refused(reason) => Some(reason.clone()),
            _ => None,
        }
    }
}

impl HostError {
    pub fn to_json(&self) -> JsonValue {
        serde_json::json!({
            // A refusal of a value carries the person's wording alone, without the `invalid payload:` prefix.
            "error": match self {
                Self::Refused(reason) => reason.clone(),
                _ => self.to_string(),
            },
            "kind": match self {
                Self::Capability(_) => "capability",
                Self::UnknownCommand(_) => "unknown_command",
                Self::InvalidPayload(_) => "invalid_payload",
                Self::Refused(_) => "refused",
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refused_value_reaches_a_program_as_the_persons_wording_with_its_own_kind() {
        let json = HostError::Refused("task.due: must be a date like 2026-10-03".into()).to_json();
        assert_eq!(json["kind"], "refused");
        assert_eq!(json["error"], "task.due: must be a date like 2026-10-03");
        // A field the model does not have stays the author's mistake.
        let json = HostError::InvalidPayload("task: no field `x`".into()).to_json();
        assert_eq!(json["kind"], "invalid_payload");
        assert_eq!(HostError::Denied("no".into()).to_json()["kind"], "denied");
    }
}
