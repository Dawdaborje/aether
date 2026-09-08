use aether_orm::models::core::{
    Company, CompanyUser, CoreUser, NewCompany, NewCoreUser, NewOrgDatabase, NewOrganization,
    OrgUser, Organization, OrganizationUser, TenantOrganization,
};
use aether_orm::{hash_password, migrate_org};
use surrealdb::{
    Surreal,
    engine::remote::ws::Client,
    types::{Datetime, ToSql},
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum OrganizationError {
    #[error("surrealdb error: {0}")]
    Surreal(#[from] surrealdb::Error),

    #[error("organization name cannot be empty")]
    EmptyOrganizationName,

    #[error("company name cannot be empty")]
    EmptyCompanyName,

    #[error("user username, email, and password are required")]
    MissingUserCredentials,

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

    #[error("company `{0}` was not found")]
    CompanyNotFound(String),

    #[error("organization migration failed: {0}")]
    Migration(#[from] aether_orm::MigrationError),
}

pub async fn create_organization(
    db: &Surreal<Client>,
    namespace: &str,
    organization_name: &str,
    organization_db_name: Option<&str>,
    company_name: &str,
    company_email: Option<&str>,
    username: &str,
    email: &str,
    password: &str,
) -> Result<String, OrganizationError> {
    if organization_name.trim().is_empty() {
        return Err(OrganizationError::EmptyOrganizationName);
    }
    if company_name.trim().is_empty() {
        return Err(OrganizationError::EmptyCompanyName);
    }
    if username.trim().is_empty() || email.trim().is_empty() || password.is_empty() {
        return Err(OrganizationError::MissingUserCredentials);
    }

    let db_name = organization_db_name
        .map(str::to_owned)
        .unwrap_or_else(|| slugify(organization_name));
    if db_name.is_empty()
        || !db_name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return Err(OrganizationError::InvalidDatabaseName(db_name));
    }

    db.use_ns(namespace).await?;
    db.use_db("core").await?;

    let user_id = if let Some(existing_user_id) = find_existing_user_id(db, username, email).await?
    {
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

    let company_code = slugify(company_name);
    let company_id = company_code.clone();
    let now = Datetime::now();
    let _: Option<Company> = db
        .create::<Option<Company>>(("companies", company_id.clone()))
        .content(NewCompany {
            name: company_name.to_string(),
            code: company_code,
            email: company_email.map(str::to_owned),
            date_created: now.clone(),
            date_updated: now.clone(),
        })
        .await?;
    let _: Option<Organization> = db
        .create::<Option<Organization>>(("organizations", db_name.clone()))
        .content(NewOrganization {
            name: organization_name.to_string(),
            db_name: db_name.clone(),
            company_id: format!("companies:{company_id}"),
            date_created: now.clone(),
            date_updated: now.clone(),
        })
        .await?;
    let _: Option<CompanyUser> = db
        .update::<Option<CompanyUser>>((
            "company_users",
            format!("{}_{}", company_id, slugify(&user_id)),
        ))
        .content(CompanyUser {
            company_id: format!("companies:{company_id}"),
            user_id: user_id.clone(),
            date_created: now.clone(),
        })
        .await?;
    let _: Option<OrganizationUser> = db
        .update::<Option<OrganizationUser>>((
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
        .update::<Option<aether_orm::models::core::OrgDatabase>>(("org_databases", db_name.clone()))
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

    Ok(db_name)
}

pub async fn assign_user(
    db: &Surreal<Client>,
    namespace: &str,
    user_login: &str,
    organization_db_name: &str,
    company_name: &str,
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
    let companies: Vec<Company> = db.select("companies").await?;
    let company_id = companies
        .into_iter()
        .find(|company| company.name == company_name)
        .map(|company| record_id_string(&company.id))
        .ok_or_else(|| OrganizationError::CompanyNotFound(company_name.to_string()))?;

    let membership_key = slugify(&user_id);
    let _: Option<OrganizationUser> = db
        .update::<Option<OrganizationUser>>((
            "organization_users",
            format!("{}_{}", slugify(organization_db_name), membership_key),
        ))
        .content(OrganizationUser {
            organization_id,
            user_id: user_id.clone(),
            date_created: Datetime::now(),
        })
        .await?;
    let _: Option<CompanyUser> = db
        .update::<Option<CompanyUser>>((
            "company_users",
            format!("{}_{}", slugify(company_name), membership_key),
        ))
        .content(CompanyUser {
            company_id,
            user_id: user_id.clone(),
            date_created: Datetime::now(),
        })
        .await?;

    create_and_migrate_org_database(db, namespace, organization_db_name).await?;
    db.use_ns(namespace).await?;
    db.use_db(organization_db_name).await?;
    let _: Option<OrgUser> = db
        .update::<Option<OrgUser>>(("org_users", membership_key))
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

fn slugify(value: &str) -> String {
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
    fn builds_company_code_from_name() {
        assert_eq!(slugify("Acme  Holdings, Ltd."), "acme_holdings_ltd");
    }
}
