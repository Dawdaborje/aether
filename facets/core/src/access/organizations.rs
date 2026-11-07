//! The organization a user can enter. An installation serves exactly one, so nothing asks
//! which one a request is for. (Several organizations per installation are an Aether
//! Enterprise feature.)

use aether_orm::ValidSession;
use axum::http::{HeaderMap, Uri};
use serde::{Deserialize, Serialize};
use surrealdb::types::SurrealValue;

use super::visitor::record_id_string;
use crate::state::{AppState, Databases};

/// An organization as read from the catalog.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, SurrealValue)]
pub struct OrgChoice {
    /// The database name; what is sent as the organization identifier.
    pub db_name: String,
    pub name: String,
    #[serde(default)]
    pub logo_file: Option<String>,
}

/// An organization a user can enter, as shown in a chooser.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct OrgEntry {
    pub db_name: String,
    pub name: String,
    /// The user belongs to it. A developer can also enter organizations they
    /// do not belong to; those have `member: false`.
    pub member: bool,
    /// `/api/ui/organizations/{db}/logo` when a logo has been uploaded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logo_url: Option<String>,
}

/// How the app should treat organization choice. Always `single`; the field stays in the
/// API so clients written against it keep working.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SelectionMode {
    /// There is one organization, so there is nothing to choose and nothing to switch.
    Single,
}

#[derive(Debug, SurrealValue)]
struct MembershipRow {
    organization_id: String,
}

/// The organizations `session`'s user can enter, with display names: their
/// memberships, or for a developer every organization. Disabled organizations are left out.
pub async fn user_organizations(
    databases: &Databases,
    session: &ValidSession,
) -> Result<Vec<OrgChoice>, surrealdb::Error> {
    let core = databases.core().await?;
    if session.user.is_super_user {
        let mut response = core
            .query("SELECT name, db_name, logo_file FROM organizations WHERE disabled_at = NONE ORDER BY name;")
            .await?
            .check()?;
        return response.take(0);
    }

    let user_id = record_id_string(&session.user.id);
    let mut response = core
        .query(
            "SELECT organization_id FROM organization_users \
             WHERE user_id = $user AND deleted_at = NONE;",
        )
        .bind(("user", user_id))
        .await?
        .check()?;
    let memberships: Vec<MembershipRow> = response.take(0)?;
    let db_names: Vec<String> = memberships
        .into_iter()
        .filter_map(|row| {
            row.organization_id
                .strip_prefix("organizations:")
                .map(str::to_string)
        })
        .collect();
    if db_names.is_empty() {
        return Ok(Vec::new());
    }
    let mut response = core
        .query(
            "SELECT name, db_name, logo_file FROM organizations \
             WHERE db_name IN $dbs AND disabled_at = NONE ORDER BY name;",
        )
        .bind(("dbs", db_names))
        .await?
        .check()?;
    response.take(0)
}

fn org_logo_url(db_name: &str, logo_file: Option<&str>) -> Option<String> {
    logo_file
        .map(str::trim)
        .filter(|file| !file.is_empty())
        .map(|_| format!("/api/ui/organizations/{db_name}/logo"))
}

/// What the web app needs to know to offer an organization switch.
#[derive(Debug, Clone, Serialize)]
pub struct OrgOverview {
    pub mode: SelectionMode,
    /// Everything the user can enter; `member` says which they belong to.
    pub organizations: Vec<OrgEntry>,
    /// The organization this request is already for, if it is known.
    pub current: Option<String>,
    /// The user must pick before anything else can load.
    pub selection_required: bool,
}

/// The user's organization: the one organization of this installation.
pub async fn overview(
    state: &AppState,
    _headers: &HeaderMap,
    _uri: &Uri,
    session: &ValidSession,
) -> Result<OrgOverview, surrealdb::Error> {
    let enterable = user_organizations(state.databases(), session).await?;
    let only = &state.config.tenancy.single_org_db_name;
    let organizations: Vec<OrgEntry> = enterable
        .into_iter()
        .filter(|org| &org.db_name == only)
        .map(|org| {
            let logo_url = org_logo_url(&org.db_name, org.logo_file.as_deref());
            OrgEntry { member: true, db_name: org.db_name, name: org.name, logo_url }
        })
        .collect();
    Ok(OrgOverview {
        mode: SelectionMode::Single,
        organizations,
        current: Some(only.clone()),
        selection_required: false,
    })
}

/// The organization this session is in: always the installation's one organization.
pub async fn effective_org_database(
    state: &AppState,
    _session: &ValidSession,
) -> Result<Option<String>, surrealdb::Error> {
    Ok(Some(state.config.tenancy.single_org_db_name.clone()))
}

/// Memberships of a user, for deciding whether the request needs a choice.
pub async fn membership_db_names(
    state: &AppState,
    user_id: &str,
) -> Result<Vec<String>, surrealdb::Error> {
    if let Ok(Some(orgs)) = state.cache.warm_get_memberships(user_id) {
        return Ok(orgs);
    }
    let core = state.core().await?;
    let mut response = core
        .query(
            "SELECT organization_id FROM organization_users \
             WHERE user_id = $user AND deleted_at = NONE;",
        )
        .bind(("user", user_id.to_string()))
        .await?
        .check()?;
    let rows: Vec<MembershipRow> = response.take(0)?;
    let orgs: Vec<String> = rows
        .into_iter()
        .filter_map(|row| {
            row.organization_id
                .strip_prefix("organizations:")
                .map(str::to_string)
        })
        .collect();
    let _ = state.cache.warm_set_memberships(user_id, &orgs);
    Ok(orgs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn org_entries_serialize_with_the_membership_flag() -> Result<(), serde_json::Error> {
        let entry = OrgEntry {
            db_name: "acme".into(),
            name: "Acme".into(),
            member: true,
            logo_url: None,
        };
        assert_eq!(
            serde_json::to_value(&entry)?,
            serde_json::json!({ "db_name": "acme", "name": "Acme", "member": true })
        );
        Ok(())
    }
}
