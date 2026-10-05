//! What a plugin may touch on behalf of different callers.
//!
//! * A logged-in user gets the plugin's `access_models` and `capabilities`.
//! * An anonymous visitor gets far less, and only what the plugin declares:
//!   read access to the models its public pages show, plus whatever
//!   `public_access_models` and `public_capabilities` list.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::Arc;

use super::catalog::model_name;
use super::models::plugin_def::PluginManifest;
use crate::data_model::ModelSchema;
use crate::kernel::ModelGrant;

/// The grant for model `name`: the table and field schema from its definition when the plugin
/// has one.
fn grant_for(
    name: &str,
    permissions: &[String],
    schemas: &HashMap<String, Arc<ModelSchema>>,
) -> ModelGrant {
    let schema = schemas.get(name);
    let mut grant = ModelGrant::from_access(name, permissions, schema.map(|s| s.table.as_str()));
    grant.schema = schema.cloned();
    grant
}

/// Grants for a logged-in user: the plugin's `access_models`.
pub fn user_grants(
    manifest: &PluginManifest,
    schemas: &HashMap<String, Arc<ModelSchema>>,
) -> HashMap<String, ModelGrant> {
    manifest
        .plugin
        .access_models
        .iter()
        .map(|access| (access.name.clone(), grant_for(&access.name, &access.permissions, schemas)))
        .collect()
}

/// Grants for an anonymous visitor.
///
/// `public_page_models` are the models named by the plugin's public pages;
/// each is readable and nothing more. `public_access_models` can add models
/// and, explicitly, write permission.
pub fn anonymous_grants(
    manifest: &PluginManifest,
    schemas: &HashMap<String, Arc<ModelSchema>>,
    public_page_models: &BTreeSet<String>,
) -> HashMap<String, ModelGrant> {
    let mut grants: HashMap<String, ModelGrant> = HashMap::new();

    for reference in public_page_models {
        let name = model_name(reference);
        let mut grant = grant_for(name, &[], schemas);
        grant.can_read = true;
        grant.can_write = false;
        grants.insert(name.to_string(), grant);
    }
    for access in &manifest.plugin.public_access_models {
        let declared = grant_for(&access.name, &access.permissions, schemas);
        grants
            .entry(access.name.clone())
            .and_modify(|existing| {
                existing.can_read |= declared.can_read;
                existing.can_write |= declared.can_write;
            })
            .or_insert(declared);
    }
    grants
}

/// Capabilities for an anonymous visitor: those the plugin lists in
/// `public_capabilities` (and also holds), plus `db::query` when it has any
/// public data to read.
pub fn anonymous_capabilities(manifest: &PluginManifest, has_readable_models: bool) -> HashSet<String> {
    let plugin = &manifest.plugin;
    let mut granted: HashSet<String> = plugin
        .public_capabilities
        .iter()
        .filter(|capability| plugin.capabilities.contains(capability))
        .cloned()
        .collect();
    if has_readable_models && plugin.capabilities.iter().any(|c| c == "db::query") {
        granted.insert("db::query".to_string());
    }
    granted
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest() -> Result<PluginManifest, super::super::models::plugin_def::ManifestError> {
        PluginManifest::parse(
            r#"
[plugin]
name = "chat"
capabilities = ["db::query", "db::mutate"]
access_models = [
  { name = "message", permissions = ["read", "write"] },
  { name = "channel", permissions = ["read", "write"] },
  { name = "secret", permissions = ["read"] },
]
public_capabilities = ["db::mutate", "events::emit"]
public_access_models = [{ name = "guestbook", permissions = ["write"] }]
"#,
        )
    }

    /// The plugin's models, as `models/*.json` would define them (ids assigned).
    fn schemas() -> HashMap<String, Arc<ModelSchema>> {
        let models: Vec<crate::data_model::ModelDef> = ["message", "guestbook"]
            .iter()
            .map(|name| {
                let mut model: crate::data_model::ModelDef = serde_json::from_value(serde_json::json!({
                    "name": name, "fields": [{ "name": "body", "type": "text" }]
                }))
                .unwrap_or_else(|error| panic!("{error}"));
                crate::data_model::sync_ids(&mut model);
                model
            })
            .collect();
        crate::data_model::schemas_of(&models)
    }

    #[test]
    fn users_get_their_access_models_with_the_table_of_their_definition() -> Result<(), Box<dyn std::error::Error>> {
        let schemas = schemas();
        let grants = user_grants(&manifest()?, &schemas);
        assert_eq!(grants["message"].table, schemas["message"].table, "records live in the model's id");
        assert!(grants["message"].schema.is_some());
        assert!(grants["message"].can_write);
        assert_eq!(grants["channel"].table, "channel", "a model without a definition keeps its name");
        assert!(grants["channel"].schema.is_none());
        Ok(())
    }

    #[test]
    fn public_page_models_are_read_only_by_default() -> Result<(), Box<dyn std::error::Error>> {
        let pages = BTreeSet::from(["chat.message".to_string()]);
        let schemas = schemas();
        let grants = anonymous_grants(&manifest()?, &schemas, &pages);
        let message = &grants["message"];
        assert!(message.can_read && !message.can_write);
        assert_eq!(message.table, schemas["message"].table);
        assert!(!grants.contains_key("secret"), "undeclared models stay private");
        assert!(!grants.contains_key("channel"));
        Ok(())
    }

    #[test]
    fn public_access_models_can_grant_write_explicitly() -> Result<(), Box<dyn std::error::Error>> {
        let schemas = schemas();
        let grants = anonymous_grants(&manifest()?, &schemas, &BTreeSet::new());
        let guestbook = &grants["guestbook"];
        assert!(guestbook.can_write && !guestbook.can_read);
        assert_eq!(guestbook.table, schemas["guestbook"].table);
        Ok(())
    }

    #[test]
    fn a_declared_model_merges_page_read_with_declared_write() -> Result<(), Box<dyn std::error::Error>> {
        let manifest = PluginManifest::parse(
            r#"
[plugin]
name = "x"
public_access_models = [{ name = "post", permissions = ["write"] }]
"#,
        )?;
        let grants = anonymous_grants(&manifest, &HashMap::new(), &BTreeSet::from(["post".to_string()]));
        assert!(grants["post"].can_read && grants["post"].can_write);
        Ok(())
    }

    #[test]
    fn anonymous_capabilities_are_the_declared_subset()
    -> Result<(), Box<dyn std::error::Error>> {
        let caps = anonymous_capabilities(&manifest()?, true);
        assert!(caps.contains("db::mutate"));
        assert!(caps.contains("db::query"), "implied by readable public data");
        assert!(!caps.contains("events::emit"), "the plugin does not hold it");

        assert!(!anonymous_capabilities(&manifest()?, false).contains("db::query"));
        Ok(())
    }

    #[test]
    fn a_manifest_that_still_uses_the_old_model_table_is_refused() {
        let text = "[plugin]\nname = \"old\"\n[[models]]\nname = \"x\"\ntable = \"x_table\"\n";
        assert!(matches!(
            PluginManifest::parse(text),
            Err(super::super::models::plugin_def::ManifestError::LegacyModels)
        ));
    }

    #[test]
    fn a_manifest_that_still_asks_for_raw_surql_is_refused() {
        for field in ["capabilities", "public_capabilities"] {
            let text = format!("[plugin]\nname = \"old\"\n{field} = [\"db::query\", \"db::surql\"]\n");
            assert!(
                matches!(PluginManifest::parse(&text), Err(super::super::models::plugin_def::ManifestError::RawSurql(f)) if f == field),
                "{field}"
            );
        }
    }
}
