//! Plugin documentation from the catalog, for the documentation website (`aether --serve`, `/docs`).
//!
//! Visibility is decided here, per viewer: `public` docs are for everyone; `authenticated` docs for any signed-in
//! user; `org` docs for members of an organization that has the plugin installed. A developer (superuser)
//! sees everything, as everywhere else. Docs a viewer may not read are left out entirely.

use crate::access::identity::SESSION_COOKIE;
use crate::state::{AppState, Db};
use aether_docs::PluginDocsProvider;
use aether_docs::plugin_docs::{PluginDocs, Visibility};
use async_trait::async_trait;
use axum::http::{HeaderMap, header};
use serde::Deserialize;
use std::collections::HashSet;
use surrealdb::types::{RecordId, SurrealValue};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// How long the list of documented plugins is reused before the catalog is read again.
const CATALOG_TTL: Duration = Duration::from_secs(20);

/// What matters about the person asking.
#[derive(Debug, Clone, Default)]
pub struct Viewer {
    pub signed_in: bool,
    /// Sees everything.
    pub developer: bool,
    /// Plugins installed in an organization the viewer belongs to.
    pub org_plugins: HashSet<String>,
}

/// The docs `viewer` may read, from `all` (the newest version of each documented plugin).
pub fn visible_to(all: &[Arc<PluginDocs>], viewer: &Viewer) -> Vec<Arc<PluginDocs>> {
    let may_read = |docs: &PluginDocs| {
        viewer.developer
            || match docs.visibility {
                Visibility::Public => true,
                Visibility::Authenticated => viewer.signed_in,
                Visibility::Org => viewer.signed_in && viewer.org_plugins.contains(&docs.plugin),
            }
    };
    all.iter().filter(|docs| may_read(docs)).cloned().collect()
}

/// The docs of the newest active version of every plugin that has any, sorted by label. Reads only the catalog.
///
/// The newest version decides: when it has no `[docs]` any more, the plugin is not documented, even if an
/// older version had docs.
pub async fn read_catalog(core: &Db) -> Result<Vec<Arc<PluginDocs>>, surrealdb::Error> {
    #[derive(Deserialize, SurrealValue)]
    struct Row {
        id: RecordId,
        name: String,
        has_docs: bool,
    }
    let mut response = core
        .query("SELECT id, name, docs != NONE AS has_docs, date_created FROM plugins WHERE is_active = true ORDER BY date_created DESC;")
        .await?
        .check()?;
    let rows: Vec<Row> = response.take(0)?;
    let mut seen: HashSet<String> = HashSet::new();
    let mut docs: Vec<Arc<PluginDocs>> = Vec::new();
    for row in rows {
        if !seen.insert(row.name.clone()) || !row.has_docs {
            continue; // an older version of a plugin already seen, or a newest version without docs
        }
        let mut response = core.query("SELECT VALUE docs FROM ONLY $id;").bind(("id", row.id)).await?.check()?;
        let value: Option<serde_json::Value> = response.take(0)?;
        match value.map(serde_json::from_value::<PluginDocs>) {
            Some(Ok(found)) => docs.push(Arc::new(found)),
            Some(Err(e)) => log::warn!("the stored documentation of plugin `{}` cannot be read: {e}", row.name),
            None => {}
        }
    }
    docs.sort_by(|a, b| a.label.to_lowercase().cmp(&b.label.to_lowercase()).then_with(|| a.plugin.cmp(&b.plugin)));
    Ok(docs)
}

/// The documentation site's view of the catalog, for `aether --serve`.
pub struct CatalogDocs {
    state: AppState,
    cached: tokio::sync::Mutex<Option<(Instant, Vec<Arc<PluginDocs>>)>>,
}

impl CatalogDocs {
    pub fn new(state: AppState) -> Self {
        Self { state, cached: tokio::sync::Mutex::new(None) }
    }

    async fn catalog(&self) -> Vec<Arc<PluginDocs>> {
        let mut cached = self.cached.lock().await;
        if let Some((at, docs)) = cached.as_ref()
            && at.elapsed() < CATALOG_TTL
        {
            return docs.clone();
        }
        let fresh = match self.state.core().await {
            Ok(core) => read_catalog(&core).await.unwrap_or_else(|e| {
                log::error!("plugin documentation: cannot read the catalog: {e}");
                Vec::new()
            }),
            Err(e) => {
                log::error!("plugin documentation: cannot open the core database: {e}");
                Vec::new()
            }
        };
        *cached = Some((Instant::now(), fresh.clone()));
        fresh
    }

    /// Who is asking. Organization membership is looked up only when some docs need it.
    async fn viewer(&self, headers: &HeaderMap, need_orgs: bool) -> Viewer {
        let token = headers
            .get(header::COOKIE)
            .and_then(|v| v.to_str().ok())
            .and_then(|cookies| {
                cookies.split(';').find_map(|part| part.trim().strip_prefix(&format!("{SESSION_COOKIE}=")).filter(|t| !t.is_empty()).map(str::to_owned))
            });
        let Some(token) = token else { return Viewer::default() };
        let Ok(core) = self.state.core().await else { return Viewer::default() };
        let session = match aether_orm::find_session_by_token(&core, &token).await {
            Ok(Some(session)) if session.user.is_active => session,
            _ => return Viewer::default(),
        };
        let mut viewer = Viewer { signed_in: true, developer: session.user.is_super_user, org_plugins: HashSet::new() };
        if need_orgs && !viewer.developer {
            match crate::access::organizations::user_organizations(&self.state, &session).await {
                Ok(orgs) => {
                    for org in orgs {
                        let Ok(db) = self.state.org(&org.db_name).await else { continue };
                        if let Ok(installed) = crate::plugin_manager::installed::list_cached(&self.state.cache, &org.db_name, &db).await {
                            viewer.org_plugins.extend(installed.into_iter().filter(|p| p.is_enabled).map(|p| p.plugin_name));
                        }
                    }
                }
                Err(e) => log::warn!("plugin documentation: cannot list the viewer's organizations: {e}"),
            }
        }
        viewer
    }
}

#[async_trait]
impl PluginDocsProvider for CatalogDocs {
    async fn visible(&self, headers: &HeaderMap) -> Vec<Arc<PluginDocs>> {
        let all = self.catalog().await;
        if all.is_empty() {
            return all;
        }
        let need_orgs = all.iter().any(|d| d.visibility == Visibility::Org);
        let viewer = self.viewer(headers, need_orgs).await;
        visible_to(&all, &viewer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_docs::plugin_docs::{SourceFile, build};

    fn docs(name: &str, visibility: Visibility) -> Arc<PluginDocs> {
        Arc::new(build(name, "1.0.0", name, visibility, &[SourceFile { path: "README.md".into(), text: "# Hi".into() }]).unwrap())
    }

    fn names(list: Vec<Arc<PluginDocs>>) -> Vec<String> {
        list.into_iter().map(|d| d.plugin.clone()).collect()
    }

    fn all() -> Vec<Arc<PluginDocs>> {
        vec![docs("open", Visibility::Public), docs("members", Visibility::Authenticated), docs("payroll", Visibility::Org), docs("hr", Visibility::Org)]
    }

    #[test]
    fn anonymous_visitors_see_only_public_docs() {
        assert_eq!(names(visible_to(&all(), &Viewer::default())), ["open"]);
    }

    #[test]
    fn signed_in_users_see_authenticated_docs_but_org_docs_need_the_plugin_installed() {
        let viewer = Viewer { signed_in: true, ..Default::default() };
        assert_eq!(names(visible_to(&all(), &viewer)), ["open", "members"]);
        let member = Viewer { signed_in: true, org_plugins: HashSet::from(["payroll".to_string()]), ..Default::default() };
        assert_eq!(names(visible_to(&all(), &member)), ["open", "members", "payroll"]);
    }

    #[test]
    fn org_membership_does_not_help_an_anonymous_visitor() {
        // Defensive: a viewer who is not signed in has no organizations, but even if the set were filled in...
        let odd = Viewer { signed_in: false, developer: false, org_plugins: HashSet::from(["payroll".to_string()]) };
        assert_eq!(names(visible_to(&all(), &odd)), ["open"]);
    }

    #[test]
    fn developers_see_everything() {
        let dev = Viewer { signed_in: true, developer: true, ..Default::default() };
        assert_eq!(visible_to(&all(), &dev).len(), 4);
    }
}
