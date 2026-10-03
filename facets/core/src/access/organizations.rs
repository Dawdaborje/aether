//! Which organizations a user can enter, and which one a request is for.
//!
//! How the organization is chosen depends on `[tenancy] org_resolution`:
//!
//! | Mode | Who decides | The app needs to ask? |
//! |---|---|---|
//! | `subdomain`, `path` | the address (`acme.example.com`, `/o/acme`) | no |
//! | `header` | the client sends `X-Org-Slug` on every request | yes, when the user has several |
//! | `session_only` | the session remembers the choice | yes, when the user has several |
//!
//! Whoever belongs to exactly one organization is never asked: it is used. A user
//! with two or more must choose, developers included. A developer who belongs
//! to fewer than two is not asked; they work from the Organizations page, and
//! can enter any organization from there.

use aether_orm::ValidSession;
use axum::http::{HeaderMap, Request, Uri};
use serde::Serialize;
use surrealdb::types::SurrealValue;

use super::visitor::record_id_string;
use crate::config_manager::models::OrgResolutionMode;
use crate::state::AppState;
use crate::tenancy::resolve_org_slug;

/// An organization as read from the catalog.
#[derive(Debug, Clone, PartialEq, Eq, SurrealValue)]
pub struct OrgChoice {
    /// The database name; what is sent as the organization identifier.
    pub db_name: String,
    pub name: String,
}

/// An organization a user can enter, as shown in a chooser.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct OrgEntry {
    pub db_name: String,
    pub name: String,
    /// The user belongs to it. A developer can also enter organizations they
    /// do not belong to; those have `member: false`.
    pub member: bool,
}

/// How the app should treat organization choice for this deployment.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SelectionMode {
    /// The address decides (`subdomain`, `path`): never ask.
    Address,
    /// The client sends `X-Org-Slug`: ask, and remember the choice client-side.
    Header,
    /// The session remembers the choice: ask, and store it in the session.
    Session,
}

impl SelectionMode {
    pub fn of(mode: &OrgResolutionMode) -> Self {
        match mode {
            OrgResolutionMode::Subdomain | OrgResolutionMode::Path => Self::Address,
            OrgResolutionMode::Header => Self::Header,
            OrgResolutionMode::SessionOnly => Self::Session,
        }
    }
}

#[derive(Debug, SurrealValue)]
struct MembershipRow {
    organization_id: String,
}

/// The organizations `session`'s user can enter, with display names: their
/// memberships, or for a developer every organization.
pub async fn user_organizations(
    state: &AppState,
    session: &ValidSession,
) -> Result<Vec<OrgChoice>, surrealdb::Error> {
    let core = state.core().await?;
    if session.user.is_super_user {
        let mut response = core
            .query("SELECT name, db_name FROM organizations ORDER BY name;")
            .await?
            .check()?;
        return response.take(0);
    }

    let user_id = record_id_string(&session.user.id);
    let mut response = core
        .query("SELECT organization_id FROM organization_users WHERE user_id = $user;")
        .bind(("user", user_id))
        .await?
        .check()?;
    let memberships: Vec<MembershipRow> = response.take(0)?;
    let db_names: Vec<String> = memberships
        .into_iter()
        .filter_map(|row| row.organization_id.strip_prefix("organizations:").map(str::to_string))
        .collect();
    if db_names.is_empty() {
        return Ok(Vec::new());
    }
    let mut response = core
        .query("SELECT name, db_name FROM organizations WHERE db_name IN $dbs ORDER BY name;")
        .bind(("dbs", db_names))
        .await?
        .check()?;
    response.take(0)
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

/// Work out the user's organizations and which one (if any) the request is for.
pub async fn overview(
    state: &AppState,
    headers: &HeaderMap,
    uri: &Uri,
    session: &ValidSession,
) -> Result<OrgOverview, surrealdb::Error> {
    let enterable = user_organizations(state, session).await?;
    let memberships = membership_db_names(
        state,
        &super::visitor::record_id_string(&session.user.id),
    )
    .await?;
    let organizations: Vec<OrgEntry> = enterable
        .into_iter()
        .map(|org| OrgEntry {
            member: memberships.contains(&org.db_name),
            db_name: org.db_name,
            name: org.name,
        })
        .collect();
    let mode = SelectionMode::of(&state.config.tenancy.org_resolution);

    // The organization the request itself names (header, subdomain or path).
    let mut request = Request::builder().uri(uri.clone());
    for (name, value) in headers {
        request = request.header(name, value);
    }
    let named_by_request = request
        .body(())
        .ok()
        .and_then(|request| resolve_org_slug(&request, &state.config.tenancy, None));

    let known = |candidate: &str| organizations.iter().any(|org| org.db_name == candidate);
    let from_request = named_by_request.as_ref().and_then(|org| {
        [org.slug.as_str(), org.db_name.as_str()]
            .into_iter()
            .find(|candidate| known(candidate))
            .map(str::to_string)
    });
    let from_session = session
        .org_database_id
        .as_deref()
        .filter(|candidate| known(candidate))
        .map(str::to_string);

    let (current, selection_required) = decide_current(
        from_request,
        from_session,
        &memberships,
        session_left_organization(session),
        mode,
    );

    Ok(OrgOverview {
        mode,
        organizations,
        current,
        selection_required,
    })
}

/// A developer who chose to leave their organization: they are in none until they enter
/// one again, even if they have exactly one membership.
pub fn session_left_organization(session: &ValidSession) -> bool {
    session.user.is_super_user
        && session.org_database_id.as_deref() == Some(aether_orm::LEFT_ORGANIZATION)
}

/// Which organization the request is for, and whether the user must choose first.
///
/// An organization named by the request or remembered by the session wins. Otherwise a
/// single membership is the organization. Two or more memberships with nothing choosing
/// one means the user must choose (developers too); a developer who left is never asked.
fn decide_current(
    from_request: Option<String>,
    from_session: Option<String>,
    memberships: &[String],
    left: bool,
    mode: SelectionMode,
) -> (Option<String>, bool) {
    let only = match memberships {
        [only] if !left => Some(only.clone()),
        _ => None,
    };
    let current = from_request.or(from_session).or(only);
    let selection_required =
        mode != SelectionMode::Address && memberships.len() > 1 && current.is_none() && !left;
    (current, selection_required)
}

/// Memberships of a user, for deciding whether the request needs a choice.
pub async fn membership_db_names(
    state: &AppState,
    user_id: &str,
) -> Result<Vec<String>, surrealdb::Error> {
    let core = state.core().await?;
    let mut response = core
        .query("SELECT organization_id FROM organization_users WHERE user_id = $user;")
        .bind(("user", user_id.to_string()))
        .await?
        .check()?;
    let rows: Vec<MembershipRow> = response.take(0)?;
    Ok(rows
        .into_iter()
        .filter_map(|row| row.organization_id.strip_prefix("organizations:").map(str::to_string))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn org_entries_serialize_with_the_membership_flag() -> Result<(), serde_json::Error> {
        let entry = OrgEntry { db_name: "acme".into(), name: "Acme".into(), member: true };
        assert_eq!(
            serde_json::to_value(&entry)?,
            serde_json::json!({ "db_name": "acme", "name": "Acme", "member": true })
        );
        Ok(())
    }

    fn names(names: &[&str]) -> Vec<String> {
        names.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn a_single_membership_is_the_organization() {
        let (current, required) =
            decide_current(None, None, &names(&["acme"]), false, SelectionMode::Session);
        assert_eq!((current.as_deref(), required), (Some("acme"), false));
    }

    #[test]
    fn several_memberships_and_no_choice_ask_the_user() {
        let (current, required) =
            decide_current(None, None, &names(&["acme", "globex"]), false, SelectionMode::Session);
        assert_eq!((current, required), (None, true));
        // An address-decided deployment never asks.
        let (_, required) =
            decide_current(None, None, &names(&["acme", "globex"]), false, SelectionMode::Address);
        assert!(!required);
    }

    #[test]
    fn a_developer_who_left_is_in_no_organization_and_is_not_asked() {
        for memberships in [names(&[]), names(&["acme"]), names(&["acme", "globex"])] {
            let (current, required) =
                decide_current(None, None, &memberships, true, SelectionMode::Session);
            assert_eq!((current, required), (None, false), "{memberships:?}");
        }
        // Entering one again works.
        let (current, _) = decide_current(
            None,
            Some("globex".into()),
            &names(&["acme", "globex"]),
            true,
            SelectionMode::Session,
        );
        assert_eq!(current.as_deref(), Some("globex"));
    }

    #[test]
    fn only_header_and_session_tenancy_ask_the_user_to_choose() {
        assert_eq!(SelectionMode::of(&OrgResolutionMode::Subdomain), SelectionMode::Address);
        assert_eq!(SelectionMode::of(&OrgResolutionMode::Path), SelectionMode::Address);
        assert_eq!(SelectionMode::of(&OrgResolutionMode::Header), SelectionMode::Header);
        assert_eq!(SelectionMode::of(&OrgResolutionMode::SessionOnly), SelectionMode::Session);
    }
}
