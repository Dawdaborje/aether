//! Creating organizations and putting users in them.
//!
//! Shared by the command line (`--create-org`, `--assign-user`) and the web app, so both
//! do exactly the same work.

use aether_orm::models::core::{
    CoreUser, NewCoreUser, NewOrgDatabase, NewOrganization, OrgUser, Organization,
    OrganizationUser, TenantOrganization,
};
use aether_orm::{hash_password, migrate_org};
use surrealdb::{
    Surreal,
    engine::remote::ws::Client,
    types::{Datetime, SurrealValue, ToSql},
};
use std::sync::Arc;

use crate::{
    app_dir::AppDir,
    org_storage::{OrgStorage, OrgStorageError, provision_org_storage},
};
use aether_storage::MediaBackend;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum OrganizationError {
    #[error("surrealdb error: {0}")]
    Surreal(#[from] surrealdb::Error),

    #[error("organization name cannot be empty")]
    EmptyOrganizationName,

    #[error("user username, email, and password are required")]
    MissingUserCredentials,

    #[error("organization `{0}` already exists")]
    AlreadyExists(String),

    #[error("invalid organization database name: {0}")]
    InvalidDatabaseName(String),

    #[error("password hashing failed: {0}")]
    PasswordHash(String),

    #[error("user creation returned no record id")]
    UserCreationReturnedNoId,

    #[error("user `{0}` was not found")]
    UserNotFound(String),

    #[error("organization database `{0}` was not found")]
    OrganizationNotFound(String),

    #[error("organization migration failed: {0}")]
    Migration(#[from] aether_orm::MigrationError),

    #[error("could not set up the organization's storage: {0}")]
    Storage(#[from] OrgStorageError),
}

#[derive(Debug, SurrealValue)]
struct OrgDatabaseRow {
    #[allow(dead_code)]
    db_name: String,
}

/// Where an organization's files go: its folder in `app_dir` and its place in
/// the media backend.
pub struct StorageTarget {
    pub app_dir: AppDir,
    pub media: Arc<dyn MediaBackend>,
}

/// Who the organization starts with.
#[derive(Clone, Copy)]
pub enum FirstMember<'a> {
    /// A user, created now unless one with this username or email already exists.
    User {
        username: &'a str,
        email: &'a str,
        password: &'a str,
    },
    /// A user that already exists, found by username or email.
    Existing { login: &'a str },
}

/// What to create for a new organization.
pub struct OrganizationRequest<'a> {
    pub name: &'a str,
    /// Database name; defaults to a slug of `name`.
    pub db_name: Option<&'a str>,
    pub member: FirstMember<'a>,
}

/// Database names that belong to the kernel and cannot be an organization's.
const RESERVED_DATABASES: &[&str] = &["core"];

/// Create an organization: its storage first (the folder in `app_dir` and its
/// media prefix, so an unwritable location fails before anything exists), then
/// its first user, its core records and its database.
pub async fn create_organization(
    db: &Surreal<Client>,
    namespace: &str,
    request: &OrganizationRequest<'_>,
    storage: &StorageTarget,
) -> Result<(String, OrgStorage), OrganizationError> {
    let OrganizationRequest {
        name: organization_name,
        db_name: organization_db_name,
        member,
    } = *request;
    if organization_name.trim().is_empty() {
        return Err(OrganizationError::EmptyOrganizationName);
    }
    if let FirstMember::User { username, email, password } = member
        && (username.trim().is_empty() || email.trim().is_empty() || password.is_empty())
    {
        return Err(OrganizationError::MissingUserCredentials);
    }

    let db_name = organization_db_name
        .map(str::to_owned)
        .unwrap_or_else(|| slugify(organization_name));
    if db_name.is_empty()
        || RESERVED_DATABASES.contains(&db_name.as_str())
        || !db_name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return Err(OrganizationError::InvalidDatabaseName(db_name));
    }

    // Checked before anything is created, so a taken name leaves nothing behind.
    db.use_ns(namespace).await?;
    db.use_db("core").await?;
    let mut taken = db
        .query("SELECT db_name FROM org_databases WHERE db_name = $name LIMIT 1;")
        .bind(("name", db_name.clone()))
        .await?
        .check()?;
    let taken: Vec<OrgDatabaseRow> = taken.take(0)?;
    if !taken.is_empty() {
        return Err(OrganizationError::AlreadyExists(db_name));
    }
    // The user is looked up before anything is created too.
    let existing_member = match member {
        FirstMember::Existing { login } => Some(
            find_existing_user_id(db, login, login)
                .await?
                .ok_or_else(|| OrganizationError::UserNotFound(login.to_string()))?,
        ),
        FirstMember::User { .. } => None,
    };

    let org_storage =
        provision_org_storage(&storage.app_dir, storage.media.clone(), &db_name).await?;
    log::info!(
        "Organization storage ready: files in {}, media under `{}`",
        org_storage.directory.display(),
        org_storage.media_prefix
    );

    db.use_ns(namespace).await?;
    db.use_db("core").await?;

    let (user_id, username) = match (member, existing_member) {
        (FirstMember::Existing { login }, Some(user_id)) => (user_id, login),
        (FirstMember::User { username, email, password }, _) => {
            let id = if let Some(existing_user_id) = find_existing_user_id(db, username, email).await? {
                log::info!("Using existing user for organization membership: {username}");
                existing_user_id
            } else {
                let hashed_password = hash_password(password)
                    .map_err(|err| OrganizationError::PasswordHash(err.to_string()))?;
                let now = Datetime::now();
                let _: Option<CoreUser> = db
                    .create::<Option<CoreUser>>("users")
                    .content(NewCoreUser {
                        username: username.to_string(),
                        email: email.to_string(),
                        display_name: username.to_string(),
                        hashed_password,
                        is_super_user: false,
                        is_active: true,
                        is_email_verified: false,
                        date_created: now.clone(),
                        date_updated: now,
                    })
                    .await?;

                find_existing_user_id(db, username, email)
                    .await?
                    .ok_or(OrganizationError::UserCreationReturnedNoId)?
            };
            (id, username)
        }
        (FirstMember::Existing { login }, None) => {
            return Err(OrganizationError::UserNotFound(login.to_string()));
        }
    };

    let now = Datetime::now();
    let _: Option<Organization> = db
        .create::<Option<Organization>>(("organizations", db_name.clone()))
        .content(NewOrganization {
            name: organization_name.to_string(),
            db_name: db_name.clone(),
            date_created: now.clone(),
            date_updated: now.clone(),
        })
        .await?;
    let _: Option<OrganizationUser> = db
        .upsert::<Option<OrganizationUser>>((
            "organization_users",
            format!("{}_{}", db_name, slugify(&user_id)),
        ))
        .content(OrganizationUser {
            organization_id: format!("organizations:{db_name}"),
            user_id: user_id.clone(),
            date_created: now.clone(),
        })
        .await?;
    let _: Option<aether_orm::models::core::OrgDatabase> = db
        .upsert::<Option<aether_orm::models::core::OrgDatabase>>(("org_databases", db_name.clone()))
        .content(NewOrgDatabase {
            db_name: db_name.clone(),
            date_created: now.clone(),
            date_updated: now,
        })
        .await?;

    create_and_migrate_org_database(db, namespace, &db_name).await?;
    db.use_ns(namespace).await?;
    db.use_db(&db_name).await?;
    let now = Datetime::now();
    let _: Option<TenantOrganization> = db
        .create::<Option<TenantOrganization>>("organizations")
        .content(TenantOrganization {
            name: organization_name.to_string(),
            date_created: now.clone(),
            date_updated: now.clone(),
        })
        .await?;
    let _: Option<OrgUser> = db
        .create::<Option<OrgUser>>(("org_users", slugify(&user_id)))
        .content(OrgUser {
            core_user_id: user_id,
            display_name: Some(username.to_string()),
            is_active: true,
            date_created: now.clone(),
            date_updated: now,
        })
        .await?;

    Ok((db_name, org_storage))
}

/// Create the folder and media prefix for an organization that already exists
/// (for example one created before storage was provisioned). Safe to repeat.
pub async fn provision_existing_organization(
    db: &Surreal<Client>,
    namespace: &str,
    organization: &str,
    storage: &StorageTarget,
) -> Result<OrgStorage, OrganizationError> {
    db.use_ns(namespace).await?;
    db.use_db("core").await?;
    let mut response = db
        .query("SELECT db_name FROM org_databases WHERE db_name = $org LIMIT 1;")
        .bind(("org", organization.to_string()))
        .await?
        .check()?;
    let found: Vec<OrgDatabaseRow> = response.take(0)?;
    if found.is_empty() {
        return Err(OrganizationError::OrganizationNotFound(organization.to_string()));
    }
    Ok(provision_org_storage(&storage.app_dir, storage.media.clone(), organization).await?)
}

pub async fn assign_user(
    db: &Surreal<Client>,
    namespace: &str,
    user_login: &str,
    organization_db_name: &str,
) -> Result<(), OrganizationError> {
    db.use_ns(namespace).await?;
    db.use_db("core").await?;

    let users: Vec<CoreUser> = db.select("users").await?;
    let user_id = users
        .into_iter()
        .find(|user| {
            user.username.as_deref() == Some(user_login)
                || user.email.as_deref() == Some(user_login)
        })
        .map(|user| record_id_string(&user.id))
        .ok_or_else(|| OrganizationError::UserNotFound(user_login.to_string()))?;
    let organizations: Vec<Organization> = db.select("organizations").await?;
    let organization_id = organizations
        .into_iter()
        .find(|organization| organization.db_name == organization_db_name)
        .map(|organization| record_id_string(&organization.id))
        .ok_or_else(|| OrganizationError::OrganizationNotFound(organization_db_name.to_string()))?;

    let membership_key = slugify(&user_id);
    let _: Option<OrganizationUser> = db
        .upsert::<Option<OrganizationUser>>((
            "organization_users",
            format!("{}_{}", slugify(organization_db_name), membership_key),
        ))
        .content(OrganizationUser {
            organization_id,
            user_id: user_id.clone(),
            date_created: Datetime::now(),
        })
        .await?;
    create_and_migrate_org_database(db, namespace, organization_db_name).await?;
    db.use_ns(namespace).await?;
    db.use_db(organization_db_name).await?;
    let _: Option<OrgUser> = db
        .upsert::<Option<OrgUser>>(("org_users", membership_key))
        .content(OrgUser {
            core_user_id: user_id,
            display_name: Some(user_login.to_string()),
            is_active: true,
            date_created: Datetime::now(),
            date_updated: Datetime::now(),
        })
        .await?;

    Ok(())
}

async fn find_existing_user_id(
    db: &Surreal<Client>,
    username: &str,
    email: &str,
) -> Result<Option<String>, OrganizationError> {
    let users: Vec<CoreUser> = db.select("users").await?;
    Ok(users
        .into_iter()
        .find(|user| {
            user.username.as_deref() == Some(username) || user.email.as_deref() == Some(email)
        })
        .map(|user| record_id_string(&user.id)))
}

fn record_id_string(id: &surrealdb::types::RecordId) -> String {
    format!("{}:{}", id.table.as_str(), id.key.to_sql())
}

/// Selecting a new SurrealDB database creates it when it does not exist.
/// Migrations then establish the complete tenant schema before tenant records
/// are inserted.
async fn create_and_migrate_org_database(
    db: &Surreal<Client>,
    namespace: &str,
    database: &str,
) -> Result<(), OrganizationError> {
    db.use_ns(namespace).await?;
    db.use_db(database).await?;
    log::info!("Created or selected organization database: {namespace}/{database}");

    let applied = migrate_org(db, namespace, database).await?;
    if applied.is_empty() {
        log::info!("Organization database {database} migrations are up to date");
    } else {
        log::info!(
            "Applied organization database migrations to {database}: {}",
            applied.join(", ")
        );
    }

    Ok(())
}

pub fn slugify(value: &str) -> String {
    value
        .trim()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>()
        .split('_')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("_")
}

#[cfg(test)]
mod tests {
    use super::slugify;

    #[test]
    fn slugifies_organization_names() {
        assert_eq!(slugify("Acme Corporation"), "acme_corporation");
    }

    #[test]
    fn collapses_punctuation_and_whitespace() {
        assert_eq!(slugify("Acme  Holdings, Ltd."), "acme_holdings_ltd");
    }
}
