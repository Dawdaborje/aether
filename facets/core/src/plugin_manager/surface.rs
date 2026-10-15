//! Mantle Tier B: per-plugin-version UI surface (pages, components, i18n).

use serde::{Deserialize, Serialize};
use serde_json::Value;
use surrealdb::{Surreal, engine::remote::ws::Client, types::SurrealValue};

use crate::cache::Cache;
use crate::plugin_manager::components::CatalogComponentRow;

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
pub struct WarmPage {
    pub route: String,
    pub title: String,
    pub is_public: bool,
    pub layout: Option<String>,
    #[serde(default)]
    pub wrap: Option<String>,
    #[serde(default)]
    pub required_roles: Option<Vec<String>>,
    pub component_tree: Value,
    #[serde(default)]
    pub is_pattern: bool,
    pub plugin_name: String,
    pub plugin_version: String,
}

const PAGE_SELECT: &str = "route, title, is_public, layout, wrap, required_roles, component_tree, \
     is_pattern, plugin.name AS plugin_name, plugin.version AS plugin_version";

/// All UI pages for one catalogued plugin version.
pub async fn pages_cached(
    cache: &Cache,
    core: &Surreal<Client>,
    name: &str,
    version: &str,
) -> Result<Vec<WarmPage>, surrealdb::Error> {
    if let Ok(Some(json)) = cache.warm_get_plugin_json(name, version, "pages")
        && let Ok(pages) = serde_json::from_value::<Vec<WarmPage>>(json)
    {
        return Ok(pages);
    }
    let query = format!(
        "SELECT {PAGE_SELECT} FROM plugin_ui_pages \
         WHERE plugin.name = $name AND plugin.version = $version;"
    );
    let mut response = core
        .query(query)
        .bind(("name", name.to_string()))
        .bind(("version", version.to_string()))
        .await?
        .check()?;
    let pages: Vec<WarmPage> = response.take(0)?;
    if let Ok(json) = serde_json::to_value(&pages) {
        let _ = cache.warm_set_plugin_json(name, version, "pages", &json);
    }
    Ok(pages)
}

/// Components for one catalogued plugin version.
pub async fn components_cached(
    cache: &Cache,
    core: &Surreal<Client>,
    name: &str,
    version: &str,
) -> Result<Vec<CatalogComponentRow>, surrealdb::Error> {
    if let Ok(Some(json)) = cache.warm_get_plugin_json(name, version, "components")
        && let Ok(rows) = serde_json::from_value::<Vec<CatalogComponentRow>>(json)
    {
        return Ok(rows);
    }
    let mut response = core
        .query(
            "SELECT full_name, component_tree, plugin.name AS plugin_name, plugin.version AS plugin_version \
             FROM plugin_components WHERE plugin.name = $name AND plugin.version = $version;",
        )
        .bind(("name", name.to_string()))
        .bind(("version", version.to_string()))
        .await?
        .check()?;
    let rows: Vec<CatalogComponentRow> = response.take(0)?;
    if let Ok(json) = serde_json::to_value(&rows) {
        let _ = cache.warm_set_plugin_json(name, version, "components", &json);
    }
    Ok(rows)
}

/// `(model name, definition)` for every model of one catalogued plugin version. The
/// definition is the model file as stored (`fields`, …); see [`super::field_hints`].
pub async fn models_cached(
    cache: &Cache,
    core: &Surreal<Client>,
    name: &str,
    version: &str,
) -> Result<Vec<(String, Value)>, surrealdb::Error> {
    if let Ok(Some(json)) = cache.warm_get_plugin_json(name, version, "models")
        && let Ok(rows) = serde_json::from_value::<Vec<(String, Value)>>(json)
    {
        return Ok(rows);
    }
    let mut response = core
        .query(
            "SELECT VALUE [name, definition] FROM plugin_models \
             WHERE plugin.name = $name AND plugin.version = $version;",
        )
        .bind(("name", name.to_string()))
        .bind(("version", version.to_string()))
        .await?
        .check()?;
    let rows: Vec<Vec<Value>> = response.take(0)?;
    let rows: Vec<(String, Value)> = rows
        .into_iter()
        .filter_map(|mut pair| {
            let definition = pair.pop()?;
            let name = pair.pop()?.as_str()?.to_string();
            Some((name, definition))
        })
        .collect();
    if let Ok(json) = serde_json::to_value(&rows) {
        let _ = cache.warm_set_plugin_json(name, version, "models", &json);
    }
    Ok(rows)
}

/// Raw i18n JSON for one catalogued plugin version (`None` when absent).
pub async fn i18n_cached(
    cache: &Cache,
    core: &Surreal<Client>,
    name: &str,
    version: &str,
) -> Result<Option<Value>, surrealdb::Error> {
    if let Ok(Some(json)) = cache.warm_get_plugin_json(name, version, "i18n") {
        // Sentinel null means "looked up, no i18n".
        if json.is_null() {
            return Ok(None);
        }
        return Ok(Some(json));
    }
    let mut response = core
        .query(
            "SELECT VALUE i18n FROM plugins \
             WHERE name = $name AND version = $version AND i18n != NONE LIMIT 1;",
        )
        .bind(("name", name.to_string()))
        .bind(("version", version.to_string()))
        .await?
        .check()?;
    let rows: Vec<Value> = response.take(0)?;
    let i18n = rows.into_iter().next();
    let store = i18n.clone().unwrap_or(Value::Null);
    let _ = cache.warm_set_plugin_json(name, version, "i18n", &store);
    Ok(i18n)
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PublicSurface {
    pub public_functions: Vec<String>,
    pub public_models: Vec<String>,
}

/// Anon-invoke surface for one catalogued plugin version.
pub async fn public_cached(
    cache: &Cache,
    core: &Surreal<Client>,
    name: &str,
    version: &str,
) -> Result<PublicSurface, surrealdb::Error> {
    if let Ok(Some(json)) = cache.warm_get_plugin_json(name, version, "public")
        && let Ok(surface) = serde_json::from_value::<PublicSurface>(json)
    {
        return Ok(surface);
    }
    let mut response = core
        .query(
            "SELECT public_functions FROM plugins WHERE name = $name AND version = $version LIMIT 1; \
             SELECT models FROM plugin_ui_pages \
             WHERE is_public = true AND plugin.name = $name AND plugin.version = $version;",
        )
        .bind(("name", name.to_string()))
        .bind(("version", version.to_string()))
        .await?
        .check()?;
    #[derive(Deserialize, SurrealValue)]
    struct FnRow {
        public_functions: Vec<String>,
    }
    #[derive(Deserialize, SurrealValue)]
    struct ModelRow {
        models: Vec<String>,
    }
    let fns: Vec<FnRow> = response.take(0)?;
    let pages: Vec<ModelRow> = response.take(1)?;
    let surface = PublicSurface {
        public_functions: fns
            .into_iter()
            .next()
            .map(|row| row.public_functions)
            .unwrap_or_default(),
        public_models: pages.into_iter().flat_map(|page| page.models).collect(),
    };
    if let Ok(json) = serde_json::to_value(&surface) {
        let _ = cache.warm_set_plugin_json(name, version, "public", &json);
    }
    Ok(surface)
}

pub fn invalidate(cache: &Cache, name: &str, version: &str) {
    let _ = cache.warm_invalidate_plugin_surface(name, version);
}
