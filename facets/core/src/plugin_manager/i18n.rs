//! A plugin's translated text.
//!
//! A plugin ships `i18n/<locale>.json` files (flat `key: text` objects) and may name its base
//! language with `[plugin.i18n] default = "en"`. They are read when the plugin is loaded,
//! stored on the plugin's catalog row, and put together into a [`Translator`] for an
//! organization when one of its pages is served. See `docs/architecture/localization.md`.

use std::path::{Path, PathBuf};

use aether_localization::{Catalogs, Translator};
use serde_json::{Map, Value, json};
use surrealdb::{Surreal, engine::remote::ws::Client};
use surrealdb::types::SurrealValue;

use super::models::plugin_def::PluginManifest;

/// The folder of catalogs in a plugin package.
pub const I18N_DIR: &str = "i18n";

/// The capability that lets a plugin replace another plugin's text.
pub const OVERRIDE_CAPABILITY: &str = "i18n::override";

/// A plugin's catalogs and the package-relative files they came from.
#[derive(Debug)]
pub struct PluginCatalogs {
    pub catalogs: Catalogs,
    pub files: Vec<PathBuf>,
}

/// Read `<package>/i18n/*.json`. `None` when the package has no such folder or it holds no
/// catalogs. A file that is not a locale name or not a flat object of text is an error.
pub async fn read_catalogs(package_dir: &Path, manifest: &PluginManifest) -> Result<Option<PluginCatalogs>, String> {
    let dir = package_dir.join(I18N_DIR);
    let mut entries = match tokio::fs::read_dir(&dir).await {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("cannot read {}: {error}", dir.display())),
    };
    let mut texts: Vec<(String, String)> = Vec::new();
    let mut files = Vec::new();
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|error| format!("cannot read {}: {error}", dir.display()))?
    {
        let path = entry.path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
            continue;
        }
        let Some(locale) = path.file_stem().and_then(|stem| stem.to_str()).map(str::to_string) else {
            continue;
        };
        let text = tokio::fs::read_to_string(&path)
            .await
            .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
        files.push(Path::new(I18N_DIR).join(format!("{locale}.json")));
        texts.push((locale, text));
    }
    if texts.is_empty() {
        return Ok(None);
    }
    files.sort();
    let default = manifest.plugin.i18n.as_ref().map_or("en", |i18n| i18n.default.as_str());
    let catalogs = Catalogs::from_files(default, texts.iter().map(|(locale, text)| (locale.as_str(), text.as_str())))
        .map_err(|error| error.to_string())?;
    Ok(Some(PluginCatalogs { catalogs, files }))
}

/// Overrides need the capability and a dependency on the plugin they change; a catalog that
/// writes `<plugin>:<key>` without both is refused, not silently ignored.
pub fn check_overrides(manifest: &PluginManifest, catalogs: &Catalogs) -> Result<(), String> {
    let plugin = &manifest.plugin;
    let mut targets: Vec<&str> = catalogs
        .locales
        .values()
        .flat_map(|messages| messages.keys())
        .filter_map(|key| key.split_once(':').map(|(target, _)| target))
        .collect();
    targets.sort_unstable();
    targets.dedup();
    for target in targets {
        if !plugin.capabilities.iter().any(|capability| capability == OVERRIDE_CAPABILITY) {
            return Err(format!(
                "the i18n files override text of `{target}`, which needs the capability `{OVERRIDE_CAPABILITY}`"
            ));
        }
        if !plugin.dependencies.iter().any(|dependency| dependency == target) {
            return Err(format!(
                "the i18n files override text of `{target}`, which must be listed under `dependencies`"
            ));
        }
        if target == plugin.name {
            return Err("a plugin does not override itself; write its keys without a prefix".into());
        }
    }
    Ok(())
}

/// The form stored in the catalog row.
pub fn to_value(catalogs: &Catalogs) -> Value {
    let mut locales = Map::new();
    for (locale, messages) in &catalogs.locales {
        let messages: Map<String, Value> =
            messages.iter().map(|(key, text)| (key.clone(), Value::String(text.clone()))).collect();
        locales.insert(locale.clone(), Value::Object(messages));
    }
    json!({ "default": catalogs.default, "locales": locales })
}

/// The catalogs of a stored value; `None` when it is not one.
pub fn from_value(value: &Value) -> Option<Catalogs> {
    let default = value.get("default")?.as_str()?.to_string();
    let mut catalogs = Catalogs { default, locales: Default::default() };
    for (locale, messages) in value.get("locales")?.as_object()? {
        let messages = messages
            .as_object()?
            .iter()
            .filter_map(|(key, text)| Some((key.clone(), text.as_str()?.to_string())))
            .collect();
        catalogs.locales.insert(locale.clone(), messages);
    }
    Some(catalogs)
}

#[derive(Debug, serde::Deserialize, SurrealValue)]
struct I18nRow {
    name: String,
    version: String,
    i18n: Option<Value>,
}

/// What `plugins` (name, version) installed in an organization say, put together. `installed`
/// is in install order, so a later plugin's override wins over an earlier one's.
pub async fn translator_for(
    core: &Surreal<Client>,
    installed: &[(String, String)],
) -> Result<Translator, surrealdb::Error> {
    let mut translator = Translator::new();
    if installed.is_empty() {
        return Ok(translator);
    }
    let names: Vec<String> = installed.iter().map(|(name, _)| name.clone()).collect();
    let mut response = core
        .query("SELECT name, version, i18n FROM plugins WHERE name IN $names AND i18n != NONE;")
        .bind(("names", names))
        .await?
        .check()?;
    let rows: Vec<I18nRow> = response.take(0)?;
    let mut all: Vec<(String, Catalogs)> = Vec::new();
    for (name, version) in installed {
        let Some(row) = rows.iter().find(|row| &row.name == name && &row.version == version) else {
            continue;
        };
        if let Some(catalogs) = row.i18n.as_ref().and_then(from_value) {
            all.push((name.clone(), catalogs));
        }
    }
    for (name, catalogs) in &all {
        translator.add_plugin(name, catalogs);
    }
    for (by, catalogs) in &all {
        for (target, _) in &all {
            if target != by {
                translator.add_override(target, catalogs);
            }
        }
    }
    Ok(translator)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(text: &str) -> Result<PluginManifest, Box<dyn std::error::Error>> {
        Ok(PluginManifest::parse(text)?)
    }

    fn catalogs(text: &str) -> Result<Catalogs, Box<dyn std::error::Error>> {
        Ok(Catalogs::from_files("en", [("en", text)])?)
    }

    #[test]
    fn overriding_needs_the_capability_and_a_dependency() -> Result<(), Box<dyn std::error::Error>> {
        let text = catalogs(r#"{"company:title": "Subject"}"#)?;
        let plain = manifest("[plugin]\nname = \"custom\"\nversion = \"1\"\n")?;
        assert!(check_overrides(&plain, &text).is_err());
        let no_dependency =
            manifest("[plugin]\nname = \"custom\"\nversion = \"1\"\ncapabilities = [\"i18n::override\"]\n")?;
        assert!(check_overrides(&no_dependency, &text).is_err());
        let full = manifest(
            "[plugin]\nname = \"custom\"\nversion = \"1\"\ncapabilities = [\"i18n::override\"]\ndependencies = [\"company\"]\n",
        )?;
        assert!(check_overrides(&full, &text).is_ok());
        assert!(check_overrides(&plain, &catalogs(r#"{"title": "Own"}"#)?).is_ok());
        Ok(())
    }

    #[tokio::test]
    async fn catalogs_are_read_from_the_package() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let none = manifest("[plugin]\nname = \"notes\"\nversion = \"1\"\n")?;
        assert!(read_catalogs(dir.path(), &none).await?.is_none(), "no i18n folder");

        tokio::fs::create_dir(dir.path().join("i18n")).await?;
        tokio::fs::write(dir.path().join("i18n/en.json"), r#"{"a": "A"}"#).await?;
        tokio::fs::write(dir.path().join("i18n/fr.json"), r#"{"a": "Un"}"#).await?;
        tokio::fs::write(dir.path().join("i18n/README.md"), "not a catalog").await?;
        let with_default = manifest("[plugin]\nname = \"notes\"\nversion = \"1\"\n[plugin.i18n]\ndefault = \"fr\"\n")?;
        let found = read_catalogs(dir.path(), &with_default).await?.ok_or("catalogs not found")?;
        assert_eq!(found.catalogs.default, "fr");
        assert_eq!(found.catalogs.locales.len(), 2);
        assert_eq!(found.files, [PathBuf::from("i18n/en.json"), PathBuf::from("i18n/fr.json")]);

        tokio::fs::write(dir.path().join("i18n/de.json"), r#"{"a": 1}"#).await?;
        assert!(read_catalogs(dir.path(), &none).await.is_err(), "values must be text");
        Ok(())
    }

    #[test]
    fn catalogs_survive_the_trip_through_the_database_form() -> Result<(), Box<dyn std::error::Error>> {
        let original = catalogs(r#"{"a": "A", "b": "B"}"#)?;
        assert_eq!(from_value(&to_value(&original)), Some(original));
        Ok(())
    }
}
