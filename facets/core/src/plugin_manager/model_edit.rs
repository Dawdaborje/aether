//! Publishing an edited model as a new version of its plugin.
//!
//! A plugin version is immutable, so editing a model (in the web app) makes a new version:
//! the same plugin with the model file replaced. Nothing else is copied: the new revision
//! (see [`super::revisions`]) stores only `models/<name>.json`, and every other file is shared
//! with the version it was made from. Organizations keep the version they have until they are
//! upgraded to the new one.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use surrealdb::{Surreal, engine::remote::ws::Client, types::SurrealValue};
use thiserror::Error;

use super::catalog::{base_version, sha256_hex};
use super::revisions;
use crate::app_dir::AppDir;
use crate::data_model::{ModelDef, ModelFileError, validate_set};

#[derive(Debug, Error)]
pub enum EditError {
    #[error("database error: {0}")]
    Database(#[from] surrealdb::Error),
    #[error("plugin `{0}` is not in the catalog")]
    PluginNotFound(String),
    #[error("plugin `{plugin}` has no version `{version}`")]
    VersionNotFound { plugin: String, version: String },
    #[error("the files of `{0}` are not on disk any more, so it cannot be edited")]
    FilesMissing(String),
    #[error("{0}")]
    Invalid(String),
    #[error(transparent)]
    Revision(#[from] revisions::RevisionError),
    #[error(transparent)]
    Models(#[from] ModelFileError),
    #[error("file error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// What was made.
#[derive(Debug, Clone, PartialEq)]
pub struct Published {
    /// The new catalog version (`0.1.0+<stamp>`).
    pub version: String,
    /// The folder under `plugins/<name>/` that holds what changed.
    pub revision: String,
    /// The files written for it.
    pub written: Vec<String>,
    /// The version it was made from.
    pub from: String,
    /// Models of the plugin as the new version has them.
    pub models: Vec<ModelDef>,
}

#[derive(Debug, Deserialize, SurrealValue)]
struct BaseVersion {
    version: String,
    revision: Option<String>,
    artifact_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
struct PageCopy {
    layout: Option<String>,
    route: String,
    is_pattern: bool,
    route_shape: String,
    title: String,
    model: Option<String>,
    models: Vec<String>,
    is_public: bool,
    component_tree: Value,
    source_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
struct ThemeCopy {
    name: String,
    label: String,
    is_system: bool,
    color_mode: String,
    tokens: Value,
    layout: String,
    error_pages: String,
    nav: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
struct ModelCopy {
    name: String,
    model_id: String,
    label: Option<String>,
    definition: Value,
}

/// The models of a catalog version, from the catalog. Caller must be on the core database.
pub async fn models_of_version(
    db: &Surreal<Client>,
    plugin: &str,
    version: &str,
) -> Result<Vec<ModelDef>, EditError> {
    let mut response = db
        .query(
            "SELECT VALUE definition FROM plugin_models \
             WHERE plugin.name = $name AND plugin.version = $version ORDER BY name;",
        )
        .bind(("name", plugin.to_string()))
        .bind(("version", version.to_string()))
        .await?
        .check()?;
    let definitions: Vec<Value> = response.take(0)?;
    definitions
        .into_iter()
        .map(|definition| {
            serde_json::from_value(definition).map_err(|error| EditError::Invalid(format!("a stored model is unreadable: {error}")))
        })
        .collect()
}

/// Merge `changed` into the plugin's models: a model with the same id (or, for a new model,
/// the same name) is replaced; anything else is added.
pub fn merge_models(existing: &[ModelDef], changed: &[ModelDef]) -> Result<Vec<ModelDef>, EditError> {
    let mut merged = existing.to_vec();
    for model in changed {
        let by_id = model
            .model_id
            .as_ref()
            .and_then(|id| merged.iter().position(|other| other.model_id.as_ref() == Some(id)));
        match by_id {
            Some(index) => {
                // A model's name is used by the plugin's code, pages and grants: not editable here.
                if merged[index].name != model.name {
                    return Err(EditError::Invalid(format!(
                        "the model `{}` cannot be renamed here: its name is used by the plugin's code, pages and `access_models`; change its label instead",
                        merged[index].name
                    )));
                }
                merged[index] = model.clone();
            }
            None => match merged.iter().position(|other| other.name == model.name) {
                Some(_) => {
                    return Err(EditError::Invalid(format!(
                        "a model called `{}` already exists with another id",
                        model.name
                    )));
                }
                None => merged.push(model.clone()),
            },
        }
    }
    merged.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(merged)
}

/// Check every model on its own and together.
pub fn validate_all(models: &[ModelDef]) -> Result<(), EditError> {
    let problems: Vec<String> = models.iter().flat_map(ModelDef::problems).collect();
    if !problems.is_empty() {
        return Err(EditError::Invalid(problems.join("; ")));
    }
    validate_set(models)?;
    Ok(())
}

/// The content hash `load_plugin` computes for a set of files, so loading the same files from
/// disk later is recognised as the same content.
fn content_hash(files: &BTreeMap<String, Vec<u8>>, artifact: Option<&str>) -> String {
    let mut hash = Sha256::new();
    for (path, bytes) in files {
        if Some(path.as_str()) == artifact {
            continue;
        }
        hash.update(path.as_bytes());
        hash.update((bytes.len() as u64).to_le_bytes());
        hash.update(bytes);
    }
    if let Some(name) = artifact
        && let Some(bytes) = files.get(name)
    {
        hash.update(name.as_bytes());
        hash.update(bytes);
    }
    hash.finalize().iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Make a new version of `plugin` whose models are `changed` merged into those of `base_version`
/// (the latest when none is named). Everything else is carried over from the base: the files are
/// shared, and its pages, theme and catalog entry are copied.
pub async fn publish_models(
    db: &Surreal<Client>,
    layout: &AppDir,
    plugin: &str,
    base_version: Option<&str>,
    changed: &[ModelDef],
) -> Result<Published, EditError> {
    let query = match base_version {
        Some(_) => "SELECT version, revision, artifact_path, date_created FROM plugins WHERE name = $name AND version = $version AND is_active = true;",
        None => "SELECT version, revision, artifact_path, date_created FROM plugins WHERE name = $name AND is_active = true ORDER BY date_created DESC LIMIT 1;",
    };
    let mut request = db.query(query).bind(("name", plugin.to_string()));
    if let Some(version) = base_version {
        request = request.bind(("version", version.to_string()));
    }
    let mut response = request.await?.check()?;
    let bases: Vec<BaseVersion> = response.take(0)?;
    let base = bases.into_iter().next().ok_or_else(|| match base_version {
        Some(version) => EditError::VersionNotFound { plugin: plugin.to_string(), version: version.to_string() },
        None => EditError::PluginNotFound(plugin.to_string()),
    })?;

    // The base version's files, wherever their bytes are.
    let folder = base.revision.clone().unwrap_or_else(|| base.version.clone());
    let index = revisions::read_index(layout, plugin, &folder)
        .await?
        .ok_or_else(|| EditError::FilesMissing(plugin.to_string()))?;
    let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for (logical, entry) in &index.files {
        let path = layout.revision_dir(plugin, &entry.revision).map_err(revisions::RevisionError::from)?.join(logical);
        let bytes = tokio::fs::read(&path).await.map_err(|source| EditError::Io { path, source })?;
        files.insert(logical.clone(), bytes);
    }

    // The new set of models replaces the model files; nothing else changes.
    let existing = models_of_version(db, plugin, &base.version).await?;
    let merged = merge_models(&existing, changed)?;
    validate_all(&merged)?;
    files.retain(|logical, _| !(logical.starts_with("models/") && logical.ends_with(".json")));
    for model in &merged {
        let mut text = serde_json::to_string_pretty(model)
            .map_err(|error| EditError::Invalid(format!("model `{}` cannot be written: {error}", model.name)))?;
        text.push('\n');
        files.insert(format!("models/{}.json", model.name), text.into_bytes());
    }

    let list: Vec<(String, Vec<u8>)> = files.iter().map(|(path, bytes)| (path.clone(), bytes.clone())).collect();
    let stored = revisions::store(layout, plugin, base_version_of(&base.version), Some((folder.as_str(), &index)), &list).await?;
    let artifact_name = base
        .artifact_path
        .as_deref()
        .and_then(|path| path.rsplit('/').next())
        .map(str::to_string);
    let hash = content_hash(&files, artifact_name.as_deref());
    let version = format!("{}+{}", base_version_of(&base.version), stored.revision);

    // The base's catalog entry, pages and theme, carried over to the new version.
    let mut response = db
        .query("SELECT * OMIT id, date_created, date_updated FROM plugins WHERE name = $name AND version = $version;")
        .bind(("name", plugin.to_string()))
        .bind(("version", base.version.clone()))
        .await?
        .check()?;
    let rows: Vec<Value> = response.take(0)?;
    let Some(Value::Object(mut record)) = rows.into_iter().next() else {
        return Err(EditError::PluginNotFound(plugin.to_string()));
    };
    record.retain(|_, value| !value.is_null());
    record.insert("version".into(), Value::String(version.clone()));
    record.insert("revision".into(), Value::String(stored.revision.clone()));
    record.insert("content_hash".into(), Value::String(hash));

    let mut response = db
        .query(
            "SELECT layout, route, is_pattern, route_shape, title, model, models, is_public, component_tree, source_path \
             FROM plugin_ui_pages WHERE plugin.name = $name AND plugin.version = $version; \
             SELECT name, label, is_system, color_mode, tokens, layout, error_pages, nav \
             FROM plugin_themes WHERE plugin.name = $name AND plugin.version = $version;",
        )
        .bind(("name", plugin.to_string()))
        .bind(("version", base.version.clone()))
        .await?
        .check()?;
    let pages: Vec<PageCopy> = response.take(0)?;
    let themes: Vec<ThemeCopy> = response.take(1)?;
    let model_rows: Vec<ModelCopy> = merged
        .iter()
        .map(|model| ModelCopy {
            name: model.name.clone(),
            model_id: model.model_id.clone().unwrap_or_default(),
            label: model.label.clone(),
            definition: serde_json::to_value(model).unwrap_or(Value::Null),
        })
        .collect();

    db.query(
        r#"
        BEGIN TRANSACTION;
        LET $plugin = (CREATE ONLY plugins CONTENT $record).id;
        FOR $t IN $themes {
            CREATE plugin_themes CONTENT {
                plugin: $plugin, name: $t.name, label: $t.label, is_system: $t.is_system,
                color_mode: $t.color_mode, tokens: $t.tokens, layout: $t.layout,
                error_pages: $t.error_pages, nav: $t.nav
            };
        };
        FOR $m IN $models {
            CREATE plugin_models CONTENT {
                plugin: $plugin, name: $m.name, table_name: $m.model_id,
                model_id: $m.model_id, label: $m.label, definition: $m.definition
            };
        };
        FOR $page IN $pages {
            CREATE plugin_ui_pages CONTENT {
                plugin: $plugin, route: $page.route, layout: $page.layout,
                is_pattern: $page.is_pattern, route_shape: $page.route_shape,
                title: $page.title, model: $page.model, models: $page.models,
                is_public: $page.is_public, component_tree: $page.component_tree,
                source_path: $page.source_path
            };
        };
        COMMIT TRANSACTION;
        "#,
    )
    .bind(("record", Value::Object(record)))
    .bind(("themes", themes))
    .bind(("models", model_rows))
    .bind(("pages", pages))
    .await?
    .check()?;

    Ok(Published {
        version,
        revision: stored.revision,
        written: stored.written,
        from: base.version,
        models: merged,
    })
}

fn base_version_of(version: &str) -> &str {
    base_version(version)
}

/// The names of the model files in a set of files, for tests and tooling.
pub fn model_files(files: &BTreeMap<String, Vec<u8>>) -> BTreeSet<String> {
    files
        .keys()
        .filter(|path| path.starts_with("models/") && path.ends_with(".json"))
        .cloned()
        .collect()
}

/// `sha256_hex` of a model's file text, to compare with what is stored.
pub fn model_file_hash(model: &ModelDef) -> Option<String> {
    let mut text = serde_json::to_string_pretty(model).ok()?;
    text.push('\n');
    Some(sha256_hex(text.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data_model::sync_ids;
    use serde_json::json;

    fn model(name: &str) -> ModelDef {
        let mut model: ModelDef = serde_json::from_value(json!({
            "name": name, "fields": [{ "name": "title", "type": "string" }]
        }))
        .unwrap_or_else(|error| panic!("{error}"));
        sync_ids(&mut model);
        model
    }

    #[test]
    fn a_changed_model_replaces_the_one_with_its_id_and_new_ones_are_added() {
        let note = model("note");
        let mut edited = note.clone();
        edited.label = Some("Note!".into());
        let task = model("task");
        let merged = merge_models(std::slice::from_ref(&note), &[edited.clone(), task.clone()]).unwrap();
        // Sorted by name: the edited note replaced the old one, and the task was added.
        assert_eq!(merged, vec![edited, task]);
    }

    #[test]
    fn a_model_cannot_be_renamed_and_a_name_cannot_be_taken_twice() {
        let note = model("note");
        let mut renamed = note.clone();
        renamed.name = "memo".into();
        assert!(merge_models(std::slice::from_ref(&note), &[renamed]).is_err());
        let other_note = model("note");
        assert!(merge_models(std::slice::from_ref(&note), &[other_note]).is_err());
    }

    #[test]
    fn the_content_hash_matches_what_loading_computes() {
        let files = BTreeMap::from([
            ("pages/a.xml".to_string(), b"page".to_vec()),
            ("plugin.toml".to_string(), b"toml".to_vec()),
            ("plugin.wasm".to_string(), b"wasm".to_vec()),
        ]);
        // Files in path order with their length, then the artifact by name and bytes.
        let mut expected = Sha256::new();
        for (path, bytes) in [("pages/a.xml", &b"page"[..]), ("plugin.toml", &b"toml"[..])] {
            expected.update(path.as_bytes());
            expected.update((bytes.len() as u64).to_le_bytes());
            expected.update(bytes);
        }
        expected.update(b"plugin.wasm");
        expected.update(b"wasm");
        let expected: String = expected.finalize().iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(content_hash(&files, Some("plugin.wasm")), expected);
    }
}
