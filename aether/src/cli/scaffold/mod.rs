//! Native plugin scaffolding: no Extism CLI required.
//!
//! `aether --gen plugin --plugin-path company` creates `./company` with a
//! manifest, a sample source file for the chosen language, a build `Makefile`,
//! `README.md`, `.gitignore` and a BSL 1.1 `LICENSE`. When the directory sits
//! inside a plugin workspace (an ancestor holds a `workspace.toml`), the plugin
//! is also registered under `[workspace.plugins]`, the way `cargo new`
//! registers a workspace member.

mod language;
mod license;
mod workspace;

use std::path::{Component, Path, PathBuf};

use thiserror::Error;
use tokio::fs;

pub use language::Language;
use workspace::Workspace;

#[derive(Debug, Error)]
pub enum ScaffoldError {
    #[error("filesystem error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("invalid plugin name `{0}`: use lowercase letters, digits and `_`, starting with a letter")]
    InvalidName(String),

    #[error("unsupported plugin language `{0}`; choose go, rust, typescript, javascript or python")]
    UnsupportedLanguage(String),

    #[error("{0} already exists and is not an empty directory")]
    DestinationNotEmpty(PathBuf),

    #[error("could not read the system clock")]
    Clock,

    #[error("invalid workspace manifest {path}: {source}")]
    WorkspaceParse {
        path: PathBuf,
        #[source]
        source: toml_edit::TomlError,
    },

    #[error("invalid workspace manifest {path}: {reason}")]
    WorkspaceInvalid { path: PathBuf, reason: &'static str },

    #[error("{0} uses `[[addons]]`, which is no longer supported; declare plugins under [workspace.plugins]")]
    LegacyAddons(PathBuf),

    #[error("plugin `{name}` is already registered in workspace `{workspace}`")]
    AlreadyRegistered { name: String, workspace: String },
}

fn io_error(path: &Path) -> impl FnOnce(std::io::Error) -> ScaffoldError + '_ {
    move |source| ScaffoldError::Io {
        path: path.to_path_buf(),
        source,
    }
}

#[derive(Debug)]
pub struct CreatedPlugin {
    pub name: String,
    pub directory: PathBuf,
    pub language: Language,
    /// Name of the workspace the plugin was registered in, if any.
    pub workspace: Option<String>,
}

const DEFAULT_AUTHOR: (&str, &str) = ("Your Name", "your.email@example.com");

const PLUGIN_TOML: &str = r#"[plugin]
name = "__NAME__"
label = "__LABEL__"
version = "0.1.0"
description = "A short description of __LABEL__."
authors = [{ name = __AUTHOR_NAME__, email = __AUTHOR_EMAIL__ }]
dependencies = []
capabilities = ["db::query", "db::mutate"]
is_builtin = false
wasm_file = "out/plugin.wasm"

# Anonymous visitors can use nothing unless it is declared here (all off by
# default). Pages opt in with `public="true"` on their `<page>` element.
# public_functions = ["list_messages"]
# public_capabilities = ["db::query"]
# public_access_models = [{ name = "message", permissions = ["read"] }]

# Contract version; bump only when breaking required keys.
[plugin.api]
version = "0.1"

[plugin.meta]
kind = "addon"
__WORKSPACE_LINE__
[communication]
# Planned channels: events | email | sms | http | plugins
channels = []
"#;

/// Create the plugin at `path` (relative paths resolve against the current
/// directory); the last path segment is the plugin name.
pub async fn create_plugin(path: &Path, language: Language) -> Result<CreatedPlugin, ScaffoldError> {
    let current_dir = std::env::current_dir().map_err(io_error(Path::new(".")))?;
    let directory = normalize(&current_dir.join(path));
    let name = directory
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| ScaffoldError::InvalidName(path.to_string_lossy().into_owned()))?
        .to_string();
    validate_name(&name)?;
    ensure_empty_destination(&directory).await?;

    let mut workspace = match directory.parent() {
        Some(parent) => Workspace::find(parent).await?,
        None => None,
    };
    if let Some(workspace) = workspace.as_mut() {
        let relative = workspace.relative_plugin_path(&directory)?;
        workspace.register(&name, &relative)?;
    }

    let (author_name, author_email) = workspace
        .as_ref()
        .and_then(|workspace| workspace.author().cloned())
        .unwrap_or_else(|| (DEFAULT_AUTHOR.0.to_string(), DEFAULT_AUTHOR.1.to_string()));
    let label = humanize(&name);

    let mut files = language.files(&name);
    files.push(language::TemplateFile {
        path: "plugin.toml",
        content: render_manifest(
            &name,
            &label,
            &author_name,
            &author_email,
            workspace.as_ref().map(Workspace::name),
        ),
    });
    files.push(language::TemplateFile {
        path: "LICENSE",
        content: license::bsl_license(license::current_year()?, &author_name),
    });
    files.push(language::TemplateFile {
        path: "README.md",
        content: language.readme(&name, &label),
    });

    for file in files {
        let target = directory.join(file.path);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).await.map_err(io_error(parent))?;
        }
        fs::write(&target, file.content)
            .await
            .map_err(io_error(&target))?;
    }

    // Registered last, so a failed scaffold never leaves a dangling entry.
    if let Some(workspace) = &workspace {
        let manifest_path = workspace.manifest_path();
        fs::write(manifest_path, workspace.render())
            .await
            .map_err(io_error(manifest_path))?;
    }

    Ok(CreatedPlugin {
        name,
        directory,
        language,
        workspace: workspace.map(|workspace| workspace.name().to_string()),
    })
}

fn render_manifest(
    name: &str,
    label: &str,
    author_name: &str,
    author_email: &str,
    workspace: Option<&str>,
) -> String {
    let quoted = |value: &str| toml_edit::Value::from(value).to_string().trim().to_string();
    let workspace_line = workspace
        .map(|workspace| format!("workspace = {}\n", quoted(workspace)))
        .unwrap_or_default();
    PLUGIN_TOML
        .replace("__NAME__", name)
        .replace("__LABEL__", label)
        .replace("__AUTHOR_NAME__", &quoted(author_name))
        .replace("__AUTHOR_EMAIL__", &quoted(author_email))
        .replace("__WORKSPACE_LINE__", &workspace_line)
}

fn validate_name(name: &str) -> Result<(), ScaffoldError> {
    let mut characters = name.chars();
    let valid = name.len() <= 64
        && characters.next().is_some_and(|first| first.is_ascii_lowercase())
        && characters.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    if valid {
        Ok(())
    } else {
        Err(ScaffoldError::InvalidName(name.to_string()))
    }
}

async fn ensure_empty_destination(directory: &Path) -> Result<(), ScaffoldError> {
    match fs::metadata(directory).await {
        Ok(metadata) if metadata.is_dir() => {
            let mut entries = fs::read_dir(directory).await.map_err(io_error(directory))?;
            if entries
                .next_entry()
                .await
                .map_err(io_error(directory))?
                .is_some()
            {
                return Err(ScaffoldError::DestinationNotEmpty(directory.to_path_buf()));
            }
            Ok(())
        }
        Ok(_) => Err(ScaffoldError::DestinationNotEmpty(directory.to_path_buf())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io_error(directory)(error)),
    }
}

/// Lexically resolve `.` and `..` so ancestors and `strip_prefix` behave
/// even though the path does not exist yet.
fn normalize(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

/// `my_plugin` -> `My Plugin`.
fn humanize(name: &str) -> String {
    name.split(['_', ' ', '-'])
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_plugin_names() {
        for good in ["company", "base_components", "a1"] {
            assert!(validate_name(good).is_ok(), "{good}");
        }
        for bad in ["", "Company", "1abc", "has-dash", "has space", "../x"] {
            assert!(validate_name(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn normalizes_dot_segments() {
        assert_eq!(normalize(Path::new("/a/./b/../c")), PathBuf::from("/a/c"));
    }

    #[test]
    fn humanizes_names() {
        assert_eq!(humanize("base_components"), "Base Components");
    }

    #[test]
    fn manifest_parses_as_plugin_toml() -> Result<(), toml_edit::TomlError> {
        let text = render_manifest("company", "Company", "Ada \"A\" L", "a@x.io", Some("base"));
        let document: toml_edit::DocumentMut = text.parse()?;
        assert_eq!(document["plugin"]["name"].as_str(), Some("company"));
        assert_eq!(
            document["plugin"]["wasm_file"].as_str(),
            Some("out/plugin.wasm")
        );
        assert_eq!(document["plugin"]["meta"]["workspace"].as_str(), Some("base"));
        assert!(document["plugin"].get("public_functions").is_none(), "public access is opt-in");

        let standalone = render_manifest("company", "Company", "Ada", "a@x.io", None);
        assert!(!standalone.contains("workspace ="));
        standalone.parse::<toml_edit::DocumentMut>()?;
        Ok(())
    }
}
