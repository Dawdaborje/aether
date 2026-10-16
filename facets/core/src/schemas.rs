//! JSON Schemas of the files a plugin author writes, generated from the very types the kernel parses them
//! with, so they cannot drift from what a load accepts. Doc comments on the types become the descriptions.
//!
//! Only built with the `schemars` feature (`aether --mcp` uses it).

/// The formats with a schema: (kind, the file it describes).
pub const KINDS: &[(&str, &str)] = &[
    ("manifest", "plugin.toml"),
    ("workspace", "workspace.toml"),
    ("model", "models/<name>.json"),
    ("rules", "rules/<model>.json"),
];

/// The schema of one format, or `None` for an unknown kind. A TOML file is described as the JSON the same
/// document would be (tables are objects, `[[x]]` is an array under `x`).
pub fn schema(kind: &str) -> Option<serde_json::Value> {
    let schema = match kind {
        "manifest" => schemars::schema_for!(crate::plugin_manager::models::plugin_def::PluginManifest),
        "workspace" => schemars::schema_for!(crate::plugin_manager::models::plugin_workspace_def::PluginWorkspaceManifest),
        "model" => schemars::schema_for!(crate::data_model::definition::ModelDef),
        "rules" => schemars::schema_for!(crate::data_model::rules::RuleSet),
        _ => return None,
    };
    serde_json::to_value(schema).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_has_a_schema_with_descriptions() {
        for (kind, _) in KINDS {
            let schema = schema(kind).unwrap_or_else(|| panic!("no schema for {kind}"));
            assert_eq!(schema["type"], "object", "{kind}");
            assert!(schema.to_string().contains("\"description\""), "{kind} has no descriptions");
        }
        assert!(schema("nope").is_none());
    }

    #[test]
    fn the_manifest_schema_matches_the_real_fields() {
        let s = schema("manifest").unwrap();
        for key in ["plugin", "schedule", "command", "roles", "ai_tool"] {
            assert!(s["properties"].get(key).is_some(), "manifest schema lacks `{key}`");
        }
        assert!(s["properties"].get("legacy_pages").is_none() && s["properties"].get("pages").is_none());
        assert!(schema("model").unwrap()["properties"].get("name").is_some());
    }
}
