//! Collects a plugin's documentation (`[docs]` in `plugin.toml`) when the plugin is loaded: reads the markdown
//! files from the plugin's folder, checks them and renders them (see `aether_docs::plugin_docs`).
//!
//! The folder is the author's, so nothing outside the plugin is read: the docs folder and every file must stay
//! inside the plugin directory even through symbolic links, and sizes are checked before a file is read.

use super::models::plugin_def::PluginManifest;
use aether_docs::plugin_docs::{MAX_FILE_BYTES, MAX_FILES, PluginDocs, SourceFile, build};
use std::fs;
use std::path::{Component, Path, PathBuf};

/// The rendered docs of a plugin package, or `None` when its manifest has no `[docs]` table.
/// `Err` holds every problem found, written for the plugin's author.
pub fn read(package_dir: &Path, manifest: &PluginManifest) -> Result<Option<PluginDocs>, Vec<String>> {
    let Some(def) = &manifest.docs else {
        return Ok(None);
    };
    let dir = def.dir.trim();
    let relative = Path::new(dir);
    if dir.is_empty() || relative.is_absolute() || relative.components().any(|c| !matches!(c, Component::Normal(_))) {
        return Err(vec![format!("[docs] dir `{dir}` must be a folder inside the plugin, such as `docs`")]);
    }
    let package = fs::canonicalize(package_dir).map_err(|e| vec![format!("cannot read the plugin folder: {e}")])?;
    let root = fs::canonicalize(package.join(relative))
        .map_err(|_| vec![format!("[docs] dir `{dir}` does not exist in the plugin; create it with a README.md, or remove the [docs] table")])?;
    if !root.starts_with(&package) || !root.is_dir() {
        return Err(vec![format!("[docs] dir `{dir}` must be a folder inside the plugin")]);
    }

    let mut paths = Vec::new();
    let mut problems = Vec::new();
    collect(&root, &root, &mut paths, &mut problems);
    if !problems.is_empty() {
        return Err(problems);
    }
    if paths.len() > MAX_FILES {
        return Err(vec![format!("{} .md files under `{dir}`; at most {MAX_FILES} are allowed", paths.len())]);
    }
    paths.sort();

    let mut files = Vec::new();
    for path in paths {
        let relative = path.strip_prefix(&root).unwrap_or(&path).to_string_lossy().replace('\\', "/");
        match fs::read_to_string(&path) {
            Ok(text) => files.push(SourceFile { path: relative, text }),
            Err(e) => problems.push(format!("{relative}: cannot be read as UTF-8 text ({e})")),
        }
    }
    if !problems.is_empty() {
        return Err(problems);
    }
    if files.is_empty() {
        return Err(vec![format!("`{dir}` has no .md files; add a README.md or remove the [docs] table")]);
    }
    let label = if manifest.plugin.label.trim().is_empty() { manifest.plugin.name.clone() } else { manifest.plugin.label.clone() };
    build(&manifest.plugin.name, &manifest.plugin.version, &label, def.visibility, &files).map(Some)
}

/// Every `.md` file under `dir`, without reading any. Hidden entries are skipped; a link that leads outside
/// `root` is a problem; reading stops being sensible past the limits, so the walk gives up there.
fn collect(root: &Path, dir: &Path, out: &mut Vec<PathBuf>, problems: &mut Vec<String>) {
    let Ok(entries) = fs::read_dir(dir) else {
        problems.push(format!("cannot read the folder `{}`", dir.display()));
        return;
    };
    for entry in entries.flatten() {
        if out.len() > MAX_FILES {
            return;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let Ok(real) = fs::canonicalize(entry.path()) else { continue };
        if !real.starts_with(root) {
            problems.push(format!("`{name}` leads outside the docs folder; the docs may only use files inside it"));
            continue;
        }
        if real.is_dir() {
            collect(root, &real, out, problems);
        } else if real.extension().is_some_and(|e| e == "md") {
            match fs::metadata(&real) {
                Ok(m) if m.len() as usize > MAX_FILE_BYTES => problems.push(format!(
                    "{}: {} KB; a file may be at most {} KB",
                    real.strip_prefix(root).unwrap_or(&real).display(),
                    m.len() / 1024,
                    MAX_FILE_BYTES / 1024
                )),
                _ => out.push(real),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn package(manifest_extra: &str, files: &[(&str, &str)]) -> (tempfile::TempDir, PluginManifest) {
        let dir = tempfile::tempdir().unwrap();
        let text = format!("[plugin]\nname = \"notes\"\nlabel = \"Notes\"\nversion = \"1.0.0\"\n{manifest_extra}");
        fs::write(dir.path().join("plugin.toml"), &text).unwrap();
        for (path, body) in files {
            let target = dir.path().join(path);
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            fs::write(target, body).unwrap();
        }
        (dir, PluginManifest::parse(&text).unwrap())
    }

    #[test]
    fn no_docs_table_means_no_docs() {
        let (dir, manifest) = package("", &[("docs/README.md", "# Hi")]);
        assert!(read(dir.path(), &manifest).unwrap().is_none());
    }

    #[test]
    fn reads_and_renders_the_docs_folder() {
        let (dir, manifest) = package(
            "\n[docs]\nvisibility = \"public\"\n",
            &[("docs/README.md", "# Notes\n\nSee [more](guide/more.md)."), ("docs/guide/more.md", "# More"), ("docs/image.png", "x"), ("docs/.hidden.md", "no")],
        );
        let docs = read(dir.path(), &manifest).unwrap().unwrap();
        assert_eq!((docs.plugin.as_str(), docs.label.as_str(), docs.version.as_str()), ("notes", "Notes", "1.0.0"));
        assert_eq!(docs.visibility, aether_docs::plugin_docs::Visibility::Public);
        let slugs: Vec<&str> = docs.pages.iter().map(|p| p.slug.as_str()).collect();
        assert_eq!(slugs, ["index", "guide/more"]);
    }

    #[test]
    fn visibility_defaults_to_authenticated_and_dir_can_move() {
        let (dir, manifest) = package("\n[docs]\ndir = \"help\"\n", &[("help/README.md", "# H")]);
        let docs = read(dir.path(), &manifest).unwrap().unwrap();
        assert_eq!(docs.visibility, aether_docs::plugin_docs::Visibility::Authenticated);
    }

    #[test]
    fn problems_are_reported_for_the_author() {
        let (dir, manifest) = package("\n[docs]\n", &[]);
        assert!(read(dir.path(), &manifest).unwrap_err()[0].contains("does not exist"));

        let (dir, manifest) = package("\n[docs]\n", &[("docs/notes.txt", "x")]);
        assert!(read(dir.path(), &manifest).unwrap_err()[0].contains("no .md files"));

        let (dir, manifest) = package("\n[docs]\n", &[("docs/README.md", "[x](gone.md)")]);
        assert!(read(dir.path(), &manifest).unwrap_err()[0].contains("does not exist"));

        for bad in ["../outside", "/etc", "", "a/../b"] {
            let (dir, manifest) = package(&format!("\n[docs]\ndir = \"{bad}\"\n"), &[]);
            assert!(read(dir.path(), &manifest).unwrap_err()[0].contains("inside the plugin"), "{bad}");
        }
        assert!(PluginManifest::parse("[plugin]\nname=\"n\"\nversion=\"1\"\n[docs]\nvisibilty = \"public\"\n").is_err(), "a typo in a key is refused");
        assert!(PluginManifest::parse("[plugin]\nname=\"n\"\nversion=\"1\"\n[docs]\nvisibility = \"everyone\"\n").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn a_link_out_of_the_docs_folder_is_refused() {
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("secret.md"), "top secret").unwrap();
        let (dir, manifest) = package("\n[docs]\n", &[("docs/README.md", "# Hi")]);
        std::os::unix::fs::symlink(outside.path().join("secret.md"), dir.path().join("docs/leak.md")).unwrap();
        let e = read(dir.path(), &manifest).unwrap_err();
        assert!(e[0].contains("leads outside the docs folder"), "{e:?}");

        // The docs folder itself may not be a link out of the plugin either.
        let (dir, manifest) = package("\n[docs]\ndir = \"docs\"\n", &[]);
        std::os::unix::fs::symlink(outside.path(), dir.path().join("docs")).unwrap();
        assert!(read(dir.path(), &manifest).unwrap_err()[0].contains("inside the plugin"));
    }
}
