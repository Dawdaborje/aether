//! Part of the plugin catalog; see [`super`].

use super::*;

/// `0.1.0+20261003T210100Z` is a rebuild of `0.1.0`.
/// Folders that are never part of a package's content (build output, dependencies, VCS data).
pub(super) const SKIPPED_DIRS: &[&str] = &["target", "node_modules", ".git", ".svelte-kit", "__pycache__"];

/// A SHA-256 over every regular file under `dir` (relative path, length, bytes), in path order.
pub(super) async fn hash_package_tree(dir: &Path) -> Result<String, CatalogError> {
    let dir = dir.to_path_buf();
    let result = tokio::task::spawn_blocking(move || -> io::Result<String> {
        fn walk(root: &Path, here: &Path, out: &mut Vec<PathBuf>) -> io::Result<()> {
            for entry in std::fs::read_dir(here)? {
                let entry = entry?;
                let kind = entry.file_type()?;
                let path = entry.path();
                if kind.is_dir() {
                    let name = entry.file_name();
                    if SKIPPED_DIRS.iter().any(|skip| name == *skip) {
                        continue;
                    }
                    walk(root, &path, out)?;
                } else if kind.is_file() {
                    if let Ok(relative) = path.strip_prefix(root) {
                        out.push(relative.to_path_buf());
                    }
                }
            }
            Ok(())
        }
        let mut files = Vec::new();
        walk(&dir, &dir, &mut files)?;
        files.sort();
        let mut hash = Sha256::new();
        for relative in files {
            let bytes = std::fs::read(dir.join(&relative))?;
            hash.update(relative.to_string_lossy().as_bytes());
            hash.update((bytes.len() as u64).to_le_bytes());
            hash.update(&bytes);
        }
        Ok(hash.finalize().iter().map(|byte| format!("{byte:02x}")).collect())
    })
    .await
    .map_err(|error| CatalogError::Io {
        path: PathBuf::new(),
        source: io::Error::other(error.to_string()),
    })?;
    result.map_err(|source| CatalogError::Io {
        path: PathBuf::new(),
        source,
    })
}

/// The catalog row this exact folder produced last time, if its stored files are all still there.
/// `None` means "do the full load".
pub(super) async fn unchanged_since_last_load(
    db: &Surreal<Client>,
    namespace: &str,
    core_database: &str,
    layout: &AppDir,
    manifest: &PluginManifest,
    tree_hash: &str,
) -> Result<Option<LoadedPlugin>, CatalogError> {
    db.use_ns(namespace).await?;
    db.use_db(core_database).await?;
    let mut response = db
        .query(
            "SELECT version, content_hash, revision, date_created FROM plugins \
             WHERE name = $name AND source_hash = $hash ORDER BY date_created DESC LIMIT 1;",
        )
        .bind(("name", manifest.plugin.name.clone()))
        .bind(("hash", tree_hash.to_string()))
        .await?
        .check()?;
    let rows: Vec<KnownVersion> = response.take(0)?;
    let Some(row) = rows.into_iter().next() else {
        return Ok(None);
    };
    let Some(revision) = row.revision.clone() else {
        return Ok(None);
    };
    let Some(index) = revisions::read_index(layout, &manifest.plugin.name, &revision).await? else {
        return Ok(None);
    };
    // A file missing or damaged in `app_dir` is repaired by the full load.
    if !revisions::verify(layout, &index).await?.is_empty() {
        return Ok(None);
    }
    let artifact_path = manifest
        .plugin
        .code_file()
        .and_then(|file| Path::new(file).file_name().map(PathBuf::from))
        .and_then(|name| {
            let logical = logical_path(&name);
            index
                .files
                .get(&logical)
                .map(|entry| revisions::relative_path(&manifest.plugin.name, entry, &logical))
        })
        .map(|path| path.to_string_lossy().into_owned());
    Ok(Some(LoadedPlugin {
        name: manifest.plugin.name.clone(),
        version: row.version,
        artifact_path,
        // Not read: nothing was parsed. The stored catalog has the real figures.
        pages: 0,
        public_pages: 0,
        theme: None,
        app: None,
        created: false,
        revision: Some(revision),
        written_files: 0,
        reused_files: 0,
    }))
}

/// The plugin's `rules/*.json`, as package-relative paths with their text.
pub(super) async fn read_rule_files(package_dir: &Path) -> Result<Vec<(PathBuf, String)>, CatalogError> {
    let directory = package_dir.join(crate::data_model::rules::RULE_DIR);
    let mut found = Vec::new();
    let Ok(mut entries) = tokio::fs::read_dir(&directory).await else {
        return Ok(found);
    };
    while let Some(entry) = entries.next_entry().await.map_err(io_error(&directory))? {
        let path = entry.path();
        if path
            .extension()
            .is_some_and(|extension| extension == "json")
        {
            let text = tokio::fs::read_to_string(&path)
                .await
                .map_err(io_error(&path))?;
            let relative = path
                .strip_prefix(package_dir)
                .unwrap_or(&path)
                .to_path_buf();
            found.push((relative, text));
        }
    }
    found.sort();
    Ok(found)
}

/// Package-relative files, beyond `plugin.toml` and the WASM artifact, that
/// the manifest points at.
pub(super) fn declared_files(manifest: &PluginManifest) -> Vec<&str> {
    manifest
        .theme
        .iter()
        .filter_map(|theme| theme.tokens_file.as_deref())
        .filter(|file| !file.is_empty())
        .collect()
}

/// Every regular file under the package's `assets/` directory, as paths relative
/// to the package root. Missing `assets/` is fine — many plugins only use a
/// Lucide `icon` name.
pub(super) async fn collect_asset_files(package_dir: &Path) -> Result<Vec<PathBuf>, CatalogError> {
    let root = package_dir.join("assets");
    let Ok(meta) = tokio::fs::metadata(&root).await else {
        return Ok(Vec::new());
    };
    if !meta.is_dir() {
        return Ok(Vec::new());
    }
    let mut found = Vec::new();
    let mut pending = vec![root];
    while let Some(current) = pending.pop() {
        let mut entries = tokio::fs::read_dir(&current)
            .await
            .map_err(io_error(&current))?;
        while let Some(entry) = entries.next_entry().await.map_err(io_error(&current))? {
            let path = entry.path();
            let file_type = entry.file_type().await.map_err(io_error(&path))?;
            if file_type.is_dir() {
                pending.push(path);
            } else if file_type.is_file() {
                let relative = path
                    .strip_prefix(package_dir)
                    .unwrap_or(&path)
                    .to_path_buf();
                found.push(relative);
            }
        }
    }
    found.sort();
    Ok(found)
}

/// Resolve a manifest-declared file to a path relative to `package_dir`,
/// requiring that it exists, is a regular file, and stays inside the package.
pub(super) async fn resolve_package_file(package_dir: &Path, declared: &str) -> Result<PathBuf, CatalogError> {
    let candidate = package_dir.join(declared);
    let canonical = tokio::fs::canonicalize(&candidate)
        .await
        .map_err(io_error(&candidate))?;
    let relative =
        canonical
            .strip_prefix(package_dir)
            .map_err(|_| CatalogError::FileOutsidePackage {
                path: canonical.clone(),
                package: package_dir.to_path_buf(),
            })?;
    if !canonical.is_file() {
        return Err(CatalogError::FileOutsidePackage {
            path: canonical,
            package: package_dir.to_path_buf(),
        });
    }
    Ok(relative.to_path_buf())
}

/// Whether a manifest key names a file.
pub(super) fn declares(file: &Option<String>) -> bool {
    file.as_deref().is_some_and(|file| !file.is_empty())
}

/// A plugin has a WebAssembly module or a script, never both.
pub(super) fn check_one_code_file(
    manifest: &PluginManifest,
    manifest_path: &Path,
) -> Result<(), CatalogError> {
    if declares(&manifest.plugin.wasm_file) && declares(&manifest.plugin.script) {
        return Err(CatalogError::BothWasmAndScript(manifest_path.to_path_buf()));
    }
    Ok(())
}

/// A script must end in `.rhai` or `.lua` and compile: refused when the plugin is loaded or
/// checked, not at the first call.
pub(super) fn check_script(source: &Path, text: &str) -> Result<(), CatalogError> {
    let kind = super::super::script::ScriptKind::of_path(source).ok_or_else(|| {
        CatalogError::ScriptExtension {
            path: source.to_path_buf(),
        }
    })?;
    super::super::script::ScriptProgram::compile(kind, text).map_err(|error| {
        CatalogError::ScriptInvalid {
            path: source.to_path_buf(),
            message: error.to_string(),
        }
    })?;
    Ok(())
}
