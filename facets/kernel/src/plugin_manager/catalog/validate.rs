//! Part of the plugin catalog; see [`super`].

use super::*;

/// The capabilities a manifest asks for must exist, and a public capability must be one the
/// plugin holds.
pub(super) fn validate_capabilities(
    manifest: &PluginManifest,
    manifest_path: &Path,
) -> Result<(), CatalogError> {
    use std::sync::OnceLock;
    static CATALOG: OnceLock<Result<CapabilityCatalog, String>> = OnceLock::new();
    let catalog = CATALOG
        .get_or_init(|| CapabilityCatalog::builtin().map_err(|error| error.to_string()))
        .as_ref()
        .map_err(|message| CatalogError::CapabilityCatalog(message.clone()))?;
    let plugin = &manifest.plugin;
    catalog
        .validate_declared(&plugin.capabilities, &plugin.public_capabilities)
        .map_err(|source| CatalogError::UnknownCapability {
            path: manifest_path.to_path_buf(),
            source,
        })?;
    if let Some(capability) = plugin
        .public_capabilities
        .iter()
        .find(|capability| !plugin.capabilities.contains(capability))
    {
        return Err(CatalogError::PublicCapabilityNotHeld {
            path: manifest_path.to_path_buf(),
            capability: capability.clone(),
        });
    }
    Ok(())
}

/// Parse and validate a plugin package without writing to the catalog or `app_dir`.
///
/// Runs the same checks as [`load_plugin`] (manifest, pages, models, foreign links, scripts,
/// capabilities, and so on) and stops before storing files or catalog rows. Used by
/// `aether --load-plugins --dry-run`.
///
/// `batch_models` maps `plugin.model` → `model_id` for other packages in the same dry-run,
/// so a dependency that is only being checked (not yet catalogued) still satisfies foreign links.
pub async fn check_plugin(
    db: &Surreal<Client>,
    namespace: &str,
    core_database: &str,
    app_dir: &Path,
    plugin_path: &Path,
    batch_models: &HashMap<String, String>,
) -> Result<LoadedPlugin, CatalogError> {
    load_plugin_inner(
        db,
        namespace,
        core_database,
        app_dir,
        plugin_path,
        true,
        batch_models,
    )
    .await
}

/// What [`check_plugin_offline`] looked at.
#[derive(Debug, Clone)]
pub struct OfflineCheck {
    pub name: String,
    pub version: String,
    pub models: Vec<String>,
    pub pages: usize,
    pub components: usize,
    /// The number of documentation pages, when the manifest has a `[docs]` table.
    pub doc_pages: Option<usize>,
    /// Checks that were not made, and why (most need the catalog, which is a database).
    pub skipped: Vec<&'static str>,
}

/// [`check_plugin`] without a database: parses and validates a package on disk and stops there.
///
/// Makes every check of a load that does not read the catalog (manifest, models, pages, components,
/// granted models, capabilities, theme, layouts, rules, texts, declared files, script syntax). It
/// cannot check links to other plugins' models or components taken from other plugins, and says so
/// in [`OfflineCheck::skipped`]. Used by the MCP server (`aether --mcp`) so a plugin can be checked on
/// a machine that has no database. Like a real load it stops at the first problem.
pub async fn check_plugin_offline(plugin_path: &Path) -> Result<OfflineCheck, CatalogError> {
    let manifest_path = if plugin_path.is_dir() {
        plugin_path.join(MANIFEST_FILE)
    } else {
        plugin_path.to_path_buf()
    };
    let manifest_path = tokio::fs::canonicalize(&manifest_path)
        .await
        .map_err(io_error(&manifest_path))?;
    let package_dir = manifest_path
        .parent()
        .ok_or_else(|| CatalogError::InvalidManifestPath(manifest_path.clone()))?
        .to_path_buf();
    let manifest_text = tokio::fs::read_to_string(&manifest_path)
        .await
        .map_err(io_error(&manifest_path))?;
    let manifest =
        PluginManifest::parse(&manifest_text).map_err(|source| CatalogError::Manifest {
            path: manifest_path.clone(),
            source,
        })?;
    if manifest.plugin.name.trim().is_empty() || manifest.plugin.version.trim().is_empty() {
        return Err(CatalogError::IncompleteManifest(manifest_path));
    }

    let components = discover_components(&package_dir).await?;
    let pages = discover_pages(&package_dir).await?;
    let layouts = discover_layouts(&package_dir).await?;
    let error_sets = discover_error_sets(&package_dir).await?;
    let screens = super::super::screens::discover_screens(&package_dir).await?;
    let model_files = read_models(&package_dir)?;
    let models: Vec<ModelDef> = model_files.iter().map(|(_, model)| model.clone()).collect();
    validate_page_models(&manifest, &models, &pages)?;
    validate_component_models(&models, &components)?;
    validate_granted_models(&manifest, &models, &manifest_path)?;
    validate_capabilities(&manifest, &manifest_path)?;

    let mut theme = read_theme(&manifest, &manifest_path, &package_dir).await?;
    if let Some(theme) = theme.as_mut() {
        bind_theme_shell_defs(theme, &layouts, &error_sets, &screens, &manifest_path)?;
    }
    validate_page_layouts(&pages, &layouts)?;
    read_app(&manifest, &manifest_path, &pages)?;
    let rule_files = read_rule_files(&package_dir).await?;
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

    for declared in declared_files(&manifest) {
        resolve_package_file(&package_dir, declared).await?;
    }
    collect_asset_files(&package_dir).await?;
    if let Some(icon) = manifest.plugin.icon_path.as_deref().filter(|path| !path.is_empty()) {
        resolve_package_file(&package_dir, icon).await?;
    }
    if let Some(found) = super::super::i18n::read_catalogs(&package_dir, &manifest)
        .await
        .map_err(CatalogError::I18n)?
    {
        super::super::i18n::check_overrides(&manifest, &found.catalogs).map_err(CatalogError::I18n)?;
    }

    check_one_code_file(&manifest, &manifest_path)?;
    let docs = super::super::docs::read(&package_dir, &manifest).map_err(|problems| CatalogError::Docs {
        plugin: manifest.plugin.name.clone(),
        problems: problems.join("; "),
    })?;
    let mut skipped = vec![
        "links from this plugin's models to models of other plugins (need the catalog)",
        "components used from other plugins (need the catalog)",
        "model ids already taken in the catalog",
    ];
    if let Some(code_file) = manifest.plugin.code_file() {
        // A compiled plugin is checked before it is built; only a script is read here.
        if !manifest.plugin.is_script() && resolve_package_file(&package_dir, code_file).await.is_err() {
            skipped.push("the WebAssembly module is not built yet (see the README), so it was not checked");
        }
    }
    if let Some(code_file) = manifest.plugin.code_file().filter(|_| manifest.plugin.is_script()) {
        let source = package_dir.join(resolve_package_file(&package_dir, code_file).await?);
        if manifest.plugin.is_script() {
            let bytes = tokio::fs::read(&source).await.map_err(io_error(&source))?;
            check_script(&source, &String::from_utf8_lossy(&bytes))?;
        }
    }

    Ok(OfflineCheck {
        name: manifest.plugin.name.clone(),
        version: manifest.plugin.version.clone(),
        models: models.iter().map(|model| model.name.clone()).collect(),
        pages: pages.len(),
        components: components.len(),
        doc_pages: docs.map(|docs| docs.pages.len()),
        skipped,
    })
}

/// `plugin.model` → `model_id` for every package in `plugin_paths` that has models on disk.
/// Used by `--dry-run` so foreign links can resolve against the batch, not only the catalog.
pub fn batch_model_ids(plugin_paths: &[PathBuf]) -> Result<HashMap<String, String>, CatalogError> {
    let mut found = HashMap::new();
    for plugin_path in plugin_paths {
        let manifest_path = if plugin_path.is_dir() {
            plugin_path.join(MANIFEST_FILE)
        } else {
            plugin_path.to_path_buf()
        };
        let package_dir = manifest_path
            .parent()
            .ok_or_else(|| CatalogError::InvalidManifestPath(manifest_path.clone()))?;
        let Ok(manifest_text) = std::fs::read_to_string(&manifest_path) else {
            continue;
        };
        let Ok(manifest) = PluginManifest::parse(&manifest_text) else {
            continue;
        };
        let Ok(model_files) = read_models(package_dir) else {
            continue;
        };
        for (_, model) in model_files {
            if let Some(id) = model.model_id {
                found.insert(format!("{}.{}", manifest.plugin.name, model.name), id);
            }
        }
    }
    Ok(found)
}

/// A page's `layout=` must be a built-in name or one this package defines.
pub(super) fn validate_page_layouts(
    pages: &[PageDocument],
    layouts: &[LayoutDef],
) -> Result<(), CatalogError> {
    let known = layouts_by_name(layouts);
    for page in pages {
        let Some(layout) = page.layout.as_deref() else {
            continue;
        };
        if is_builtin_layout(layout) || known.contains_key(layout) {
            continue;
        }
        return Err(CatalogError::UnknownShellDef {
            path: page.source.clone(),
            field: "layout",
            name: layout.to_string(),
            folder: "layouts",
        });
    }
    Ok(())
}

/// Every model a page names must be one the plugin defines in `models/`. A page may write the
/// model as `workspace.name`; only the part after the last `.` is the model name.
pub(super) fn validate_page_models(
    manifest: &PluginManifest,
    models: &[ModelDef],
    pages: &[PageDocument],
) -> Result<(), CatalogError> {
    let _ = manifest;
    let declared: BTreeSet<&str> = models.iter().map(|model| model.name.as_str()).collect();
    for page in pages {
        for model in &page.models {
            if !declared.contains(model_name(model)) {
                return Err(CatalogError::UnknownPageModel {
                    page: page.source.clone(),
                    model: model.clone(),
                });
            }
        }
    }
    Ok(())
}

/// Every model a component names must be one the plugin defines in `models/`.
pub(super) fn validate_component_models(
    models: &[ModelDef],
    components: &[ComponentDocument],
) -> Result<(), CatalogError> {
    let declared: BTreeSet<&str> = models.iter().map(|model| model.name.as_str()).collect();
    for component in components {
        for model in &component.models {
            if !declared.contains(model_name(model)) {
                return Err(CatalogError::UnknownComponentModel {
                    path: component.source.clone(),
                    model: model.clone(),
                });
            }
        }
    }
    Ok(())
}

/// Every `<use component="…">` in pages and components must resolve: bare names
/// against this package, qualified names against a declared dependency that is
/// already in the catalog.
pub(super) async fn validate_component_uses(
    db: &Surreal<Client>,
    namespace: &str,
    core_database: &str,
    manifest: &PluginManifest,
    components: &[ComponentDocument],
    pages: &[PageDocument],
) -> Result<(), CatalogError> {
    let local: BTreeSet<&str> = components
        .iter()
        .map(|component| component.name.as_str())
        .collect();
    let plugin = &manifest.plugin;
    let mut foreign: BTreeSet<(String, String)> = BTreeSet::new();

    let mut check_tree = |path: &Path, tree: &serde_json::Value| -> Result<(), CatalogError> {
        for (reference, raw) in collect_uses(tree) {
            if reference.name.is_empty() {
                return Err(CatalogError::BadComponentUse {
                    path: path.to_path_buf(),
                    reference: raw,
                    reason: "invalid component name".into(),
                });
            }
            match &reference.plugin {
                None => {
                    if !local.contains(reference.name.as_str()) {
                        return Err(CatalogError::BadComponentUse {
                            path: path.to_path_buf(),
                            reference: raw,
                            reason: format!(
                                "this plugin has no component `{}`; define it under `components/`",
                                reference.name
                            ),
                        });
                    }
                }
                Some(other) if other == &plugin.name => {
                    if !local.contains(reference.name.as_str()) {
                        return Err(CatalogError::BadComponentUse {
                            path: path.to_path_buf(),
                            reference: raw,
                            reason: format!(
                                "this plugin has no component `{}`; define it under `components/`",
                                reference.name
                            ),
                        });
                    }
                }
                Some(other) => {
                    if !plugin
                        .dependencies
                        .iter()
                        .any(|dependency| dependency == other)
                    {
                        return Err(CatalogError::BadComponentUse {
                            path: path.to_path_buf(),
                            reference: raw,
                            reason: format!(
                                "`{other}` must be listed under `dependencies` in plugin.toml"
                            ),
                        });
                    }
                    foreign.insert((other.clone(), reference.name.clone()));
                }
            }
        }
        Ok(())
    };

    for component in components {
        check_tree(&component.source, &component.tree)?;
    }
    for page in pages {
        check_tree(&page.source, &page.tree)?;
    }
    drop(check_tree);

    for page in pages {
        let Some(wrap) = page.wrap.as_deref() else {
            continue;
        };
        let reference =
            ComponentRef::parse(wrap).map_err(|reason| CatalogError::BadComponentUse {
                path: page.source.clone(),
                reference: wrap.to_string(),
                reason,
            })?;
        match &reference.plugin {
            None => {
                if !local.contains(reference.name.as_str()) {
                    return Err(CatalogError::BadComponentUse {
                        path: page.source.clone(),
                        reference: wrap.to_string(),
                        reason: format!(
                            "wrap `{}`: this plugin has no component `{}`; define it under `components/`",
                            wrap, reference.name
                        ),
                    });
                }
            }
            Some(other) if other == &plugin.name => {
                if !local.contains(reference.name.as_str()) {
                    return Err(CatalogError::BadComponentUse {
                        path: page.source.clone(),
                        reference: wrap.to_string(),
                        reason: format!(
                            "wrap `{}`: this plugin has no component `{}`; define it under `components/`",
                            wrap, reference.name
                        ),
                    });
                }
            }
            Some(other) => {
                if !plugin
                    .dependencies
                    .iter()
                    .any(|dependency| dependency == other)
                {
                    return Err(CatalogError::BadComponentUse {
                        path: page.source.clone(),
                        reference: wrap.to_string(),
                        reason: format!(
                            "wrap `{wrap}`: `{other}` must be listed under `dependencies` in plugin.toml"
                        ),
                    });
                }
                foreign.insert((other.clone(), reference.name.clone()));
            }
        }
    }

    let mut registry: ComponentRegistry = registry_from_documents(&plugin.name, components);
    if !foreign.is_empty() {
        db.use_ns(namespace).await?;
        db.use_db(core_database).await?;
        for (other_plugin, name) in &foreign {
            let mut response = db
                .query(
                    "SELECT full_name, component_tree FROM plugin_components WHERE name = $name \
                     AND plugin IN (SELECT VALUE id FROM plugins WHERE name = $plugin AND is_active = true) LIMIT 1;",
                )
                .bind(("name", name.clone()))
                .bind(("plugin", other_plugin.clone()))
                .await?
                .check()?;
            let rows: Vec<ForeignComponentRow> = response.take(0)?;
            let Some(row) = rows.into_iter().next() else {
                return Err(CatalogError::BadComponentUse {
                    path: PathBuf::from("plugin.toml"),
                    reference: format!("{other_plugin}.{name}"),
                    reason: format!(
                        "the catalog has no such component: load `{other_plugin}` first (`aether --load-plugins`)"
                    ),
                });
            };
            registry.insert(row.full_name, row.component_tree);
        }
    }

    // Expand once at load so cycles and missing nested uses fail before install.
    for component in components {
        expand_tree(&component.tree, &plugin.name, &registry).map_err(|error| {
            CatalogError::BadComponentUse {
                path: component.source.clone(),
                reference: component.name.clone(),
                reason: error.to_string(),
            }
        })?;
    }
    for page in pages {
        expand_tree(&page.tree, &plugin.name, &registry).map_err(|error| {
            CatalogError::BadComponentUse {
                path: page.source.clone(),
                reference: page.route.clone(),
                reason: error.to_string(),
            }
        })?;
    }
    Ok(())
}

/// The models a plugin is granted access to must exist.
pub(super) fn validate_granted_models(
    manifest: &PluginManifest,
    models: &[ModelDef],
    manifest_path: &Path,
) -> Result<(), CatalogError> {
    let declared: BTreeSet<&str> = models.iter().map(|model| model.name.as_str()).collect();
    let plugin = &manifest.plugin;
    for (list, entries) in [
        ("access_models", &plugin.access_models),
        ("public_access_models", &plugin.public_access_models),
    ] {
        if let Some(entry) = entries
            .iter()
            .find(|entry| !declared.contains(entry.name.as_str()))
        {
            return Err(CatalogError::UnknownGrantedModel {
                path: manifest_path.to_path_buf(),
                name: entry.name.clone(),
                list,
            });
        }
    }
    Ok(())
}

/// `base.currency` -> `currency`.
pub fn model_name(reference: &str) -> &str {
    reference.rsplit('.').next().unwrap_or(reference)
}

/// Links to another plugin's model (`"target": "currency.currency"`): the plugin must list that plugin
/// as a dependency, and the catalog must have the model under the `target_id` written in the field,
/// so a typo or a model that moved is found when the plugin is loaded, not when it is used.
pub(super) async fn check_foreign_links(
    db: &Surreal<Client>,
    namespace: &str,
    core_database: &str,
    plugin: &super::super::models::plugin_def::PluginDefinition,
    models: &[ModelDef],
    batch_models: &HashMap<String, String>,
) -> Result<(), CatalogError> {
    let mut wanted: Vec<(String, String, String, String)> = Vec::new();
    for model in models {
        for field in model.live_fields() {
            let Some(target) = field.target.as_deref() else {
                continue;
            };
            let Some((other_plugin, other_model)) = crate::data_model::foreign_target(target)
            else {
                continue;
            };
            if other_plugin != plugin.name
                && !plugin
                    .dependencies
                    .iter()
                    .any(|dependency| dependency == other_plugin)
            {
                return Err(CatalogError::ForeignLink(format!(
                    "{}.{} links to `{target}`, so `{other_plugin}` must be listed under `dependencies` in plugin.toml",
                    model.name, field.name
                )));
            }
            wanted.push((
                format!("{}.{}", model.name, field.name),
                other_plugin.to_string(),
                other_model.to_string(),
                field.target_id.clone().unwrap_or_default(),
            ));
        }
    }
    if wanted.is_empty() {
        return Ok(());
    }
    db.use_ns(namespace).await?;
    db.use_db(core_database).await?;
    for (field, other_plugin, other_model, id) in wanted {
        // Links inside this package: the target is not in the catalog yet while we load.
        if other_plugin == plugin.name {
            let Some(own) = models.iter().find(|model| model.name == other_model) else {
                return Err(CatalogError::ForeignLink(format!(
                    "{field} links to `{other_plugin}.{other_model}`, but this package has no such model"
                )));
            };
            match own.model_id.as_deref() {
                Some(own_id) if own_id == id => continue,
                Some(own_id) => {
                    return Err(CatalogError::ForeignLink(format!(
                        "{field}: `target_id` `{id}` is not the id of `{other_plugin}.{other_model}` (package has `{own_id}`); run `aether --sync-models` again"
                    )));
                }
                None => {
                    return Err(CatalogError::ForeignLink(format!(
                        "{field} links to `{other_plugin}.{other_model}`, which has no `model_id`; run `aether --sync-models`"
                    )));
                }
            }
        }

        let mut response = db
            .query(
                "SELECT VALUE model_id FROM plugin_models WHERE name = $model \
                 AND plugin IN (SELECT VALUE id FROM plugins WHERE name = $plugin AND is_active = true);",
            )
            .bind(("model", other_model.clone()))
            .bind(("plugin", other_plugin.clone()))
            .await?
            .check()?;
        let ids: Vec<Option<String>> = response.take(0)?;
        let key = format!("{other_plugin}.{other_model}");
        let known: Vec<&str> = if ids.is_empty() {
            match batch_models.get(&key).map(String::as_str) {
                Some(batch_id) => vec![batch_id],
                None => {
                    return Err(CatalogError::ForeignLink(format!(
                        "{field} links to `{other_plugin}.{other_model}`, but the catalog has no such model: load `{other_plugin}` first (`aether --load-plugins`)"
                    )));
                }
            }
        } else {
            ids.iter().flatten().map(String::as_str).collect()
        };
        if !known.iter().any(|known_id| *known_id == id) {
            return Err(CatalogError::ForeignLink(format!(
                "{field}: `target_id` `{id}` is not the id of `{other_plugin}.{other_model}`; run `aether --sync-models` again"
            )));
        }
    }
    Ok(())
}

/// The ids of other plugins' models, by `plugin.model`, for `aether --sync-models` to write into links.
/// A name the catalog does not have is simply missing from the answer.
pub async fn foreign_model_ids(
    db: &Surreal<Client>,
    namespace: &str,
    core_database: &str,
    names: &std::collections::BTreeSet<String>,
) -> Result<HashMap<String, String>, CatalogError> {
    db.use_ns(namespace).await?;
    db.use_db(core_database).await?;
    let mut found = HashMap::new();
    for name in names {
        let Some((plugin, model)) = crate::data_model::foreign_target(name) else {
            continue;
        };
        let mut response = db
            .query(
                "SELECT VALUE model_id FROM plugin_models WHERE name = $model \
                 AND plugin IN (SELECT VALUE id FROM plugins WHERE name = $plugin AND is_active = true) LIMIT 1;",
            )
            .bind(("model", model.to_string()))
            .bind(("plugin", plugin.to_string()))
            .await?
            .check()?;
        let ids: Vec<Option<String>> = response.take(0)?;
        if let Some(Some(id)) = ids.into_iter().next() {
            found.insert(name.clone(), id);
        }
    }
    Ok(found)
}

/// Fail when upgrading `plugin` would drop a component that another installed
/// plugin still references. Caller must be on the core database.
pub(super) async fn check_component_consumers(
    db: &Surreal<Client>,
    others: &HashMap<String, String>,
    plugin: &str,
    target: &PluginDbDefinition,
) -> Result<(), CatalogError> {
    if others.is_empty() {
        return Ok(());
    }
    let mut response = db
        .query("SELECT VALUE name FROM plugin_components WHERE plugin = $plugin;")
        .bind(("plugin", target.id.clone()))
        .await?
        .check()?;
    let provided: HashSet<String> = response.take::<Vec<String>>(0)?.into_iter().collect();

    let mut response = db
        .query(
            "SELECT component_tree, plugin.name AS plugin_name, plugin.version AS plugin_version \
             FROM plugin_ui_pages; \
             SELECT component_tree, plugin.name AS plugin_name, plugin.version AS plugin_version \
             FROM plugin_components;",
        )
        .await?
        .check()?;
    let mut trees: Vec<TreeOwnerRow> = response.take(0)?;
    let more: Vec<TreeOwnerRow> = response.take(1)?;
    trees.extend(more);

    let prefix = format!("{plugin}.");
    for row in trees {
        let serves = others
            .get(&row.plugin_name)
            .is_some_and(|version| *version == row.plugin_version);
        if !serves {
            continue;
        }
        for (reference, raw) in collect_uses(&row.component_tree) {
            let owner = reference
                .plugin
                .as_deref()
                .unwrap_or(row.plugin_name.as_str());
            if owner != plugin {
                continue;
            }
            if !provided.contains(&reference.name) {
                return Err(CatalogError::ComponentStillUsed {
                    plugin: plugin.to_string(),
                    component: if raw.starts_with(&prefix) {
                        raw
                    } else {
                        format!("{plugin}.{}", reference.name)
                    },
                    consumer: row.plugin_name,
                });
            }
        }
    }
    Ok(())
}

/// A made-up route of a page that only extends another page (never served on its own).
pub(super) fn is_extension_shape(shape: &str) -> bool {
    shape.starts_with(&format!(
        "{}/",
        crate::plugin_manager::pages::EXTENSION_ROUTE_PREFIX
    ))
}

/// Fail when a page route of a plugin about to be installed would match the
/// same URLs as a route already served in the organization (by an installed
/// plugin, or by another plugin in the same install). Routes conflict when
/// they have the same shape: `/chat/{channel}` and `/chat/{room}` do,
/// `/chat/new` and `/chat/{channel}` do not (the literal wins). Caller must be
/// on the core database.
pub(super) async fn check_route_conflicts(
    db: &Surreal<Client>,
    installed: &HashMap<String, String>,
    order: &[PluginDbDefinition],
) -> Result<(), CatalogError> {
    if order.is_empty() {
        return Ok(());
    }
    let ids: Vec<_> = order.iter().map(|record| record.id.clone()).collect();
    let mut response = db
        .query("SELECT route, route_shape, plugin FROM plugin_ui_pages WHERE plugin IN $ids;")
        .bind(("ids", ids))
        .await?
        .check()?;
    let mut planned: Vec<PlannedPageRoute> = response.take(0)?;
    planned.retain(|page| !is_extension_shape(&page.route_shape));
    if planned.is_empty() {
        return Ok(());
    }

    let name_of = |id: &surrealdb::types::RecordId| {
        order
            .iter()
            .find(|record| &record.id == id)
            .map(|record| record.name.clone())
            .unwrap_or_default()
    };

    let mut owners: HashMap<&str, String> = HashMap::new();
    for page in &planned {
        let owner = name_of(&page.plugin);
        match owners.get(page.route_shape.as_str()) {
            Some(existing) if *existing != owner => {
                return Err(CatalogError::RouteConflict {
                    route: page.route.clone(),
                    existing: existing.clone(),
                    plugin: owner,
                });
            }
            _ => {
                owners.insert(page.route_shape.as_str(), owner);
            }
        }
    }

    let shapes: Vec<String> = owners.keys().map(|shape| shape.to_string()).collect();
    let mut response = db
        .query(
            "SELECT route, route_shape, plugin.name AS plugin_name, plugin.version AS plugin_version \
             FROM plugin_ui_pages WHERE route_shape IN $shapes;",
        )
        .bind(("shapes", shapes))
        .await?
        .check()?;
    let claimed: Vec<ClaimedRoute> = response.take(0)?;
    for claim in claimed {
        if is_extension_shape(&claim.route_shape) {
            continue;
        }
        let serves_this_org = installed
            .get(&claim.plugin_name)
            .is_some_and(|version| *version == claim.plugin_version);
        if serves_this_org && let Some(owner) = owners.get(claim.route_shape.as_str()) {
            return Err(CatalogError::RouteConflict {
                route: claim.route,
                existing: claim.plugin_name,
                plugin: owner.clone(),
            });
        }
    }
    Ok(())
}
