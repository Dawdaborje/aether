//! Roles: what an administrator gives people so that plugins know what they may do.
//!
//! A plugin declares the roles it offers in `plugin.toml` (`[[roles]]`). Installing the plugin
//! in an organization creates them there as `<plugin>.<name>` (`hr.hr_manager`), and an
//! administrator gives them to people (`aether --grant-role hr.hr_manager --username ann
//! --org acme`). When a person calls a plugin, `context::get` tells it their roles, and the
//! plugin refuses what the role does not allow. The kernel's own roles are `org_admin` (every
//! plugin role; held by the owner and appointed admins) and `user_manager` (member and role
//! management without full admin). Ownership is separate (`organizations.owner_user_id`).
//!
//! Roles are plain data (`roles`, `org_user_roles` in the organization's database), so they
//! can be listed, given and taken away at any time, and a call sees the change at once.

use serde::Deserialize;
use serde_json::Value;
use surrealdb::{Surreal, engine::remote::ws::Client};
use thiserror::Error;

/// The role of an organization's administrators. It counts as every plugin role.
pub const ORG_ADMIN: &str = "org_admin";

/// Can list and manage members and roles in the organization (not a full admin).
pub const USER_MANAGER: &str = "user_manager";

/// Prefix for organization-defined roles (`custom.shift_lead`).
pub const CUSTOM_PREFIX: &str = "custom.";

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
    #[error("role name `{0}`: use 1 to 64 lower-case letters, digits or `_`")]
    InvalidSlug(String),
    #[error("a role named `{0}` already exists")]
    AlreadyExists(String),
    #[error("only custom roles (`custom.…`) can be created, edited or deleted this way")]
    NotCustom,
    #[error("system and plugin roles cannot be changed here")]
    SystemRole,
    #[error("enter a label for the role")]
    EmptyLabel,
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
pub async fn sync_roles(
    db: &Surreal<Client>,
    plugin: &str,
    roles: Option<&[Value]>,
) -> Result<(), surrealdb::Error> {
    for value in roles.unwrap_or_default() {
        let Ok(role) = serde_json::from_value::<Declared>(value.clone()) else {
            continue;
        };
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

/// Make sure the kernel's administrator role exists in the organization `db` is on.
pub async fn ensure_org_admin(db: &Surreal<Client>) -> Result<(), surrealdb::Error> {
    db.query("UPSERT roles SET name = $name, label = 'Administrator', description = 'Manages the organization and holds every role', is_system = true WHERE name = $name;")
        .bind(("name", ORG_ADMIN))
        .await?
        .check()?;
    Ok(())
}

/// Make sure the user-manager role exists in the organization `db` is on.
pub async fn ensure_user_manager(db: &Surreal<Client>) -> Result<(), surrealdb::Error> {
    db.query("UPSERT roles SET name = $name, label = 'User manager', description = 'Lists and manages members and roles in this organization', is_system = true WHERE name = $name;")
        .bind(("name", USER_MANAGER))
        .await?
        .check()?;
    Ok(())
}

/// Kernel system roles that every organization should have.
pub async fn ensure_system_roles(db: &Surreal<Client>) -> Result<(), surrealdb::Error> {
    ensure_org_admin(db).await?;
    ensure_user_manager(db).await?;
    Ok(())
}

/// Give the person `core_user_id` (`users:abc`) a role, in the organization `db` is on. Giving a
/// role twice is harmless.
pub async fn grant_by_id(
    db: &Surreal<Client>,
    core_user_id: &str,
    role: &str,
) -> Result<(), RoleError> {
    if role == ORG_ADMIN {
        ensure_org_admin(db).await?;
    } else if role == USER_MANAGER {
        ensure_user_manager(db).await?;
    }
    let mut response = db
        .query(
            "SELECT VALUE id FROM roles WHERE name = $role LIMIT 1; \
             SELECT VALUE id FROM org_users WHERE core_user_id = $user AND deleted_at = NONE LIMIT 1;",
        )
        .bind(("role", role.to_string()))
        .bind(("user", core_user_id.to_string()))
        .await?
        .check()?;
    let role_ids: Vec<surrealdb::types::RecordId> = response.take(0)?;
    let user_ids: Vec<surrealdb::types::RecordId> = response.take(1)?;
    let Some(role_id) = role_ids.into_iter().next() else {
        return Err(RoleError::UnknownRole {
            role: role.to_string(),
            known: known_roles(db).await?,
        });
    };
    let Some(user_id) = user_ids.into_iter().next() else {
        return Err(RoleError::NotAMember(
            core_user_id.to_string(),
            "this organization".into(),
        ));
    };
    db.query(
        "UPSERT org_user_roles SET org_user = $user, role = $role, deleted_at = NONE, deleted_by = NONE \
         WHERE org_user = $user AND role = $role;",
    )
        .bind(("user", user_id))
        .bind(("role", role_id))
        .await?
        .check()?;
    Ok(())
}

/// Take a role away (soft-delete). Taking a role the person does not have is harmless.
pub async fn revoke_by_id(
    db: &Surreal<Client>,
    core_user_id: &str,
    role: &str,
) -> Result<(), RoleError> {
    db.query(
        "LET $r = (SELECT VALUE id FROM roles WHERE name = $role LIMIT 1)[0]; \
         LET $u = (SELECT VALUE id FROM org_users WHERE core_user_id = $user AND deleted_at = NONE LIMIT 1)[0]; \
         UPDATE org_user_roles SET deleted_at = time::now() \
         WHERE role = $r AND org_user = $u AND deleted_at = NONE;",
    )
    .bind(("role", role.to_string()))
    .bind(("user", core_user_id.to_string()))
    .await?
    .check()?;
    Ok(())
}

async fn known_roles(db: &Surreal<Client>) -> Result<String, surrealdb::Error> {
    let mut response = db
        .query("SELECT VALUE name FROM roles ORDER BY name;")
        .await?
        .check()?;
    let names: Vec<String> = response.take(0)?;
    Ok(if names.is_empty() {
        "none yet: install a plugin that offers some".into()
    } else {
        names.join(", ")
    })
}

/// Qualify a role written on a page or `[app]`: short names become `<plugin>.<name>`.
/// Already-qualified names (`hr.hr_user`) and kernel roles stay as written.
pub fn qualify_role(plugin: &str, role: &str) -> String {
    let role = role.trim();
    if role.is_empty() {
        return String::new();
    }
    if role.contains('.') || role == ORG_ADMIN || role == USER_MANAGER {
        role.to_string()
    } else {
        format!("{plugin}.{role}")
    }
}

/// Parse a comma-separated `roles` attribute into trimmed non-empty names.
pub fn parse_roles_list(raw: &str) -> Vec<String> {
    raw.split([',', ' '])
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
}

/// Like [`allows`], but a page or app that names no roles is not open to every member.
///
/// It then needs at least one role of its own plugin (a role named `<plugin>.…`, as offered by the
/// plugin's `[[roles]]`). A plugin that offers no roles (shared master data such as parties or
/// units) has nothing of its own to gate on, so it is open to anyone holding at least one role;
/// a member with no roles at all sees nothing.
/// `org_roles` is every role defined in the organization.
pub fn allows_in_plugin(
    user_roles: &[String],
    required: &[String],
    plugin: &str,
    org_roles: &[String],
) -> bool {
    if !required.is_empty() {
        return allows(user_roles, required);
    }
    let prefix = format!("{plugin}.");
    let offered: Vec<&String> = org_roles.iter().filter(|r| r.starts_with(&prefix)).collect();
    if offered.is_empty() {
        return !user_roles.is_empty();
    }
    user_roles.iter().any(|role| role == ORG_ADMIN)
        || user_roles.iter().any(|role| offered.contains(&role))
}

/// The names of every role defined in the organization `db` is on.
pub async fn all_names(db: &Surreal<Client>) -> Result<Vec<String>, surrealdb::Error> {
    let mut response = db.query("SELECT VALUE name FROM roles;").await?.check()?;
    let names: Vec<String> = response.take(0)?;
    Ok(names)
}

/// True when the page/app has no required roles, the caller is `org_admin`, or
/// they hold at least one of the required roles.
pub fn allows(user_roles: &[String], required: &[String]) -> bool {
    if required.is_empty() {
        return true;
    }
    if user_roles.iter().any(|role| role == ORG_ADMIN) {
        return true;
    }
    required
        .iter()
        .any(|need| user_roles.iter().any(|have| have == need))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qualifies_short_role_names() {
        assert_eq!(qualify_role("hr", "hr_user"), "hr.hr_user");
        assert_eq!(qualify_role("hr", "hr.hr_manager"), "hr.hr_manager");
        assert_eq!(qualify_role("hr", ORG_ADMIN), ORG_ADMIN);
    }

    #[test]
    fn unroled_page_needs_a_role_of_its_plugin() {
        let org = vec!["erp_inventory.stock_reader".to_string(), "hr.hr_user".to_string()];
        let none: Vec<String> = vec![];
        // The plugin offers roles and the user has none of them: hidden.
        assert!(!allows_in_plugin(&none, &[], "erp_inventory", &org));
        assert!(!allows_in_plugin(&["hr.hr_user".to_string()], &[], "erp_inventory", &org));
        // Holding one of the plugin's roles opens it.
        assert!(allows_in_plugin(&["erp_inventory.stock_reader".to_string()], &[], "erp_inventory", &org));
        // org_admin holds every role.
        assert!(allows_in_plugin(&[ORG_ADMIN.to_string()], &[], "erp_inventory", &org));
        // A plugin with no roles of its own is open to anyone who holds some role, but not to a member with none.
        assert!(allows_in_plugin(&["hr.hr_user".to_string()], &[], "party", &org));
        assert!(!allows_in_plugin(&none, &[], "party", &org));
        // Declared roles still decide, as before.
        assert!(!allows_in_plugin(&none, &["hr.hr_user".to_string()], "hr", &org));
    }

    #[test]
    fn allows_empty_required_or_matching_role() {
        assert!(allows(&[], &[]));
        assert!(allows(&["hr.hr_user".into()], &[]));
        assert!(allows(&["hr.hr_user".into()], &["hr.hr_user".into()]));
        assert!(allows(&[ORG_ADMIN.into()], &["hr.hr_user".into()]));
        assert!(!allows(
            &["hr.hr_user".into()],
            &["hr_leave.leave_admin".into()]
        ));
    }

    #[test]
    fn custom_role_names() {
        assert_eq!(custom_role_name("shift_lead").unwrap(), "custom.shift_lead");
        assert_eq!(
            custom_role_name("custom.shift_lead").unwrap(),
            "custom.shift_lead"
        );
        assert!(custom_role_name("Shift Lead").is_err());
        assert!(custom_role_name("hr.manager").is_err());
        assert!(is_custom_role("custom.shift_lead"));
        assert!(!is_custom_role("hr.hr_manager"));
        assert!(!is_custom_role("custom."));
    }
}

/// The names of the roles the person `core_user_id` holds in the organization `db` is on.
pub async fn roles_of(
    db: &Surreal<Client>,
    core_user_id: &str,
) -> Result<Vec<String>, surrealdb::Error> {
    roles_of_uncached(db, core_user_id).await
}

async fn roles_of_uncached(
    db: &Surreal<Client>,
    core_user_id: &str,
) -> Result<Vec<String>, surrealdb::Error> {
    let mut response = db
        .query(
            "SELECT VALUE role.name FROM org_user_roles \
             WHERE org_user.core_user_id = $user AND deleted_at = NONE \
               AND org_user.deleted_at = NONE;",
        )
        .bind(("user", core_user_id.to_string()))
        .await?
        .check()?;
    let mut names: Vec<String> = response.take(0)?;
    names.sort();
    names.dedup();
    Ok(names)
}

/// Like [`roles_of`], but serves from Mantle warm-set when present.
pub async fn roles_of_cached(
    cache: &crate::cache::Cache,
    org: &str,
    db: &Surreal<Client>,
    core_user_id: &str,
) -> Result<Vec<String>, surrealdb::Error> {
    if let Ok(Some(roles)) = cache.warm_get_roles(org, core_user_id) {
        return Ok(roles);
    }
    let roles = roles_of_uncached(db, core_user_id).await?;
    let _ = cache.warm_set_roles(org, core_user_id, &roles);
    Ok(roles)
}

/// Someone who holds a role.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct RoleHolder {
    pub user_id: String,
    pub display_name: String,
}

/// A role with the people who hold it.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RoleSummary {
    pub name: String,
    pub label: String,
    pub description: Option<String>,
    pub is_system: bool,
    pub holders: Vec<RoleHolder>,
}

/// True when `name` is an organization-defined custom role.
pub fn is_custom_role(name: &str) -> bool {
    name.starts_with(CUSTOM_PREFIX) && name.len() > CUSTOM_PREFIX.len()
}

/// Normalize a slug or full name into `custom.<slug>`.
pub fn custom_role_name(slug_or_name: &str) -> Result<String, RoleError> {
    let raw = slug_or_name.trim();
    let slug = raw.strip_prefix(CUSTOM_PREFIX).unwrap_or(raw).trim();
    if slug.is_empty()
        || slug.len() > 64
        || !slug
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    {
        return Err(RoleError::InvalidSlug(slug_or_name.to_string()));
    }
    Ok(format!("{CUSTOM_PREFIX}{slug}"))
}

/// Create an organization-defined role (`custom.<slug>`).
pub async fn create_custom(
    db: &Surreal<Client>,
    slug: &str,
    label: &str,
    description: Option<&str>,
) -> Result<RoleSummary, RoleError> {
    let name = custom_role_name(slug)?;
    let label = label.trim();
    if label.is_empty() {
        return Err(RoleError::EmptyLabel);
    }
    let mut response = db
        .query("SELECT VALUE name FROM roles WHERE name = $name LIMIT 1;")
        .bind(("name", name.clone()))
        .await?
        .check()?;
    let existing: Vec<String> = response.take(0)?;
    if existing.into_iter().next().is_some() {
        return Err(RoleError::AlreadyExists(name));
    }
    let description = description
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    db.query(
        "CREATE roles SET name = $name, label = $label, description = $description, is_system = false;",
    )
    .bind(("name", name.clone()))
    .bind(("label", label.to_string()))
    .bind(("description", description.clone()))
    .await?
    .check()?;
    Ok(RoleSummary {
        name,
        label: label.to_string(),
        description,
        is_system: false,
        holders: Vec::new(),
    })
}

/// Update label/description of a custom role.
pub async fn update_custom(
    db: &Surreal<Client>,
    role: &str,
    label: Option<&str>,
    description: Option<Option<&str>>,
) -> Result<(), RoleError> {
    let name = role.trim();
    if !is_custom_role(name) {
        return Err(RoleError::NotCustom);
    }
    let mut response = db
        .query("SELECT name, is_system FROM roles WHERE name = $name LIMIT 1;")
        .bind(("name", name.to_string()))
        .await?
        .check()?;
    #[derive(Deserialize)]
    struct Row {
        is_system: Option<bool>,
    }
    let rows: Vec<serde_json::Value> = response.take(0)?;
    let Some(row) = rows
        .into_iter()
        .next()
        .and_then(|v| serde_json::from_value::<Row>(v).ok())
    else {
        return Err(RoleError::UnknownRole {
            role: name.to_string(),
            known: known_roles(db).await?,
        });
    };
    if row.is_system.unwrap_or(false) {
        return Err(RoleError::SystemRole);
    }
    if let Some(label) = label {
        let label = label.trim();
        if label.is_empty() {
            return Err(RoleError::EmptyLabel);
        }
        db.query("UPDATE roles SET label = $label WHERE name = $name;")
            .bind(("name", name.to_string()))
            .bind(("label", label.to_string()))
            .await?
            .check()?;
    }
    if let Some(description) = description {
        let description = description
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        db.query("UPDATE roles SET description = $description WHERE name = $name;")
            .bind(("name", name.to_string()))
            .bind(("description", description))
            .await?
            .check()?;
    }
    Ok(())
}

/// Soft-revoke every holder, then delete a custom role row.
pub async fn delete_custom(db: &Surreal<Client>, role: &str) -> Result<(), RoleError> {
    let name = role.trim();
    if !is_custom_role(name) {
        return Err(RoleError::NotCustom);
    }
    let mut response = db
        .query("SELECT VALUE id FROM roles WHERE name = $name AND is_system = false LIMIT 1;")
        .bind(("name", name.to_string()))
        .await?
        .check()?;
    let ids: Vec<surrealdb::types::RecordId> = response.take(0)?;
    let Some(role_id) = ids.into_iter().next() else {
        // Either missing or is_system — distinguish.
        let mut check = db
            .query("SELECT VALUE is_system FROM roles WHERE name = $name LIMIT 1;")
            .bind(("name", name.to_string()))
            .await?
            .check()?;
        let flags: Vec<bool> = check.take(0)?;
        return match flags.into_iter().next() {
            Some(true) => Err(RoleError::SystemRole),
            Some(false) => Err(RoleError::UnknownRole {
                role: name.to_string(),
                known: known_roles(db).await?,
            }),
            None => Err(RoleError::UnknownRole {
                role: name.to_string(),
                known: known_roles(db).await?,
            }),
        };
    };
    db.query(
        "UPDATE org_user_roles SET deleted_at = time::now() \
         WHERE role = $role AND deleted_at = NONE; \
         DELETE $role;",
    )
    .bind(("role", role_id))
    .await?
    .check()?;
    Ok(())
}

/// Every role of the organization `db` is on, with who holds it.
pub async fn list(db: &Surreal<Client>) -> Result<Vec<RoleSummary>, surrealdb::Error> {
    ensure_system_roles(db).await?;
    #[derive(Deserialize)]
    struct Row {
        name: String,
        label: Option<String>,
        description: Option<String>,
        #[serde(default)]
        is_system: Option<bool>,
    }
    #[derive(Deserialize)]
    struct Held {
        role: String,
        user_id: Option<String>,
        who: Option<String>,
    }
    let mut response = db
        .query(
            "SELECT name, label, description, is_system FROM roles ORDER BY name; \
             SELECT role.name AS role, org_user.core_user_id AS user_id, \
                    org_user.display_name AS who \
             FROM org_user_roles \
             WHERE deleted_at = NONE AND org_user.deleted_at = NONE;",
        )
        .await?
        .check()?;
    let roles: Vec<serde_json::Value> = response.take(0)?;
    let held: Vec<serde_json::Value> = response.take(1)?;
    let held: Vec<Held> = held
        .into_iter()
        .filter_map(|v| serde_json::from_value(v).ok())
        .collect();
    Ok(roles
        .into_iter()
        .filter_map(|v| serde_json::from_value::<Row>(v).ok())
        .map(|row| {
            let is_system = row.is_system.unwrap_or_else(|| {
                row.name == ORG_ADMIN
                    || row.name == USER_MANAGER
                    || (row.name.contains('.') && !is_custom_role(&row.name))
            });
            RoleSummary {
                holders: held
                    .iter()
                    .filter(|h| h.role == row.name)
                    .filter_map(|h| {
                        let user_id = h.user_id.clone()?;
                        Some(RoleHolder {
                            display_name: h
                                .who
                                .clone()
                                .filter(|s| !s.is_empty())
                                .unwrap_or_else(|| user_id.clone()),
                            user_id,
                        })
                    })
                    .collect(),
                label: row.label.unwrap_or_else(|| row.name.clone()),
                description: row.description,
                is_system,
                name: row.name,
            }
        })
        .collect())
}

/// The id (`users:abc`) of the account called `login` (a username or an email), looked up in the
/// core database.
async fn core_user_id(
    db: &Surreal<Client>,
    namespace: &str,
    login: &str,
) -> Result<String, RoleError> {
    db.use_ns(namespace).await?;
    db.use_db("core").await?;
    let mut response = db
        .query("SELECT VALUE <string> id FROM users WHERE username = $login OR email = $login LIMIT 1;")
        .bind(("login", login.to_string()))
        .await?
        .check()?;
    let found: Vec<String> = response.take(0)?;
    found
        .into_iter()
        .next()
        .ok_or_else(|| RoleError::UnknownUser(login.to_string()))
}

/// Give the user `login` a role in the organization whose database is `organization`.
pub async fn grant(
    db: &Surreal<Client>,
    namespace: &str,
    organization: &str,
    login: &str,
    role: &str,
) -> Result<(), RoleError> {
    let user = core_user_id(db, namespace, login).await?;
    db.use_db(organization).await?;
    match grant_by_id(db, &user, role).await {
        Err(RoleError::NotAMember(_, _)) => Err(RoleError::NotAMember(
            login.to_string(),
            organization.to_string(),
        )),
        other => other,
    }
}

/// Take a role away from the user `login`.
pub async fn revoke(
    db: &Surreal<Client>,
    namespace: &str,
    organization: &str,
    login: &str,
    role: &str,
) -> Result<(), RoleError> {
    let user = core_user_id(db, namespace, login).await?;
    db.use_db(organization).await?;
    revoke_by_id(db, &user, role).await
}

/// The roles of the organization whose database is `organization`.
pub async fn list_in(
    db: &Surreal<Client>,
    namespace: &str,
    organization: &str,
) -> Result<Vec<RoleSummary>, RoleError> {
    db.use_ns(namespace).await?;
    db.use_db(organization).await?;
    Ok(list(db).await?)
}
