//! Upload and download of the files that `image` and `file` fields hold.
//!
//! * `POST /api/media?name=<file name>`: the request body is the file's bytes. The server
//!   decides what the file is from its content, stores it in the organization's media
//!   storage under an unguessable key, and answers with the object a record keeps
//!   (`{ key, name, mime, size, sha256 }`; see `data_model::media`). A picture is first
//!   decoded, shrunk and stripped of its metadata (`media_image`), and a small copy is
//!   kept beside it.
//! * `GET /api/media/{*key}?name=<file name>`: the bytes back, to a signed-in member.
//!   `?thumb=1` asks for the small copy of a picture.
//!
//! Only members of the organization may upload or read, and a key only ever resolves
//! inside the caller's own organization (the backend is prefixed per organization).
//! Reading is by key: an object's 128-bit random name is its capability, the same model a
//! record's own id gives a page. Files are never served as something a browser would run:
//! the type comes from the bytes, `nosniff` and a sandboxing CSP go with every response,
//! and anything that is not a picture is a download.

use std::net::SocketAddr;

use aether_storage::{MediaKey, StorageError};
use axum::{
    Extension, Json, Router,
    body::Body,
    extract::{ConnectInfo, Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, Uri, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use bytes::Bytes;
use rand::RngExt;
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::access::identity::{IdentityError, identify, with_retry_after};
use crate::data_model::media::{self, MAX_MEDIA_BYTES};
use crate::media_cleanup::THUMB_SUFFIX;
use crate::media_image;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/media", post(upload))
        .route("/api/media/{*key}", get(download))
}

#[derive(Debug, Deserialize)]
struct NameQuery {
    #[serde(default)]
    name: Option<String>,
    /// `1` for a picture's small copy.
    #[serde(default)]
    thumb: Option<String>,
}

fn failure(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({ "error": message }))).into_response()
}

/// The organization database of a signed-in member, or the response that refuses.
async fn member_org(
    state: &AppState,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: &HeaderMap,
    uri: &Uri,
) -> Result<String, Response> {
    let peer_ip = peer.map(|Extension(ConnectInfo(address))| address.ip());
    let identity = identify(state, headers, peer_ip, uri)
        .await
        .map_err(|error| {
            if matches!(error, IdentityError::Database(_) | IdentityError::Audit(_)) {
                log::error!("media identity: {error}");
            }
            let (status, message) = error.http();
            with_retry_after(failure(status, message))
        })?;
    if identity.session.is_none() {
        return Err(failure(StatusCode::UNAUTHORIZED, "not authenticated"));
    }
    if identity.foreign_user {
        return Err(failure(
            StatusCode::FORBIDDEN,
            "not a member of this organization",
        ));
    }
    Ok(identity.org_db)
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

async fn upload(
    State(state): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    uri: Uri,
    Query(query): Query<NameQuery>,
    body: Bytes,
) -> Response {
    let org = match member_org(&state, peer, &headers, &uri).await {
        Ok(org) => org,
        Err(response) => return response,
    };
    if body.is_empty() {
        return failure(StatusCode::BAD_REQUEST, "the file is empty");
    }
    if body.len() > MAX_MEDIA_BYTES {
        return failure(
            StatusCode::PAYLOAD_TOO_LARGE,
            &format!(
                "a file may be at most {} MB",
                MAX_MEDIA_BYTES / (1024 * 1024)
            ),
        );
    }
    let Some(kind) = media::sniff(&body) else {
        return failure(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "that kind of file is not allowed (pictures, PDF, Office documents, zip and plain text are)",
        );
    };

    // A picture is decoded, shrunk and rewritten without its metadata; the decoding is
    // the slow part, so it runs off the request's thread.
    let (body, kind, thumbnail) = if kind.is_image() {
        let source = body.clone();
        let done = tokio::task::spawn_blocking(move || media_image::process(&source, kind)).await;
        match done {
            Ok(Ok(done)) => (Bytes::from(done.bytes), done.kind, Some(done.thumbnail)),
            Ok(Err(reason)) => return failure(StatusCode::UNSUPPORTED_MEDIA_TYPE, &reason),
            Err(error) => {
                log::error!("media image task: {error}");
                return failure(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "could not read the picture",
                );
            }
        }
    } else {
        (body, kind, None)
    };

    let mut random = [0u8; 16];
    rand::rng().fill(&mut random);
    let now = chrono::Utc::now();
    let key = media::new_key(
        chrono::Datelike::year(&now),
        chrono::Datelike::month(&now),
        random,
        kind,
    );
    let name = media::clean_name(query.name.as_deref().unwrap_or("file"));
    let size = body.len();
    let sha256 = sha256_hex(&body);

    let backend = match state.org_media(&org) {
        Ok(backend) => backend,
        Err(error) => {
            log::error!("media backend for `{org}`: {error}");
            return failure(StatusCode::INTERNAL_SERVER_ERROR, "storage is unavailable");
        }
    };
    let media_key = match MediaKey::parse(&key) {
        Ok(key) => key,
        Err(error) => {
            log::error!("media key `{key}`: {error}");
            return failure(StatusCode::INTERNAL_SERVER_ERROR, "could not name the file");
        }
    };
    if let Err(error) = backend.put(&media_key, body).await {
        log::error!("media upload in `{org}`: {error}");
        return failure(StatusCode::INTERNAL_SERVER_ERROR, "storage is unavailable");
    }
    if let Some(thumbnail) = thumbnail {
        // The small copy is a convenience: without it the picture is shown at full size.
        match MediaKey::parse(&format!("{key}{THUMB_SUFFIX}")) {
            Ok(thumb_key) => {
                if let Err(error) = backend.put(&thumb_key, Bytes::from(thumbnail)).await {
                    log::warn!("media thumbnail in `{org}`: {error}");
                }
            }
            Err(error) => log::warn!("media thumbnail key `{key}`: {error}"),
        }
    }

    Json::<Value>(json!({
        "key": key,
        "name": name,
        "mime": kind.mime(),
        "size": size,
        "sha256": sha256,
    }))
    .into_response()
}

async fn download(
    State(state): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    uri: Uri,
    Path(key): Path<String>,
    Query(query): Query<NameQuery>,
) -> Response {
    let org = match member_org(&state, peer, &headers, &uri).await {
        Ok(org) => org,
        Err(response) => return response,
    };
    if !media::is_media_key(&key) {
        return failure(StatusCode::NOT_FOUND, "not found");
    }
    let Ok(media_key) = MediaKey::parse(&key) else {
        return failure(StatusCode::NOT_FOUND, "not found");
    };
    let backend = match state.org_media(&org) {
        Ok(backend) => backend,
        Err(error) => {
            log::error!("media backend for `{org}`: {error}");
            return failure(StatusCode::INTERNAL_SERVER_ERROR, "storage is unavailable");
        }
    };
    // The small copy when asked for and there is one; the picture itself otherwise (files
    // made before thumbnails existed have none).
    let small = if query.thumb.as_deref() == Some("1") {
        match MediaKey::parse(&format!("{key}{THUMB_SUFFIX}")) {
            Ok(thumb_key) => backend.get(&thumb_key).await.ok(),
            Err(_) => None,
        }
    } else {
        None
    };
    let bytes = match small {
        Some(bytes) => bytes,
        None => match backend.get(&media_key).await {
            Ok(bytes) => bytes,
            Err(StorageError::NotFound(_)) => return failure(StatusCode::NOT_FOUND, "not found"),
            Err(error) => {
                log::error!("media download in `{org}`: {error}");
                return failure(StatusCode::INTERNAL_SERVER_ERROR, "storage is unavailable");
            }
        },
    };
    // The type is what the bytes are, whatever was claimed at upload.
    let Some(kind) = media::sniff(&bytes) else {
        return failure(StatusCode::NOT_FOUND, "not found");
    };
    let disposition = if kind.is_image() {
        "inline".to_string()
    } else {
        let name = media::clean_name(query.name.as_deref().unwrap_or("file"));
        format!("attachment; filename=\"{name}\"")
    };

    let mut response = Response::new(Body::from(bytes));
    let out = response.headers_mut();
    out.insert(header::CONTENT_TYPE, HeaderValue::from_static(kind.mime()));
    if let Ok(value) = HeaderValue::from_str(&disposition) {
        out.insert(header::CONTENT_DISPOSITION, value);
    }
    out.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    out.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("default-src 'none'; img-src 'self'; sandbox"),
    );
    // The key is never reused for different bytes.
    out.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, max-age=31536000, immutable"),
    );
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_like_sha256() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
