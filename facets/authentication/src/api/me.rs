use aether_core::access::organizations::effective_org_database;
use aether_core::app_dir::AppDir;
use aether_core::org_admin::{can_manage_admin_role, can_manage_users, get_profile, is_org_developer};
use aether_core::roles;
use aether_core::state::AppState;
use aether_core::user_avatar::{
    AvatarError, avatar_url_for, clear_avatar, read_avatar, set_avatar, update_display_name,
};
use axum::{
    Json,
    body::Body,
    extract::State,
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use axum_extra::extract::CookieJar;
use serde::Deserialize;
use serde_json::json;
use surrealdb::types::ToSql;

use crate::authz::AuthSession;
use crate::session_cookie::read_session_token;

pub async fn me(
    State(state): State<AppState>,
    AuthSession(session): AuthSession,
) -> impl IntoResponse {
    let user_id = format!(
        "{}:{}",
        session.user.id.table.as_str(),
        session.user.id.key.to_sql()
    );
    let is_developer = session.user.is_super_user;

    let mut roles: Vec<String> = Vec::new();
    let mut can_manage = is_developer;
    let mut can_manage_org = is_developer;
    // Developer of the current organization: unrestricted, or made one of this organization.
    let mut is_org_dev = is_developer;
    let mut is_owner = false;

    // Prefer the stored session org; fall back to the sole membership (Apps already does).
    let effective_org = match effective_org_database(&state, &session).await {
        Ok(org) => org,
        Err(error) => {
            log::warn!("me: effective_org_database: {error}");
            session
                .org_database_id
                .as_deref()
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .filter(|v| *v != aether_orm::LEFT_ORGANIZATION)
                .map(str::to_string)
        }
    };

    if let Some(org_db) = effective_org.as_deref() {
        let core = state.fresh_session();
        match get_profile(&core, &state.namespace, org_db).await {
            Ok(profile) => {
                is_owner = profile.owner_user_id.as_deref() == Some(user_id.as_str());
            }
            Err(error) => log::warn!("me: organization profile `{org_db}`: {error}"),
        }
        match can_manage_users(&core, &state.namespace, org_db, &user_id, is_developer).await {
            Ok(allowed) => can_manage = allowed,
            Err(error) => log::warn!("me: can_manage_users `{org_db}`: {error}"),
        }
        match can_manage_admin_role(&core, &state.namespace, org_db, &user_id, is_developer).await {
            Ok(allowed) => can_manage_org = allowed,
            Err(error) => log::warn!("me: can_manage_admin_role `{org_db}`: {error}"),
        }
        match is_org_developer(&core, &state.namespace, org_db, &user_id, is_developer).await {
            Ok(allowed) => is_org_dev = allowed,
            Err(error) => log::warn!("me: is_org_developer `{org_db}`: {error}"),
        }
        let org = state.fresh_session();
        if let Err(error) = org.use_ns(&state.namespace).await {
            log::warn!("me: use_ns: {error}");
        } else if let Err(error) = org.use_db(org_db).await {
            log::warn!("me: use_db `{org_db}`: {error}");
        } else {
            let _ = roles::ensure_system_roles(&org).await;
            match roles::roles_of_cached(&state.cache, org_db, &org, &user_id).await {
                Ok(names) => roles = names,
                Err(error) => log::warn!("me: roles of `{user_id}` in `{org_db}`: {error}"),
            }
        }
    }

    let body = Json(json!({
        "user": {
            "id": user_id,
            "username": session.user.username,
            "email": session.user.email,
            "display_name": session.user.display_name,
            "avatar_url": avatar_url_for(session.user.avatar_file.as_deref()),
            "is_super_user": session.user.is_super_user,
            "is_developer": is_developer,
        },
        "session": {
            "provider": session.provider,
            // Effective org (stored choice or sole membership) so the UI matches Apps.
            "org_database_id": effective_org,
        },
        "roles": roles,
        "is_owner": is_owner,
        "can_manage_users": can_manage,
        "can_manage_org": can_manage_org,
        "is_org_developer": is_org_dev,
    }));
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-store, private"),
    );
    (headers, body)
}

#[derive(Debug, Deserialize)]
pub struct PatchMeBody {
    pub display_name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AvatarBody {
    pub filename: String,
    pub data: String,
}

pub async fn patch_me(
    State(state): State<AppState>,
    jar: CookieJar,
    AuthSession(session): AuthSession,
    Json(body): Json<PatchMeBody>,
) -> Response {
    let Some(display_name) = body.display_name else {
        return failure(StatusCode::BAD_REQUEST, "Nothing to update.");
    };
    let db = state.fresh_session();
    match update_display_name(&db, &state.namespace, &session.user.id, &display_name).await {
        Ok(user) => {
            refresh_session(&state, &jar);
            Json(json!({ "user": user })).into_response()
        }
        Err(error) => map_avatar_error(error),
    }
}

pub async fn upload_avatar(
    State(state): State<AppState>,
    jar: CookieJar,
    AuthSession(session): AuthSession,
    Json(body): Json<AvatarBody>,
) -> Response {
    let db = state.fresh_session();
    let app_dir = AppDir::new(&state.config.app_dir);
    match set_avatar(
        &db,
        &state.namespace,
        &app_dir,
        &session.user.id,
        body.filename.trim(),
        &body.data,
    )
    .await
    {
        Ok(user) => {
            refresh_session(&state, &jar);
            Json(json!({ "user": user })).into_response()
        }
        Err(error) => map_avatar_error(error),
    }
}

pub async fn delete_avatar(
    State(state): State<AppState>,
    jar: CookieJar,
    AuthSession(session): AuthSession,
) -> Response {
    let db = state.fresh_session();
    let app_dir = AppDir::new(&state.config.app_dir);
    match clear_avatar(&db, &state.namespace, &app_dir, &session.user.id).await {
        Ok(user) => {
            refresh_session(&state, &jar);
            Json(json!({ "user": user })).into_response()
        }
        Err(error) => map_avatar_error(error),
    }
}

pub async fn get_avatar(
    State(state): State<AppState>,
    AuthSession(session): AuthSession,
) -> Response {
    let db = state.fresh_session();
    let app_dir = AppDir::new(&state.config.app_dir);
    match read_avatar(&db, &state.namespace, &app_dir, &session.user.id).await {
        Ok(Some((bytes, content_type))) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, content_type)
            .header(header::CACHE_CONTROL, "private, no-cache")
            .body(Body::from(bytes))
            .unwrap_or_else(|_| failure(StatusCode::INTERNAL_SERVER_ERROR, "response error")),
        Ok(None) => failure(StatusCode::NOT_FOUND, "No profile photo."),
        Err(error) => map_avatar_error(error),
    }
}

/// The signed-in session is cached without the new photo or name. Drop it so `/me` reloads.
fn refresh_session(state: &AppState, jar: &CookieJar) {
    if let Some(token) = read_session_token(jar) {
        aether_core::session_cache::invalidate_token(&state.cache, &token);
    }
}

fn failure(status: StatusCode, message: impl AsRef<str>) -> Response {
    (status, Json(json!({ "error": message.as_ref() }))).into_response()
}

fn map_avatar_error(error: AvatarError) -> Response {
    let (status, message) = match &error {
        AvatarError::Invalid(message) => (StatusCode::BAD_REQUEST, message.clone()),
        AvatarError::NotFound => (StatusCode::NOT_FOUND, "User not found.".into()),
        AvatarError::Io(message) => {
            log::error!("avatar io: {message}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Could not store the profile photo.".into(),
            )
        }
        AvatarError::Surreal(error) => {
            log::error!("avatar db: {error}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Could not update the profile.".into(),
            )
        }
    };
    failure(status, message)
}
