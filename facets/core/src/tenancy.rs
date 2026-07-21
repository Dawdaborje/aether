use axum::http::Request;
use serde::{Deserialize, Serialize};

use crate::config_manager::models::{OrgResolutionMode, TenancyConfig};

/// Resolved org context used for settings override and DB switching.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OrgRef {
    /// Public slug (header / subdomain / path segment).
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

/// Resolve org for this request using tenancy mode.
///
/// - `session_only`: use `session_org` only (None on login / anonymous).
/// - `header` / `subdomain` / `path`: prefer request-derived org; fall back to session.
pub fn resolve_org_slug<B>(
    req: &Request<B>,
    tenancy: &TenancyConfig,
    session_org: Option<&OrgRef>,
) -> Option<OrgRef> {
    let from_request = match tenancy.org_resolution {
        OrgResolutionMode::SessionOnly => None,
        OrgResolutionMode::Header => org_from_header(req, &tenancy.org_header),
        OrgResolutionMode::Subdomain => org_from_subdomain(req),
        OrgResolutionMode::Path => org_from_path(req, &tenancy.org_path_prefix),
    };

    from_request.or_else(|| session_org.cloned())
}

fn org_from_header<B>(req: &Request<B>, header_name: &str) -> Option<OrgRef> {
    req.headers()
        .get(header_name)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(OrgRef::from_slug)
}

fn org_from_subdomain<B>(req: &Request<B>) -> Option<OrgRef> {
    let host = req
        .headers()
        .get(axum::http::header::HOST)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let host = host.split(':').next().unwrap_or(host);
    let parts: Vec<&str> = host.split('.').collect();
    // Require at least org.domain.tld
    if parts.len() < 3 {
        return None;
    }
    let label = parts[0];
    if label.eq_ignore_ascii_case("www") || label.eq_ignore_ascii_case("localhost") {
        return None;
    }
    Some(OrgRef::from_slug(label))
}

fn org_from_path<B>(req: &Request<B>, prefix: &str) -> Option<OrgRef> {
    let path = req.uri().path();
    let prefix = prefix.trim_end_matches('/');
    let rest = path.strip_prefix(prefix)?;
    let rest = rest.strip_prefix('/')?;
    let slug = rest.split('/').next().filter(|s| !s.is_empty())?;
    Some(OrgRef::from_slug(slug))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Request;

    #[test]
    fn session_only_ignores_header() {
        let tenancy = TenancyConfig {
            org_resolution: OrgResolutionMode::SessionOnly,
            ..Default::default()
        };
        let req = Request::builder()
            .header("X-Org-Slug", "acme")
            .body(())
            .unwrap();
        assert!(resolve_org_slug(&req, &tenancy, None).is_none());
        let session = OrgRef::from_slug("acme");
        assert_eq!(
            resolve_org_slug(&req, &tenancy, Some(&session)).as_ref(),
            Some(&session)
        );
    }

    #[test]
    fn header_mode_reads_slug() {
        let tenancy = TenancyConfig {
            org_resolution: OrgResolutionMode::Header,
            ..Default::default()
        };
        let req = Request::builder()
            .header("X-Org-Slug", "Acme")
            .body(())
            .unwrap();
        let org = resolve_org_slug(&req, &tenancy, None).unwrap();
        assert_eq!(org.slug, "acme");
        assert_eq!(org.db_name, "org_acme");
    }

    #[test]
    fn path_mode_reads_prefix() {
        let tenancy = TenancyConfig {
            org_resolution: OrgResolutionMode::Path,
            org_path_prefix: "/o".into(),
            ..Default::default()
        };
        let req = Request::builder().uri("/o/beta/settings").body(()).unwrap();
        let org = resolve_org_slug(&req, &tenancy, None).unwrap();
        assert_eq!(org.slug, "beta");
    }
}
