use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
};
use rand::RngExt;
use serde::{Deserialize, Serialize};
use surrealdb::{Surreal, engine::remote::ws::Client, types::RecordId, types::SurrealValue};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum UserServiceError {
    #[error("surrealdb error: {0}")]
    Surreal(#[from] surrealdb::Error),

    #[error("password hash error: {0}")]
    PasswordHash(String),

    #[error("invalid credentials")]
    InvalidCredentials,

    #[error("user is inactive")]
    InactiveUser,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuperUserCredentials {
    pub username: String,
    pub email: String,
    pub password: String,
}

#[derive(Debug, Deserialize, SurrealValue)]
pub struct ExistingSuperUser {
    pub username: Option<String>,
    pub email: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
pub struct AuthUser {
    pub id: RecordId,
    pub username: Option<String>,
    pub email: Option<String>,
    pub display_name: Option<String>,
    /// Filename under `app_dir/users/<id>/` when a profile photo is set.
    #[serde(default)]
    pub avatar_file: Option<String>,
    pub hashed_password: Option<String>,
    pub is_super_user: bool,
    pub is_active: bool,
}

pub fn hash_password(plain: &str) -> Result<String, UserServiceError> {
    // Avoid SaltString::generate(OsRng): argon2's password-hash pulls
    // rand_core 0.6, while the workspace also has newer rand_core versions.
    let salt_bytes: [u8; 16] = rand::rng().random();
    let salt = SaltString::encode_b64(&salt_bytes)
        .map_err(|e| UserServiceError::PasswordHash(e.to_string()))?;
    Argon2::default()
        .hash_password(plain.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| UserServiceError::PasswordHash(e.to_string()))
}

pub fn verify_password(plain: &str, hashed: &str) -> Result<bool, UserServiceError> {
    let parsed =
        PasswordHash::new(hashed).map_err(|e| UserServiceError::PasswordHash(e.to_string()))?;
    Ok(Argon2::default()
        .verify_password(plain.as_bytes(), &parsed)
        .is_ok())
}

pub fn generate_password(length: usize) -> String {
    const CHARSET: &[u8] =
        b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789!@#$%";
    let mut rng = rand::rng();
    (0..length)
        .map(|_| {
            let idx = rng.random_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}

/// Returns `Some(existing)` if a superuser already exists.
pub async fn find_superuser(
    db: &Surreal<Client>,
) -> Result<Option<ExistingSuperUser>, UserServiceError> {
    let mut response = db
        .query("SELECT username, email FROM users WHERE is_super_user = true LIMIT 1;")
        .await?
        .check()?;
    let rows: Vec<ExistingSuperUser> = response.take(0)?;
    Ok(rows.into_iter().next())
}

/// Create the initial platform superuser. Caller must already be on the core DB.
pub async fn create_superuser(
    db: &Surreal<Client>,
    username: &str,
    email: &str,
    password: &str,
) -> Result<SuperUserCredentials, UserServiceError> {
    let hashed = hash_password(password)?;

    db.query(
        r#"
            CREATE users SET
                username = $username,
                email = $email,
                hashed_password = $hashed_password,
                display_name = $username,
                is_super_user = true,
                is_active = true,
                is_email_verified = true;
            "#,
    )
    .bind(("username", username.to_string()))
    .bind(("email", email.to_string()))
    .bind(("hashed_password", hashed))
    .await?
    .check()?;

    Ok(SuperUserCredentials {
        username: username.to_string(),
        email: email.to_string(),
        password: password.to_string(),
    })
}

/// Look up an active user by username or email for local login.
pub async fn find_user_by_login(
    db: &Surreal<Client>,
    login: &str,
) -> Result<Option<AuthUser>, UserServiceError> {
    let mut response = db
        .query(
            r#"
            SELECT id, username, email, display_name, avatar_file, hashed_password, is_super_user, is_active
            FROM users
            WHERE username = $login OR email = $login
            LIMIT 1;
            "#,
        )
        .bind(("login", login.to_string()))
        .await?
        .check()?;
    let rows: Vec<AuthUser> = response.take(0)?;
    Ok(rows.into_iter().next())
}

pub async fn authenticate_local(
    db: &Surreal<Client>,
    login: &str,
    password: &str,
) -> Result<AuthUser, UserServiceError> {
    let user = find_user_by_login(db, login).await?;
    // Every attempt costs one password hash, whether or not the account exists, so the time a
    // refusal takes does not say which usernames are real.
    let hashed = user
        .as_ref()
        .and_then(|user| user.hashed_password.clone())
        .unwrap_or_else(|| dummy_hash().to_string());
    let real = user.as_ref().is_some_and(|user| user.hashed_password.is_some());
    let plain = password.to_string();
    // Argon2 is slow on purpose: keep it off the threads that serve requests.
    let matches = tokio::task::spawn_blocking(move || verify_password(&plain, &hashed))
        .await
        .map_err(|error| UserServiceError::PasswordHash(error.to_string()))??;
    let user = match user {
        Some(user) if real && matches => user,
        _ => return Err(UserServiceError::InvalidCredentials),
    };
    // Only someone who knew the password learns the account is switched off.
    if !user.is_active {
        return Err(UserServiceError::InactiveUser);
    }

    let _ = db
        .query("UPDATE $id SET last_login_at = time::now();")
        .bind(("id", user.id.clone()))
        .await;

    Ok(user)
}

/// A hash nobody has the password of, checked when the username is unknown or has no password.
fn dummy_hash() -> &'static str {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let secret: [u8; 24] = rand::rng().random();
        hash_password(&format!("{secret:?}")).unwrap_or_default()
    })
}
