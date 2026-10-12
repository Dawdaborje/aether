//! Plugin-shipped layout and error-page definitions (JSON under `layouts/` and `errors/`).
//!
//! Themes name them with `"layout": "sea"` / `"error_pages": "sea"`. Built-in names
//! (`default`, `desk`, `custom`, `bare` for layouts; `default`, `custom` for errors)
//! need no file. Everything else must be defined in the package.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::Value;
use thiserror::Error;

use super::themes::is_identifier;

pub const LAYOUTS_DIR: &str = "layouts";
pub const ERRORS_DIR: &str = "errors";

pub const BUILTIN_LAYOUTS: &[&str] = &["default", "desk", "custom", "bare"];
pub const BUILTIN_ERROR_SETS: &[&str] = &["default", "custom"];

const CHROMES: &[&str] = &["top", "sidebar", "custom", "bare"];
const VARIANTS: &[&str] = &["default", "custom"];

#[derive(Debug, Error)]
pub enum ShellDefError {
    #[error("filesystem error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("{path}: not valid JSON: {source}")]
    Json {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },

    #[error("{path}: {reason}")]
    Invalid { path: PathBuf, reason: String },

    #[error("{0} is a symbolic link; layout and error definitions must be regular files")]
    Symlink(PathBuf),
}

#[derive(Debug, Clone, PartialEq)]
pub struct LayoutDef {
    pub name: String,
    pub label: String,
    /// `top` | `sidebar` | `custom` | `bare` — which first-party chrome to use.
    pub chrome: String,
    pub source: PathBuf,
    pub definition: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ErrorSetDef {
    pub name: String,
    pub label: String,
    /// `default` | `custom` — which first-party error look to base on.
    pub variant: String,
    pub source: PathBuf,
    pub definition: Value,
}

pub fn is_builtin_layout(name: &str) -> bool {
    BUILTIN_LAYOUTS.contains(&name)
}

pub fn is_builtin_error_set(name: &str) -> bool {
    BUILTIN_ERROR_SETS.contains(&name)
}

pub async fn discover_layouts(package_dir: &Path) -> Result<Vec<LayoutDef>, ShellDefError> {
    let files = list_json_files(package_dir, LAYOUTS_DIR).await?;
    let mut found = Vec::with_capacity(files.len());
    for file in files {
        let relative = file.strip_prefix(package_dir).unwrap_or(&file).to_path_buf();
        let text = tokio::fs::read_to_string(&file)
            .await
            .map_err(|source| ShellDefError::Io {
                path: file.clone(),
                source,
            })?;
        let def = parse_layout(&text, &relative)?;
        if found.iter().any(|existing: &LayoutDef| existing.name == def.name) {
            return Err(ShellDefError::Invalid {
                path: relative,
                reason: format!("layout `{}` is declared more than once", def.name),
            });
        }
        if is_builtin_layout(&def.name) {
            return Err(ShellDefError::Invalid {
                path: relative,
                reason: format!("`{}` is a built-in layout name; choose another", def.name),
            });
        }
        found.push(def);
    }
    Ok(found)
}

pub async fn discover_error_sets(package_dir: &Path) -> Result<Vec<ErrorSetDef>, ShellDefError> {
    let files = list_json_files(package_dir, ERRORS_DIR).await?;
    let mut found = Vec::with_capacity(files.len());
    for file in files {
        let relative = file.strip_prefix(package_dir).unwrap_or(&file).to_path_buf();
        let text = tokio::fs::read_to_string(&file)
            .await
            .map_err(|source| ShellDefError::Io {
                path: file.clone(),
                source,
            })?;
        let def = parse_error_set(&text, &relative)?;
        if found.iter().any(|existing: &ErrorSetDef| existing.name == def.name) {
            return Err(ShellDefError::Invalid {
                path: relative,
                reason: format!("error set `{}` is declared more than once", def.name),
            });
        }
        if is_builtin_error_set(&def.name) {
            return Err(ShellDefError::Invalid {
                path: relative,
                reason: format!("`{}` is a built-in error-page set name; choose another", def.name),
            });
        }
        found.push(def);
    }
    Ok(found)
}

pub fn parse_layout(text: &str, source: &Path) -> Result<LayoutDef, ShellDefError> {
    let invalid = |reason: &str| ShellDefError::Invalid {
        path: source.to_path_buf(),
        reason: reason.to_string(),
    };
    let value: Value = serde_json::from_str(text).map_err(|source_err| ShellDefError::Json {
        path: source.to_path_buf(),
        source: source_err,
    })?;
    let object = value.as_object().ok_or_else(|| invalid("must be a JSON object"))?;
    let name = object
        .get("name")
        .and_then(Value::as_str)
        .filter(|name| !name.is_empty())
        .ok_or_else(|| invalid("`name` is required"))?;
    if !is_identifier(name) {
        return Err(invalid(
            "`name` must be lowercase letters, digits, `-` and `_`, starting with a letter",
        ));
    }
    let chrome = object
        .get("chrome")
        .and_then(Value::as_str)
        .unwrap_or("top");
    if !CHROMES.contains(&chrome) {
        return Err(invalid(&format!(
            "`chrome` must be one of {}",
            CHROMES.join(", ")
        )));
    }
    let label = object
        .get("label")
        .and_then(Value::as_str)
        .filter(|label| !label.is_empty())
        .unwrap_or(name)
        .to_string();
    let mut definition = value.clone();
    if let Some(map) = definition.as_object_mut() {
        map.insert("name".into(), Value::String(name.to_string()));
        map.insert("chrome".into(), Value::String(chrome.to_string()));
        map.insert("label".into(), Value::String(label.clone()));
    }
    Ok(LayoutDef {
        name: name.to_string(),
        label,
        chrome: chrome.to_string(),
        source: source.to_path_buf(),
        definition,
    })
}

pub fn parse_error_set(text: &str, source: &Path) -> Result<ErrorSetDef, ShellDefError> {
    let invalid = |reason: &str| ShellDefError::Invalid {
        path: source.to_path_buf(),
        reason: reason.to_string(),
    };
    let value: Value = serde_json::from_str(text).map_err(|source_err| ShellDefError::Json {
        path: source.to_path_buf(),
        source: source_err,
    })?;
    let object = value.as_object().ok_or_else(|| invalid("must be a JSON object"))?;
    let name = object
        .get("name")
        .and_then(Value::as_str)
        .filter(|name| !name.is_empty())
        .ok_or_else(|| invalid("`name` is required"))?;
    if !is_identifier(name) {
        return Err(invalid(
            "`name` must be lowercase letters, digits, `-` and `_`, starting with a letter",
        ));
    }
    let variant = object
        .get("variant")
        .and_then(Value::as_str)
        .unwrap_or("default");
    if !VARIANTS.contains(&variant) {
        return Err(invalid("`variant` must be \"default\" or \"custom\""));
    }
    for key in ["not_found", "forbidden", "too_many_requests", "server_error", "card"] {
        if let Some(entry) = object.get(key) {
            validate_copy(entry).map_err(|reason| invalid(&format!("`{key}`: {reason}")))?;
        }
    }
    let label = object
        .get("label")
        .and_then(Value::as_str)
        .filter(|label| !label.is_empty())
        .unwrap_or(name)
        .to_string();
    let mut definition = value.clone();
    if let Some(map) = definition.as_object_mut() {
        map.insert("name".into(), Value::String(name.to_string()));
        map.insert("variant".into(), Value::String(variant.to_string()));
        map.insert("label".into(), Value::String(label.clone()));
    }
    Ok(ErrorSetDef {
        name: name.to_string(),
        label,
        variant: variant.to_string(),
        source: source.to_path_buf(),
        definition,
    })
}

fn validate_copy(value: &Value) -> Result<(), String> {
    let object = value.as_object().ok_or("must be an object")?;
    for key in ["title", "message"] {
        if let Some(entry) = object.get(key)
            && !entry.is_string()
        {
            return Err(format!("`{key}` must be a string"));
        }
    }
    Ok(())
}

async fn list_json_files(package_dir: &Path, folder: &str) -> Result<Vec<PathBuf>, ShellDefError> {
    let root = package_dir.join(folder);
    let mut files = Vec::new();
    let mut pending = vec![root.clone()];
    while let Some(directory) = pending.pop() {
        let mut entries = match tokio::fs::read_dir(&directory).await {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && directory == root => {
                return Ok(Vec::new());
            }
            Err(source) => {
                return Err(ShellDefError::Io {
                    path: directory,
                    source,
                });
            }
        };
        while let Some(entry) = entries.next_entry().await.map_err(|source| ShellDefError::Io {
            path: directory.clone(),
            source,
        })? {
            let path = entry.path();
            let file_type = entry.file_type().await.map_err(|source| ShellDefError::Io {
                path: path.clone(),
                source,
            })?;
            if file_type.is_symlink() {
                return Err(ShellDefError::Symlink(path));
            }
            if file_type.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|extension| extension == "json") {
                files.push(path);
            }
        }
    }
    files.sort();
    Ok(files)
}

/// Index layout defs by name.
pub fn layouts_by_name(layouts: &[LayoutDef]) -> BTreeMap<&str, &LayoutDef> {
    layouts.iter().map(|layout| (layout.name.as_str(), layout)).collect()
}

/// Index error-set defs by name.
pub fn error_sets_by_name(sets: &[ErrorSetDef]) -> BTreeMap<&str, &ErrorSetDef> {
    sets.iter().map(|set| (set.name.as_str(), set)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_layout_chrome() -> Result<(), ShellDefError> {
        let def = parse_layout(
            r#"{"name":"sea","chrome":"sidebar","label":"Sea"}"#,
            Path::new("layouts/sea.json"),
        )?;
        assert_eq!(def.name, "sea");
        assert_eq!(def.chrome, "sidebar");
        Ok(())
    }

    #[test]
    fn parses_error_set_copy() -> Result<(), ShellDefError> {
        let def = parse_error_set(
            r#"{"name":"sea","variant":"custom","not_found":{"title":"Gone"}}"#,
            Path::new("errors/sea.json"),
        )?;
        assert_eq!(def.variant, "custom");
        assert_eq!(def.definition["not_found"]["title"], "Gone");
        Ok(())
    }

    #[test]
    fn rejects_bad_chrome() {
        assert!(matches!(
            parse_layout(r#"{"name":"sea","chrome":"floating"}"#, Path::new("layouts/sea.json")),
            Err(ShellDefError::Invalid { .. })
        ));
    }
}
