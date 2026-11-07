use axum::http::Request;
use serde::{Deserialize, Serialize};

use crate::config_manager::models::TenancyConfig;

/// Resolved org context used for settings override and DB switching.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OrgRef {
    /// Public slug.
    pub slug: String,
    /// Surreal database name (typically `org_{slug}` or `org_databases.db_name`).
    pub db_name: String,
}

impl OrgRef {
    pub fn from_slug(slug: &str) -> Self {
        let slug = slug.trim().trim_matches('/').to_lowercase();
        let db_name = if slug.starts_with("org_") {
            slug.clone()
        } else {
            format!("org_{slug}")
        };
        Self { slug, db_name }
    }
}

/// The organization this request is for. An installation serves one organization, so
/// nothing in the request names one: it is the session's, when there is a session.
pub fn resolve_org_slug<B>(
    _req: &Request<B>,
    _tenancy: &TenancyConfig,
    session_org: Option<&OrgRef>,
) -> Option<OrgRef> {
    session_org.cloned()
}
