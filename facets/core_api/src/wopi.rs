//! The kernel as a WOPI host: what an office editor (Collabora Online and other WOPI clients) calls to open and save a file.
//!
//! ```text
//! GET   /api/wopi/files/<file>?access_token=       CheckFileInfo: name, size, who, may they write
//! GET   /api/wopi/files/<file>/contents            GetFile: the bytes
//! POST  /api/wopi/files/<file>/contents            PutFile: the edited bytes
//! ```
//!
//! `<file>` is the file's media key in URL-safe base64. The editor has no account here: what lets it in is the **access token** the
//! `collabora` bridge made for one person and one file (see there). The signing key is the organization's own bridge setting, so the
//! organization is read from the token first and the signature checked against its key before anything else is believed.
//!
//! A saved file is not written over the old one. The edited bytes get a new media key and the plugin named in the token is asked,
//! **as the person**, to take them (`save` is its function, with `{ file: <ref>, media: { key, name, mime, size } }`): the drive makes
//! a new version, with its own rules for who may edit and for the quota. If the plugin refuses, the new bytes are removed again.
//! No locks are offered (`SupportsLocks` is false): two people editing the same file at once is the editor's own co-editing session.

use aether_storage::{MediaKey, StorageError};
use axum::{
    Json, Router,
    body::Bytes,
    extract::{DefaultBodyLimit, Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use rand::RngExt;
use serde::Deserialize;
use serde_json::{Value, json};

use aether_core::data_model::media::{self, MAX_MEDIA_BYTES};
use aether_core::state::AppState;
use aether_core::tenancy::OrgRef;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/wopi/files/{file}", get(check_file_info))
        .route(
            "/api/wopi/files/{file}/contents",
            get(get_file).post(put_file),
        )
        .layer(DefaultBodyLimit::max(MAX_MEDIA_BYTES + 1024))
}

#[derive(Deserialize)]
struct TokenQuery {
    access_token: Option<String>,
}

fn refuse(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({ "error": message }))).into_response()
}

/// What a valid token allows: its claims and the media key of the file.
struct Allowed {
    claims: Value,
    org: String,
    media: String,
    key: MediaKey,
}

async fn authorize(
    state: &AppState,
    file: &str,
    query: &TokenQuery,
    headers: &HeaderMap,
) -> Result<Allowed, Response> {
    let token = query
        .access_token
        .clone()
        .or_else(|| {
            headers
                .get(header::AUTHORIZATION)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.strip_prefix("Bearer "))
                .map(str::to_string)
        })
        .ok_or_else(|| refuse(StatusCode::UNAUTHORIZED, "no access token"))?;
    let unauthorized = || refuse(StatusCode::UNAUTHORIZED, "the access token is not valid");
    let org = collabora::organization_of(&token)
        .filter(|o| !o.is_empty() && o.len() <= 120)
        .ok_or_else(unauthorized)?;
    // The name comes from a token nobody has verified yet: only an organization that exists may
    // have its signing key looked up, so a made-up name never reaches a database.
    match aether_core::access::org_databases::list_cached(state).await {
        Ok(names) if names.iter().any(|name| *name == org) => {}
        _ => return Err(unauthorized()),
    }
    let values = aether_core::messaging::resolve_values(
        state,
        &OrgRef {
            slug: org.clone(),
            db_name: org.clone(),
        },
        &collabora::SPEC,
    )
    .await
    .map_err(|_| unauthorized())?;
    let key = values.get("signing_key").cloned().unwrap_or_default();
    if key.len() < 16 {
        return Err(unauthorized());
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let claims = collabora::verify(&key, &token, now).ok_or_else(unauthorized)?;
    let media = claims["media"].as_str().unwrap_or_default().to_string();
    if collabora::file_id(&media) != file || !media::is_media_key(&media) {
        return Err(unauthorized());
    }
    let key = MediaKey::parse(&media).map_err(|_| unauthorized())?;
    Ok(Allowed {
        claims,
        org,
        media,
        key,
    })
}

async fn check_file_info(
    State(state): State<AppState>,
    Path(file): Path<String>,
    Query(query): Query<TokenQuery>,
    headers: HeaderMap,
) -> Response {
    let allowed = match authorize(&state, &file, &query, &headers).await {
        Ok(a) => a,
        Err(response) => return response,
    };
    let Ok(backend) = state.org_media(&allowed.org) else {
        return refuse(StatusCode::INTERNAL_SERVER_ERROR, "storage is unavailable");
    };
    let size = match backend.get(&allowed.key).await {
        Ok(bytes) => bytes.len(),
        Err(StorageError::NotFound(_)) => return refuse(StatusCode::NOT_FOUND, "not found"),
        Err(error) => {
            log::error!("wopi size in `{}`: {error}", allowed.org);
            return refuse(StatusCode::INTERNAL_SERVER_ERROR, "storage is unavailable");
        }
    };
    let c = &allowed.claims;
    Json(json!({
        "BaseFileName": c["name"], "OwnerId": c["user"], "UserId": c["user"], "UserFriendlyName": c["user_name"], "Size": size,
        "Version": allowed.media, "UserCanWrite": c["can_write"], "UserCanNotWriteRelative": true, "SupportsUpdate": c["can_write"],
        "SupportsLocks": false, "ReadOnly": !c["can_write"].as_bool().unwrap_or(false),
    }))
    .into_response()
}

async fn get_file(
    State(state): State<AppState>,
    Path(file): Path<String>,
    Query(query): Query<TokenQuery>,
    headers: HeaderMap,
) -> Response {
    let allowed = match authorize(&state, &file, &query, &headers).await {
        Ok(a) => a,
        Err(response) => return response,
    };
    let Ok(backend) = state.org_media(&allowed.org) else {
        return refuse(StatusCode::INTERNAL_SERVER_ERROR, "storage is unavailable");
    };
    match backend.get(&allowed.key).await {
        Ok(bytes) => (
            [
                (header::CONTENT_TYPE, "application/octet-stream"),
                (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            ],
            bytes,
        )
            .into_response(),
        Err(StorageError::NotFound(_)) => refuse(StatusCode::NOT_FOUND, "not found"),
        Err(error) => {
            log::error!("wopi get in `{}`: {error}", allowed.org);
            refuse(StatusCode::INTERNAL_SERVER_ERROR, "storage is unavailable")
        }
    }
}

async fn put_file(
    State(state): State<AppState>,
    Path(file): Path<String>,
    Query(query): Query<TokenQuery>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let allowed = match authorize(&state, &file, &query, &headers).await {
        Ok(a) => a,
        Err(response) => return response,
    };
    let c = &allowed.claims;
    if !c["can_write"].as_bool().unwrap_or(false) {
        return refuse(StatusCode::FORBIDDEN, "this person may only read the file");
    }
    if body.is_empty() || body.len() > MAX_MEDIA_BYTES {
        return refuse(
            StatusCode::PAYLOAD_TOO_LARGE,
            "the file is empty or too large",
        );
    }
    let Some(kind) = media::sniff(&body) else {
        return refuse(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "that kind of file is not accepted",
        );
    };
    let mut random = [0u8; 16];
    rand::rng().fill(&mut random);
    let now = chrono::Utc::now();
    let new_key = media::new_key(
        chrono::Datelike::year(&now),
        chrono::Datelike::month(&now),
        random,
        kind,
    );
    let Ok(media_key) = MediaKey::parse(&new_key) else {
        return refuse(StatusCode::INTERNAL_SERVER_ERROR, "could not name the file");
    };
    let Ok(backend) = state.org_media(&allowed.org) else {
        return refuse(StatusCode::INTERNAL_SERVER_ERROR, "storage is unavailable");
    };
    let size = body.len();
    if let Err(error) = backend.put(&media_key, body).await {
        log::error!("wopi put in `{}`: {error}", allowed.org);
        return refuse(StatusCode::INTERNAL_SERVER_ERROR, "storage is unavailable");
    }
    let payload = json!({ "file": c["ref"], "media": { "key": new_key, "name": c["name"], "mime": kind.mime(), "size": size } });
    let request_id = format!("wopi-{}", &new_key[new_key.len().saturating_sub(12)..]);
    let taken = aether_core::plugin_manager::api::run_user_call(
        &state,
        &allowed.org,
        c["user"].as_str().unwrap_or_default(),
        c["plugin"].as_str().unwrap_or_default(),
        c["save"].as_str().unwrap_or_default(),
        payload,
        &request_id,
    )
    .await;
    match taken {
        Ok(_) => Json::<Value>(json!({ "LastModifiedTime": now.to_rfc3339() })).into_response(),
        Err(error) => {
            let _ = backend.delete(&media_key).await;
            log::info!(
                "wopi save refused by {}.{}: {}",
                c["plugin"],
                c["save"],
                error.message
            );
            let status = if error.status == 403 || error.status == 404 {
                StatusCode::FORBIDDEN
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            refuse(status, &error.message)
        }
    }
}
