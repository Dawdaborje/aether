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

use aether_core::access::{
    audit::Actor,
    identity::{IdentityError, SESSION_COOKIE, identify, with_retry_after},
};
use aether_core::app_dir::AppDir;
use aether_core::application::settings_api::{require_active_user, require_session};
use aether_core::magic_link::{self, CreateMagicLink, PURPOSE_PASSWORD_RESET};
use aether_core::member_mail;
use aether_core::org_admin::{
    OrganizationError, OrganizationUpdate,
    add_member, assign_user, can_manage_admin_role, has_permission, clear_logo as clear_org_logo,
    create_org_role, create_user, delete_org_role, get_profile,
    grant_member_role, invite_member, is_owner_or_developer, list_members,
    list_ownership_transfers, list_platform_users, list_profiles, read_logo, revoke_member_role,
    set_logo, set_member_active, set_member_password, set_org_developer, soft_kick_member,
    transfer_ownership, update_org_role, update_profile,
};
use aether_core::roles;
use aether_core::state::{AppState, Databases};

const MAX_NAME: usize = 100;
const MIN_PASSWORD: usize = 8;
const MAX_PASSWORD: usize = 128;
const MAX_DESCRIPTION: usize = 4000;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/ui/users",
            get(platform_users).post(create_platform_user),
        )
        .route("/api/ui/organizations", get(list))
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
            "/api/ui/organizations/{db}/roles/{role}/permissions",
            get(role_permissions),
        )
        .route(
            "/api/ui/organizations/{db}/roles/{role}/kernel-permissions",
            get(role_kernel_permissions_get).put(role_kernel_permissions_put),
        )
        .route(
            "/api/ui/organizations/{db}/roles/{role}/record-grants",
            get(role_record_grants_get).put(role_record_grants_put),
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
struct UpdateBody {
    /// When set, replaces the display name.
    name: Option<String>,
    /// When set, replaces the organization's email.
    #[serde(default)]
    email: Option<String>,
    /// When the key is present, replaces description (`""` clears it).
    #[serde(default)]
    description: Option<String>,
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
        OrganizationError::InvalidLogo(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
        OrganizationError::EmptyOrganizationName
        | OrganizationError::InvalidOrganizationEmail
        | OrganizationError::MissingUserCredentials => (StatusCode::BAD_REQUEST, error.to_string()),
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
    cache: &aether_core::cache::Cache,
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
    let _ = cache.warm_invalidate_memberships(&user_id);
    let _ = cache.warm_invalidate_roles(org_db, &user_id);
}

/// Any logged-in active user (not developer-only).
async fn require_user(state: &Databases, headers: &HeaderMap) -> Result<ActorSession, Response> {
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

/// The caller, if they hold `permission` in the organization (developers and the owner always do).
async fn require_can_manage(
    databases: &Databases,
    headers: &HeaderMap,
    db_name: &str,
    permission: &str,
) -> Result<ActorSession, Response> {
    require_any_permission(databases, headers, db_name, &[permission]).await
}

/// The caller, if they hold at least one of `permissions` in the organization.
async fn require_any_permission(
    databases: &Databases,
    headers: &HeaderMap,
    db_name: &str,
    permissions: &[&str],
) -> Result<ActorSession, Response> {
    let actor = require_user(databases, headers).await?;
    let session = databases.fresh_session();
    for permission in permissions {
        match has_permission(
            &session,
            &databases.namespace(),
            db_name,
            &actor.user_id,
            actor.is_developer,
            permission,
        )
        .await
        {
            Ok(true) => return Ok(actor),
            Ok(false) => {}
            Err(error) => return Err(map_org_error(error)),
        }
    }
    Err(map_org_error(OrganizationError::Forbidden))
}

/// Every kernel permission the actor holds, for checking that they do not hand out more than that.
async fn actor_permissions(
    databases: &Databases,
    db_name: &str,
    actor: &ActorSession,
) -> Result<Vec<String>, Response> {
    let mut held = Vec::new();
    let session = databases.fresh_session();
    for (key, _, _) in roles::permission::CATALOG {
        match has_permission(
            &session,
            &databases.namespace(),
            db_name,
            &actor.user_id,
            actor.is_developer,
            key,
        )
        .await
        {
            Ok(true) => held.push(key.to_string()),
            Ok(false) => {}
            Err(error) => return Err(map_org_error(error)),
        }
    }
    Ok(held)
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

    let session = state.fresh_session();
    match update_profile(
        &session,
        &state.namespace,
        &db,
        OrganizationUpdate {
            name,
            email: body
                .email
                .as_deref()
                .map(str::trim)
                .filter(|v| !v.is_empty()),
            description,
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
    let actor = match require_user(state.databases(), &headers).await {
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
    if let Err(error) =
        create_user(&session, &state.namespace, username, email, &body.password).await
    {
        return map_org_error(error);
    }
    // One organization: every account belongs to it from the start.
    if let Err(error) = assign_user(
            &session,
            &state.namespace,
            username,
            &state.config.tenancy.single_org_db_name,
        )
        .await
    {
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
    if let Err(response) = require_can_manage(state.databases(), &headers, &db, roles::permission::MEMBERS_MANAGE).await {
        return response;
    }
    match crate::plugin_activity::list_for_org(
        state.databases(),
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
    if let Err(response) = require_any_permission(state.databases(), &headers, &db, &[roles::permission::MEMBERS_MANAGE, roles::permission::ROLES_MANAGE]).await {
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
    let actor = match require_can_manage(state.databases(), &headers, &db, roles::permission::MEMBERS_MANAGE).await {
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
            bust_membership_warm(&state.cache, &session, &state.namespace, &db, &login_name).await;
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
                    bust_membership_warm(
                        &state.cache,
                        &session,
                        &state.namespace,
                        &db,
                        username.trim(),
                    )
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
                    bust_membership_warm(
                        &state.cache,
                        &session,
                        &state.namespace,
                        &db,
                        login.trim(),
                    )
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
    let actor = match require_can_manage(state.databases(), &headers, &db, roles::permission::MEMBERS_MANAGE).await {
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
    let actor = match require_user(state.databases(), &headers).await {
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
    let actor = match require_user(state.databases(), &headers).await {
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
    let actor = match require_user(state.databases(), &headers).await {
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
async fn place_roles(state: &Databases, list: Vec<roles::RoleSummary>) -> Vec<PlacedRole> {
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
        if let Ok(mut response) = core
            .query("SELECT name, label, workspace FROM plugins;")
            .await
        {
            if let Ok(rows) = response.take::<Vec<PluginRow>>(0) {
                for row in rows {
                    plugins
                        .entry(row.name)
                        .or_insert((row.label, row.workspace));
                }
            }
        }
        if let Ok(mut response) = core
            .query("SELECT name, label, group_label FROM plugin_workspaces;")
            .await
        {
            if let Ok(rows) = response.take::<Vec<WorkspaceRow>>(0) {
                for row in rows {
                    let label = row
                        .group_label
                        .or(row.label)
                        .unwrap_or_else(|| row.name.clone());
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
    if let Err(response) = require_any_permission(state.databases(), &headers, &db, &[roles::permission::MEMBERS_MANAGE, roles::permission::ROLES_MANAGE]).await {
        return response;
    }
    if let Ok(Some(cached)) = state.cache.warm_get_role_catalog(&db) {
        return Json(cached).into_response();
    }
    let session = state.fresh_session();
    match roles::list_in(&session, &state.namespace, &db).await {
        Ok(roles) => {
            let placed = place_roles(state.databases(), roles).await;
            let body = json!({ "roles": placed });
            let _ = state.cache.warm_set_role_catalog(&db, &body);
            Json(body).into_response()
        }
        Err(error) => {
            log::error!("list org roles: {error}");
            failure(StatusCode::INTERNAL_SERVER_ERROR, "database error")
        }
    }
}

/// One thing a role may do, read off the rule files of the plugins installed on the organization.
#[derive(Debug, Clone, serde::Serialize, Deserialize)]
struct RolePermission {
    plugin: String,
    plugin_label: String,
    model: String,
    model_label: String,
    /// `access`, `field_read`, `field_write`, `transition` or `exempt`.
    kind: String,
    /// The rule's name, the fields, or the move.
    name: String,
    operations: Vec<String>,
    /// The rule only covers records matching a condition.
    conditional: bool,
}

/// What every role is allowed to do, by role name: the access rows, field rights, workflow moves
/// and restriction exemptions that name it, across the enabled plugins of the organization. Read
/// from the plugins' files (no compile) and kept in Mantle until roles or plugins change.
async fn role_permission_map(
    state: &AppState,
    db: &str,
) -> Result<std::collections::HashMap<String, Vec<RolePermission>>, Response> {
    use aether_core::data_model::rules::qualify;
    use std::collections::HashMap;
    if let Ok(Some(cached)) = state.cache.warm_get_role_permissions(db) {
        if let Ok(map) = serde_json::from_value(cached) {
            return Ok(map);
        }
    }
    let internal = |error: &dyn std::fmt::Display| {
        log::error!("role permissions: {error}");
        failure(StatusCode::INTERNAL_SERVER_ERROR, "database error")
    };
    let org = state.org(db).await.map_err(|e| internal(&e))?;
    let core = state.core().await.map_err(|e| internal(&e))?;
    let installed = aether_core::plugin_manager::installed::list_cached(&state.cache, db, &org)
        .await
        .map_err(|e| internal(&e))?;
    let mut map: HashMap<String, Vec<RolePermission>> = HashMap::new();
    for row in installed.into_iter().filter(|row| row.is_enabled) {
        let Ok(declared) = state
            .plugin_runtime
            .declared(&core, &row.plugin_name, &row.version)
            .await
        else {
            continue;
        };
        let plugin = row.plugin_name.as_str();
        let plugin_label = if declared.manifest.plugin.label.is_empty() {
            plugin.to_string()
        } else {
            declared.manifest.plugin.label.clone()
        };
        for (model, rules) in &declared.rules {
            let model_label = declared
                .models
                .iter()
                .find(|def| &def.name == model)
                .and_then(|def| def.label.clone())
                .unwrap_or_else(|| model.clone());
            let mut push = |roles: &[String],
                            kind: &str,
                            name: String,
                            operations: Vec<&'static str>,
                            conditional: bool| {
                for role in roles {
                    map.entry(qualify(plugin, role)).or_default().push(RolePermission {
                        plugin: plugin.to_string(),
                        plugin_label: plugin_label.clone(),
                        model: model.clone(),
                        model_label: model_label.clone(),
                        kind: kind.to_string(),
                        name: name.clone(),
                        operations: operations.iter().map(|op| op.to_string()).collect(),
                        conditional,
                    });
                }
            };
            for rule in &rules.access {
                let ops = rule.operations.iter().map(|op| op.as_str()).collect();
                push(&rule.roles, "access", rule.name.clone(), ops, rule.when.is_some());
            }
            for rule in &rules.fields {
                let fields = rule.fields.join(", ");
                if let Some(roles) = &rule.read_roles {
                    push(roles, "field_read", fields.clone(), vec!["read"], false);
                }
                if let Some(roles) = &rule.write_roles {
                    push(roles, "field_write", fields, vec!["write"], false);
                }
            }
            for rule in &rules.restrict {
                let ops = rule.operations.iter().map(|op| op.as_str()).collect();
                push(&rule.exempt_roles, "exempt", rule.name.clone(), ops, false);
            }
            if let Some(workflow) = &rules.workflow {
                for step in &workflow.transitions {
                    let label = step.label.clone().unwrap_or_else(|| step.name.clone());
                    push(
                        &step.roles,
                        "transition",
                        format!("{label}: {} → {}", step.from.join(", "), step.to),
                        vec!["write"],
                        step.when.is_some(),
                    );
                }
            }
        }
    }
    for list in map.values_mut() {
        list.sort_by(|a, b| {
            (&a.plugin_label, &a.model_label, &a.kind, &a.name)
                .cmp(&(&b.plugin_label, &b.model_label, &b.kind, &b.name))
        });
    }
    if let Ok(value) = serde_json::to_value(&map) {
        let _ = state.cache.warm_set_role_permissions(db, &value);
    }
    Ok(map)
}

/// Put the `%key%` labels of models and plugins into the server's language (the cached map is
/// shared by every reader, so it keeps the keys). A key nobody translated reads as plain words.
async fn localize_permissions(state: &AppState, db: &str, permissions: &mut [RolePermission]) {
    let translator = async {
        let org = state.org(db).await.ok()?;
        let installed = aether_core::plugin_manager::installed::list_cached(&state.cache, db, &org)
            .await
            .ok()?;
        let pairs = aether_core::plugin_manager::installed::enabled_pairs(&installed);
        let core = state.core().await.ok()?;
        aether_core::plugin_manager::i18n::translator_for_cached(Some(&state.cache), &core, &pairs)
            .await
            .ok()
    }
    .await;
    let chain = aether_localization::fallback_chain(&[Some(state.config.i18n.default_locale.as_str())]);
    let resolve = |plugin: &str, text: &mut String| {
        let mut value = Value::String(std::mem::take(text));
        if let Some(translator) = &translator {
            aether_localization::resolve_tree(&mut value, plugin, translator, &chain);
        }
        let resolved = value.as_str().unwrap_or_default();
        *text = match resolved.strip_prefix('%').and_then(|r| r.strip_suffix('%')) {
            Some(key) => {
                let words = key.rsplit('.').next().unwrap_or(key).replace('_', " ");
                let mut chars = words.chars();
                chars
                    .next()
                    .map(|first| first.to_uppercase().chain(chars).collect())
                    .unwrap_or_default()
            }
            None => resolved.to_string(),
        };
    };
    for item in permissions {
        let plugin = item.plugin.clone();
        resolve(&plugin, &mut item.model_label);
        resolve(&plugin, &mut item.plugin_label);
    }
}

async fn role_permissions(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((db, role)): Path<(String, String)>,
) -> Response {
    if let Err(response) = require_any_permission(state.databases(), &headers, &db, &[roles::permission::MEMBERS_MANAGE, roles::permission::ROLES_MANAGE]).await {
        return response;
    }
    match role_permission_map(&state, &db).await {
        Ok(mut map) => {
            let mut permissions = map.remove(&role).unwrap_or_default();
            localize_permissions(&state, &db, &mut permissions).await;
            Json(json!({ "permissions": permissions })).into_response()
        }
        Err(response) => response,
    }
}

/// What a role may do to the organization itself (manage members, roles, settings).
async fn role_kernel_permissions_get(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((db, role)): Path<(String, String)>,
) -> Response {
    if let Err(response) = require_any_permission(
        state.databases(),
        &headers,
        &db,
        &[roles::permission::MEMBERS_MANAGE, roles::permission::ROLES_MANAGE],
    )
    .await
    {
        return response;
    }
    let session = state.fresh_session();
    match aether_core::org_admin::role_kernel_permissions(&session, &state.namespace, &db, role.trim()).await {
        Ok(permissions) => Json(json!({ "permissions": permissions })).into_response(),
        Err(error) => map_org_error(error),
    }
}

#[derive(Debug, Deserialize)]
struct KernelPermissionsBody {
    permissions: Vec<String>,
}

/// Set what a custom role may do to the organization. Nobody can give a role a permission they lack.
async fn role_kernel_permissions_put(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((db, role)): Path<(String, String)>,
    Json(body): Json<KernelPermissionsBody>,
) -> Response {
    let actor = match require_can_manage(state.databases(), &headers, &db, roles::permission::ROLES_MANAGE).await {
        Ok(actor) => actor,
        Err(response) => return response,
    };
    let held = match actor_permissions(state.databases(), &db, &actor).await {
        Ok(held) => held,
        Err(response) => return response,
    };
    let session = state.fresh_session();
    match aether_core::org_admin::set_org_role_permissions(
        &session,
        &state.namespace,
        &db,
        role.trim(),
        &body.permissions,
        &held,
    )
    .await
    {
        Ok(()) => {
            let _ = state.cache.warm_invalidate_role_catalog(&db);
            Json(json!({ "ok": true })).into_response()
        }
        Err(error) => map_org_error(error),
    }
}

/// The models whose plugin rules restrict some operation, with the operations they restrict: the places an
/// administrator's record grant can open anything. A model with no `access` rows for an operation is already open.
async fn restricted_models(state: &AppState, db: &str) -> Result<Vec<RolePermission>, Response> {
    let internal = |error: &dyn std::fmt::Display| {
        log::error!("record grants: {error}");
        failure(StatusCode::INTERNAL_SERVER_ERROR, "database error")
    };
    let org = state.org(db).await.map_err(|e| internal(&e))?;
    let core = state.core().await.map_err(|e| internal(&e))?;
    let installed = aether_core::plugin_manager::installed::list_cached(&state.cache, db, &org)
        .await
        .map_err(|e| internal(&e))?;
    let mut models = Vec::new();
    for row in installed.into_iter().filter(|row| row.is_enabled) {
        let Ok(declared) = state
            .plugin_runtime
            .declared(&core, &row.plugin_name, &row.version)
            .await
        else {
            continue;
        };
        let plugin_label = if declared.manifest.plugin.label.is_empty() {
            row.plugin_name.clone()
        } else {
            declared.manifest.plugin.label.clone()
        };
        for (model, rules) in &declared.rules {
            let operations: Vec<String> = ["read", "create", "write", "delete"]
                .into_iter()
                .filter(|op| rules.access.iter().any(|rule| rule.operations.iter().any(|o| o.as_str() == *op)))
                .map(str::to_string)
                .collect();
            if operations.is_empty() {
                continue;
            }
            let model_label = declared
                .models
                .iter()
                .find(|def| &def.name == model)
                .and_then(|def| def.label.clone())
                .unwrap_or_else(|| model.clone());
            models.push(RolePermission {
                plugin: row.plugin_name.clone(),
                plugin_label: plugin_label.clone(),
                model: model.clone(),
                model_label,
                kind: "access".into(),
                name: "restricted".into(),
                operations,
                conditional: false,
            });
        }
    }
    localize_permissions(state, db, &mut models).await;
    models.sort_by(|a, b| (&a.plugin_label, &a.model_label).cmp(&(&b.plugin_label, &b.model_label)));
    Ok(models)
}

/// The record access an administrator gave a custom role, and the models it could be given on.
async fn role_record_grants_get(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((db, role)): Path<(String, String)>,
) -> Response {
    if let Err(response) = require_any_permission(
        state.databases(),
        &headers,
        &db,
        &[roles::permission::MEMBERS_MANAGE, roles::permission::ROLES_MANAGE],
    )
    .await
    {
        return response;
    }
    let models = match restricted_models(&state, &db).await {
        Ok(models) => models,
        Err(response) => return response,
    };
    let org = match state.org(&db).await {
        Ok(org) => org,
        Err(error) => {
            log::error!("record grants: {error}");
            return failure(StatusCode::INTERNAL_SERVER_ERROR, "database error");
        }
    };
    let grants = match roles::grants_for_roles(&org, &[role.trim().to_string()]).await {
        Ok(grants) => grants,
        Err(error) => {
            log::error!("record grants: {error}");
            return failure(StatusCode::INTERNAL_SERVER_ERROR, "database error");
        }
    };
    let models: Vec<Value> = models
        .into_iter()
        .map(|m| {
            json!({
                "model": format!("{}.{}", m.plugin, m.model),
                "plugin": m.plugin,
                "plugin_label": m.plugin_label,
                "model_label": m.model_label,
                "operations": m.operations,
            })
        })
        .collect();
    let grants: Vec<Value> = grants
        .into_iter()
        .map(|g| json!({ "model": g.model, "operations": g.operations }))
        .collect();
    Json(json!({ "grants": grants, "models": models })).into_response()
}

#[derive(Debug, Deserialize)]
struct RecordGrantBody {
    model: String,
    operations: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct RecordGrantsBody {
    grants: Vec<RecordGrantBody>,
}

/// Set the record access of a custom role. Opening records to people is an administrator's decision, so this
/// needs `org.settings` as well as `roles.manage`: a role manager can hand out the roles that exist, not widen
/// what they open.
async fn role_record_grants_put(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((db, role)): Path<(String, String)>,
    Json(body): Json<RecordGrantsBody>,
) -> Response {
    let actor = match require_can_manage(state.databases(), &headers, &db, roles::permission::ROLES_MANAGE).await {
        Ok(actor) => actor,
        Err(response) => return response,
    };
    match actor_permissions(state.databases(), &db, &actor).await {
        Ok(held) if held.iter().any(|key| key == roles::permission::ORG_SETTINGS) => {}
        Ok(_) => return map_org_error(OrganizationError::Forbidden),
        Err(response) => return response,
    }
    let known: Vec<String> = match restricted_models(&state, &db).await {
        Ok(models) => models.into_iter().map(|m| format!("{}.{}", m.plugin, m.model)).collect(),
        Err(response) => return response,
    };
    if let Some(unknown) = body.grants.iter().find(|g| !known.contains(&g.model)) {
        return failure(
            StatusCode::BAD_REQUEST,
            &format!("`{}` is not a model that an installed plugin restricts.", unknown.model),
        );
    }
    let session = state.fresh_session();
    let grants: Vec<(String, Vec<String>)> = body
        .grants
        .into_iter()
        .map(|g| (g.model, g.operations))
        .collect();
    match aether_core::org_admin::set_org_role_grants(&session, &state.namespace, &db, role.trim(), &grants).await {
        Ok(()) => Json(json!({ "ok": true })).into_response(),
        Err(error) => map_org_error(error),
    }
}

async fn create_role(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(db): Path<String>,
    Json(body): Json<CreateRoleBody>,
) -> Response {
    if let Err(response) = require_can_manage(state.databases(), &headers, &db, roles::permission::ROLES_MANAGE).await {
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
        Ok(role) => {
            let _ = state.cache.warm_invalidate_role_catalog(&db);
            Json(json!({ "ok": true, "role": role })).into_response()
        }
        Err(error) => map_org_error(error),
    }
}

async fn patch_role(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((db, role)): Path<(String, String)>,
    Json(body): Json<PatchRoleBody>,
) -> Response {
    if let Err(response) = require_can_manage(state.databases(), &headers, &db, roles::permission::ROLES_MANAGE).await {
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
        Ok(()) => {
            let _ = state.cache.warm_invalidate_role_catalog(&db);
            Json(json!({ "ok": true })).into_response()
        }
        Err(error) => map_org_error(error),
    }
}

async fn delete_role(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((db, role)): Path<(String, String)>,
) -> Response {
    if let Err(response) = require_can_manage(state.databases(), &headers, &db, roles::permission::ROLES_MANAGE).await {
        return response;
    }
    let role = role.trim();
    if role.is_empty() {
        return failure(StatusCode::BAD_REQUEST, "Enter a role name.");
    }
    let session = state.fresh_session();
    match delete_org_role(&session, &state.namespace, &db, role).await {
        Ok(()) => {
            let _ = state.cache.warm_invalidate_role_catalog(&db);
            Json(json!({ "ok": true })).into_response()
        }
        Err(error) => map_org_error(error),
    }
}

async fn grant_role(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((db, user_id)): Path<(String, String)>,
    Json(body): Json<RoleBody>,
) -> Response {
    let actor = match require_can_manage(state.databases(), &headers, &db, roles::permission::ROLES_MANAGE).await {
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
    databases: &Databases,
    headers: &HeaderMap,
    db: &str,
    user_id: &str,
    on: bool,
) -> Response {
    let actor = match require_user(databases, headers).await {
        Ok(actor) => actor,
        Err(response) => return response,
    };
    if !actor.is_developer {
        return failure(
            StatusCode::FORBIDDEN,
            "Only an unrestricted developer may give or take developer access.",
        );
    }
    let session = databases.fresh_session();
    match set_org_developer(&session, &databases.namespace(), db, user_id, on).await {
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
    set_developer(state.databases(), &headers, &db, &user_id, true).await
}

async fn revoke_developer(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((db, user_id)): Path<(String, String)>,
) -> Response {
    set_developer(state.databases(), &headers, &db, &user_id, false).await
}

async fn revoke_role(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((db, user_id, role)): Path<(String, String, String)>,
) -> Response {
    let actor = match require_can_manage(state.databases(), &headers, &db, roles::permission::ROLES_MANAGE).await {
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

async fn transfer(
    State(state): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    uri: Uri,
    Json(body): Json<TransferBody>,
) -> Response {
    // A developer hands a named organization to one of its members from the desk, whatever
    // organization their own session is in.
    if let Some(organization) = body
        .organization
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        && let Ok(actor) = require_user(state.databases(), &headers).await
        && actor.is_developer
    {
        let to = body.to.trim();
        if to.is_empty() {
            return failure(StatusCode::BAD_REQUEST, "Choose the new owner.");
        }
        let session = state.fresh_session();
        return match transfer_ownership(
            &session,
            &state.namespace,
            organization,
            &actor.user_id,
            true,
            to,
            body.note.as_deref(),
        )
        .await
        {
            Ok(owner) => {
                let _ = state.cache.warm_invalidate_roles(organization, &owner);
                Json(json!({ "ok": true, "organization": organization, "owner": to })).into_response()
            }
            Err(error) => map_org_error(error),
        };
    }

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
        false,
        to,
        body.note.as_deref(),
    )
    .await
    {
        Ok(owner) => {
            let _ = state.cache.warm_invalidate_roles(organization, &owner);
            Json(json!({
                "ok": true,
                "organization": organization,
                "owner": to,
            }))
            .into_response()
        }
        Err(error) => map_org_error(error),
    }
}
