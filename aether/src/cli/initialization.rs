use aether_orm::services::{
    MigrationError, SuperUserCredentials, UserServiceError, create_superuser, find_superuser,
    generate_password, migrate_core,
};
use colored::Colorize;
use surrealdb::{Surreal, engine::remote::ws::Client};
use thiserror::Error;

use super::seed::{SeedError, seed_system};

#[derive(Debug, Error)]
pub enum InitError {
    #[error(transparent)]
    Migration(#[from] MigrationError),

    #[error(transparent)]
    User(#[from] UserServiceError),

    #[error(transparent)]
    Seed(#[from] SeedError),
}

pub struct BootstrapResult {
    pub applied_migrations: Vec<String>,
    pub namespace: String,
    pub database: String,
    pub superuser: Option<SuperUserCredentials>,
    pub superuser_already_existed: bool,
}

/// Bootstrap the platform core DB: apply migrations, seed settings, ensure superuser.
pub async fn initialize_system(
    db: &Surreal<Client>,
    namespace: &str,
    database: &str,
    admin_username: Option<&str>,
    admin_email: Option<&str>,
    admin_password: Option<&str>,
) -> Result<BootstrapResult, InitError> {
    println!("{}", "Bootstrapping Aether…".bold());
    println!("  target: {} / {}", namespace.cyan(), database.cyan());

    let applied = migrate_core(db, namespace, database).await?;

    if applied.is_empty() {
        println!("{}", "  migrations: already up to date".green());
    } else {
        println!(
            "{}",
            format!("  migrations: applied {}", applied.join(", ")).green()
        );
    }

    println!("{}", "  seeding global settings…".dimmed());
    let seeded = seed_system(db, namespace, database).await?;
    if seeded.is_empty() {
        println!("{}", "  seeds: already up to date".green());
    } else {
        println!(
            "{}",
            format!("  seeds: applied {}", seeded.join(", ")).green()
        );
    }

    let (superuser, already_existed) = match find_superuser(db).await? {
        Some(existing) => {
            println!(
                "{}",
                format!(
                    "  superuser: already exists ({})",
                    existing
                        .username
                        .or(existing.email)
                        .unwrap_or_else(|| "unknown".into())
                )
                .yellow()
            );
            (None, true)
        }
        None => {
            let username = admin_username.unwrap_or("admin");
            let email = admin_email.unwrap_or("admin@localhost");
            let password = admin_password
                .map(str::to_owned)
                .unwrap_or_else(|| generate_password(20));
            let creds = create_superuser(db, username, email, &password).await?;
            println!("{}", "  superuser: created".green());
            (Some(creds), false)
        }
    };

    Ok(BootstrapResult {
        applied_migrations: applied,
        namespace: namespace.to_string(),
        database: database.to_string(),
        superuser,
        superuser_already_existed: already_existed,
    })
}

pub fn print_bootstrap_summary(result: &BootstrapResult) {
    println!();
    println!("{}", "══════════════════════════════════════".green());
    println!("{}", "  Aether init complete".green().bold());
    println!("{}", "══════════════════════════════════════".green());
    println!(
        "  {}  {} / {}",
        "Database:".green().bold(),
        result.namespace,
        result.database
    );

    if result.applied_migrations.is_empty() {
        println!(
            "  {}  {}",
            "Migrations:".green().bold(),
            "none pending".dimmed()
        );
    } else {
        println!(
            "  {}  {}",
            "Migrations:".green().bold(),
            result.applied_migrations.join(", ")
        );
    }

    if let Some(creds) = &result.superuser {
        println!();
        println!("{}", "  Superuser credentials (save these):".green().bold());
        println!(
            "  {}  {}",
            "Username:".green(),
            creds.username.bright_white()
        );
        println!("  {}  {}", "Email:".green(), creds.email.bright_white());
        println!(
            "  {}  {}",
            "Password:".green(),
            creds.password.bright_white().bold()
        );
        println!();
        println!(
            "{}",
            "  This password is shown once. Change it after first login.".yellow()
        );
    } else if result.superuser_already_existed {
        println!(
            "  {}  {}",
            "Superuser:".green().bold(),
            "already present — password not changed".dimmed()
        );
    }

    println!("{}", "══════════════════════════════════════".green());
    println!();
}
