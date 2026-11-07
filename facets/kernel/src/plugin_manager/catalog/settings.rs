//! Part of the plugin catalog; see [`super`]. Adds the settings a plugin declares (`[[setting_group]]`
//! and `[[setting]]`) to the settings catalog, so they show in Settings and `settings::get` can read them.

use super::*;

/// Create or refresh the plugin's groups and settings. A setting that already exists keeps its value
/// (an administrator may have changed the global default); only its text is refreshed. Safe to run on
/// every load.
pub(super) async fn seed_plugin_settings(
    db: &Surreal<Client>,
    manifest: &PluginManifest,
) -> Result<(), CatalogError> {
    let plugin = manifest.plugin.name.as_str();
    for group in &manifest.setting_group {
        db.query(
            r#"
            IF array::len((SELECT VALUE id FROM type::record("gl_settings_groups", $id))) = 0 {
                CREATE type::record("gl_settings_groups", $id) CONTENT {
                    label: $label, color: $color, icon: $icon, icon_type: "lucide",
                    date_created: time::now(), date_updated: time::now()
                };
            } ELSE {
                UPDATE type::record("gl_settings_groups", $id) MERGE {
                    label: $label, color: $color, icon: $icon, date_updated: time::now()
                };
            };
            "#,
        )
        .bind(("id", group.id.clone()))
        .bind(("label", group.label.clone()))
        .bind(("color", group.color.clone()))
        .bind(("icon", group.icon.clone()))
        .await?
        .check()?;
    }
    for setting in &manifest.setting {
        let mut found = db
            .query(r#"SELECT VALUE id FROM type::record("gl_settings_groups", $id);"#)
            .bind(("id", setting.group.clone()))
            .await?
            .check()?;
        let exists: Vec<surrealdb::types::RecordId> = found.take(0)?;
        if exists.is_empty() {
            return Err(CatalogError::Settings(format!(
                "`{plugin}` puts `{}` in the group `{}`, which no plugin or kernel seed declares",
                setting.key, setting.group
            )));
        }
        let value = if setting.value.is_null() {
            serde_json::Value::String(String::new())
        } else {
            setting.value.clone()
        };
        db.query(
            r#"
            IF array::len((SELECT VALUE id FROM type::record("gl_settings_items", $id))) = 0 {
                CREATE type::record("gl_settings_items", $id) CONTENT {
                    label: $label, s_key: $key, s_value: $value, is_secret: false,
                    description: $description, long_description: $long_description,
                    date_created: time::now(), date_updated: time::now()
                };
            } ELSE {
                UPDATE type::record("gl_settings_items", $id) MERGE {
                    label: $label, description: $description, long_description: $long_description,
                    date_updated: time::now()
                };
            };
            UPSERT type::record("gl_settings_group_items", $relation) CONTENT {
                group: type::record("gl_settings_groups", $group),
                item: type::record("gl_settings_items", $id),
                date_created: time::now(), date_updated: time::now()
            };
            "#,
        )
        .bind(("id", setting.row_id()))
        .bind(("relation", format!("{}_{}", setting.group, setting.row_id())))
        .bind(("group", setting.group.clone()))
        .bind(("key", setting.key.clone()))
        .bind(("label", setting.label.clone()))
        .bind(("value", value))
        .bind(("description", setting.description.clone()))
        .bind(("long_description", setting.long_description.clone()))
        .await?
        .check()?;
    }
    Ok(())
}
