//! What a plugin may touch on behalf of different callers.
//!
//! * A logged-in user gets the plugin's `access_models` and `capabilities`.
//! * An anonymous visitor gets far less, and only what the plugin declares:
//!   read access to the models its public pages show, plus whatever
//!   `public_access_models` and `public_capabilities` list.

use std::collections::{BTreeSet, HashMap, HashSet};

use super::catalog::model_name;
use super::models::plugin_def::PluginManifest;
use crate::kernel::ModelGrant;

/// Physical table for a logical model name, from `[[models]]` (default: the name).
fn table_for<'a>(manifest: &'a PluginManifest, name: &'a str) -> &'a str {
    manifest
        .models
        .iter()
        .find(|model| model.name == name)
        .and_then(|model| model.table.as_deref())
        .unwrap_or(name)
}

/// Grants for a logged-in user: the plugin's `access_models`.
pub fn user_grants(manifest: &PluginManifest) -> HashMap<String, ModelGrant> {
    manifest
        .plugin
        .access_models
        .iter()
        .map(|access| {
            let table = table_for(manifest, &access.name);
            (
                access.name.clone(),
                ModelGrant::from_access(&access.name, &access.permissions, Some(table)),
            )
        })
        .collect()
}

/// Grants for an anonymous visitor.
///
/// `public_page_models` are the models named by the plugin's public pages;
/// each is readable and nothing more. `public_access_models` can add models
/// and, explicitly, write permission.
pub fn anonymous_grants(
    manifest: &PluginManifest,
    public_page_models: &BTreeSet<String>,
) -> HashMap<String, ModelGrant> {
    let mut grants: HashMap<String, ModelGrant> = HashMap::new();

    for reference in public_page_models {
        let name = model_name(reference);
        grants.insert(
            name.to_string(),
            ModelGrant {
                name: name.to_string(),
                table: table_for(manifest, name).to_string(),
                can_read: true,
                can_write: false,
            },
        );
    }
    for access in &manifest.plugin.public_access_models {
        let declared = ModelGrant::from_access(
            &access.name,
            &access.permissions,
            Some(table_for(manifest, &access.name)),
        );
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

[[models]]
name = "message"
table = "chat_message"

[[models]]
name = "guestbook"
table = "chat_guestbook"
"#,
        )
    }

    #[test]
    fn users_get_their_access_models_with_table_mapping() -> Result<(), Box<dyn std::error::Error>> {
        let grants = user_grants(&manifest()?);
        assert_eq!(grants["message"].table, "chat_message");
        assert!(grants["message"].can_write);
        assert_eq!(grants["channel"].table, "channel", "unmapped models use their name");
        Ok(())
    }

    #[test]
    fn public_page_models_are_read_only_by_default() -> Result<(), Box<dyn std::error::Error>> {
        let pages = BTreeSet::from(["chat.message".to_string()]);
        let grants = anonymous_grants(&manifest()?, &pages);
        let message = &grants["message"];
        assert!(message.can_read && !message.can_write);
        assert_eq!(message.table, "chat_message");
        assert!(!grants.contains_key("secret"), "undeclared models stay private");
        assert!(!grants.contains_key("channel"));
        Ok(())
    }

    #[test]
    fn public_access_models_can_grant_write_explicitly() -> Result<(), Box<dyn std::error::Error>> {
        let grants = anonymous_grants(&manifest()?, &BTreeSet::new());
        let guestbook = &grants["guestbook"];
        assert!(guestbook.can_write && !guestbook.can_read);
        assert_eq!(guestbook.table, "chat_guestbook");
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
        let grants = anonymous_grants(&manifest, &BTreeSet::from(["post".to_string()]));
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
