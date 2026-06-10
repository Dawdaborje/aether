use aether_orm::services::migrations::run_migrations;
use surrealdb::{Surreal, engine::remote::ws::Client};

pub async fn initialize_system(db_conn: &Surreal<Client>) {
    log::info!("Performing system initialization...");
    run_migrations(db_conn).await;
}
