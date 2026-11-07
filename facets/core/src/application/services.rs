use aether_orm::{UserServiceError, find_user_by_login, hash_password};
use surrealdb::{Surreal, engine::remote::ws::Client};

pub fn initialize_plugins(plugins_paths: Vec<String>) {
    log::info!("Initializing plugins... {:?}", plugins_paths);
}

pub async fn change_user_password(
    username: &str,
    new_password: &str,
    db: &Surreal<Client>,
) -> Result<(), UserServiceError> {
    log::info!("Changing password for user: {}", username);

    let user = find_user_by_login(db, username)
        .await?
        .ok_or(UserServiceError::InvalidCredentials)?;
    let hashed_password = hash_password(new_password)?;

    db.query("UPDATE $id SET hashed_password = $hashed_password;")
        .bind(("id", user.id))
        .bind(("hashed_password", hashed_password))
        .await?
        .check()?;

    Ok(())
}
