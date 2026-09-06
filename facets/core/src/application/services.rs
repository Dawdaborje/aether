use surrealdb::{Surreal, engine::remote::ws::Client};

pub fn initialize_plugins(plugins_paths: Vec<String>) {
    log::info!("Initializing plugins... {:?}", plugins_paths);
}

pub async fn change_user_password(username: &str, new_password: &str, db: &Surreal<Client>) {
    log::info!("Changing password for user: {}", username);
}
