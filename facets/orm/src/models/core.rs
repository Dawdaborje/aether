use serde::{Deserialize, Serialize};
use surrealdb::types::{Datetime, RecordId, SurrealValue};

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
pub struct CoreUser {
    pub id: RecordId,
    pub username: Option<String>,
    pub email: Option<String>,
    pub display_name: Option<String>,
    pub hashed_password: Option<String>,
    pub is_super_user: bool,
    pub is_active: bool,
    pub is_email_verified: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
pub struct NewCoreUser {
    pub username: String,
    pub email: String,
    pub display_name: String,
    pub hashed_password: String,
    pub is_super_user: bool,
    pub is_active: bool,
    pub is_email_verified: bool,
    pub date_created: Datetime,
    pub date_updated: Datetime,
}

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
pub struct Organization {
    pub id: RecordId,
    pub name: String,
    pub db_name: String,
    /// Core `users:…` id of the organization owner (transferable; admins are separate).
    #[serde(default)]
    pub owner_user_id: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    /// Parent organization's `db_name`, when this org sits under another.
    #[serde(default)]
    pub parent_db_name: Option<String>,
    /// Filename under `app_dir/orgs/<db>/` for the logo (`logo.png`, …).
    #[serde(default)]
    pub logo_file: Option<String>,
    /// The organization's contact address. Required when an organization is created; older
    /// organizations may not have one until an administrator sets it.
    #[serde(default)]
    pub email: Option<String>,
    pub date_created: Datetime,
    pub date_updated: Datetime,
}

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
pub struct NewOrganization {
    pub name: String,
    pub db_name: String,
    #[serde(default)]
    pub owner_user_id: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub parent_db_name: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    pub date_created: Datetime,
    pub date_updated: Datetime,
}

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
pub struct OrgDatabase {
    pub id: RecordId,
    pub db_name: String,
    pub date_created: Datetime,
    pub date_updated: Datetime,
}

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
pub struct NewOrgDatabase {
    pub db_name: String,
    pub date_created: Datetime,
    pub date_updated: Datetime,
}

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
pub struct OrganizationUser {
    pub organization_id: String,
    pub user_id: String,
    pub date_created: Datetime,
    #[serde(default)]
    pub deleted_at: Option<Datetime>,
    #[serde(default)]
    pub deleted_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
pub struct OrgUser {
    pub core_user_id: String,
    pub display_name: Option<String>,
    pub is_active: bool,
    pub date_created: Datetime,
    pub date_updated: Datetime,
    #[serde(default)]
    pub deleted_at: Option<Datetime>,
    #[serde(default)]
    pub deleted_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
pub struct TenantOrganization {
    pub name: String,
    pub date_created: Datetime,
    pub date_updated: Datetime,
}
