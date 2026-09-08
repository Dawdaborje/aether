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
pub struct Company {
    pub id: RecordId,
    pub name: String,
    pub code: String,
    pub email: Option<String>,
    pub description: Option<String>,
    pub date_created: Datetime,
    pub date_updated: Datetime,
}

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
pub struct NewCompany {
    pub name: String,
    pub code: String,
    pub email: Option<String>,
    pub date_created: Datetime,
    pub date_updated: Datetime,
}

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
pub struct Organization {
    pub id: RecordId,
    pub name: String,
    pub db_name: String,
    pub company_id: String,
    pub date_created: Datetime,
    pub date_updated: Datetime,
}

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
pub struct NewOrganization {
    pub name: String,
    pub db_name: String,
    pub company_id: String,
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
pub struct CompanyUser {
    pub company_id: String,
    pub user_id: String,
    pub date_created: Datetime,
}

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
pub struct OrganizationUser {
    pub organization_id: String,
    pub user_id: String,
    pub date_created: Datetime,
}

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
pub struct OrgUser {
    pub core_user_id: String,
    pub display_name: Option<String>,
    pub is_active: bool,
    pub date_created: Datetime,
    pub date_updated: Datetime,
}

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
pub struct TenantOrganization {
    pub name: String,
    pub date_created: Datetime,
    pub date_updated: Datetime,
}
