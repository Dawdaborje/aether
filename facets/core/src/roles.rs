//! Roles: what an administrator gives people so that plugins know what they may do.
//!
//! A plugin declares the roles it offers in `plugin.toml` (`[[roles]]`). Installing the plugin
//! in an organization creates them there as `<plugin>.<name>` (`hr.hr_manager`), and an
//! administrator gives them to people (`aether --grant-role hr.hr_manager --username ann
//! --org acme`). When a person calls a plugin, `context::get` tells it their roles, and the
//! plugin refuses what the role does not allow. The kernel's own role is `org_admin`, held by
//! whoever created the organization: an administrator has every plugin role.
//!
//! Roles are plain data (`roles`, `org_user_roles` in the organization's database), so they
//! can be listed, given and taken away at any time, and a call sees the change at once.

use serde::Deserialize;
use serde_json::Value;
use surrealdb::{Surreal, engine::remote::ws::Client};
use thiserror::Error;

/// The role of an organization's administrators. It counts as every plugin role.
pub const ORG_ADMIN: &str = "org_admin";

#[derive(Debug, Error)]
pub enum RoleError {
    #[error("database error: {0}")]
    Db(#[from] surrealdb::Error),
    #[error("there is no user `{0}`")]
    UnknownUser(String),
    #[error("`{0}` is not a member of organization `{1}`: assign them first")]
    NotAMember(String, String),
    #[error("there is no role `{role}` here; the roles are: {known}")]
    UnknownRole { role: String, known: String },
}

#[derive(Debug, Deserialize)]
struct Declared {
    name: String,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    description: Option<String>,
}

/// Create (or refresh) the roles a plugin offers in the organization `db` is on. Roles the
/// plugin no longer declares are left alone, so nobody loses a role in an upgrade.
pub async fn sync_roles(db: &Surreal<Client>, plugin: &str, roles: Option<&[Value]>) -> Result<(), surrealdb::Error> {
    for value in roles.unwrap_or_default() {
        let Ok(role) = serde_json::from_value::<Declared>(value.clone()) else { continue };
        let name = format!("{plugin}.{}", role.name);
        let label = role.label.unwrap_or_else(|| role.name.clone());
        db.query(
            "UPSERT roles SET name = $name, label = $label, description = $description, is_system = true \
             WHERE name = $name;",
        )
        .bind(("name", name))
        .bind(("label", label))
        .bind(("description", role.description))
        .await?
        .check()?;
    }
    Ok(())
}

/// Make sure the kernel's own role exists in the organization `db` is on.
pub async fn ensure_org_admin(db: &Surreal<Client>) -> Result<(), surrealdb::Error> {
    db.query("UPSERT roles SET name = $name, label = 'Administrator', description = 'Manages the organization and holds every role', is_system = true WHERE name = $name;")
        .bind(("name", ORG_ADMIN))
        .await?
        .check()?;
    Ok(())
}

/// Give the person `core_user_id` (`users:abc`) a role, in the organization `db` is on. Giving a
/// role twice is harmless.
pub async fn grant_by_id(db: &Surreal<Client>, core_user_id: &str, role: &str) -> Result<(), RoleError> {
    if role == ORG_ADMIN {
        ensure_org_admin(db).await?;
    }
    let mut response = db
        .query("SELECT VALUE id FROM roles WHERE name = $role LIMIT 1; SELECT VALUE id FROM org_users WHERE core_user_id = $user LIMIT 1;")
        .bind(("role", role.to_string()))
        .bind(("user", core_user_id.to_string()))
        .await?
        .check()?;
    let role_ids: Vec<surrealdb::types::RecordId> = response.take(0)?;
    let user_ids: Vec<surrealdb::types::RecordId> = response.take(1)?;
    let Some(role_id) = role_ids.into_iter().next() else {
        return Err(RoleError::UnknownRole { role: role.to_string(), known: known_roles(db).await? });
    };
    let Some(user_id) = user_ids.into_iter().next() else {
        return Err(RoleError::NotAMember(core_user_id.to_string(), "this organization".into()));
    };
    db.query("UPSERT org_user_roles SET org_user = $user, role = $role WHERE org_user = $user AND role = $role;")
        .bind(("user", user_id))
        .bind(("role", role_id))
        .await?
        .check()?;
    Ok(())
}

/// Take a role away. Taking a role the person does not have is harmless.
pub async fn revoke_by_id(db: &Surreal<Client>, core_user_id: &str, role: &str) -> Result<(), RoleError> {
    db.query(
        "LET $r = (SELECT VALUE id FROM roles WHERE name = $role LIMIT 1)[0]; \
         LET $u = (SELECT VALUE id FROM org_users WHERE core_user_id = $user LIMIT 1)[0]; \
         DELETE org_user_roles WHERE role = $r AND org_user = $u;",
    )
    .bind(("role", role.to_string()))
    .bind(("user", core_user_id.to_string()))
    .await?
    .check()?;
    Ok(())
}

async fn known_roles(db: &Surreal<Client>) -> Result<String, surrealdb::Error> {
    let mut response = db.query("SELECT VALUE name FROM roles ORDER BY name;").await?.check()?;
    let names: Vec<String> = response.take(0)?;
    Ok(if names.is_empty() { "none yet: install a plugin that offers some".into() } else { names.join(", ") })
}

/// The names of the roles the person `core_user_id` holds in the organization `db` is on.
pub async fn roles_of(db: &Surreal<Client>, core_user_id: &str) -> Result<Vec<String>, surrealdb::Error> {
    let mut response = db
        .query("SELECT VALUE role.name FROM org_user_roles WHERE org_user.core_user_id = $user;")
        .bind(("user", core_user_id.to_string()))
        .await?
        .check()?;
    let mut names: Vec<String> = response.take(0)?;
    names.sort();
    names.dedup();
    Ok(names)
}

/// A role with the people who hold it.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RoleSummary {
    pub name: String,
    pub label: String,
    pub description: Option<String>,
    pub holders: Vec<String>,
}

/// Every role of the organization `db` is on, with who holds it.
pub async fn list(db: &Surreal<Client>) -> Result<Vec<RoleSummary>, surrealdb::Error> {
    #[derive(Deserialize)]
    struct Row {
        name: String,
        label: Option<String>,
        description: Option<String>,
    }
    #[derive(Deserialize)]
    struct Held {
        role: String,
        who: Option<String>,
    }
    let mut response = db
        .query(
            "SELECT name, label, description FROM roles ORDER BY name; \
             SELECT role.name AS role, org_user.display_name AS who FROM org_user_roles;",
        )
        .await?
        .check()?;
    let roles: Vec<serde_json::Value> = response.take(0)?;
    let held: Vec<serde_json::Value> = response.take(1)?;
    let held: Vec<Held> = held.into_iter().filter_map(|v| serde_json::from_value(v).ok()).collect();
    Ok(roles
        .into_iter()
        .filter_map(|v| serde_json::from_value::<Row>(v).ok())
        .map(|row| RoleSummary {
            holders: held.iter().filter(|h| h.role == row.name).filter_map(|h| h.who.clone()).collect(),
            label: row.label.unwrap_or_else(|| row.name.clone()),
            description: row.description,
            name: row.name,
        })
        .collect())
}

/// The id (`users:abc`) of the account called `login` (a username or an email), looked up in the
/// core database.
async fn core_user_id(db: &Surreal<Client>, namespace: &str, login: &str) -> Result<String, RoleError> {
    db.use_ns(namespace).await?;
    db.use_db("core").await?;
    let mut response = db
        .query("SELECT VALUE <string> id FROM users WHERE username = $login OR email = $login LIMIT 1;")
        .bind(("login", login.to_string()))
        .await?
        .check()?;
    let found: Vec<String> = response.take(0)?;
    found.into_iter().next().ok_or_else(|| RoleError::UnknownUser(login.to_string()))
}

/// Give the user `login` a role in the organization whose database is `organization`.
pub async fn grant(db: &Surreal<Client>, namespace: &str, organization: &str, login: &str, role: &str) -> Result<(), RoleError> {
    let user = core_user_id(db, namespace, login).await?;
    db.use_db(organization).await?;
    match grant_by_id(db, &user, role).await {
        Err(RoleError::NotAMember(_, _)) => Err(RoleError::NotAMember(login.to_string(), organization.to_string())),
        other => other,
    }
}

/// Take a role away from the user `login`.
pub async fn revoke(db: &Surreal<Client>, namespace: &str, organization: &str, login: &str, role: &str) -> Result<(), RoleError> {
    let user = core_user_id(db, namespace, login).await?;
    db.use_db(organization).await?;
    revoke_by_id(db, &user, role).await
}

/// The roles of the organization whose database is `organization`.
pub async fn list_in(db: &Surreal<Client>, namespace: &str, organization: &str) -> Result<Vec<RoleSummary>, RoleError> {
    db.use_ns(namespace).await?;
    db.use_db(organization).await?;
    Ok(list(db).await?)
}
