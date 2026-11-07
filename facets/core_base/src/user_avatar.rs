//! Profile photo for a signed-in user: stored under `app_dir/users/<id>/`.

use base64::Engine;
use serde::{Deserialize, Serialize};
use surrealdb::{
    Surreal,
    engine::remote::ws::Client,
    types::{RecordId, SurrealValue, ToSql},
};
use thiserror::Error;

use crate::app_dir::AppDir;

const AVATAR_URL: &str = "/api/auth/me/avatar";
const MAX_BYTES: usize = 2_000_000;

#[derive(Debug, Error)]
pub enum AvatarError {
    #[error("surrealdb error: {0}")]
    Surreal(#[from] surrealdb::Error),

    #[error("{0}")]
    Invalid(String),

    #[error("{0}")]
    Io(String),

    #[error("user not found")]
    NotFound,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileUser {
    pub id: String,
    pub username: Option<String>,
    pub email: Option<String>,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub is_super_user: bool,
    pub is_developer: bool,
}

/// Storage-safe folder name for a user record (`users:abc` → `abc`).
pub fn storage_id(user: &RecordId) -> String {
    let key = user.key.to_sql();
    let key = key
        .trim_matches(|c| matches!(c, '\'' | '"' | '⟨' | '⟩' | '`'))
        .trim();
    let mut out = String::with_capacity(key.len());
    for c in key.chars() {
        if c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.') {
            out.push(c);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        out.push_str("user");
    }
    out
}

pub fn avatar_url_for(avatar_file: Option<&str>) -> Option<String> {
    avatar_file
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(|_| AVATAR_URL.to_string())
}

fn image_ext(filename: &str, data_base64: &str) -> Result<String, AvatarError> {
    let from_name = std::path::Path::new(filename)
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let from_name = match from_name.as_str() {
        "png" | "jpg" | "jpeg" | "webp" | "gif" | "jfif" => Some(if from_name == "jfif" {
            "jpg".into()
        } else {
            from_name
        }),
        _ => None,
    };
    if let Some(ext) = from_name {
        return Ok(ext);
    }
    // Camera / paste often omits an extension; use the data-URL MIME type.
    let mime = data_base64
        .strip_prefix("data:")
        .and_then(|rest| rest.split(';').next())
        .unwrap_or("")
        .to_ascii_lowercase();
    let from_mime = match mime.as_str() {
        "image/png" => Some("png"),
        "image/jpeg" | "image/jpg" => Some("jpg"),
        "image/webp" => Some("webp"),
        "image/gif" => Some("gif"),
        _ => None,
    };
    from_mime
        .map(str::to_string)
        .ok_or_else(|| AvatarError::Invalid("use a png, jpg, webp or gif file".into()))
}

pub async fn set_avatar(
    db: &Surreal<Client>,
    namespace: &str,
    app_dir: &AppDir,
    user_id: &RecordId,
    filename: &str,
    data_base64: &str,
) -> Result<ProfileUser, AvatarError> {
    let ext = image_ext(filename, data_base64)?;
    let raw = data_base64
        .split(',')
        .next_back()
        .unwrap_or(data_base64)
        .trim();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(raw)
        .map_err(|_| AvatarError::Invalid("the file data is not valid base64".into()))?;
    if bytes.is_empty() || bytes.len() > MAX_BYTES {
        return Err(AvatarError::Invalid(
            "photo must be between 1 byte and 2 MB".into(),
        ));
    }

    let storage = storage_id(user_id);
    let directory = app_dir
        .ensure_user(&storage)
        .await
        .map_err(|error| AvatarError::Io(error.to_string()))?;
    let file_name = format!("avatar.{ext}");
    let path = directory.join(&file_name);
    if let Ok(mut entries) = tokio::fs::read_dir(&directory).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with("avatar.") && name != file_name {
                let _ = tokio::fs::remove_file(entry.path()).await;
            }
        }
    }
    tokio::fs::write(&path, &bytes)
        .await
        .map_err(|error| AvatarError::Io(error.to_string()))?;

    db.use_ns(namespace).await?;
    db.use_db("core").await?;
    db.query("UPDATE $user SET avatar_file = $file, date_updated = time::now();")
        .bind(("user", user_id.clone()))
        .bind(("file", file_name))
        .await?
        .check()?;

    load_profile(db, namespace, user_id).await
}

pub async fn clear_avatar(
    db: &Surreal<Client>,
    namespace: &str,
    app_dir: &AppDir,
    user_id: &RecordId,
) -> Result<ProfileUser, AvatarError> {
    db.use_ns(namespace).await?;
    db.use_db("core").await?;
    let mut response = db
        .query("SELECT avatar_file FROM $user LIMIT 1;")
        .bind(("user", user_id.clone()))
        .await?
        .check()?;
    #[derive(Debug, Deserialize, SurrealValue)]
    struct FileRow {
        avatar_file: Option<String>,
    }
    let rows: Vec<FileRow> = response.take(0)?;
    if rows.is_empty() {
        return Err(AvatarError::NotFound);
    }
    if let Some(file) = rows
        .into_iter()
        .next()
        .and_then(|row| row.avatar_file)
        .filter(|f| !f.trim().is_empty())
    {
        let path = app_dir
            .user_dir(&storage_id(user_id))
            .map_err(|error| AvatarError::Io(error.to_string()))?
            .join(file);
        let _ = tokio::fs::remove_file(path).await;
    }
    db.query("UPDATE $user SET avatar_file = NONE, date_updated = time::now();")
        .bind(("user", user_id.clone()))
        .await?
        .check()?;
    load_profile(db, namespace, user_id).await
}

pub async fn read_avatar(
    db: &Surreal<Client>,
    namespace: &str,
    app_dir: &AppDir,
    user_id: &RecordId,
) -> Result<Option<(Vec<u8>, &'static str)>, AvatarError> {
    db.use_ns(namespace).await?;
    db.use_db("core").await?;
    let mut response = db
        .query("SELECT avatar_file FROM $user LIMIT 1;")
        .bind(("user", user_id.clone()))
        .await?
        .check()?;
    #[derive(Debug, Deserialize, SurrealValue)]
    struct FileRow {
        avatar_file: Option<String>,
    }
    let rows: Vec<FileRow> = response.take(0)?;
    let Some(row) = rows.into_iter().next() else {
        return Err(AvatarError::NotFound);
    };
    let Some(file) = row.avatar_file.filter(|f| !f.trim().is_empty()) else {
        return Ok(None);
    };
    let path = app_dir
        .user_dir(&storage_id(user_id))
        .map_err(|error| AvatarError::Io(error.to_string()))?
        .join(&file);
    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|error| AvatarError::Io(error.to_string()))?;
    let content_type = match std::path::Path::new(&file)
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        _ => "application/octet-stream",
    };
    Ok(Some((bytes, content_type)))
}

pub async fn update_display_name(
    db: &Surreal<Client>,
    namespace: &str,
    user_id: &RecordId,
    display_name: &str,
) -> Result<ProfileUser, AvatarError> {
    let name = display_name.trim();
    if name.is_empty() || name.chars().count() > 100 {
        return Err(AvatarError::Invalid(
            "display name must be 1 to 100 characters".into(),
        ));
    }
    db.use_ns(namespace).await?;
    db.use_db("core").await?;
    db.query("UPDATE $user SET display_name = $name, date_updated = time::now();")
        .bind(("user", user_id.clone()))
        .bind(("name", name.to_string()))
        .await?
        .check()?;
    load_profile(db, namespace, user_id).await
}

async fn load_profile(
    db: &Surreal<Client>,
    namespace: &str,
    user_id: &RecordId,
) -> Result<ProfileUser, AvatarError> {
    db.use_ns(namespace).await?;
    db.use_db("core").await?;
    let mut response = db
        .query(
            r#"
            SELECT id, username, email, display_name, avatar_file, is_super_user
            FROM $user LIMIT 1;
            "#,
        )
        .bind(("user", user_id.clone()))
        .await?
        .check()?;
    #[derive(Debug, Deserialize, SurrealValue)]
    struct Row {
        id: RecordId,
        username: Option<String>,
        email: Option<String>,
        display_name: Option<String>,
        avatar_file: Option<String>,
        is_super_user: bool,
    }
    let rows: Vec<Row> = response.take(0)?;
    let Some(row) = rows.into_iter().next() else {
        return Err(AvatarError::NotFound);
    };
    Ok(ProfileUser {
        id: format!("{}:{}", row.id.table.as_str(), row.id.key.to_sql()),
        username: row.username,
        email: row.email,
        display_name: row.display_name,
        avatar_url: avatar_url_for(row.avatar_file.as_deref()),
        is_super_user: row.is_super_user,
        is_developer: row.is_super_user,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_ext_from_filename() {
        assert_eq!(image_ext("photo.PNG", "unused").unwrap(), "png");
        assert_eq!(image_ext("x.jfif", "unused").unwrap(), "jpg");
    }

    #[test]
    fn image_ext_from_data_url_when_name_has_no_extension() {
        assert_eq!(
            image_ext("blob", "data:image/jpeg;base64,/9j/4AAQ").unwrap(),
            "jpg"
        );
        assert_eq!(
            image_ext("", "data:image/webp;base64,UklG").unwrap(),
            "webp"
        );
    }

    #[test]
    fn image_ext_rejects_unknown_types() {
        assert!(image_ext("photo.heic", "data:image/heic;base64,xx").is_err());
    }
}
