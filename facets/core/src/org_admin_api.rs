//! `POST /api/ui/organizations`: create an organization from the web app.
//!
//! Developer-only. It does what `aether --create-org` does: the organization's folder and
//! media location, its database with the full schema, its records in the core database,
//! and its first member.

use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
};
use serde::Deserialize;
use serde_json::json;

use crate::app_dir::AppDir;
use crate::application::settings_api::require_session;
use crate::org_admin::{
    FirstMember, OrganizationError, OrganizationRequest, StorageTarget, create_organization,
};
use crate::state::AppState;

const MAX_NAME: usize = 100;
const MAX_IDENTIFIER: usize = 48;
const MIN_PASSWORD: usize = 8;
const MAX_PASSWORD: usize = 128;

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/ui/organizations", post(create))
}

#[derive(Debug, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
enum MemberBody {
    /// Create a user (or reuse the one with this username or email).
    New {
        username: String,
        email: String,
        password: String,
    },
    /// Add a user who already exists.
    Existing { login: String },
}

#[derive(Debug, Deserialize)]
struct CreateBody {
    name: String,
    /// The database name; made from the name when left out.
    identifier: Option<String>,
    member: MemberBody,
}

fn failure(status: StatusCode, message: impl AsRef<str>) -> Response {
    (status, Json(json!({ "error": message.as_ref() }))).into_response()
}

/// What is wrong with the request, in words for the person filling in the form.
fn problem(body: &CreateBody) -> Option<String> {
    let name = body.name.trim();
    if name.is_empty() || name.chars().count() > MAX_NAME {
        return Some(format!("The name must be 1 to {MAX_NAME} characters."));
    }
    if let Some(identifier) = &body.identifier
        && identifier.chars().count() > MAX_IDENTIFIER
    {
        return Some(format!("The identifier must be at most {MAX_IDENTIFIER} characters."));
    }
    match &body.member {
        MemberBody::New { username, email, password } => {
            if username.trim().len() < 3 || username.chars().any(char::is_whitespace) {
                return Some("The username must be at least 3 characters, with no spaces.".into());
            }
            if !email.contains('@') || email.chars().any(char::is_whitespace) {
                return Some("Enter a valid email address.".into());
            }
            let length = password.chars().count();
            if !(MIN_PASSWORD..=MAX_PASSWORD).contains(&length) {
                return Some(format!(
                    "The password must be {MIN_PASSWORD} to {MAX_PASSWORD} characters."
                ));
            }
        }
        MemberBody::Existing { login } => {
            if login.trim().is_empty() {
                return Some("Enter the username or email of an existing user.".into());
            }
        }
    }
    None
}

async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<CreateBody>,
) -> Response {
    if let Err((status, response)) = require_session(&state, &headers).await {
        return (status, response).into_response();
    }
    if let Some(message) = problem(&body) {
        return failure(StatusCode::BAD_REQUEST, message);
    }

    let storage = StorageTarget {
        app_dir: AppDir::new(&state.config.app_dir),
        media: state.media.clone(),
    };
    let member = match &body.member {
        MemberBody::New { username, email, password } => FirstMember::User {
            username: username.trim(),
            email: email.trim(),
            password,
        },
        MemberBody::Existing { login } => FirstMember::Existing { login: login.trim() },
    };
    let identifier = body.identifier.as_deref().map(str::trim).filter(|value| !value.is_empty());
    let request = OrganizationRequest {
        name: body.name.trim(),
        db_name: identifier,
        member,
    };

    // Creating selects databases itself, so it gets a session of its own.
    let session = state.fresh_session();
    match create_organization(&session, &state.namespace, &request, &storage).await {
        Ok((db_name, org_storage)) => {
            log::info!("organization `{db_name}` created from the web app");
            (
                StatusCode::CREATED,
                Json(json!({
                    "organization": { "name": body.name.trim(), "db_name": db_name },
                    "files": org_storage.directory.display().to_string(),
                    "media": org_storage.media_prefix,
                })),
            )
                .into_response()
        }
        Err(error) => {
            let (status, message) = match &error {
                OrganizationError::AlreadyExists(name) => (
                    StatusCode::CONFLICT,
                    format!("An organization with the identifier `{name}` already exists."),
                ),
                OrganizationError::InvalidDatabaseName(name) => (
                    StatusCode::BAD_REQUEST,
                    format!(
                        "`{name}` cannot be an identifier: use letters, digits and underscores, and not `core`."
                    ),
                ),
                OrganizationError::UserNotFound(login) => (
                    StatusCode::NOT_FOUND,
                    format!("No user `{login}` exists."),
                ),
                OrganizationError::EmptyOrganizationName
                | OrganizationError::MissingUserCredentials => {
                    (StatusCode::BAD_REQUEST, error.to_string())
                }
                other => {
                    log::error!("creating an organization from the web app failed: {other}");
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "The organization could not be created. See the server log.".to_string(),
                    )
                }
            };
            failure(status, message)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_user(username: &str, email: &str, password: &str) -> CreateBody {
        CreateBody {
            name: "Acme".into(),
            identifier: None,
            member: MemberBody::New {
                username: username.into(),
                email: email.into(),
                password: password.into(),
            },
        }
    }

    #[test]
    fn a_complete_request_has_no_problem() {
        assert_eq!(problem(&new_user("ada", "ada@acme.io", "longenough")), None);
    }

    #[test]
    fn explains_what_is_wrong() {
        assert!(problem(&new_user("ad", "ada@acme.io", "longenough")).is_some());
        assert!(problem(&new_user("a da", "ada@acme.io", "longenough")).is_some());
        assert!(problem(&new_user("ada", "not-an-email", "longenough")).is_some());
        assert!(problem(&new_user("ada", "ada@acme.io", "short")).is_some());
        let mut unnamed = new_user("ada", "ada@acme.io", "longenough");
        unnamed.name = "  ".into();
        assert!(problem(&unnamed).is_some());
        let existing = CreateBody {
            name: "Acme".into(),
            identifier: None,
            member: MemberBody::Existing { login: " ".into() },
        };
        assert!(problem(&existing).is_some());
    }
}
