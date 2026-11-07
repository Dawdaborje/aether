//! Part of the plugin catalog; see [`super`].

use super::stage::{Package, read_package};
use super::*;

/// Register the plugin package at `plugin_path` in the core catalog.
///
/// `plugin_path` is a package directory (or its `plugin.toml`) anywhere on
/// disk. Pages are the XML files under `pages/`; each is parsed and validated
/// here, so a malformed page or a duplicate route fails the load. The files a
/// plugin ships (`plugin.toml`, page XML, model and theme files, and the WASM
/// artifact, which may sit in a build folder such as `out/` and is stored
/// beside `plugin.toml`) are stored under `<app_dir>/plugins/<name>/`, and each page is
/// compiled to JSON under `<app_dir>/views/`. The catalog records the plugin and its pages
/// together.
///
/// Files are stored as revisions (see [`super::super::revisions`]): they are compared by hash with
/// the plugin's latest revision, and only the ones that differ are written, into a new
/// folder named after the date and time. Loading identical content again is a no-op; loading
/// changed content under a version that is already catalogued adds `<version>+<stamp>`, so a
/// rebuild needs no version bump. A plugin may have no WASM artifact (a pure UI plugin).
pub async fn load_plugin(
    db: &Surreal<Client>,
    namespace: &str,
    core_database: &str,
    app_dir: &Path,
    plugin_path: &Path,
) -> Result<LoadedPlugin, CatalogError> {
    load_plugin_with_batch(
        db,
        namespace,
        core_database,
        app_dir,
        plugin_path,
        &HashMap::new(),
    )
    .await
}

/// Like [`load_plugin`], but foreign links may also resolve against `batch_models`
/// (`plugin.model` → `model_id` for other packages in the same `--load-plugins` run).
pub async fn load_plugin_with_batch(
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
        false,
        batch_models,
    )
    .await
}


pub(super) async fn load_plugin_inner(
    db: &Surreal<Client>,
    namespace: &str,
    core_database: &str,
    app_dir: &Path,
    plugin_path: &Path,
    dry_run: bool,
    batch_models: &HashMap<String, String>,
) -> Result<LoadedPlugin, CatalogError> {
    let layout = AppDir::new(app_dir);
    layout.ensure().await?;

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

    // Nothing on disk changed since the last load: say so before parsing or validating anything.
    // The hash covers every file of the package folder (not only the ones that are stored), so any
    // edit takes the full path below, which then decides whether the stored content differs.
    let source_hash: Option<String> = if dry_run {
        None
    } else {
        let tree = hash_package_tree(&package_dir).await?;
        if let Some(done) =
            unchanged_since_last_load(db, namespace, core_database, &layout, &manifest, &tree).await?
        {
            return Ok(done);
        }
        Some(tree)
    };

    let package = read_package(
        db,
        namespace,
        core_database,
        &manifest_path,
        &package_dir,
        manifest,
        batch_models,
    )
    .await?;

    db.use_ns(namespace).await?;
    db.use_db(core_database).await?;

    // Everything this plugin already has in the catalog, newest first.
    let mut response = db
        .query("SELECT version, content_hash, revision, date_created FROM plugins WHERE name = $name ORDER BY date_created DESC;")
        .bind(("name", package.manifest.plugin.name.clone()))
        .await?
        .check()?;
    let known: Vec<KnownVersion> = response.take(0)?;

    check_model_ids_free(db, &package).await?;

    if dry_run {
        return Ok(package.summary(
            package.manifest.plugin.version.clone(),
            package.artifact_logical.clone(),
            false,
            None,
            0,
            0,
        ));
    }

    super::settings::seed_plugin_settings(db, &package.manifest).await?;

    // The same content again is not a new revision.
    if let Some(same) = known
        .iter()
        .filter(|row| base_version(&row.version) == package.manifest.plugin.version)
        .find(|row| row.content_hash.as_deref() == Some(package.content_hash.as_str()))
    {
        return reuse_stored(db, &layout, &package, same, source_hash.as_deref()).await;
    }

    store_new(db, &layout, package, &known, source_hash).await
}

/// A model id belongs to one plugin, in every version of it.
async fn check_model_ids_free(db: &Surreal<Client>, package: &Package) -> Result<(), CatalogError> {
    let models = &package.models;
    if models.is_empty() {
        return Ok(());
    }
    let ids: Vec<String> = models
        .iter()
        .filter_map(|model| model.model_id.clone())
        .collect();
    let mut response = db
        .query("SELECT model_id, plugin.name AS plugin FROM plugin_models WHERE model_id IN $ids;")
        .bind(("ids", ids))
        .await?
        .check()?;
    let taken: Vec<ModelIdOwner> = response.take(0)?;
    if let Some(other) = taken
        .into_iter()
        .find(|owner| owner.plugin != package.manifest.plugin.name)
    {
        let model = models
            .iter()
            .find(|model| model.model_id.as_deref() == Some(other.model_id.as_str()))
            .map_or_else(String::new, |model| model.name.clone());
        return Err(CatalogError::ModelIdTaken {
            model_id: other.model_id,
            model,
            plugin: other.plugin,
        });
    }
    Ok(())
}

/// The content is already catalogued as `same`: repair its folder if damaged, remember the
/// source hash, and report it as not created.
async fn reuse_stored(
    db: &Surreal<Client>,
    layout: &AppDir,
    package: &Package,
    same: &KnownVersion,
    source_hash: Option<&str>,
) -> Result<LoadedPlugin, CatalogError> {
    let name = &package.manifest.plugin.name;
    let artifact_path = match &same.revision {
        Some(revision) => {
            let index = revisions::read_index(layout, name, revision).await?;
            // Only repair `app_dir` if its copy has gone missing or been damaged.
            if let Some(index) = &index {
                if !revisions::verify(layout, index).await?.is_empty() {
                    revisions::restore(layout, index, &package.contents).await?;
                }
            }
            package
                .artifact_logical
                .as_ref()
                .and_then(|logical| {
                    index
                        .as_ref()?
                        .files
                        .get(logical)
                        .map(|entry| (logical, entry))
                })
                .map(|(logical, entry)| revisions::relative_path(name, entry, logical))
                .map(|path| path.to_string_lossy().into_owned())
        }
        // Stored in the older layout: files are in plugins/<name>/<version>/.
        None => package
            .artifact_name
            .as_ref()
            .map(|file| {
                AppDir::plugin_relative_dir(name, &same.version)
                    .map(|dir| dir.join(file).to_string_lossy().into_owned())
            })
            .transpose()?,
    };
    // Remember what the folder hashed to, so the next load can skip all of the above.
    if let Some(hash) = source_hash {
        db.query("UPDATE plugins SET source_hash = $hash WHERE name = $name AND version = $version;")
            .bind(("hash", hash.to_string()))
            .bind(("name", name.clone()))
            .bind(("version", same.version.clone()))
            .await?
            .check()?;
    }
    Ok(package.summary(same.version.clone(), artifact_path, false, same.revision.clone(), 0, 0))
}

/// New content: store a revision (only the files that differ from the latest one are written),
/// then write the catalog rows for it in one transaction.
async fn store_new(
    db: &Surreal<Client>,
    layout: &AppDir,
    mut package: Package,
    known: &[KnownVersion],
    source_hash: Option<String>,
) -> Result<LoadedPlugin, CatalogError> {
    let theme = package.theme.take();
    let app = package.app.take();
    let wasm_hash = package.wasm_hash.take();
    let is_system_theme = package.manifest.theme.as_ref().is_some_and(|theme| theme.is_system);
    let manifest = &package.manifest;
    let definition = &manifest.plugin;
    let package_dir = &package.package_dir;
    let (models, pages, components) = (&package.models, &package.pages, &package.components);
    let (layouts, error_sets) = (&package.layouts, &package.error_sets);
    let (contents, artifact_logical) = (&package.contents, &package.artifact_logical);
    let (text_catalogs, docs) = (&package.text_catalogs, &package.docs);
    let schedules: Vec<serde_json::Value> = manifest
        .schedule
        .iter()
        .filter_map(|task| serde_json::to_value(task).ok())
        .collect();
    let commands: Vec<serde_json::Value> = manifest
        .command
        .iter()
        .filter_map(|command| serde_json::to_value(command).ok())
        .collect();
    let watches: Vec<serde_json::Value> = manifest
        .watch
        .iter()
        .filter_map(|watch| serde_json::to_value(watch).ok())
        .collect();
    let roles: Vec<serde_json::Value> = manifest
        .roles
        .iter()
        .filter_map(|role| serde_json::to_value(role).ok())
        .collect();
    let ai_tools: Vec<serde_json::Value> = manifest
        .ai_tool
        .iter()
        .filter_map(|tool| serde_json::to_value(tool).ok())
        .collect();
    let event_listeners: Vec<serde_json::Value> = manifest
        .events
        .iter()
        .filter(|event| event.direction() == "listen")
        .map(|event| {
            serde_json::json!({
                "event": event.name,
                "function": event.handler,
                "queue": event.queue.clone().unwrap_or_else(|| "default".into()),
                "max_attempts": event.max_attempts.unwrap_or(3),
            })
        })
        .collect();

    let previous = match known.first() {
        Some(latest) => {
            let folder = latest
                .revision
                .clone()
                .unwrap_or_else(|| latest.version.clone());
            revisions::read_index(layout, &definition.name, &folder)
                .await?
                .map(|index| (folder, index))
        }
        None => None,
    };
    let stored = revisions::store(
        layout,
        &definition.name,
        &definition.version,
        previous
            .as_ref()
            .map(|(folder, index)| (folder.as_str(), index)),
        &contents,
    )
    .await?;

    // The first load of a version keeps the version as written; a rebuild of one that is
    // already catalogued is `<version>+<stamp>`, so nothing needs a manual version bump.
    let catalog_version = if known
        .iter()
        .any(|row| base_version(&row.version) == definition.version)
    {
        format!("{}+{}", definition.version, stored.revision)
    } else {
        definition.version.clone()
    };
    let artifact_path = artifact_logical
        .as_ref()
        .and_then(|logical| {
            stored
                .index
                .files
                .get(logical)
                .map(|entry| (logical, entry))
        })
        .map(|(logical, entry)| {
            revisions::relative_path(&definition.name, entry, logical)
                .to_string_lossy()
                .into_owned()
        });
    write_view_files(layout, &definition.name, &catalog_version, &pages).await?;

    let record = NewPlugin {
        name: definition.name.clone(),
        label: if definition.label.is_empty() {
            definition.name.clone()
        } else {
            definition.label.clone()
        },
        version: catalog_version.clone(),
        description: definition.description.clone(),
        long_description: definition.long_description.clone(),
        icon_path: definition.icon_path.clone(),
        website: definition.website.clone(),
        authors: definition
            .authors
            .iter()
            .map(|author| PluginDbAuthor {
                name: author.name.clone(),
                email: author.email.clone(),
                github: author.github.clone(),
                website: author.website.clone(),
            })
            .collect(),
        categories: definition
            .categories
            .iter()
            .map(|category| PluginDbCategory {
                name: category.name.clone(),
                label: category.label.clone(),
            })
            .collect(),
        dependencies: definition.dependencies.clone(),
        workspace: definition.workspace.clone(),
        kind: definition.kind.clone(),
        artifact_path: artifact_path.clone(),
        artifact_hash: wasm_hash,
        content_hash: package.content_hash.clone(),
        source_hash: source_hash.clone(),
        revision: Some(stored.revision.clone()),
        source_path: Some(package_dir.to_string_lossy().into_owned()),
        app,
        public_functions: definition.public_functions.clone(),
        schedules,
        commands,
        i18n: text_catalogs
            .as_ref()
            .map(|found| super::super::i18n::to_value(&found.catalogs)),
        watches,
        event_listeners,
        roles,
        ai_tools,
        docs: docs.as_ref().and_then(|docs| serde_json::to_value(docs).ok()),
        is_builtin: definition.is_builtin,
        is_active: true,
    };
    let model_rows: Vec<NewModelRow> = models
        .iter()
        .map(|model| NewModelRow {
            name: model.name.clone(),
            model_id: model.model_id.clone().unwrap_or_default(),
            label: model.label.clone(),
            definition: serde_json::to_value(model).unwrap_or(serde_json::Value::Null),
        })
        .collect();
    let plugin_name = definition.name.as_str();
    let rows: Vec<NewUiPage> = pages
        .iter()
        .map(|page| NewUiPage {
            layout: page.layout.clone(),
            wrap: page.wrap.clone(),
            route: page.route.clone(),
            is_pattern: page.is_pattern,
            route_shape: page.shape.clone(),
            title: page.title.clone(),
            model: page.model.clone(),
            models: page.models.iter().cloned().collect(),
            is_public: page.public,
            is_menu_visible: page.menu_visible,
            icon: page.icon.clone(),
            menu_group: page.menu_group.clone(),
            menu_group_icon: page.menu_group_icon.clone(),
            sort_order: page.sort_order,
            required_roles: page
                .roles
                .iter()
                .map(|role| crate::roles::qualify_role(plugin_name, role))
                .filter(|role| !role.is_empty())
                .collect(),
            component_tree: page.tree.clone(),
            source_path: page.source.to_string_lossy().into_owned(),
        })
        .collect();
    let component_rows: Vec<NewUiComponent> = components
        .iter()
        .map(|component| NewUiComponent {
            name: component.name.clone(),
            full_name: format!("{}.{}", definition.name, component.name),
            component_tree: component.tree.clone(),
            source_path: component.source.to_string_lossy().into_owned(),
        })
        .collect();
    db.query(
        r#"
        BEGIN TRANSACTION;
        LET $plugin = (CREATE ONLY plugins CONTENT $record).id;
        IF $theme != NONE {
            CREATE plugin_themes CONTENT {
                plugin: $plugin,
                name: $theme.name,
                label: $theme.label,
                is_system: $theme.is_system,
                color_mode: $theme.color_mode,
                tokens: $theme.tokens,
                layout: $theme.layout,
                error_pages: $theme.error_pages,
                nav: $theme.nav,
                layout_def: $theme.layout_def,
                error_pages_def: $theme.error_pages_def,
                shell: $theme.shell
            };
        };
        FOR $layout IN $layouts {
            CREATE plugin_layouts CONTENT {
                plugin: $plugin,
                name: $layout.name,
                label: $layout.label,
                chrome: $layout.chrome,
                definition: $layout.definition,
                source_path: $layout.source_path
            };
        };
        FOR $error_set IN $error_sets {
            CREATE plugin_error_sets CONTENT {
                plugin: $plugin,
                name: $error_set.name,
                label: $error_set.label,
                variant: $error_set.variant,
                definition: $error_set.definition,
                source_path: $error_set.source_path
            };
        };
        FOR $m IN $models {
            CREATE plugin_models CONTENT {
                plugin: $plugin,
                name: $m.name,
                table_name: $m.model_id,
                model_id: $m.model_id,
                label: $m.label,
                definition: $m.definition
            };
        };
        FOR $page IN $pages {
            CREATE plugin_ui_pages CONTENT {
                plugin: $plugin,
                route: $page.route,
                layout: $page.layout,
                wrap: $page.wrap,
                is_pattern: $page.is_pattern,
                route_shape: $page.route_shape,
                title: $page.title,
                model: $page.model,
                models: $page.models,
                is_public: $page.is_public,
                is_menu_visible: $page.is_menu_visible,
                icon: $page.icon,
                menu_group: $page.menu_group,
                menu_group_icon: $page.menu_group_icon,
                sort_order: $page.sort_order,
                required_roles: $page.required_roles,
                component_tree: $page.component_tree,
                source_path: $page.source_path
            };
        };
        FOR $component IN $components {
            CREATE plugin_components CONTENT {
                plugin: $plugin,
                name: $component.name,
                full_name: $component.full_name,
                component_tree: $component.component_tree,
                source_path: $component.source_path
            };
        };
        COMMIT TRANSACTION;
        "#,
    )
    .bind(("record", record))
    .bind((
        "theme",
        theme.map(|theme| NewTheme {
            name: theme.name,
            label: theme.label,
            is_system: is_system_theme,
            color_mode: theme.color_mode,
            tokens: theme.tokens,
            layout: theme.layout,
            error_pages: theme.error_pages,
            nav: theme.nav,
            layout_def: theme.layout_def,
            error_pages_def: theme.error_pages_def,
            shell: theme.shell,
        }),
    ))
    .bind(("pages", rows))
    .bind(("components", component_rows))
    .bind((
        "layouts",
        layouts
            .iter()
            .map(|layout| NewLayoutRow {
                name: layout.name.clone(),
                label: layout.label.clone(),
                chrome: layout.chrome.clone(),
                definition: layout.definition.clone(),
                source_path: layout.source.to_string_lossy().into_owned(),
            })
            .collect::<Vec<_>>(),
    ))
    .bind((
        "error_sets",
        error_sets
            .iter()
            .map(|set| NewErrorSetRow {
                name: set.name.clone(),
                label: set.label.clone(),
                variant: set.variant.clone(),
                definition: set.definition.clone(),
                source_path: set.source.to_string_lossy().into_owned(),
            })
            .collect::<Vec<_>>(),
    ))
    .bind(("models", model_rows))
    .await?
    .check()?;

    Ok(package.summary(
        catalog_version,
        artifact_path,
        true,
        Some(stored.revision),
        stored.written.len(),
        stored.reused.len(),
    ))
}
