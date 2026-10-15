//! Creating organizations and putting users in them.
//!
//! Shared by the command line (`--create-org`, `--assign-user`) and the web app, so both
//! do exactly the same work.

use aether_orm::models::core::{
    CoreUser, NewCoreUser, NewOrgDatabase, NewOrganization, OrgUser, Organization,
    OrganizationUser, TenantOrganization,
};
use aether_orm::{hash_password, migrate_org};
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use surrealdb::{
    Surreal,
    engine::remote::ws::Client,
    types::{Datetime, SurrealValue, ToSql},
};

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

    #[error("could not give the administrator role: {0}")]
    Role(String),

    #[error("a user called `{0}` (or with that email) already exists")]
    UserExists(String),

    #[error("an account with this email already exists")]
    AccountAlreadyExists,

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

    #[error("only the organization owner can transfer ownership")]
    NotOwner,

    #[error("`{0}` is already the owner of this organization")]
    AlreadyOwner(String),

    #[error("`{0}` is not a member of organization `{1}`")]
    NotAMember(String, String),

    #[error("parent organization `{0}` was not found")]
    ParentNotFound(String),

    #[error("an organization cannot be its own parent")]
    ParentIsSelf,

    #[error("setting that parent would create a cycle")]
    ParentCycle,

    #[error("invalid logo: {0}")]
    InvalidLogo(String),

    #[error("filesystem error: {0}")]
    Io(String),

    #[error("not allowed to manage users in this organization")]
    Forbidden,

    #[error("only an administrator or developer may grant or revoke the administrator role")]
    CannotManageAdminRole,

    #[error("only the organization owner or a developer may do that")]
    OwnerOrDeveloperRequired,

    #[error("the organization owner cannot be removed or deactivated")]
    CannotTargetOwner,

    #[error("you cannot perform that action on your own account")]
    CannotTargetSelf,
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
    pub description: Option<&'a str>,
    /// Parent organization's database name, if any.
    pub parent_db_name: Option<&'a str>,
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
        description,
        parent_db_name,
    } = *request;
    if organization_name.trim().is_empty() {
        return Err(OrganizationError::EmptyOrganizationName);
    }
    if let FirstMember::User {
        username,
        email,
        password,
    } = member
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
    let parent = normalize_optional(parent_db_name);
    if let Some(ref parent) = parent {
        validate_parent(db, &db_name, parent).await?;
    }
    let description = normalize_optional(description);
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
        (
            FirstMember::User {
                username,
                email,
                password,
            },
            _,
        ) => {
            let id =
                if let Some(existing_user_id) = find_existing_user_id(db, username, email).await? {
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
            owner_user_id: user_id.clone(),
            description: description.clone(),
            parent_db_name: parent.clone(),
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
            deleted_at: None,
            deleted_by: None,
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
    let user_id_for_role = user_id.clone();
    let _: Option<OrgUser> = db
        .create::<Option<OrgUser>>(("org_users", slugify(&user_id)))
        .content(OrgUser {
            core_user_id: user_id,
            display_name: Some(username.to_string()),
            is_active: true,
            date_created: now.clone(),
            date_updated: now,
            deleted_at: None,
            deleted_by: None,
        })
        .await?;
    // First member owns the organization and is its administrator.
    crate::roles::ensure_system_roles(db)
        .await
        .map_err(|error| OrganizationError::Role(error.to_string()))?;
    crate::roles::grant_by_id(db, &user_id_for_role, crate::roles::ORG_ADMIN)
        .await
        .map_err(|error| OrganizationError::Role(error.to_string()))?;

    Ok((db_name, org_storage))
}

#[derive(Debug, Deserialize, SurrealValue)]
struct OrgOwnerRow {
    owner_user_id: Option<String>,
}

#[derive(Debug, Deserialize, SurrealValue)]
struct MembershipRow {
    user_id: String,
}

/// Transfer organization ownership. The previous owner keeps `org_admin`; the new
/// owner is granted it. Every transfer is appended to `org_ownership_transfers`.
pub async fn transfer_ownership(
    db: &Surreal<Client>,
    namespace: &str,
    organization_db_name: &str,
    actor_user_id: &str,
    to_login: &str,
    note: Option<&str>,
) -> Result<(), OrganizationError> {
    db.use_ns(namespace).await?;
    db.use_db("core").await?;

    let mut response = db
        .query("SELECT owner_user_id FROM organizations WHERE db_name = $org LIMIT 1;")
        .bind(("org", organization_db_name.to_string()))
        .await?
        .check()?;
    let rows: Vec<OrgOwnerRow> = response.take(0)?;
    let Some(org) = rows.into_iter().next() else {
        return Err(OrganizationError::OrganizationNotFound(
            organization_db_name.to_string(),
        ));
    };

    let mut owner_id = org.owner_user_id.filter(|id| !id.is_empty());
    if owner_id.is_none() {
        // Pre-ownership orgs: first member becomes owner so transfer can proceed.
        owner_id = backfill_owner(db, organization_db_name).await?;
    }
    let from_user_id = owner_id
        .ok_or_else(|| OrganizationError::OrganizationNotFound(organization_db_name.to_string()))?;

    if from_user_id != actor_user_id {
        return Err(OrganizationError::NotOwner);
    }

    let to_user_id = find_existing_user_id(db, to_login, to_login)
        .await?
        .ok_or_else(|| OrganizationError::UserNotFound(to_login.to_string()))?;
    if to_user_id == from_user_id {
        return Err(OrganizationError::AlreadyOwner(to_login.to_string()));
    }

    let organization_id = format!("organizations:{organization_db_name}");
    let mut members = db
        .query(
            "SELECT user_id FROM organization_users WHERE organization_id = $org AND user_id = $user LIMIT 1;",
        )
        .bind(("org", organization_id.clone()))
        .bind(("user", to_user_id.clone()))
        .await?
        .check()?;
    let member_rows: Vec<MembershipRow> = members.take(0)?;
    if member_rows.is_empty() {
        return Err(OrganizationError::NotAMember(
            to_login.to_string(),
            organization_db_name.to_string(),
        ));
    }

    let note = note
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string);
    db.query(
        "UPDATE organizations SET owner_user_id = $to, date_updated = time::now() WHERE db_name = $org;\
         CREATE org_ownership_transfers CONTENT {\
           organization_id: $organization_id,\
           from_user_id: $from,\
           to_user_id: $to,\
           transferred_by: $actor,\
           note: $note\
         };",
    )
    .bind(("org", organization_db_name.to_string()))
    .bind(("organization_id", organization_id))
    .bind(("from", from_user_id))
    .bind(("to", to_user_id.clone()))
    .bind(("actor", actor_user_id.to_string()))
    .bind(("note", note))
    .await?
    .check()?;

    db.use_db(organization_db_name).await?;
    crate::roles::grant_by_id(db, &to_user_id, crate::roles::ORG_ADMIN)
        .await
        .map_err(|error| OrganizationError::Role(error.to_string()))?;

    Ok(())
}

/// When `owner_user_id` is missing, set it from the earliest membership.
async fn backfill_owner(
    db: &Surreal<Client>,
    organization_db_name: &str,
) -> Result<Option<String>, OrganizationError> {
    let organization_id = format!("organizations:{organization_db_name}");
    let mut response = db
        .query(
            "SELECT user_id FROM organization_users WHERE organization_id = $org ORDER BY date_created ASC LIMIT 1;",
        )
        .bind(("org", organization_id))
        .await?
        .check()?;
    let rows: Vec<MembershipRow> = response.take(0)?;
    let Some(first) = rows.into_iter().next() else {
        return Ok(None);
    };
    db.query(
        "UPDATE organizations SET owner_user_id = $user, date_updated = time::now() WHERE db_name = $org;",
    )
    .bind(("user", first.user_id.clone()))
    .bind(("org", organization_db_name.to_string()))
    .await?
    .check()?;
    Ok(Some(first.user_id))
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
        return Err(OrganizationError::OrganizationNotFound(
            organization.to_string(),
        ));
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
    let link_key = format!("{}_{}", slugify(organization_db_name), membership_key);
    let _: Option<OrganizationUser> = db
        .upsert::<Option<OrganizationUser>>(("organization_users", link_key.clone()))
        .content(OrganizationUser {
            organization_id: organization_id.clone(),
            user_id: user_id.clone(),
            date_created: Datetime::now(),
            deleted_at: None,
            deleted_by: None,
        })
        .await?;
    // Clear a prior soft-kick if the unique key already existed.
    db.query(
        "UPDATE type::record('organization_users', $key) SET deleted_at = NONE, deleted_by = NONE;",
    )
    .bind(("key", link_key))
    .await?
    .check()?;
    create_and_migrate_org_database(db, namespace, organization_db_name).await?;
    db.use_ns(namespace).await?;
    db.use_db(organization_db_name).await?;
    let _: Option<OrgUser> = db
        .upsert::<Option<OrgUser>>(("org_users", membership_key.clone()))
        .content(OrgUser {
            core_user_id: user_id.clone(),
            display_name: Some(user_login.to_string()),
            is_active: true,
            date_created: Datetime::now(),
            date_updated: Datetime::now(),
            deleted_at: None,
            deleted_by: None,
        })
        .await?;
    db.query(
        "UPDATE type::record('org_users', $key) SET deleted_at = NONE, deleted_by = NONE, is_active = true; \
         UPDATE org_user_roles SET deleted_at = NONE, deleted_by = NONE \
         WHERE org_user = type::record('org_users', $key);",
    )
    .bind(("key", membership_key))
    .await?
    .check()?;

    Ok(())
}

/// Create a login account. It belongs to no organization until `assign_user` puts it in one.
pub async fn create_user(
    db: &Surreal<Client>,
    namespace: &str,
    username: &str,
    email: &str,
    password: &str,
) -> Result<String, OrganizationError> {
    if username.trim().is_empty() || email.trim().is_empty() || password.is_empty() {
        return Err(OrganizationError::MissingUserCredentials);
    }
    db.use_ns(namespace).await?;
    db.use_db("core").await?;
    if find_existing_user_id(db, username, email).await?.is_some() {
        return Err(OrganizationError::UserExists(username.to_string()));
    }
    let hashed_password =
        hash_password(password).map_err(|err| OrganizationError::PasswordHash(err.to_string()))?;
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
        .ok_or(OrganizationError::UserCreationReturnedNoId)
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

fn normalize_optional(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string)
}

async fn validate_parent(
    db: &Surreal<Client>,
    child_db_name: &str,
    parent_db_name: &str,
) -> Result<(), OrganizationError> {
    if parent_db_name == child_db_name {
        return Err(OrganizationError::ParentIsSelf);
    }
    let mut response = db
        .query("SELECT db_name, parent_db_name FROM organizations WHERE db_name = $parent LIMIT 1;")
        .bind(("parent", parent_db_name.to_string()))
        .await?
        .check()?;
    let parents: Vec<OrgParentRow> = response.take(0)?;
    if parents.is_empty() {
        return Err(OrganizationError::ParentNotFound(
            parent_db_name.to_string(),
        ));
    }
    // Walk ancestors; refuse if we meet the child.
    let mut cursor = parents[0].parent_db_name.clone();
    let mut guard = 0;
    while let Some(next) = cursor {
        if next == child_db_name {
            return Err(OrganizationError::ParentCycle);
        }
        guard += 1;
        if guard > 64 {
            return Err(OrganizationError::ParentCycle);
        }
        let mut response = db
            .query("SELECT parent_db_name FROM organizations WHERE db_name = $org LIMIT 1;")
            .bind(("org", next))
            .await?
            .check()?;
        let rows: Vec<OrgParentOnly> = response.take(0)?;
        cursor = rows.into_iter().next().and_then(|row| row.parent_db_name);
    }
    Ok(())
}

#[derive(Debug, Deserialize, SurrealValue)]
struct OrgParentRow {
    #[allow(dead_code)]
    db_name: String,
    parent_db_name: Option<String>,
}

#[derive(Debug, Deserialize, SurrealValue)]
struct OrgParentOnly {
    parent_db_name: Option<String>,
}

/// Organization as shown on the desk list and detail pages.
#[derive(Debug, Clone, Serialize)]
pub struct OrganizationProfile {
    pub db_name: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_db_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logo_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_login: Option<String>,
    pub member_count: u64,
    pub date_created: String,
    pub date_updated: String,
}

#[derive(Debug, Deserialize, SurrealValue)]
struct OrgProfileRow {
    name: String,
    db_name: String,
    description: Option<String>,
    parent_db_name: Option<String>,
    logo_file: Option<String>,
    owner_user_id: Option<String>,
    date_created: Option<Datetime>,
    date_updated: Option<Datetime>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OrgMemberRow {
    pub user_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    pub is_owner: bool,
    /// Active in this organization (`org_users.is_active`).
    pub is_active: bool,
    pub date_joined: String,
    /// Role names held in this organization.
    #[serde(default)]
    pub roles: Vec<String>,
    /// A developer for this organization only (`organization_users.is_developer`); unrestricted
    /// developers are `users.is_super_user` and are not marked here.
    #[serde(default)]
    pub is_developer: bool,
}

/// A platform user for the developer Users page.
#[derive(Debug, Clone, Serialize)]
pub struct PlatformUserRow {
    pub user_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    pub is_developer: bool,
    pub is_active: bool,
    /// Organization database names this user belongs to.
    pub organizations: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OwnershipTransferRow {
    pub from_user_id: String,
    pub to_user_id: String,
    pub transferred_by: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_login: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_login: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub date_created: String,
}

/// Fields a developer may change on an organization's profile.
pub struct OrganizationUpdate<'a> {
    pub name: Option<&'a str>,
    pub description: Option<Option<&'a str>>,
    pub parent_db_name: Option<Option<&'a str>>,
}

fn logo_url_for(db_name: &str, logo_file: Option<&str>) -> Option<String> {
    logo_file
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(|_| format!("/api/ui/organizations/{db_name}/logo"))
}

fn format_dt(value: Option<Datetime>) -> String {
    value.map(|dt| dt.to_string()).unwrap_or_default()
}

async fn login_for(
    db: &Surreal<Client>,
    user_id: &str,
) -> Result<Option<String>, OrganizationError> {
    let users: Vec<CoreUser> = db.select("users").await?;
    Ok(users
        .into_iter()
        .find(|user| record_id_string(&user.id) == user_id)
        .and_then(|user| user.username.or(user.email)))
}

async fn profile_from_row(
    db: &Surreal<Client>,
    row: OrgProfileRow,
) -> Result<OrganizationProfile, OrganizationError> {
    let parent_name = if let Some(ref parent) = row.parent_db_name {
        let mut response = db
            .query("SELECT name FROM organizations WHERE db_name = $org LIMIT 1;")
            .bind(("org", parent.clone()))
            .await?
            .check()?;
        #[derive(Debug, Deserialize, SurrealValue)]
        struct NameRow {
            name: String,
        }
        let names: Vec<NameRow> = response.take(0)?;
        names.into_iter().next().map(|r| r.name)
    } else {
        None
    };
    let owner_login = match row.owner_user_id.as_deref() {
        Some(id) => login_for(db, id).await?,
        None => None,
    };
    let organization_id = format!("organizations:{}", row.db_name);
    let mut response = db
        .query("SELECT user_id FROM organization_users WHERE organization_id = $org;")
        .bind(("org", organization_id))
        .await?
        .check()?;
    let members: Vec<MembershipRow> = response.take(0)?;
    let member_count = members.len() as u64;
    Ok(OrganizationProfile {
        logo_url: logo_url_for(&row.db_name, row.logo_file.as_deref()),
        db_name: row.db_name,
        name: row.name,
        description: row.description,
        parent_db_name: row.parent_db_name,
        parent_name,
        owner_user_id: row.owner_user_id,
        owner_login,
        member_count,
        date_created: format_dt(row.date_created),
        date_updated: format_dt(row.date_updated),
    })
}

/// Every organization in the catalog (desk list).
pub async fn list_profiles(
    db: &Surreal<Client>,
    namespace: &str,
) -> Result<Vec<OrganizationProfile>, OrganizationError> {
    db.use_ns(namespace).await?;
    db.use_db("core").await?;
    let mut response = db
        .query(
            "SELECT name, db_name, description, parent_db_name, logo_file, owner_user_id, \
             date_created, date_updated FROM organizations ORDER BY name;",
        )
        .await?
        .check()?;
    let rows: Vec<OrgProfileRow> = response.take(0)?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        out.push(profile_from_row(db, row).await?);
    }
    Ok(out)
}

/// One organization by database name.
pub async fn get_profile(
    db: &Surreal<Client>,
    namespace: &str,
    db_name: &str,
) -> Result<OrganizationProfile, OrganizationError> {
    db.use_ns(namespace).await?;
    db.use_db("core").await?;
    let mut response = db
        .query(
            "SELECT name, db_name, description, parent_db_name, logo_file, owner_user_id, \
             date_created, date_updated FROM organizations WHERE db_name = $org LIMIT 1;",
        )
        .bind(("org", db_name.to_string()))
        .await?
        .check()?;
    let rows: Vec<OrgProfileRow> = response.take(0)?;
    let row = rows
        .into_iter()
        .next()
        .ok_or_else(|| OrganizationError::OrganizationNotFound(db_name.to_string()))?;
    profile_from_row(db, row).await
}

/// Update name, description and/or parent.
pub async fn update_profile(
    db: &Surreal<Client>,
    namespace: &str,
    db_name: &str,
    update: OrganizationUpdate<'_>,
) -> Result<OrganizationProfile, OrganizationError> {
    db.use_ns(namespace).await?;
    db.use_db("core").await?;
    let _ = get_profile(db, namespace, db_name).await?;

    if let Some(name) = update.name {
        let name = name.trim();
        if name.is_empty() {
            return Err(OrganizationError::EmptyOrganizationName);
        }
        db.query(
            "UPDATE organizations SET name = $name, date_updated = time::now() WHERE db_name = $org;",
        )
        .bind(("name", name.to_string()))
        .bind(("org", db_name.to_string()))
        .await?
        .check()?;
    }
    if let Some(description) = update.description {
        let description = description
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_string);
        db.query(
            "UPDATE organizations SET description = $description, date_updated = time::now() WHERE db_name = $org;",
        )
        .bind(("description", description))
        .bind(("org", db_name.to_string()))
        .await?
        .check()?;
    }
    if let Some(parent) = update.parent_db_name {
        let parent = parent
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_string);
        if let Some(ref parent) = parent {
            validate_parent(db, db_name, parent).await?;
        }
        db.query(
            "UPDATE organizations SET parent_db_name = $parent, date_updated = time::now() WHERE db_name = $org;",
        )
        .bind(("parent", parent))
        .bind(("org", db_name.to_string()))
        .await?
        .check()?;
    }
    get_profile(db, namespace, db_name).await
}

/// Save a logo from base64 bytes into `app_dir/orgs/<db>/logo.<ext>`.
pub async fn set_logo(
    db: &Surreal<Client>,
    namespace: &str,
    app_dir: &AppDir,
    db_name: &str,
    filename: &str,
    data_base64: &str,
) -> Result<OrganizationProfile, OrganizationError> {
    let _ = get_profile(db, namespace, db_name).await?;
    let from_name = std::path::Path::new(filename)
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let ext = match from_name.as_str() {
        "png" | "jpg" | "jpeg" | "webp" | "gif" | "svg" | "ico" | "jfif" => {
            if from_name == "jfif" {
                "jpg".to_string()
            } else {
                from_name
            }
        }
        _ => {
            let mime = data_base64
                .strip_prefix("data:")
                .and_then(|rest| rest.split(';').next())
                .unwrap_or("")
                .to_ascii_lowercase();
            match mime.as_str() {
                "image/png" => "png".into(),
                "image/jpeg" | "image/jpg" => "jpg".into(),
                "image/webp" => "webp".into(),
                "image/gif" => "gif".into(),
                "image/svg+xml" => "svg".into(),
                "image/x-icon" | "image/vnd.microsoft.icon" => "ico".into(),
                _ => {
                    return Err(OrganizationError::InvalidLogo(
                        "use a png, jpg, webp, gif, svg or ico file".into(),
                    ));
                }
            }
        }
    };
    let raw = data_base64
        .split(',')
        .next_back()
        .unwrap_or(data_base64)
        .trim();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(raw)
        .map_err(|_| OrganizationError::InvalidLogo("the file data is not valid base64".into()))?;
    if bytes.is_empty() || bytes.len() > 2_000_000 {
        return Err(OrganizationError::InvalidLogo(
            "logo must be between 1 byte and 2 MB".into(),
        ));
    }
    let directory = app_dir
        .ensure_org(db_name)
        .await
        .map_err(|error| OrganizationError::Io(error.to_string()))?;
    let file_name = format!("logo.{ext}");
    let path = directory.join(&file_name);
    // Drop any previous logo.* so only one remains.
    if let Ok(mut entries) = tokio::fs::read_dir(&directory).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with("logo.") && name != file_name {
                let _ = tokio::fs::remove_file(entry.path()).await;
            }
        }
    }
    tokio::fs::write(&path, &bytes)
        .await
        .map_err(|error| OrganizationError::Io(error.to_string()))?;

    db.use_ns(namespace).await?;
    db.use_db("core").await?;
    db.query(
        "UPDATE organizations SET logo_file = $file, date_updated = time::now() WHERE db_name = $org;",
    )
    .bind(("file", file_name))
    .bind(("org", db_name.to_string()))
    .await?
    .check()?;
    get_profile(db, namespace, db_name).await
}

/// Remove the staged logo file and clear `logo_file`.
pub async fn clear_logo(
    db: &Surreal<Client>,
    namespace: &str,
    app_dir: &AppDir,
    db_name: &str,
) -> Result<OrganizationProfile, OrganizationError> {
    db.use_ns(namespace).await?;
    db.use_db("core").await?;
    let mut response = db
        .query("SELECT logo_file FROM organizations WHERE db_name = $org LIMIT 1;")
        .bind(("org", db_name.to_string()))
        .await?
        .check()?;
    #[derive(Debug, Deserialize, SurrealValue)]
    struct LogoRow {
        logo_file: Option<String>,
    }
    let rows: Vec<LogoRow> = response.take(0)?;
    if rows.is_empty() {
        return Err(OrganizationError::OrganizationNotFound(db_name.to_string()));
    }
    if let Some(file) = rows
        .into_iter()
        .next()
        .and_then(|row| row.logo_file)
        .filter(|f| !f.trim().is_empty())
    {
        let path = app_dir
            .org_dir(db_name)
            .map_err(|error| OrganizationError::Io(error.to_string()))?
            .join(file);
        let _ = tokio::fs::remove_file(path).await;
    }
    db.query(
        "UPDATE organizations SET logo_file = NONE, date_updated = time::now() WHERE db_name = $org;",
    )
    .bind(("org", db_name.to_string()))
    .await?
    .check()?;
    get_profile(db, namespace, db_name).await
}

/// Read the staged logo bytes and a content type, if present.
pub async fn read_logo(
    db: &Surreal<Client>,
    namespace: &str,
    app_dir: &AppDir,
    db_name: &str,
) -> Result<Option<(Vec<u8>, &'static str)>, OrganizationError> {
    db.use_ns(namespace).await?;
    db.use_db("core").await?;
    let mut response = db
        .query("SELECT logo_file FROM organizations WHERE db_name = $org LIMIT 1;")
        .bind(("org", db_name.to_string()))
        .await?
        .check()?;
    #[derive(Debug, Deserialize, SurrealValue)]
    struct LogoRow {
        logo_file: Option<String>,
    }
    let rows: Vec<LogoRow> = response.take(0)?;
    let Some(row) = rows.into_iter().next() else {
        return Err(OrganizationError::OrganizationNotFound(db_name.to_string()));
    };
    let Some(file) = row.logo_file.filter(|f| !f.trim().is_empty()) else {
        return Ok(None);
    };
    let path = app_dir
        .org_dir(db_name)
        .map_err(|error| OrganizationError::Io(error.to_string()))?
        .join(&file);
    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|error| OrganizationError::Io(error.to_string()))?;
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
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        _ => "application/octet-stream",
    };
    Ok(Some((bytes, content_type)))
}

/// Whether `actor_user_id` may list and manage members in `db_name`.
///
/// Developers always may. Otherwise the actor must be the owner, hold `org_admin`,
/// or hold `user_manager` in that organization.
pub async fn can_manage_users(
    db: &Surreal<Client>,
    namespace: &str,
    db_name: &str,
    actor_user_id: &str,
    is_developer: bool,
) -> Result<bool, OrganizationError> {
    if is_developer {
        return Ok(true);
    }
    let profile = get_profile(db, namespace, db_name).await?;
    if profile.owner_user_id.as_deref() == Some(actor_user_id) {
        return Ok(true);
    }
    // Must be a member first.
    let organization_id = format!("organizations:{db_name}");
    let mut response = db
        .query(
            "SELECT VALUE user_id FROM organization_users \
             WHERE organization_id = $org AND user_id = $user AND deleted_at = NONE LIMIT 1;",
        )
        .bind(("org", organization_id))
        .bind(("user", actor_user_id.to_string()))
        .await?
        .check()?;
    let links: Vec<String> = response.take(0)?;
    if links.is_empty() {
        return Ok(false);
    }
    db.use_ns(namespace).await?;
    db.use_db(db_name).await?;
    // Soft-deleted or inactive org membership cannot manage users.
    let mut active = db
        .query(
            "SELECT VALUE is_active FROM org_users \
             WHERE core_user_id = $user AND deleted_at = NONE LIMIT 1;",
        )
        .bind(("user", actor_user_id.to_string()))
        .await?
        .check()?;
    let flags: Vec<bool> = active.take(0)?;
    if !flags.first().copied().unwrap_or(false) {
        return Ok(false);
    }
    crate::roles::ensure_system_roles(db).await?;
    let roles = crate::roles::roles_of(db, actor_user_id).await?;
    Ok(roles
        .iter()
        .any(|role| role == crate::roles::ORG_ADMIN || role == crate::roles::USER_MANAGER))
}

/// True when the actor may grant or revoke `org_admin` (developer or org_admin / owner).
pub async fn can_manage_admin_role(
    db: &Surreal<Client>,
    namespace: &str,
    db_name: &str,
    actor_user_id: &str,
    is_developer: bool,
) -> Result<bool, OrganizationError> {
    if is_developer {
        return Ok(true);
    }
    let profile = get_profile(db, namespace, db_name).await?;
    if profile.owner_user_id.as_deref() == Some(actor_user_id) {
        return Ok(true);
    }
    db.use_ns(namespace).await?;
    db.use_db(db_name).await?;
    let roles = crate::roles::roles_of(db, actor_user_id).await?;
    Ok(roles.iter().any(|role| role == crate::roles::ORG_ADMIN))
}

/// Every platform user (developer Users page).
pub async fn list_platform_users(
    db: &Surreal<Client>,
    namespace: &str,
) -> Result<Vec<PlatformUserRow>, OrganizationError> {
    db.use_ns(namespace).await?;
    db.use_db("core").await?;
    let users: Vec<CoreUser> = db.select("users").await?;
    let mut response = db
        .query("SELECT user_id, organization_id FROM organization_users;")
        .await?
        .check()?;
    #[derive(Debug, Deserialize, SurrealValue)]
    struct Link {
        user_id: String,
        organization_id: String,
    }
    let links: Vec<Link> = response.take(0)?;
    let mut out = Vec::with_capacity(users.len());
    for user in users {
        let user_id = record_id_string(&user.id);
        let mut organizations: Vec<String> = links
            .iter()
            .filter(|link| link.user_id == user_id)
            .filter_map(|link| {
                link.organization_id
                    .strip_prefix("organizations:")
                    .map(str::to_string)
            })
            .collect();
        organizations.sort();
        organizations.dedup();
        out.push(PlatformUserRow {
            user_id,
            username: user.username,
            email: user.email,
            display_name: user.display_name,
            is_developer: user.is_super_user,
            is_active: user.is_active,
            organizations,
        });
    }
    out.sort_by(|a, b| {
        a.username
            .as_deref()
            .unwrap_or("")
            .cmp(b.username.as_deref().unwrap_or(""))
    });
    Ok(out)
}

/// Members of an organization (core catalog), with roles from the org database.
pub async fn list_members(
    db: &Surreal<Client>,
    namespace: &str,
    db_name: &str,
) -> Result<Vec<OrgMemberRow>, OrganizationError> {
    let profile = get_profile(db, namespace, db_name).await?;
    let organization_id = format!("organizations:{db_name}");
    let mut response = db
        .query(
            "SELECT user_id, date_created, is_developer FROM organization_users \
             WHERE organization_id = $org AND deleted_at = NONE \
             ORDER BY date_created;",
        )
        .bind(("org", organization_id))
        .await?
        .check()?;
    #[derive(Debug, Deserialize, SurrealValue)]
    struct MemberLink {
        user_id: String,
        date_created: Option<Datetime>,
        is_developer: Option<bool>,
    }
    let links: Vec<MemberLink> = response.take(0)?;
    let users: Vec<CoreUser> = db.select("users").await?;
    db.use_ns(namespace).await?;
    db.use_db(db_name).await?;
    let _ = crate::roles::ensure_system_roles(db).await;
    let mut active_response = db
        .query("SELECT core_user_id, is_active FROM org_users WHERE deleted_at = NONE;")
        .await?
        .check()?;
    #[derive(Debug, Deserialize, SurrealValue)]
    struct OrgActive {
        core_user_id: String,
        is_active: bool,
    }
    let org_active: Vec<OrgActive> = active_response.take(0)?;
    let mut out = Vec::with_capacity(links.len());
    for link in links {
        let user = users
            .iter()
            .find(|user| record_id_string(&user.id) == link.user_id);
        let is_active = org_active
            .iter()
            .find(|row| row.core_user_id == link.user_id)
            .map(|row| row.is_active)
            .unwrap_or(true);
        let roles = crate::roles::roles_of(db, &link.user_id)
            .await
            .unwrap_or_default();
        out.push(OrgMemberRow {
            is_owner: profile.owner_user_id.as_deref() == Some(link.user_id.as_str()),
            user_id: link.user_id,
            username: user.and_then(|u| u.username.clone()),
            email: user.and_then(|u| u.email.clone()),
            display_name: user.and_then(|u| u.display_name.clone()),
            is_active,
            date_joined: format_dt(link.date_created),
            roles,
            is_developer: link.is_developer.unwrap_or(false),
        });
    }
    Ok(out)
}

/// Create a new account and assign it to the organization, or assign an existing login.
pub async fn add_member(
    db: &Surreal<Client>,
    namespace: &str,
    db_name: &str,
    new_user: Option<(&str, &str, &str)>,
    existing_login: Option<&str>,
) -> Result<(), OrganizationError> {
    let _ = get_profile(db, namespace, db_name).await?;
    match (new_user, existing_login) {
        (Some((username, email, password)), None) => {
            create_user(db, namespace, username, email, password).await?;
            assign_user(db, namespace, username, db_name).await?;
        }
        (None, Some(login)) => {
            assign_user(db, namespace, login, db_name).await?;
        }
        _ => return Err(OrganizationError::MissingUserCredentials),
    }
    Ok(())
}

/// Grant a role to a member. `actor_may_manage_admin` must be true to grant `org_admin`.
pub async fn grant_member_role(
    db: &Surreal<Client>,
    namespace: &str,
    db_name: &str,
    target_user_id: &str,
    role: &str,
    actor_may_manage_admin: bool,
) -> Result<(), OrganizationError> {
    if role == crate::roles::ORG_ADMIN && !actor_may_manage_admin {
        return Err(OrganizationError::CannotManageAdminRole);
    }
    let _ = get_profile(db, namespace, db_name).await?;
    db.use_ns(namespace).await?;
    db.use_db(db_name).await?;
    crate::roles::ensure_system_roles(db).await?;
    crate::roles::grant_by_id(db, target_user_id, role)
        .await
        .map_err(|error| match error {
            crate::roles::RoleError::NotAMember(_, _) => {
                OrganizationError::NotAMember(target_user_id.to_string(), db_name.to_string())
            }
            other => OrganizationError::Role(other.to_string()),
        })?;
    Ok(())
}

/// Revoke a role from a member. `actor_may_manage_admin` must be true to revoke `org_admin`.
pub async fn revoke_member_role(
    db: &Surreal<Client>,
    namespace: &str,
    db_name: &str,
    target_user_id: &str,
    role: &str,
    actor_may_manage_admin: bool,
) -> Result<(), OrganizationError> {
    if role == crate::roles::ORG_ADMIN && !actor_may_manage_admin {
        return Err(OrganizationError::CannotManageAdminRole);
    }
    let _ = get_profile(db, namespace, db_name).await?;
    db.use_ns(namespace).await?;
    db.use_db(db_name).await?;
    crate::roles::revoke_by_id(db, target_user_id, role)
        .await
        .map_err(|error| OrganizationError::Role(error.to_string()))?;
    Ok(())
}

/// Create a custom role (`custom.<slug>`) in the organization.
pub async fn create_org_role(
    db: &Surreal<Client>,
    namespace: &str,
    db_name: &str,
    slug: &str,
    label: &str,
    description: Option<&str>,
) -> Result<crate::roles::RoleSummary, OrganizationError> {
    let _ = get_profile(db, namespace, db_name).await?;
    db.use_ns(namespace).await?;
    db.use_db(db_name).await?;
    crate::roles::create_custom(db, slug, label, description)
        .await
        .map_err(|error| OrganizationError::Role(error.to_string()))
}

/// Update a custom role's label/description.
pub async fn update_org_role(
    db: &Surreal<Client>,
    namespace: &str,
    db_name: &str,
    role: &str,
    label: Option<&str>,
    description: Option<Option<&str>>,
) -> Result<(), OrganizationError> {
    let _ = get_profile(db, namespace, db_name).await?;
    db.use_ns(namespace).await?;
    db.use_db(db_name).await?;
    crate::roles::update_custom(db, role, label, description)
        .await
        .map_err(|error| OrganizationError::Role(error.to_string()))
}

/// Delete a custom role and soft-revoke its holders.
pub async fn delete_org_role(
    db: &Surreal<Client>,
    namespace: &str,
    db_name: &str,
    role: &str,
) -> Result<(), OrganizationError> {
    let _ = get_profile(db, namespace, db_name).await?;
    db.use_ns(namespace).await?;
    db.use_db(db_name).await?;
    crate::roles::delete_custom(db, role)
        .await
        .map_err(|error| OrganizationError::Role(error.to_string()))
}

/// Ownership transfer history for an organization.
pub async fn list_ownership_transfers(
    db: &Surreal<Client>,
    namespace: &str,
    db_name: &str,
) -> Result<Vec<OwnershipTransferRow>, OrganizationError> {
    let _ = get_profile(db, namespace, db_name).await?;
    let organization_id = format!("organizations:{db_name}");
    let mut response = db
        .query(
            "SELECT from_user_id, to_user_id, transferred_by, note, date_created \
             FROM org_ownership_transfers WHERE organization_id = $org ORDER BY date_created DESC;",
        )
        .bind(("org", organization_id))
        .await?
        .check()?;
    #[derive(Debug, Deserialize, SurrealValue)]
    struct TransferDb {
        from_user_id: String,
        to_user_id: String,
        transferred_by: String,
        note: Option<String>,
        date_created: Option<Datetime>,
    }
    let rows: Vec<TransferDb> = response.take(0)?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        out.push(OwnershipTransferRow {
            from_login: login_for(db, &row.from_user_id).await?,
            to_login: login_for(db, &row.to_user_id).await?,
            from_user_id: row.from_user_id,
            to_user_id: row.to_user_id,
            transferred_by: row.transferred_by,
            note: row.note,
            date_created: format_dt(row.date_created),
        });
    }
    Ok(out)
}

/// Whether `user_id` is a developer in the organization `db_name`. An unrestricted developer
/// (`users.is_super_user`) is one in every organization; anyone else only through an active
/// membership there marked `is_developer`, which an unrestricted developer sets.
pub async fn is_org_developer(
    db: &Surreal<Client>,
    namespace: &str,
    db_name: &str,
    user_id: &str,
    is_super_user: bool,
) -> Result<bool, OrganizationError> {
    if is_super_user {
        return Ok(true);
    }
    db.use_ns(namespace).await?;
    db.use_db("core").await?;
    let mut response = db
        .query(
            "SELECT VALUE is_developer FROM organization_users \
             WHERE organization_id = $org AND user_id = $user AND deleted_at = NONE LIMIT 1;",
        )
        .bind(("org", format!("organizations:{db_name}")))
        .bind(("user", user_id.to_string()))
        .await?
        .check()?;
    let flags: Vec<Option<bool>> = response.take(0)?;
    Ok(flags.into_iter().next().flatten().unwrap_or(false))
}

/// Make a member a developer of the organization `db_name`, or stop them being one. They must be
/// an active member: leaving or being removed from the organization ends it too.
pub async fn set_org_developer(
    db: &Surreal<Client>,
    namespace: &str,
    db_name: &str,
    user_id: &str,
    on: bool,
) -> Result<(), OrganizationError> {
    db.use_ns(namespace).await?;
    db.use_db("core").await?;
    let mut response = db
        .query(
            "UPDATE organization_users SET is_developer = $on \
             WHERE organization_id = $org AND user_id = $user AND deleted_at = NONE RETURN VALUE id;",
        )
        .bind(("on", on))
        .bind(("org", format!("organizations:{db_name}")))
        .bind(("user", user_id.to_string()))
        .await?
        .check()?;
    let changed: Vec<surrealdb::types::RecordId> = response.take(0)?;
    if changed.is_empty() {
        return Err(OrganizationError::NotAMember(
            user_id.to_string(),
            db_name.to_string(),
        ));
    }
    Ok(())
}

/// Developer access for the account `login` (username or email), from the command line: in one
/// organization (`org`), or everywhere (`users.is_super_user`) when no organization is named.
pub async fn set_developer_by_login(
    db: &Surreal<Client>,
    namespace: &str,
    login: &str,
    org: Option<&str>,
    on: bool,
) -> Result<(), OrganizationError> {
    db.use_ns(namespace).await?;
    db.use_db("core").await?;
    let mut response = db
        .query("SELECT VALUE id FROM users WHERE username = $login OR email = $login LIMIT 1;")
        .bind(("login", login.to_string()))
        .await?
        .check()?;
    let found: Vec<surrealdb::types::RecordId> = response.take(0)?;
    let user = found
        .into_iter()
        .next()
        .ok_or_else(|| OrganizationError::UserNotFound(login.to_string()))?;
    match org {
        Some(db_name) => set_org_developer(db, namespace, db_name, &record_id_string(&user), on)
            .await
            .map_err(|error| match error {
                OrganizationError::NotAMember(_, org) => {
                    OrganizationError::NotAMember(login.to_string(), org)
                }
                other => other,
            }),
        None => {
            db.query("UPDATE $user SET is_super_user = $on;")
                .bind(("user", user))
                .bind(("on", on))
                .await?
                .check()?;
            Ok(())
        }
    }
}

/// True when the actor is a developer or the organization owner.
pub async fn is_owner_or_developer(
    db: &Surreal<Client>,
    namespace: &str,
    db_name: &str,
    actor_user_id: &str,
    is_developer: bool,
) -> Result<bool, OrganizationError> {
    if is_developer {
        return Ok(true);
    }
    let profile = get_profile(db, namespace, db_name).await?;
    Ok(profile.owner_user_id.as_deref() == Some(actor_user_id))
}

fn guard_target(
    profile_owner: Option<&str>,
    actor_user_id: &str,
    target_user_id: &str,
) -> Result<(), OrganizationError> {
    if target_user_id == actor_user_id {
        return Err(OrganizationError::CannotTargetSelf);
    }
    if profile_owner == Some(target_user_id) {
        return Err(OrganizationError::CannotTargetOwner);
    }
    Ok(())
}

/// Soft-kick a member: mark membership deleted, leave rows in place.
pub async fn soft_kick_member(
    db: &Surreal<Client>,
    namespace: &str,
    db_name: &str,
    target_user_id: &str,
    actor_user_id: &str,
    actor_is_developer: bool,
) -> Result<(), OrganizationError> {
    if !is_owner_or_developer(db, namespace, db_name, actor_user_id, actor_is_developer).await? {
        return Err(OrganizationError::OwnerOrDeveloperRequired);
    }
    let profile = get_profile(db, namespace, db_name).await?;
    guard_target(
        profile.owner_user_id.as_deref(),
        actor_user_id,
        target_user_id,
    )?;
    let organization_id = format!("organizations:{db_name}");
    db.use_ns(namespace).await?;
    db.use_db("core").await?;
    db.query(
        "UPDATE organization_users SET deleted_at = time::now(), deleted_by = $by \
         WHERE organization_id = $org AND user_id = $user AND deleted_at = NONE;",
    )
    .bind(("org", organization_id))
    .bind(("user", target_user_id.to_string()))
    .bind(("by", actor_user_id.to_string()))
    .await?
    .check()?;
    db.use_db(db_name).await?;
    db.query(
        "UPDATE org_users SET deleted_at = time::now(), deleted_by = $by, is_active = false \
         WHERE core_user_id = $user AND deleted_at = NONE; \
         UPDATE org_user_roles SET deleted_at = time::now(), deleted_by = $by \
         WHERE org_user.core_user_id = $user AND deleted_at = NONE;",
    )
    .bind(("user", target_user_id.to_string()))
    .bind(("by", actor_user_id.to_string()))
    .await?
    .check()?;
    Ok(())
}

/// Set `org_users.is_active` for a member of this organization.
pub async fn set_member_active(
    db: &Surreal<Client>,
    namespace: &str,
    db_name: &str,
    target_user_id: &str,
    is_active: bool,
    actor_user_id: &str,
) -> Result<(), OrganizationError> {
    let profile = get_profile(db, namespace, db_name).await?;
    guard_target(
        profile.owner_user_id.as_deref(),
        actor_user_id,
        target_user_id,
    )?;
    db.use_ns(namespace).await?;
    db.use_db(db_name).await?;
    let mut response = db
        .query(
            "UPDATE org_users SET is_active = $active \
             WHERE core_user_id = $user AND deleted_at = NONE RETURN AFTER;",
        )
        .bind(("user", target_user_id.to_string()))
        .bind(("active", is_active))
        .await?
        .check()?;
    let rows: Vec<serde_json::Value> = response.take(0)?;
    if rows.is_empty() {
        return Err(OrganizationError::NotAMember(
            target_user_id.to_string(),
            db_name.to_string(),
        ));
    }
    Ok(())
}

/// Set a member's core login password. Owner or developer only.
pub async fn set_member_password(
    db: &Surreal<Client>,
    namespace: &str,
    db_name: &str,
    target_user_id: &str,
    new_password: &str,
    actor_user_id: &str,
    actor_is_developer: bool,
) -> Result<(), OrganizationError> {
    if !is_owner_or_developer(db, namespace, db_name, actor_user_id, actor_is_developer).await? {
        return Err(OrganizationError::OwnerOrDeveloperRequired);
    }
    let profile = get_profile(db, namespace, db_name).await?;
    if target_user_id == actor_user_id {
        return Err(OrganizationError::CannotTargetSelf);
    }
    // Owner may reset others; developer may reset anyone including another developer.
    if !actor_is_developer && profile.owner_user_id.as_deref() == Some(target_user_id) {
        return Err(OrganizationError::CannotTargetOwner);
    }
    let organization_id = format!("organizations:{db_name}");
    db.use_ns(namespace).await?;
    db.use_db("core").await?;
    let mut response = db
        .query(
            "SELECT VALUE user_id FROM organization_users \
             WHERE organization_id = $org AND user_id = $user AND deleted_at = NONE LIMIT 1;",
        )
        .bind(("org", organization_id))
        .bind(("user", target_user_id.to_string()))
        .await?
        .check()?;
    let links: Vec<String> = response.take(0)?;
    if links.is_empty() {
        return Err(OrganizationError::NotAMember(
            target_user_id.to_string(),
            db_name.to_string(),
        ));
    }
    let hashed = hash_password(new_password)
        .map_err(|err| OrganizationError::PasswordHash(err.to_string()))?;
    let key = target_user_id
        .strip_prefix("users:")
        .unwrap_or(target_user_id);
    db.query("UPDATE type::record('users', $key) SET hashed_password = $hashed;")
        .bind(("key", key.to_string()))
        .bind(("hashed", hashed))
        .await?
        .check()?;
    Ok(())
}

/// Derive a username from an email local-part for invites.
pub fn username_from_email(email: &str) -> String {
    let local = email.split('@').next().unwrap_or("user");
    let mut out: String = local
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.' {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    out = out
        .trim_matches(|c| c == '.' || c == '_' || c == '-')
        .to_string();
    if out.len() < 3 {
        out = format!("user_{out}");
    }
    if out.len() < 3 {
        out = "user".into();
    }
    out.chars().take(48).collect()
}

/// Invite: create user with generated password and assign.
/// Returns `(username, temporary_password)`. If the email already exists, assigns that
/// account and returns an empty password (credentials are not changed).
pub async fn invite_member(
    db: &Surreal<Client>,
    namespace: &str,
    db_name: &str,
    email: &str,
    username: Option<&str>,
) -> Result<(String, String), OrganizationError> {
    let email = email.trim();
    if !email.contains('@') || email.chars().any(char::is_whitespace) {
        return Err(OrganizationError::MissingUserCredentials);
    }
    let base_name = username
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| username_from_email(email));
    if base_name.len() < 3 || base_name.chars().any(char::is_whitespace) {
        return Err(OrganizationError::MissingUserCredentials);
    }
    db.use_ns(namespace).await?;
    db.use_db("core").await?;
    let users: Vec<CoreUser> = db.select("users").await?;
    if users.iter().any(|u| u.email.as_deref() == Some(email)) {
        assign_user(db, namespace, email, db_name).await?;
        let login = users
            .iter()
            .find(|u| u.email.as_deref() == Some(email))
            .and_then(|u| u.username.clone())
            .unwrap_or_else(|| email.to_string());
        return Ok((login, String::new()));
    }
    let password = aether_orm::generate_password(16);
    for attempt in 0..20 {
        let candidate = if attempt == 0 {
            base_name.clone()
        } else {
            format!("{base_name}{attempt}")
        };
        if users
            .iter()
            .any(|u| u.username.as_deref() == Some(candidate.as_str()))
        {
            continue;
        }
        create_user(db, namespace, &candidate, email, &password).await?;
        assign_user(db, namespace, &candidate, db_name).await?;
        return Ok((candidate, password));
    }
    Err(OrganizationError::UserExists(base_name))
}

/// Account id (`users:…`) and username for an email, if one exists.
pub async fn user_by_email(
    db: &Surreal<Client>,
    namespace: &str,
    email: &str,
) -> Result<Option<(String, String)>, OrganizationError> {
    db.use_ns(namespace).await?;
    db.use_db("core").await?;
    let users: Vec<CoreUser> = db.select("users").await?;
    Ok(users
        .into_iter()
        .find(|user| user.email.as_deref() == Some(email))
        .map(|user| {
            let username = user.username.clone().unwrap_or_else(|| email.to_string());
            (record_id_string(&user.id), username)
        }))
}

/// Set the name shown for a member inside one organization.
pub async fn set_org_display_name(
    db: &Surreal<Client>,
    namespace: &str,
    db_name: &str,
    user_id: &str,
    display_name: &str,
) -> Result<(), OrganizationError> {
    let display_name = display_name.trim();
    if display_name.is_empty() {
        return Ok(());
    }
    db.use_ns(namespace).await?;
    db.use_db(db_name).await?;
    db.query(
        "UPDATE org_users SET display_name = $name, date_updated = time::now() \
         WHERE core_user_id = $user AND deleted_at = NONE;",
    )
    .bind(("name", display_name.to_string()))
    .bind(("user", user_id.to_string()))
    .await?
    .check()?;
    Ok(())
}

/// Create an account from an invite and assign it to the organization.
/// Refuses when the email already has an account, and does not change that password.
/// Returns the new account id.
pub async fn accept_account_invite(
    db: &Surreal<Client>,
    namespace: &str,
    db_name: &str,
    email: &str,
    username: Option<&str>,
    display_name: Option<&str>,
    password: &str,
) -> Result<String, OrganizationError> {
    let email = email.trim();
    if !email.contains('@') || email.chars().any(char::is_whitespace) {
        return Err(OrganizationError::MissingUserCredentials);
    }
    let length = password.chars().count();
    if !(8..=128).contains(&length) {
        return Err(OrganizationError::MissingUserCredentials);
    }
    if user_by_email(db, namespace, email).await?.is_some() {
        return Err(OrganizationError::AccountAlreadyExists);
    }
    let base_name = username
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| username_from_email(email));
    if base_name.len() < 3 || base_name.chars().any(char::is_whitespace) {
        return Err(OrganizationError::MissingUserCredentials);
    }
    db.use_ns(namespace).await?;
    db.use_db("core").await?;
    let users: Vec<CoreUser> = db.select("users").await?;
    for attempt in 0..20 {
        let candidate = if attempt == 0 {
            base_name.clone()
        } else {
            format!("{base_name}{attempt}")
        };
        if users
            .iter()
            .any(|user| user.username.as_deref() == Some(candidate.as_str()))
        {
            continue;
        }
        let user_id = match create_user(db, namespace, &candidate, email, password).await {
            Ok(user_id) => user_id,
            Err(OrganizationError::UserExists(_)) => {
                return Err(OrganizationError::AccountAlreadyExists);
            }
            Err(error) => return Err(error),
        };
        assign_user(db, namespace, &candidate, db_name).await?;
        if let Some(name) = display_name.map(str::trim).filter(|name| !name.is_empty()) {
            set_org_display_name(db, namespace, db_name, &user_id, name).await?;
        }
        return Ok(user_id);
    }
    Err(OrganizationError::UserExists(base_name))
}

#[cfg(test)]
mod tests {
    use super::{slugify, username_from_email};

    #[test]
    fn slugifies_organization_names() {
        assert_eq!(slugify("Acme Corporation"), "acme_corporation");
    }

    #[test]
    fn collapses_punctuation_and_whitespace() {
        assert_eq!(slugify("Acme  Holdings, Ltd."), "acme_holdings_ltd");
    }

    #[test]
    fn username_from_email_local_part() {
        assert_eq!(username_from_email("Ann.O'Brien@acme.io"), "ann.o_brien");
    }
}
