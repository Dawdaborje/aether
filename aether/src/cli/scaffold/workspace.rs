//! Plugin workspaces (`workspace.toml`).
//!
//! Member plugins are declared in exactly one way, the `[workspace.plugins]`
//! table:
//!
//! ```toml
//! [workspace.plugins]
//! company = { path = "./company" }
//! ```
//!
//! The file is edited with `toml_edit`, so comments and layout survive.

use std::path::{Path, PathBuf};

use toml_edit::{DocumentMut, InlineTable, Item, Table, Value};

use super::ScaffoldError;

const MANIFEST: &str = "workspace.toml";

#[derive(Debug)]
pub struct Workspace {
    manifest_path: PathBuf,
    name: String,
    author: Option<(String, String)>,
    document: DocumentMut,
}

impl Workspace {
    /// Walk up from `start` to the nearest `workspace.toml` that has a
    /// `[workspace]` table. Files without one belong to other tools and are skipped.
    pub async fn find(start: &Path) -> Result<Option<Self>, ScaffoldError> {
        for directory in start.ancestors() {
            let manifest_path = directory.join(MANIFEST);
            if !manifest_path.is_file() {
                continue;
            }
            let text = tokio::fs::read_to_string(&manifest_path)
                .await
                .map_err(|source| ScaffoldError::Io {
                    path: manifest_path.clone(),
                    source,
                })?;
            let document: DocumentMut =
                text.parse()
                    .map_err(|source| ScaffoldError::WorkspaceParse {
                        path: manifest_path.clone(),
                        source,
                    })?;
            if document
                .get("workspace")
                .and_then(Item::as_table_like)
                .is_none()
            {
                continue;
            }
            return Self::from_document(manifest_path, document).map(Some);
        }
        Ok(None)
    }

    fn from_document(manifest_path: PathBuf, document: DocumentMut) -> Result<Self, ScaffoldError> {
        let table = document.get("workspace").and_then(Item::as_table_like);
        let name = table
            .and_then(|table| table.get("name"))
            .and_then(Item::as_str)
            .filter(|name| !name.trim().is_empty())
            .ok_or_else(|| ScaffoldError::WorkspaceInvalid {
                path: manifest_path.clone(),
                reason: "[workspace] needs a non-empty `name`",
            })?
            .to_string();
        let author = table
            .and_then(|table| table.get("authors"))
            .and_then(Item::as_array)
            .and_then(|authors| authors.iter().next())
            .and_then(Value::as_inline_table)
            .and_then(|author| {
                let name = author.get("name")?.as_str()?.to_string();
                let email = author
                    .get("email")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                Some((name, email))
            });
        Ok(Self {
            manifest_path,
            name,
            author,
            document,
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// First `[workspace].authors` entry as `(name, email)`.
    pub fn author(&self) -> Option<&(String, String)> {
        self.author.as_ref()
    }

    pub fn manifest_path(&self) -> &Path {
        &self.manifest_path
    }

    /// Directory containing `workspace.toml`.
    pub fn dir(&self) -> &Path {
        self.manifest_path.parent().unwrap_or(Path::new("."))
    }

    /// `./company`-style path of `plugin_dir`, relative to the workspace.
    pub fn relative_plugin_path(&self, plugin_dir: &Path) -> Result<String, ScaffoldError> {
        let relative = plugin_dir
            .strip_prefix(self.dir())
            .map_err(|_| ScaffoldError::WorkspaceInvalid {
                path: self.manifest_path.clone(),
                reason: "plugin directory is not inside the workspace",
            })?;
        let parts: Vec<_> = relative
            .components()
            .map(|part| part.as_os_str().to_string_lossy())
            .collect();
        Ok(format!("./{}", parts.join("/")))
    }

    /// Add `name = { path = "<path>" }` to `[workspace.plugins]`.
    pub fn register(&mut self, name: &str, path: &str) -> Result<(), ScaffoldError> {
        let invalid = |reason| ScaffoldError::WorkspaceInvalid {
            path: self.manifest_path.clone(),
            reason,
        };

        if self.document.contains_key("addons") {
            return Err(ScaffoldError::LegacyAddons(self.manifest_path.clone()));
        }
        let workspace = self
            .document
            .get_mut("workspace")
            .and_then(Item::as_table_like_mut)
            .ok_or_else(|| invalid("missing [workspace] table"))?;
        if workspace.contains_key("addons") {
            return Err(ScaffoldError::LegacyAddons(self.manifest_path.clone()));
        }

        let plugins = workspace
            .entry("plugins")
            .or_insert_with(|| Item::Table(Table::new()))
            .as_table_like_mut()
            .ok_or_else(|| invalid("`workspace.plugins` must be a table"))?;
        if plugins.contains_key(name) {
            return Err(ScaffoldError::AlreadyRegistered {
                name: name.to_string(),
                workspace: self.name.clone(),
            });
        }

        let mut entry = InlineTable::new();
        entry.insert("path", Value::from(path));
        plugins.insert(name, Item::Value(Value::InlineTable(entry)));
        Ok(())
    }

    /// The manifest text including any registered plugins.
    pub fn render(&self) -> String {
        self.document.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace(text: &str) -> Result<Workspace, Box<dyn std::error::Error>> {
        Ok(Workspace::from_document(
            PathBuf::from("/ws/workspace.toml"),
            text.parse()?,
        )?)
    }

    #[test]
    fn registers_into_a_new_plugins_table_keeping_comments() -> Result<(), Box<dyn std::error::Error>>
    {
        let mut ws = workspace(
            "[workspace]\nname = \"base\"\n# keep me\nauthors = [{ name = \"Ada\", email = \"a@x.io\" }]\n",
        )?;
        ws.register("company", "./company")?;
        let text = ws.render();
        assert!(text.contains("# keep me"));
        assert!(text.contains("[workspace.plugins]"));
        assert!(text.contains("company = { path = \"./company\" }"));
        assert_eq!(ws.author(), Some(&("Ada".to_string(), "a@x.io".to_string())));
        Ok(())
    }

    #[test]
    fn appends_to_an_existing_plugins_table() -> Result<(), Box<dyn std::error::Error>> {
        let mut ws = workspace(
            "[workspace]\nname = \"base\"\n\n[workspace.plugins]\ncurrency = { path = \"./currency\" }\n",
        )?;
        ws.register("company", "./company")?;
        let text = ws.render();
        assert!(text.contains("currency = { path = \"./currency\" }"));
        assert!(text.contains("company = { path = \"./company\" }"));
        Ok(())
    }

    #[test]
    fn rejects_duplicates_and_legacy_addons() -> Result<(), Box<dyn std::error::Error>> {
        let mut ws = workspace(
            "[workspace]\nname = \"base\"\n[workspace.plugins]\ncompany = { path = \"./company\" }\n",
        )?;
        assert!(matches!(
            ws.register("company", "./company"),
            Err(ScaffoldError::AlreadyRegistered { .. })
        ));

        let mut legacy = workspace("[workspace]\nname = \"base\"\n[[addons]]\nname = \"x\"\npath = \"./x\"\n")?;
        assert!(matches!(
            legacy.register("company", "./company"),
            Err(ScaffoldError::LegacyAddons(_))
        ));
        Ok(())
    }

    #[test]
    fn computes_workspace_relative_paths() -> Result<(), Box<dyn std::error::Error>> {
        let ws = workspace("[workspace]\nname = \"base\"\n")?;
        assert_eq!(ws.relative_plugin_path(Path::new("/ws/company"))?, "./company");
        assert_eq!(ws.relative_plugin_path(Path::new("/ws/a/b"))?, "./a/b");
        assert!(ws.relative_plugin_path(Path::new("/elsewhere/x")).is_err());
        Ok(())
    }
}
