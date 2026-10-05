//! A plugin's folder on disk, and the rules that keep it inside that folder.
//!
//! Each plugin has one folder per organization, `<app_dir>/orgs/<organization>/plugins/<plugin>/`.
//! An administrator (or a program) drops files there; the plugin watches it ([`watch`]) and reads,
//! writes and moves files in it with the `fs::*` commands. A plugin names paths **relative to its
//! folder** and never sees or chooses an absolute path, so it cannot reach another plugin's files,
//! the organization's other files, or anything else on the machine.

use std::path::{Component, Path, PathBuf};

pub mod watch;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PathError {
    #[error("`{0}` is not a path inside the plugin's folder (use a relative path without `..`)")]
    Unsafe(String),
    #[error("`{0}` leads outside the plugin's folder")]
    Escapes(String),
    #[error("the plugin's folder cannot be used: {0}")]
    Folder(String),
}

/// A path relative to a plugin's folder, checked: no absolute paths, no `..`, no backslashes or
/// control characters, no empty parts (`a//b`), at most 512 bytes. `""` is the folder itself.
/// Returns the path in normal form (`/` separators, no leading `./`).
pub fn safe_relative(path: &str) -> Result<String, PathError> {
    let unsafe_path = || PathError::Unsafe(path.to_string());
    if path.len() > 512 || path.contains('\\') || path.chars().any(char::is_control) {
        return Err(unsafe_path());
    }
    let mut parts = Vec::new();
    for part in path.split('/') {
        match part {
            "" if path.is_empty() => {}
            "" | ".." => return Err(unsafe_path()),
            "." => {}
            other => parts.push(other),
        }
    }
    if path.starts_with('/') {
        return Err(unsafe_path());
    }
    Ok(parts.join("/"))
}

/// The real location of `relative` inside `root`, refusing anything that ends up outside it, such
/// as a symbolic link that points elsewhere. The file need not exist yet (for a write or a move);
/// then its nearest existing parent is checked instead.
pub async fn resolve(root: &Path, relative: &str) -> Result<PathBuf, PathError> {
    let relative = safe_relative(relative)?;
    let real_root = tokio::fs::canonicalize(root).await.map_err(|e| PathError::Folder(e.to_string()))?;
    let wanted = real_root.join(&relative);
    // Walk up to the nearest part that exists, resolve it, and put the rest back.
    let mut existing = wanted.clone();
    let mut missing: Vec<std::ffi::OsString> = Vec::new();
    let resolved = loop {
        match tokio::fs::canonicalize(&existing).await {
            Ok(real) => break real,
            Err(_) => match (existing.file_name().map(ToOwned::to_owned), existing.parent().map(Path::to_path_buf)) {
                (Some(name), Some(parent)) => {
                    missing.push(name);
                    existing = parent;
                }
                _ => return Err(PathError::Escapes(relative)),
            },
        }
    };
    if !resolved.starts_with(&real_root) {
        return Err(PathError::Escapes(relative));
    }
    let mut full = resolved;
    for name in missing.into_iter().rev() {
        full.push(name);
    }
    Ok(full)
}

/// `full` as a path relative to `root`, in normal form; `None` if it is not inside.
pub fn relative_to(root: &Path, full: &Path) -> Option<String> {
    let rest = full.strip_prefix(root).ok()?;
    let mut parts = Vec::new();
    for component in rest.components() {
        match component {
            Component::Normal(part) => parts.push(part.to_str()?.to_string()),
            _ => return None,
        }
    }
    Some(parts.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_plain_relative_paths_pass() {
        assert_eq!(safe_relative(""), Ok(String::new()));
        assert_eq!(safe_relative("inbox/a.csv"), Ok("inbox/a.csv".into()));
        assert_eq!(safe_relative("./inbox/./a.csv"), Ok("inbox/a.csv".into()));
        for bad in ["/etc/passwd", "../x", "a/../../b", "a/..", "a//b", "a\\b", "a/\u{1}b", "a/", &"x/".repeat(300)] {
            assert!(safe_relative(bad).is_err(), "{bad:?}");
        }
    }

    #[tokio::test]
    async fn resolving_stays_inside_the_folder() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let root = dir.path().join("plugin");
        std::fs::create_dir_all(root.join("inbox"))?;
        std::fs::write(root.join("inbox/a.txt"), "x")?;
        let outside = dir.path().join("secret");
        std::fs::create_dir_all(&outside)?;
        std::fs::write(outside.join("key"), "k")?;

        // Existing and not-yet-existing paths resolve under the real root.
        let real_root = std::fs::canonicalize(&root)?;
        assert_eq!(resolve(&root, "inbox/a.txt").await?, real_root.join("inbox/a.txt"));
        assert_eq!(resolve(&root, "inbox/new/deeper.txt").await?, real_root.join("inbox/new/deeper.txt"));
        assert_eq!(resolve(&root, "").await?, real_root);

        // `..` and absolute paths are refused outright.
        assert!(matches!(resolve(&root, "../secret/key").await, Err(PathError::Unsafe(_))));
        // A symbolic link out of the folder is refused, to a file or to a folder, existing or to be created.
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(outside.join("key"), root.join("inbox/link"))?;
            std::os::unix::fs::symlink(&outside, root.join("inbox/dir_link"))?;
            for escape in ["inbox/link", "inbox/dir_link/key", "inbox/dir_link/new.txt"] {
                assert!(matches!(resolve(&root, escape).await, Err(PathError::Escapes(_))), "{escape}");
            }
            // A link that stays inside is fine.
            std::os::unix::fs::symlink(root.join("inbox/a.txt"), root.join("inbox/inside"))?;
            assert!(resolve(&root, "inbox/inside").await.is_ok());
        }
        Ok(())
    }

    #[test]
    fn paths_are_reported_relative_to_the_folder() {
        let root = Path::new("/srv/plugin");
        assert_eq!(relative_to(root, Path::new("/srv/plugin/in/a.csv")), Some("in/a.csv".into()));
        assert_eq!(relative_to(root, Path::new("/srv/plugin")), Some(String::new()));
        assert_eq!(relative_to(root, Path::new("/srv/other/a.csv")), None);
    }
}
