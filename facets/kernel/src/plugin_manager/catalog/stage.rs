//! Part of the plugin catalog; see [`super`]. Reading and checking a package folder.

use super::*;

/// Everything read from a package folder and checked, before a revision or a row is written.
pub(super) struct Package {
    pub(super) manifest: PluginManifest,
    pub(super) package_dir: PathBuf,
    pub(super) components: Vec<ComponentDocument>,
    pub(super) pages: Vec<PageDocument>,
    pub(super) layouts: Vec<LayoutDef>,
    pub(super) error_sets: Vec<ErrorSetDef>,
    pub(super) models: Vec<ModelDef>,
    pub(super) theme: Option<ThemeDocument>,
    pub(super) theme_summary: Option<(String, String)>,
    pub(super) app: Option<NewApp>,
    pub(super) app_summary: Option<(String, String)>,
    pub(super) text_catalogs: Option<super::super::i18n::PluginCatalogs>,
    pub(super) docs: Option<aether_docs::plugin_docs::PluginDocs>,
    /// Logical path of the code file inside a revision, and its file name in the older layout.
    pub(super) artifact_logical: Option<String>,
    pub(super) artifact_name: Option<PathBuf>,
    /// Every file to store, as bytes read once: what is hashed, compared and stored.
    pub(super) contents: Vec<(String, Vec<u8>)>,
    pub(super) wasm_hash: Option<String>,
    pub(super) content_hash: String,
}

impl Package {
    pub(super) fn summary(
        &self,
        version: String,
        artifact_path: Option<String>,
        created: bool,
        revision: Option<String>,
        written_files: usize,
        reused_files: usize,
    ) -> LoadedPlugin {
        LoadedPlugin {
            name: self.manifest.plugin.name.clone(),
            version,
            artifact_path,
            pages: self.pages.len(),
            public_pages: self.pages.iter().filter(|page| page.public).count(),
            theme: self.theme_summary.clone(),
            app: self.app_summary.clone(),
            created,
            revision,
            written_files,
            reused_files,
        }
    }
}

/// Parse and check every part of the package: pages, components, models, theme, rules, text
/// catalogs and docs, then read the files to store once and hash them. Writes nothing.
pub(super) async fn read_package(
    db: &Surreal<Client>,
    namespace: &str,
    core_database: &str,
    manifest_path: &Path,
    package_dir: &Path,
    manifest: PluginManifest,
    batch_models: &HashMap<String, String>,
) -> Result<Package, CatalogError> {
    let components = discover_components(package_dir).await?;
    let pages = discover_pages(package_dir).await?;
    let layouts = discover_layouts(package_dir).await?;
    let error_sets = discover_error_sets(package_dir).await?;
    let screens = super::super::screens::discover_screens(package_dir).await?;
    let model_files = read_models(package_dir)?;
    let models: Vec<ModelDef> = model_files.iter().map(|(_, model)| model.clone()).collect();
    check_foreign_links(
        db,
        namespace,
        core_database,
        &manifest.plugin,
        &models,
        batch_models,
    )
    .await?;
    validate_page_models(&manifest, &models, &pages)?;
    validate_component_models(&models, &components)?;
    validate_component_uses(db, namespace, core_database, &manifest, &components, &pages).await?;
    validate_granted_models(&manifest, &models, manifest_path)?;
    validate_capabilities(&manifest, manifest_path)?;

    let mut theme = read_theme(&manifest, manifest_path, package_dir).await?;
    if let Some(theme) = theme.as_mut() {
        bind_theme_shell_defs(theme, &layouts, &error_sets, &screens, manifest_path)?;
    }
    validate_page_layouts(&pages, &layouts)?;
    let app = read_app(&manifest, manifest_path, &pages)?;
    let rule_files = read_rule_files(package_dir).await?;
    crate::data_model::rules::parse_all(
        &rule_files
            .iter()
            .map(|(path, text)| (logical_path(path), text.clone()))
            .collect::<Vec<_>>(),
        &models,
    )
    .map_err(|problems| CatalogError::Rules {
        plugin: manifest.plugin.name.clone(),
        problems: problems.join("; "),
    })?;

    let mut files = BTreeSet::from([PathBuf::from(MANIFEST_FILE)]);
    files.extend(rule_files.iter().map(|(path, _)| path.clone()));
    files.extend(pages.iter().map(|page| page.source.clone()));
    files.extend(components.iter().map(|component| component.source.clone()));
    files.extend(layouts.iter().map(|layout| layout.source.clone()));
    files.extend(error_sets.iter().map(|set| set.source.clone()));
    files.extend(screens.iter().map(|screen| screen.source.clone()));
    for (path, _) in &model_files {
        if let Ok(relative) = path.strip_prefix(package_dir) {
            files.insert(relative.to_path_buf());
        }
    }
    for declared in declared_files(&manifest) {
        files.insert(resolve_package_file(package_dir, declared).await?);
    }
    // Files a theme names in its token file (`css`, `icons`, also under `shell`), so a
    // restored revision has them too.
    for declared in declared_files(&manifest) {
        let Ok(text) = tokio::fs::read_to_string(package_dir.join(declared)).await else {
            continue;
        };
        let Ok(tokens) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        for holder in [Some(&tokens), tokens.get("shell")].into_iter().flatten() {
            for key in ["css", "icons"] {
                if let Some(name) = holder.get(key).and_then(|value| value.as_str())
                    && !name.is_empty()
                    && let Ok(path) = resolve_package_file(package_dir, name).await
                {
                    files.insert(path);
                }
            }
        }
    }
    // Logos and other static files under `assets/` (and a top-level `icon_path`).
    files.extend(collect_asset_files(package_dir).await?);
    if let Some(icon) = manifest
        .plugin
        .icon_path
        .as_deref()
        .filter(|path| !path.is_empty())
    {
        files.insert(resolve_package_file(package_dir, icon).await?);
    }
    let text_catalogs = super::super::i18n::read_catalogs(package_dir, &manifest)
        .await
        .map_err(CatalogError::I18n)?;
    if let Some(found) = &text_catalogs {
        super::super::i18n::check_overrides(&manifest, &found.catalogs).map_err(CatalogError::I18n)?;
        files.extend(found.files.iter().cloned());
    }

    // The plugin's own documentation. Its files join the package's, so editing a page is a change.
    let docs = super::super::docs::read(package_dir, &manifest).map_err(|problems| CatalogError::Docs {
        plugin: manifest.plugin.name.clone(),
        problems: problems.join("; "),
    })?;
    if let (Some(docs), Some(def)) = (&docs, &manifest.docs) {
        files.extend(docs.pages.iter().map(|page| PathBuf::from(def.dir.trim()).join(&page.source)));
    }

    // The artifact may live in a build folder such as `out/`; it is stored beside
    // `plugin.toml`, under its own file name.
    check_one_code_file(&manifest, manifest_path)?;
    let artifact = match manifest.plugin.code_file() {
        Some(code_file) => {
            let source = resolve_package_file(package_dir, code_file).await?;
            if manifest.plugin.is_script() && super::super::script::ScriptKind::of_path(&source).is_none()
            {
                return Err(CatalogError::ScriptExtension { path: source });
            }
            let name = source
                .file_name()
                .map(PathBuf::from)
                .ok_or_else(|| CatalogError::MissingWasmFile(manifest_path.to_path_buf()))?;
            Some(Artifact { source, name })
        }
        None => None,
    };

    // Read every file once: these bytes are what is hashed, compared and stored.
    let mut contents: Vec<(String, Vec<u8>)> = Vec::new();
    let mut wasm_hash = None;
    let mut content = Sha256::new();
    for relative in &files {
        let source = package_dir.join(relative);
        let bytes = tokio::fs::read(&source).await.map_err(io_error(&source))?;
        content.update(relative.to_string_lossy().as_bytes());
        content.update((bytes.len() as u64).to_le_bytes());
        content.update(&bytes);
        contents.push((logical_path(relative), bytes));
    }
    let artifact_logical = artifact
        .as_ref()
        .map(|artifact| logical_path(&artifact.name));
    if let Some(artifact) = &artifact {
        let source = package_dir.join(&artifact.source);
        let bytes = tokio::fs::read(&source).await.map_err(io_error(&source))?;
        if manifest.plugin.is_script() {
            check_script(&source, &String::from_utf8_lossy(&bytes))?;
        }
        wasm_hash = Some(sha256_hex(&bytes));
        content.update(artifact.name.to_string_lossy().as_bytes());
        content.update(&bytes);
        contents.push((logical_path(&artifact.name), bytes));
    }
    let content_hash = content
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();

    let theme_summary = theme
        .as_ref()
        .map(|theme| (theme.name.clone(), theme.layout.clone()));
    let app_summary = app
        .as_ref()
        .map(|app| (app.label.clone(), app.route.clone()));
    Ok(Package {
        manifest,
        package_dir: package_dir.to_path_buf(),
        components,
        pages,
        layouts,
        error_sets,
        models,
        theme,
        theme_summary,
        app,
        app_summary,
        text_catalogs,
        docs,
        artifact_logical,
        artifact_name: artifact.map(|artifact| artifact.name),
        contents,
        wasm_hash,
        content_hash,
    })
}
