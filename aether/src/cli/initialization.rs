use aether_orm::services::{
    MigrationError, SuperUserCredentials, UserServiceError, create_superuser, find_superuser,
    generate_password, migrate_core, migrate_org,
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

pub struct MigrateResult {
    pub applied_migrations: Vec<String>,
    pub namespace: String,
    pub database: String,
    /// Each organization database visited and the migrations applied to it (empty when it was
    /// already current). Empty when only the core database was asked for.
    pub organizations: Vec<(String, Vec<String>)>,
}

/// Apply pending migrations: the core database, then, when `organizations` is set, every
/// organization database listed in `org_databases` (they are otherwise migrated only when the
/// organization is created). Use `--init` for a first-time bootstrap (migrations + seeds +
/// developer account).
///
/// One organization failing does not stop the others; the first failure is returned at the end.
pub async fn migrate_system(
    db: &Surreal<Client>,
    namespace: &str,
    database: &str,
    organizations: bool,
) -> Result<MigrateResult, InitError> {
    println!("{}", "Migrating Aether…".bold());
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

    let mut migrated = Vec::new();
    let mut first_failure = None;
    if organizations {
        let mut response = db
            .query("SELECT VALUE db_name FROM org_databases ORDER BY db_name;")
            .await
            .map_err(MigrationError::from)?
            .check()
            .map_err(MigrationError::from)?;
        let names: Vec<String> = response.take(0).map_err(MigrationError::from)?;
        for name in names {
            match migrate_org(db, namespace, &name).await {
                Ok(applied) => {
                    if applied.is_empty() {
                        println!("  {}: {}", name.cyan(), "already up to date".green());
                    } else {
                        println!("  {}: {}", name.cyan(), format!("applied {}", applied.join(", ")).green());
                    }
                    migrated.push((name, applied));
                }
                Err(error) => {
                    println!("  {}: {}", name.cyan(), format!("failed: {error}").red());
                    first_failure.get_or_insert(error);
                }
            }
        }
        // Leave the connection on the core database, as the other commands expect.
        db.use_db(database).await.map_err(MigrationError::from)?;
    }
    if let Some(error) = first_failure {
        return Err(error.into());
    }

    Ok(MigrateResult {
        applied_migrations: applied,
        namespace: namespace.to_string(),
        database: database.to_string(),
        organizations: migrated,
    })
}

pub fn print_migrate_summary(result: &MigrateResult) {
    println!();
    println!("{}", "══════════════════════════════════════".green());
    println!("{}", "  Aether migrate complete".green().bold());
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
    if !result.organizations.is_empty() {
        let updated = result.organizations.iter().filter(|(_, applied)| !applied.is_empty()).count();
        println!(
            "  {}  {} updated, {} already current",
            "Organizations:".green().bold(),
            updated,
            result.organizations.len() - updated
        );
    }

    println!("{}", "══════════════════════════════════════".green());
    println!();
}

/// Bootstrap the platform core DB: apply migrations, seed settings, ensure the developer account
/// (stored as a superuser: the kernel skips RBAC checks for it).
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
                    "  developer: already exists ({})",
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
            println!("{}", "  developer: created".green());
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
        println!("{}", "  Developer credentials (save these):".green().bold());
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
            "Developer:".green().bold(),
            "already present — password not changed".dimmed()
        );
    }

    println!("{}", "══════════════════════════════════════".green());
    println!();
}
