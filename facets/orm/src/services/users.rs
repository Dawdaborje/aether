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
            SELECT id, username, email, display_name, hashed_password, is_super_user, is_active
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
    let user = find_user_by_login(db, login)
        .await?
        .ok_or(UserServiceError::InvalidCredentials)?;

    if !user.is_active {
        return Err(UserServiceError::InactiveUser);
    }

    let Some(hashed) = user.hashed_password.as_deref() else {
        return Err(UserServiceError::InvalidCredentials);
    };

    if !verify_password(password, hashed)? {
        return Err(UserServiceError::InvalidCredentials);
    }

    let _ = db
        .query("UPDATE $id SET last_login_at = time::now();")
        .bind(("id", user.id.clone()))
        .await;

    Ok(user)
}
