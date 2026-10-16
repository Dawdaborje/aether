//! Organization admin HTTP API: create, list, profile, logo, members, ownership.

use std::net::SocketAddr;

use axum::{
    Extension, Json, Router,
    body::Body,
    extract::{ConnectInfo, Path, Query, State},
    http::{HeaderMap, StatusCode, Uri, header},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};

use aether_orm::find_session_by_token;
use surrealdb::types::ToSql;

use crate::access::{
    audit::Actor,
    identity::{IdentityError, SESSION_COOKIE, identify, with_retry_after},
};
use crate::app_dir::AppDir;
use crate::application::settings_api::{require_active_user, require_session};
use crate::magic_link::{self, CreateMagicLink, PURPOSE_PASSWORD_RESET};
use crate::member_mail;
use crate::org_admin::{
    FirstMember, OrganizationError, OrganizationRequest, OrganizationUpdate, StorageTarget,
    add_member, can_manage_admin_role, can_manage_users, clear_logo as clear_org_logo,
    create_org_role, create_organization, create_user, delete_org_role, get_profile, grant_member_role,
    invite_member, is_owner_or_developer, list_members, list_ownership_transfers,
    list_platform_users, list_profiles, read_logo, revoke_member_role, set_logo, set_member_active,
    set_member_password, set_org_developer, soft_kick_member, transfer_ownership, update_org_role,
    update_profile,
};
use crate::roles;
use crate::state::AppState;

const MAX_NAME: usize = 100;
const MAX_IDENTIFIER: usize = 48;
const MIN_PASSWORD: usize = 8;
const MAX_PASSWORD: usize = 128;
const MAX_DESCRIPTION: usize = 4000;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/ui/users", get(platform_users).post(create_platform_user))
        .route("/api/ui/organizations", get(list).post(create))
        .route("/api/ui/organizations/transfer-ownership", post(transfer))
        .route("/api/ui/organizations/{db}", get(get_one).patch(update_one))
        .route(
            "/api/ui/organizations/{db}/logo",
            get(get_logo).post(upload_logo).delete(clear_logo),
        )
        .route(
            "/api/ui/organizations/{db}/members",
            get(members).post(add_org_member),
        )
        .route(
            "/api/ui/organizations/{db}/plugin-activity",
            get(plugin_activity_log),
        )
        .route(
            "/api/ui/organizations/{db}/roles",
            get(org_roles).post(create_role),
        )
        .route(
            "/api/ui/organizations/{db}/roles/{role}",
            axum::routing::patch(patch_role).delete(delete_role),
        )
        .route(
            "/api/ui/organizations/{db}/members/{user_id}",
            axum::routing::patch(patch_member).delete(kick_member),
        )
        .route(
            "/api/ui/organizations/{db}/members/{user_id}/password",
            post(set_password),
        )
        .route(
            "/api/ui/organizations/{db}/members/{user_id}/password-reset",
            post(send_password_reset),
        )
        .route(
            "/api/ui/organizations/{db}/members/{user_id}/roles",
            post(grant_role),
        )
        .route(
            "/api/ui/organizations/{db}/members/{user_id}/roles/{role}",
            delete(revoke_role),
        )
        .route(
            "/api/ui/organizations/{db}/members/{user_id}/developer",
            axum::routing::put(grant_developer).delete(revoke_developer),
        )
        .route(
            "/api/ui/organizations/{db}/ownership-transfers",
            get(ownership_transfers),
        )
}

#[derive(Debug, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
enum MemberBody {
    New {
        username: String,
        email: String,
        password: String,
    },
    Existing {
        login: String,
    },
    Invite {
        email: String,
        username: Option<String>,
    },
}

#[derive(Debug, Deserialize)]
struct MemberPatchBody {
    is_active: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct PasswordBody {
    password: String,
}

#[derive(Debug, Deserialize)]
struct CreateBody {
    name: String,
    /// The organization's contact address (required).
    email: String,
    identifier: Option<String>,
    description: Option<String>,
    parent: Option<String>,
    /// Omitted to create the organization with no member yet.
    #[serde(default)]
    member: Option<MemberBody>,
}

#[derive(Debug, Deserialize)]
struct UpdateBody {
    /// When set, replaces the display name.
    name: Option<String>,
    /// When set, replaces the organization's email.
    #[serde(default)]
    email: Option<String>,
    /// When the key is present, replaces description (`""` clears it).
    #[serde(default)]
    description: Option<String>,
    /// When the key is present, replaces parent (`""` clears it).
    #[serde(default)]
    parent: Option<String>,
    /// Which optional fields the client meant to set (so `null`/empty can clear).
    #[serde(default)]
    set: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct LogoBody {
    /// Original filename (used for the extension).
    filename: String,
    /// Raw base64, or a `data:*;base64,…` URL.
    data: String,
}

#[derive(Debug, Deserialize)]
struct TransferBody {
    organization: Option<String>,
    to: String,
    note: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RoleBody {
    role: String,
}

#[derive(Debug, Deserialize)]
struct CreateRoleBody {
    /// Slug (`shift_lead`) or full name (`custom.shift_lead`).
    name: String,
    label: String,
    #[serde(default)]
    description: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PatchRoleBody {
    #[serde(default)]
    label: Option<String>,
    /// When the key is present, replaces description (`null`/empty clears).
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    set: Vec<String>,
}

struct ActorSession {
    user_id: String,
    is_developer: bool,
}

fn failure(status: StatusCode, message: impl AsRef<str>) -> Response {
    (status, Json(json!({ "error": message.as_ref() }))).into_response()
}

fn map_org_error(error: OrganizationError) -> Response {
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
        OrganizationError::UserNotFound(login) => {
            (StatusCode::NOT_FOUND, format!("No user `{login}` exists."))
        }
        OrganizationError::OrganizationNotFound(org) => (
            StatusCode::NOT_FOUND,
            format!("Organization `{org}` was not found."),
        ),
        OrganizationError::ParentNotFound(org) => (
            StatusCode::BAD_REQUEST,
            format!("Parent organization `{org}` was not found."),
        ),
        OrganizationError::ParentIsSelf => (
            StatusCode::BAD_REQUEST,
            "An organization cannot be its own parent.".into(),
        ),
        OrganizationError::ParentCycle => (
            StatusCode::BAD_REQUEST,
            "That parent would create a cycle.".into(),
        ),
        OrganizationError::InvalidLogo(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
        OrganizationError::EmptyOrganizationName
        | OrganizationError::InvalidOrganizationEmail
        | OrganizationError::MissingUserCredentials => {
            (StatusCode::BAD_REQUEST, error.to_string())
        }
        OrganizationError::NotOwner => (
            StatusCode::FORBIDDEN,
            "Only the organization owner can transfer ownership.".into(),
        ),
        OrganizationError::AlreadyOwner(_) => (
            StatusCode::CONFLICT,
            "That person is already the owner.".into(),
        ),
        OrganizationError::NotAMember(login, org) => (
            StatusCode::BAD_REQUEST,
            format!("`{login}` must be a member of `{org}` before becoming owner."),
        ),
        OrganizationError::Forbidden => (
            StatusCode::FORBIDDEN,
            "You cannot manage users in this organization.".into(),
        ),
        OrganizationError::CannotManageAdminRole => (
            StatusCode::FORBIDDEN,
            "Only an administrator or developer may grant or revoke the administrator role.".into(),
        ),
        OrganizationError::OwnerOrDeveloperRequired => (
            StatusCode::FORBIDDEN,
            "Only the organization owner or a developer may do that.".into(),
        ),
        OrganizationError::CannotTargetOwner => (
            StatusCode::FORBIDDEN,
            "The organization owner cannot be removed or deactivated.".into(),
        ),
        OrganizationError::CannotTargetSelf => (
            StatusCode::FORBIDDEN,
            "You cannot perform that action on your own account.".into(),
        ),
        OrganizationError::UserExists(name) => (
            StatusCode::CONFLICT,
            format!("A user called `{name}` (or with that email) already exists."),
        ),
        OrganizationError::Role(message) => {
            let lower = message.to_lowercase();
            let status = if lower.contains("already exists") {
                StatusCode::CONFLICT
            } else if lower.contains("no role") {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::BAD_REQUEST
            };
            (status, message.clone())
        }
        other => {
            log::error!("organization API failed: {other}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "The request failed. See the server log.".into(),
            )
        }
    };
    failure(status, message)
}

/// Drop Mantle membership (and org roles) for a user identified by username or email.
async fn bust_membership_warm(
    state: &AppState,
    db: &surrealdb::Surreal<surrealdb::engine::remote::ws::Client>,
    namespace: &str,
    org_db: &str,
    login: &str,
) {
    let _ = db.use_ns(namespace).await;
    let _ = db.use_db("core").await;
    let Ok(response) = db
        .query(
            "SELECT VALUE <string> id FROM users WHERE username = $login OR email = $login LIMIT 1;",
        )
        .bind(("login", login.to_string()))
        .await
    else {
        return;
    };
    let Ok(mut response) = response.check() else {
        return;
    };
    let Ok(ids) = response.take::<Vec<String>>(0) else {
        return;
    };
    let Some(user_id) = ids.into_iter().next() else {
        return;
    };
    let _ = state.cache.warm_invalidate_memberships(&user_id);
    let _ = state.cache.warm_invalidate_roles(org_db, &user_id);
}

/// Any logged-in active user (not developer-only).
async fn require_user(state: &AppState, headers: &HeaderMap) -> Result<ActorSession, Response> {
    let cookie = headers
        .get(axum::http::header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let token = cookie.split(';').find_map(|part| {
        let part = part.trim();
        part.strip_prefix(&format!("{SESSION_COOKIE}="))
            .map(str::to_string)
            .filter(|s| !s.is_empty())
    });
    let Some(token) = token else {
        return Err(failure(StatusCode::UNAUTHORIZED, "not authenticated"));
    };
    let core = state.core().await.map_err(|err| {
        log::error!("user management auth core database: {err}");
        failure(StatusCode::INTERNAL_SERVER_ERROR, "database error")
    })?;
    match find_session_by_token(&core, &token).await {
        Ok(Some(session)) if session.user.is_active => Ok(ActorSession {
            user_id: format!(
                "{}:{}",
                session.user.id.table.as_str(),
                session.user.id.key.to_sql()
            ),
            is_developer: session.user.is_super_user,
        }),
        Ok(Some(_)) => Err(failure(StatusCode::FORBIDDEN, "user inactive")),
        Ok(None) => Err(failure(StatusCode::UNAUTHORIZED, "invalid session")),
        Err(err) => {
            log::error!("user management auth: {err}");
            Err(failure(StatusCode::INTERNAL_SERVER_ERROR, "database error"))
        }
    }
}

async fn require_can_manage(
    state: &AppState,
    headers: &HeaderMap,
    db_name: &str,
) -> Result<ActorSession, Response> {
    let actor = require_user(state, headers).await?;
    let session = state.fresh_session();
    match can_manage_users(
        &session,
        &state.namespace,
        db_name,
        &actor.user_id,
        actor.is_developer,
    )
    .await
    {
        Ok(true) => Ok(actor),
        Ok(false) => Err(map_org_error(OrganizationError::Forbidden)),
        Err(error) => Err(map_org_error(error)),
    }
}

fn problem(body: &CreateBody) -> Option<String> {
    let name = body.name.trim();
    if name.is_empty() || name.chars().count() > MAX_NAME {
        return Some(format!("The name must be 1 to {MAX_NAME} characters."));
    }
    let email = body.email.trim();
    if !email.contains('@') || email.chars().any(char::is_whitespace) {
        return Some("Enter the organization's email address.".into());
    }
    if let Some(identifier) = &body.identifier
        && identifier.chars().count() > MAX_IDENTIFIER
    {
        return Some(format!(
            "The identifier must be at most {MAX_IDENTIFIER} characters."
        ));
    }
    if let Some(description) = &body.description
        && description.chars().count() > MAX_DESCRIPTION
    {
        return Some(format!(
            "The description must be at most {MAX_DESCRIPTION} characters."
        ));
    }
    match body.member.as_ref() {
        None => {}
        Some(MemberBody::Invite { email, username }) => {
            if !email.contains('@') || email.chars().any(char::is_whitespace) {
                return Some("Enter a valid email address.".into());
            }
            if let Some(username) = username
                && !username.trim().is_empty()
                && (username.trim().len() < 3 || username.chars().any(char::is_whitespace))
            {
                return Some("The username must be at least 3 characters, with no spaces.".into());
            }
        }
        Some(MemberBody::New {
            username,
            email,
            password,
        }) => {
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
        Some(MemberBody::Existing { login }) => {
            if login.trim().is_empty() {
                return Some("Enter the username or email of an existing user.".into());
            }
        }
    }
    None
}

async fn list(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err((status, response)) = require_session(&state, &headers).await {
        return (status, response).into_response();
    }
    let session = state.fresh_session();
    match list_profiles(&session, &state.namespace).await {
        Ok(organizations) => Json(json!({ "organizations": organizations })).into_response(),
        Err(error) => map_org_error(error),
    }
}

async fn get_one(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(db): Path<String>,
) -> Response {
    if let Err((status, response)) = require_session(&state, &headers).await {
        return (status, response).into_response();
    }
    let session = state.fresh_session();
    match get_profile(&session, &state.namespace, &db).await {
        Ok(organization) => Json(json!({ "organization": organization })).into_response(),
        Err(error) => map_org_error(error),
    }
}

async fn update_one(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(db): Path<String>,
    Json(body): Json<UpdateBody>,
) -> Response {
    if let Err((status, response)) = require_session(&state, &headers).await {
        return (status, response).into_response();
    }
    if let Some(name) = &body.name {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > MAX_NAME {
            return failure(
                StatusCode::BAD_REQUEST,
                format!("The name must be 1 to {MAX_NAME} characters."),
            );
        }
    }
    if let Some(description) = &body.description
        && description.chars().count() > MAX_DESCRIPTION
    {
        return failure(
            StatusCode::BAD_REQUEST,
            format!("The description must be at most {MAX_DESCRIPTION} characters."),
        );
    }

    let set_description = body.set.iter().any(|f| f == "description") || body.description.is_some();
    let set_parent = body.set.iter().any(|f| f == "parent") || body.parent.is_some();
    let name = body
        .name
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty());
    let description = if set_description {
        Some(
            body.description
                .as_deref()
                .map(str::trim)
                .filter(|v| !v.is_empty()),
        )
    } else {
        None
    };
    let parent = if set_parent {
        Some(
            body.parent
                .as_deref()
                .map(str::trim)
                .filter(|v| !v.is_empty()),
        )
    } else {
        None
    };

    let session = state.fresh_session();
    match update_profile(
        &session,
        &state.namespace,
        &db,
        OrganizationUpdate {
            name,
            email: body.email.as_deref().map(str::trim).filter(|v| !v.is_empty()),
            description,
            parent_db_name: parent,
        },
    )
    .await
    {
        Ok(organization) => Json(json!({ "organization": organization })).into_response(),
        Err(error) => map_org_error(error),
    }
}

async fn upload_logo(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(db): Path<String>,
    Json(body): Json<LogoBody>,
) -> Response {
    if let Err((status, response)) = require_session(&state, &headers).await {
        return (status, response).into_response();
    }
    let session = state.fresh_session();
    let app_dir = AppDir::new(&state.config.app_dir);
    match set_logo(
        &session,
        &state.namespace,
        &app_dir,
        &db,
        body.filename.trim(),
        &body.data,
    )
    .await
    {
        Ok(organization) => Json(json!({ "organization": organization })).into_response(),
        Err(error) => map_org_error(error),
    }
}

async fn clear_logo(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(db): Path<String>,
) -> Response {
    if let Err((status, response)) = require_session(&state, &headers).await {
        return (status, response).into_response();
    }
    let session = state.fresh_session();
    let app_dir = AppDir::new(&state.config.app_dir);
    match clear_org_logo(&session, &state.namespace, &app_dir, &db).await {
        Ok(organization) => Json(json!({ "organization": organization })).into_response(),
        Err(error) => map_org_error(error),
    }
}

async fn get_logo(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(db): Path<String>,
) -> Response {
    if let Err((status, response)) = require_active_user(&state, &headers).await {
        return (status, response).into_response();
    }
    let session = state.fresh_session();
    let app_dir = AppDir::new(&state.config.app_dir);
    match read_logo(&session, &state.namespace, &app_dir, &db).await {
        Ok(Some((bytes, content_type))) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, content_type)
            .header(header::CACHE_CONTROL, "private, max-age=300")
            .body(Body::from(bytes))
            .unwrap_or_else(|_| failure(StatusCode::INTERNAL_SERVER_ERROR, "response error")),
        Ok(None) => failure(StatusCode::NOT_FOUND, "No logo for this organization."),
        Err(error) => map_org_error(error),
    }
}

#[derive(Debug, Deserialize)]
struct PlatformUserBody {
    username: String,
    email: String,
    password: String,
}

/// Create a platform user that belongs to no organization yet (developers only; the user is
/// added to organizations from the Organizations page or by invitation).
async fn create_platform_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<PlatformUserBody>,
) -> Response {
    let actor = match require_user(&state, &headers).await {
        Ok(actor) => actor,
        Err(response) => return response,
    };
    if !actor.is_developer {
        return failure(
            StatusCode::FORBIDDEN,
            "Only a developer can create platform users.",
        );
    }
    let username = body.username.trim();
    let email = body.email.trim();
    if username.chars().count() < 3 || username.chars().any(char::is_whitespace) {
        return failure(
            StatusCode::BAD_REQUEST,
            "The username must be at least 3 characters, with no spaces.",
        );
    }
    if !email.contains('@') || email.chars().any(char::is_whitespace) {
        return failure(StatusCode::BAD_REQUEST, "Enter a valid email address.");
    }
    let length = body.password.chars().count();
    if !(MIN_PASSWORD..=MAX_PASSWORD).contains(&length) {
        return failure(
            StatusCode::BAD_REQUEST,
            format!("The password must be {MIN_PASSWORD} to {MAX_PASSWORD} characters."),
        );
    }
    let session = state.fresh_session();
    if let Err(error) = create_user(&session, &state.namespace, username, email, &body.password).await {
        return map_org_error(error);
    }
    match list_platform_users(&session, &state.namespace).await {
        Ok(users) => Json(json!({ "ok": true, "users": users })).into_response(),
        Err(error) => map_org_error(error),
    }
}

async fn platform_users(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err((status, response)) = require_session(&state, &headers).await {
        return (status, response).into_response();
    }
    let session = state.fresh_session();
    match list_platform_users(&session, &state.namespace).await {
        Ok(users) => Json(json!({ "users": users })).into_response(),
        Err(error) => map_org_error(error),
    }
}

#[derive(Debug, Deserialize)]
struct ActivityQuery {
    #[serde(default)]
    limit: Option<u32>,
    #[serde(default)]
    offset: Option<u32>,
}

/// Who installed or upgraded which plugin here, and who loaded plugins into the catalog.
async fn plugin_activity_log(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(db): Path<String>,
    Query(query): Query<ActivityQuery>,
) -> Response {
    if let Err(response) = require_can_manage(&state, &headers, &db).await {
        return response;
    }
    match crate::plugin_activity::list_for_org(
        &state,
        &db,
        query.limit.unwrap_or(100),
        query.offset.unwrap_or(0),
    )
    .await
    {
        Ok(body) => Json(body).into_response(),
        Err(error) => {
            log::error!("plugin activity of `{db}`: {error}");
            failure(StatusCode::INTERNAL_SERVER_ERROR, "database error")
        }
    }
}

async fn members(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(db): Path<String>,
) -> Response {
    if let Err(response) = require_can_manage(&state, &headers, &db).await {
        return response;
    }
    let session = state.fresh_session();
    match list_members(&session, &state.namespace, &db).await {
        Ok(members) => Json(json!({ "members": members })).into_response(),
        Err(error) => map_org_error(error),
    }
}

async fn add_org_member(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(db): Path<String>,
    Json(body): Json<MemberBody>,
) -> Response {
    let actor = match require_can_manage(&state, &headers, &db).await {
        Ok(actor) => actor,
        Err(response) => return response,
    };
    let session = state.fresh_session();
    match &body {
        MemberBody::Invite { email, username } => {
            if !email.contains('@') || email.chars().any(char::is_whitespace) {
                return failure(StatusCode::BAD_REQUEST, "Enter a valid email address.");
            }
            let invited = invite_member(
                &session,
                &state.namespace,
                &db,
                email.trim(),
                username.as_deref(),
            )
            .await;
            let (login_name, temporary_password) = match invited {
                Ok(pair) => pair,
                Err(error) => return map_org_error(error),
            };
            bust_membership_warm(&state, &session, &state.namespace, &db, &login_name).await;
            let mut email_sent = false;
            let mut email_error = None;
            if !temporary_password.is_empty() {
                let profile = get_profile(&session, &state.namespace, &db).await;
                let org_name = profile
                    .as_ref()
                    .map(|p| p.name.clone())
                    .unwrap_or_else(|_| db.clone());
                let base = member_mail::public_base_for(&state, &db).await;
                let login_url = format!("{base}/login");
                let (subject, plain, html) = member_mail::invite_bodies(
                    &org_name,
                    &login_name,
                    &temporary_password,
                    &login_url,
                );
                let mail = member_mail::queue_email(
                    &state,
                    &db,
                    email.trim(),
                    &subject,
                    &plain,
                    &html,
                    Some(&actor.user_id),
                )
                .await;
                member_mail::log_mail("invite", &mail);
                email_sent = mail.email_sent;
                email_error = mail.email_error;
            }
            match list_members(&session, &state.namespace, &db).await {
                Ok(members) => Json(json!({
                    "ok": true,
                    "members": members,
                    "temporary_password": temporary_password,
                    "username": login_name,
                    "email_sent": email_sent,
                    "email_error": email_error,
                }))
                .into_response(),
                Err(error) => map_org_error(error),
            }
        }
        MemberBody::New {
            username,
            email,
            password,
        } => {
            if username.trim().len() < 3 || username.chars().any(char::is_whitespace) {
                return failure(
                    StatusCode::BAD_REQUEST,
                    "The username must be at least 3 characters, with no spaces.",
                );
            }
            if !email.contains('@') || email.chars().any(char::is_whitespace) {
                return failure(StatusCode::BAD_REQUEST, "Enter a valid email address.");
            }
            let length = password.chars().count();
            if !(MIN_PASSWORD..=MAX_PASSWORD).contains(&length) {
                return failure(
                    StatusCode::BAD_REQUEST,
                    format!("The password must be {MIN_PASSWORD} to {MAX_PASSWORD} characters."),
                );
            }
            match add_member(
                &session,
                &state.namespace,
                &db,
                Some((username.trim(), email.trim(), password.as_str())),
                None,
            )
            .await
            {
                Ok(()) => {
                    bust_membership_warm(&state, &session, &state.namespace, &db, username.trim())
                        .await;
                    match list_members(&session, &state.namespace, &db).await {
                        Ok(members) => {
                            Json(json!({ "ok": true, "members": members })).into_response()
                        }
                        Err(error) => map_org_error(error),
                    }
                }
                Err(error) => map_org_error(error),
            }
        }
        MemberBody::Existing { login } => {
            if login.trim().is_empty() {
                return failure(
                    StatusCode::BAD_REQUEST,
                    "Enter the username or email of an existing user.",
                );
            }
            match add_member(&session, &state.namespace, &db, None, Some(login.trim())).await {
                Ok(()) => {
                    bust_membership_warm(&state, &session, &state.namespace, &db, login.trim())
                        .await;
                    match list_members(&session, &state.namespace, &db).await {
                        Ok(members) => {
                            Json(json!({ "ok": true, "members": members })).into_response()
                        }
                        Err(error) => map_org_error(error),
                    }
                }
                Err(error) => map_org_error(error),
            }
        }
    }
}

async fn patch_member(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((db, user_id)): Path<(String, String)>,
    Json(body): Json<MemberPatchBody>,
) -> Response {
    let actor = match require_can_manage(&state, &headers, &db).await {
        Ok(actor) => actor,
        Err(response) => return response,
    };
    let Some(is_active) = body.is_active else {
        return failure(StatusCode::BAD_REQUEST, "Provide is_active.");
    };
    let session = state.fresh_session();
    match set_member_active(
        &session,
        &state.namespace,
        &db,
        &user_id,
        is_active,
        &actor.user_id,
    )
    .await
    {
        Ok(()) => match list_members(&session, &state.namespace, &db).await {
            Ok(members) => Json(json!({ "ok": true, "members": members })).into_response(),
            Err(error) => map_org_error(error),
        },
        Err(error) => map_org_error(error),
    }
}

async fn kick_member(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((db, user_id)): Path<(String, String)>,
) -> Response {
    let actor = match require_user(&state, &headers).await {
        Ok(actor) => actor,
        Err(response) => return response,
    };
    let session = state.fresh_session();
    match soft_kick_member(
        &session,
        &state.namespace,
        &db,
        &user_id,
        &actor.user_id,
        actor.is_developer,
    )
    .await
    {
        Ok(()) => {
            let _ = state.cache.warm_invalidate_memberships(&user_id);
            let _ = state.cache.warm_invalidate_roles(&db, &user_id);
            match list_members(&session, &state.namespace, &db).await {
                Ok(members) => Json(json!({ "ok": true, "members": members })).into_response(),
                Err(error) => map_org_error(error),
            }
        }
        Err(error) => map_org_error(error),
    }
}

async fn set_password(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((db, user_id)): Path<(String, String)>,
    Json(body): Json<PasswordBody>,
) -> Response {
    let actor = match require_user(&state, &headers).await {
        Ok(actor) => actor,
        Err(response) => return response,
    };
    let length = body.password.chars().count();
    if !(MIN_PASSWORD..=MAX_PASSWORD).contains(&length) {
        return failure(
            StatusCode::BAD_REQUEST,
            format!("The password must be {MIN_PASSWORD} to {MAX_PASSWORD} characters."),
        );
    }
    let session = state.fresh_session();
    match set_member_password(
        &session,
        &state.namespace,
        &db,
        &user_id,
        &body.password,
        &actor.user_id,
        actor.is_developer,
    )
    .await
    {
        Ok(()) => Json(json!({ "ok": true })).into_response(),
        Err(error) => map_org_error(error),
    }
}

async fn send_password_reset(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((db, user_id)): Path<(String, String)>,
) -> Response {
    let actor = match require_user(&state, &headers).await {
        Ok(actor) => actor,
        Err(response) => return response,
    };
    let session = state.fresh_session();
    match is_owner_or_developer(
        &session,
        &state.namespace,
        &db,
        &actor.user_id,
        actor.is_developer,
    )
    .await
    {
        Ok(true) => {}
        Ok(false) => return map_org_error(OrganizationError::OwnerOrDeveloperRequired),
        Err(error) => return map_org_error(error),
    }
    let email = match member_mail::email_for_user(&session, &state.namespace, &user_id).await {
        Some(email) => email,
        None => {
            return failure(
                StatusCode::BAD_REQUEST,
                "That user has no email address for a reset link.",
            );
        }
    };
    let base = member_mail::public_base_for(&state, &db).await;
    let created = match magic_link::create(
        &session,
        &state.namespace,
        CreateMagicLink {
            purpose: PURPOSE_PASSWORD_RESET.into(),
            user_id: Some(user_id.clone()),
            email: Some(email.clone()),
            payload: None,
            plugin: None,
            org_db: Some(db.clone()),
            path: Some("/login/magic".into()),
            ttl_secs: Some(60 * 60),
            single_use: true,
            supersede: true,
            created_by: Some(actor.user_id.clone()),
            public_base: base,
        },
    )
    .await
    {
        Ok(created) => created,
        Err(error) => {
            log::error!("password-reset magic link: {error}");
            return failure(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Could not create reset link.",
            );
        }
    };
    let profile = get_profile(&session, &state.namespace, &db).await;
    let org_name = profile
        .as_ref()
        .map(|p| p.name.clone())
        .unwrap_or_else(|_| db.clone());
    let (subject, plain, html) =
        member_mail::reset_bodies(&org_name, &created.url, "expires in 60 minutes");
    let mail = member_mail::queue_email(
        &state,
        &db,
        &email,
        &subject,
        &plain,
        &html,
        Some(&actor.user_id),
    )
    .await;
    member_mail::log_mail("password-reset", &mail);
    Json(json!({
        "ok": true,
        "email_sent": mail.email_sent,
        "email_error": mail.email_error,
        "url": if mail.email_sent { Value::Null } else { json!(created.url) },
    }))
    .into_response()
}

/// A role with the plugin and workspace it comes from, so the people granting roles can narrow
/// a long list by the area they are working in.
#[derive(Debug, serde::Serialize)]
struct PlacedRole {
    #[serde(flatten)]
    role: roles::RoleSummary,
    #[serde(skip_serializing_if = "Option::is_none")]
    plugin: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    plugin_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    workspace: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    workspace_label: Option<String>,
}

/// Attach plugin and workspace to each role from the catalog. A role's plugin is the part of its
/// name before the dot; kernel and custom roles have none. A catalog that cannot be read leaves the
/// roles as they were.
async fn place_roles(state: &AppState, list: Vec<roles::RoleSummary>) -> Vec<PlacedRole> {
    use std::collections::HashMap;
    use surrealdb::types::SurrealValue;
    #[derive(Debug, Deserialize, SurrealValue)]
    struct PluginRow {
        name: String,
        label: Option<String>,
        workspace: Option<String>,
    }
    #[derive(Debug, Deserialize, SurrealValue)]
    struct WorkspaceRow {
        name: String,
        label: Option<String>,
        group_label: Option<String>,
    }
    let mut plugins: HashMap<String, (Option<String>, Option<String>)> = HashMap::new();
    let mut workspaces: HashMap<String, String> = HashMap::new();
    if let Ok(core) = state.core().await {
        if let Ok(mut response) = core.query("SELECT name, label, workspace FROM plugins;").await {
            if let Ok(rows) = response.take::<Vec<PluginRow>>(0) {
                for row in rows {
                    plugins.entry(row.name).or_insert((row.label, row.workspace));
                }
            }
        }
        if let Ok(mut response) =
            core.query("SELECT name, label, group_label FROM plugin_workspaces;").await
        {
            if let Ok(rows) = response.take::<Vec<WorkspaceRow>>(0) {
                for row in rows {
                    let label = row.group_label.or(row.label).unwrap_or_else(|| row.name.clone());
                    workspaces.insert(row.name, label);
                }
            }
        }
    }
    list.into_iter()
        .map(|role| {
            let plugin = role
                .name
                .split_once('.')
                .map(|(plugin, _)| plugin.to_string())
                .filter(|plugin| plugins.contains_key(plugin));
            let (plugin_label, workspace) = match &plugin {
                Some(name) => plugins.get(name).cloned().unwrap_or((None, None)),
                None => (None, None),
            };
            let workspace_label = workspace.as_ref().and_then(|w| workspaces.get(w).cloned());
            PlacedRole {
                role,
                plugin,
                plugin_label,
                workspace,
                workspace_label,
            }
        })
        .collect()
}

async fn org_roles(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(db): Path<String>,
) -> Response {
    if let Err(response) = require_can_manage(&state, &headers, &db).await {
        return response;
    }
    let session = state.fresh_session();
    match roles::list_in(&session, &state.namespace, &db).await {
        Ok(roles) => {
            let placed = place_roles(&state, roles).await;
            Json(json!({ "roles": placed })).into_response()
        }
        Err(error) => {
            log::error!("list org roles: {error}");
            failure(StatusCode::INTERNAL_SERVER_ERROR, "database error")
        }
    }
}

async fn create_role(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(db): Path<String>,
    Json(body): Json<CreateRoleBody>,
) -> Response {
    if let Err(response) = require_can_manage(&state, &headers, &db).await {
        return response;
    }
    let session = state.fresh_session();
    match create_org_role(
        &session,
        &state.namespace,
        &db,
        body.name.trim(),
        body.label.trim(),
        body.description
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty()),
    )
    .await
    {
        Ok(role) => Json(json!({ "ok": true, "role": role })).into_response(),
        Err(error) => map_org_error(error),
    }
}

async fn patch_role(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((db, role)): Path<(String, String)>,
    Json(body): Json<PatchRoleBody>,
) -> Response {
    if let Err(response) = require_can_manage(&state, &headers, &db).await {
        return response;
    }
    let role = role.trim();
    if role.is_empty() {
        return failure(StatusCode::BAD_REQUEST, "Enter a role name.");
    }
    let set_description = body.set.iter().any(|k| k == "description");
    let description = if set_description {
        Some(
            body.description
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty()),
        )
    } else {
        None
    };
    let session = state.fresh_session();
    match update_org_role(
        &session,
        &state.namespace,
        &db,
        role,
        body.label
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty()),
        description,
    )
    .await
    {
        Ok(()) => Json(json!({ "ok": true })).into_response(),
        Err(error) => map_org_error(error),
    }
}

async fn delete_role(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((db, role)): Path<(String, String)>,
) -> Response {
    if let Err(response) = require_can_manage(&state, &headers, &db).await {
        return response;
    }
    let role = role.trim();
    if role.is_empty() {
        return failure(StatusCode::BAD_REQUEST, "Enter a role name.");
    }
    let session = state.fresh_session();
    match delete_org_role(&session, &state.namespace, &db, role).await {
        Ok(()) => Json(json!({ "ok": true })).into_response(),
        Err(error) => map_org_error(error),
    }
}

async fn grant_role(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((db, user_id)): Path<(String, String)>,
    Json(body): Json<RoleBody>,
) -> Response {
    let actor = match require_can_manage(&state, &headers, &db).await {
        Ok(actor) => actor,
        Err(response) => return response,
    };
    let role = body.role.trim();
    if role.is_empty() {
        return failure(StatusCode::BAD_REQUEST, "Enter a role name.");
    }
    let session = state.fresh_session();
    let may_admin = match can_manage_admin_role(
        &session,
        &state.namespace,
        &db,
        &actor.user_id,
        actor.is_developer,
    )
    .await
    {
        Ok(v) => v,
        Err(error) => return map_org_error(error),
    };
    match grant_member_role(&session, &state.namespace, &db, &user_id, role, may_admin).await {
        Ok(()) => {
            let _ = state.cache.warm_invalidate_roles(&db, &user_id);
            Json(json!({ "ok": true })).into_response()
        }
        Err(error) => map_org_error(error),
    }
}

/// Make a member a developer of this organization (schedules), or stop that. Only an
/// unrestricted developer may: org owners and admins cannot hand out developer access.
async fn set_developer(
    state: &AppState,
    headers: &HeaderMap,
    db: &str,
    user_id: &str,
    on: bool,
) -> Response {
    let actor = match require_user(state, headers).await {
        Ok(actor) => actor,
        Err(response) => return response,
    };
    if !actor.is_developer {
        return failure(
            StatusCode::FORBIDDEN,
            "Only an unrestricted developer may give or take developer access.",
        );
    }
    let session = state.fresh_session();
    match set_org_developer(&session, &state.namespace, db, user_id, on).await {
        Ok(()) => Json(json!({ "ok": true, "is_developer": on })).into_response(),
        Err(OrganizationError::NotAMember(_, _)) => failure(
            StatusCode::NOT_FOUND,
            "That person is not a member of this organization.",
        ),
        Err(error) => map_org_error(error),
    }
}

async fn grant_developer(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((db, user_id)): Path<(String, String)>,
) -> Response {
    set_developer(&state, &headers, &db, &user_id, true).await
}

async fn revoke_developer(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((db, user_id)): Path<(String, String)>,
) -> Response {
    set_developer(&state, &headers, &db, &user_id, false).await
}

async fn revoke_role(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((db, user_id, role)): Path<(String, String, String)>,
) -> Response {
    let actor = match require_can_manage(&state, &headers, &db).await {
        Ok(actor) => actor,
        Err(response) => return response,
    };
    let role = role.trim();
    if role.is_empty() {
        return failure(StatusCode::BAD_REQUEST, "Enter a role name.");
    }
    let session = state.fresh_session();
    let may_admin = match can_manage_admin_role(
        &session,
        &state.namespace,
        &db,
        &actor.user_id,
        actor.is_developer,
    )
    .await
    {
        Ok(v) => v,
        Err(error) => return map_org_error(error),
    };
    match revoke_member_role(&session, &state.namespace, &db, &user_id, role, may_admin).await {
        Ok(()) => {
            let _ = state.cache.warm_invalidate_roles(&db, &user_id);
            Json(json!({ "ok": true })).into_response()
        }
        Err(error) => map_org_error(error),
    }
}

async fn ownership_transfers(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(db): Path<String>,
) -> Response {
    if let Err((status, response)) = require_session(&state, &headers).await {
        return (status, response).into_response();
    }
    let session = state.fresh_session();
    match list_ownership_transfers(&session, &state.namespace, &db).await {
        Ok(transfers) => Json(json!({ "transfers": transfers })).into_response(),
        Err(error) => map_org_error(error),
    }
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
    let member = match body.member.as_ref() {
        None => FirstMember::None,
        Some(MemberBody::New {
            username,
            email,
            password,
        }) => FirstMember::User {
            username: username.trim(),
            email: email.trim(),
            password,
        },
        Some(MemberBody::Existing { login }) => FirstMember::Existing {
            login: login.trim(),
        },
        Some(MemberBody::Invite { .. }) => {
            return failure(
                StatusCode::BAD_REQUEST,
                "Creating an organization needs a new or existing first member, not an invite.",
            );
        }
    };
    let identifier = body
        .identifier
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let request = OrganizationRequest {
        name: body.name.trim(),
        email: body.email.trim(),
        db_name: identifier,
        member,
        description: body.description.as_deref(),
        parent_db_name: body.parent.as_deref(),
    };

    let session = state.fresh_session();
    match create_organization(&session, &state.namespace, &request, &storage).await {
        Ok((db_name, org_storage)) => {
            log::info!("organization `{db_name}` created from the web app");
            crate::access::org_databases::invalidate(&state.cache);
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
        Err(error) => map_org_error(error),
    }
}

async fn transfer(
    State(state): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    uri: Uri,
    Json(body): Json<TransferBody>,
) -> Response {
    let peer_ip = peer.map(|Extension(ConnectInfo(address))| address.ip());
    let identity = match identify(&state, &headers, peer_ip, &uri).await {
        Ok(identity) => identity,
        Err(error) => {
            if matches!(error, IdentityError::Database(_) | IdentityError::Audit(_)) {
                log::error!("transfer ownership identity: {error}");
            }
            let (status, message) = error.http();
            return with_retry_after(failure(status, message));
        }
    };
    let Actor::User(actor_id) = &identity.actor else {
        return failure(
            if identity.foreign_user {
                StatusCode::FORBIDDEN
            } else {
                StatusCode::UNAUTHORIZED
            },
            if identity.foreign_user {
                "not a member of this organization"
            } else {
                "not authenticated"
            },
        );
    };

    let to = body.to.trim();
    if to.is_empty() {
        return failure(
            StatusCode::BAD_REQUEST,
            "Enter the username or email of the new owner.",
        );
    }
    let organization = body
        .organization
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .unwrap_or(identity.org_db.as_str());

    let session = state.fresh_session();
    match transfer_ownership(
        &session,
        &state.namespace,
        organization,
        actor_id,
        to,
        body.note.as_deref(),
    )
    .await
    {
        Ok(()) => Json(json!({
            "ok": true,
            "organization": organization,
            "owner": to,
        }))
        .into_response(),
        Err(error) => map_org_error(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_user(username: &str, email: &str, password: &str) -> CreateBody {
        CreateBody {
            name: "Acme".into(),
            email: "hello@acme.io".into(),
            identifier: None,
            description: None,
            parent: None,
            member: Some(MemberBody::New {
                username: username.into(),
                email: email.into(),
                password: password.into(),
            }),
        }
    }

    #[test]
    fn a_request_without_a_member_has_no_problem() {
        let mut body = new_user("", "", "");
        body.member = None;
        assert_eq!(problem(&body), None);
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
            email: "hello@acme.io".into(),
            identifier: None,
            description: None,
            parent: None,
            member: Some(MemberBody::Existing { login: " ".into() }),
        };
        assert!(problem(&existing).is_some());
    }
}
