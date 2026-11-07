//! Part of the plugin catalog; see [`super`].

use super::*;

/// Result of [`install_plugins`], in installation order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InstallReport {
    /// `(name, version)` pairs newly recorded for the organization.
    pub installed: Vec<(String, String)>,
    /// `(name, version)` pairs the organization already had.
    pub already_installed: Vec<(String, String)>,
    /// Themes that came with the installed plugins.
    pub themes: Vec<InstalledTheme>,
}

/// Install catalog plugins (and missing dependencies, dependencies first) for
/// the organization whose database is `org_database`.
pub async fn install_plugins(
    db: &Surreal<Client>,
    namespace: &str,
    core_database: &str,
    org_database: &str,
    specs: &[PluginSpec],
) -> Result<InstallReport, CatalogError> {
    db.use_ns(namespace).await?;
    db.use_db(core_database).await?;
    let mut response = db
        .query("SELECT db_name FROM org_databases WHERE db_name = $org LIMIT 1;")
        .bind(("org", org_database.to_string()))
        .await?
        .check()?;
    let orgs: Vec<OrgDatabaseRow> = response.take(0)?;
    if orgs.is_empty() {
        return Err(CatalogError::OrganizationNotFound(org_database.to_string()));
    }

    db.use_db(org_database).await?;
    let mut response = db
        .query("SELECT plugin_name, version FROM installed_plugins;")
        .await?
        .check()?;
    let installed_rows: Vec<InstalledRow> = response.take(0)?;
    let installed: HashMap<String, String> = installed_rows
        .into_iter()
        .map(|row| (row.plugin_name, row.version))
        .collect();

    db.use_db(core_database).await?;
    let mut plan = Plan {
        installed: &installed,
        order: Vec::new(),
        visiting: HashSet::new(),
        visited: HashSet::new(),
        already_installed: Vec::new(),
    };
    for spec in specs {
        plan.visit(db, spec, true).await?;
    }
    let Plan {
        order,
        already_installed,
        ..
    } = plan;
    check_route_conflicts(db, &installed, &order).await?;
    let themes = fetch_planned_themes(db, &order).await?;
    let planned_models = fetch_planned_models(db, &order).await?;

    db.use_db(org_database).await?;
    // Work out what every model needs before changing anything: one that cannot be applied
    // (a new required field with no default over existing records, ...) stops the install.
    let mut model_plans = Vec::with_capacity(order.len());
    for record in &order {
        let defs = models_of(&planned_models, &record.id);
        let plans = model_apply::plan_models(db, &record.name, &defs).await?;
        if let Some((model, problems)) = model_apply::blockers(&plans).into_iter().next() {
            return Err(model_apply::ApplyError::Blocked { model, problems }.into());
        }
        model_plans.push(plans);
    }
    let mut report = InstallReport {
        installed: Vec::with_capacity(order.len()),
        already_installed,
        themes: Vec::new(),
    };
    let mut has_active_theme = {
        let mut response = db
            .query("SELECT VALUE id FROM ui_theme_config LIMIT 1;")
            .await?
            .check()?;
        let configured: Vec<surrealdb::types::RecordId> = response.take(0)?;
        !configured.is_empty()
    };
    for (index, record) in order.into_iter().enumerate() {
        let dependencies = record.dependencies.clone().unwrap_or_default();
        let theme = themes
            .iter()
            .find(|theme| theme.plugin == record.id)
            .cloned();
        model_apply::apply_plans(db, &model_plans[index]).await?;
        db.query(
            r#"
            BEGIN TRANSACTION;
            CREATE type::record('installed_plugins', $name) SET
                plugin_name = $name,
                version = $version,
                is_enabled = true,
                installed_by = $installed_by;
            LET $from = type::record('installed_plugins', $name);
            FOR $dependency IN $dependencies {
                LET $to = type::record('installed_plugins', $dependency);
                RELATE $from->installed_plugin_depends_on->$to
                    SET requirement = 'required';
            };
            IF $theme != NONE {
                UPSERT type::record('ui_themes', $theme.name) CONTENT {
                    name: $theme.name,
                    label: $theme.label,
                    is_system: $theme.is_system,
                    is_active: true,
                    color_mode: $theme.color_mode,
                    tokens: $theme.tokens,
                    layout: $theme.layout,
                    error_pages: $theme.error_pages,
                    nav: $theme.nav,
                    layout_def: $theme.layout_def,
                    error_pages_def: $theme.error_pages_def,
                    shell: $theme.shell,
                    plugin_name: $name,
                    plugin_version: $version
                };
                LET $configured = (SELECT VALUE id FROM ui_theme_config LIMIT 1);
                IF array::len($configured) = 0 {
                    CREATE ui_theme_config:org SET
                        active_theme = type::record('ui_themes', $theme.name),
                        color_mode = $theme.color_mode;
                };
            };
            COMMIT TRANSACTION;
            "#,
        )
        .bind(("theme", theme.clone()))
        .bind(("name", record.name.clone()))
        .bind(("version", record.version.clone()))
        .bind(("installed_by", INSTALLED_BY_CLI.to_string()))
        .bind(("dependencies", dependencies))
        .await?
        .check()?;
        sync_schedules(db, &record.name, record.schedules.as_deref()).await?;
        crate::plugin_files::watch::sync_watches(db, &record.name, record.watches.as_deref())
            .await?;
        crate::plugin_events::sync_listeners(db, &record.name, record.event_listeners.as_deref())
            .await?;
        crate::roles::sync_roles(db, &record.name, record.roles.as_deref()).await?;
        if let Some(theme) = theme {
            report.themes.push(InstalledTheme {
                name: theme.name,
                layout: theme.layout,
                activated: !has_active_theme,
            });
            has_active_theme = true;
        }
        report.installed.push((record.name, record.version));
    }

    if !report.installed.is_empty() {
        super::super::mantle_signals::emit(
            db,
            namespace,
            core_database,
            super::super::mantle_signals::SignalKind::Installed,
            None,
            None,
            Some(org_database),
        )
        .await;
    }

    Ok(report)
}

/// Result of [`upgrade_plugins`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UpgradeReport {
    /// `(name, from, to)` for each plugin moved to another catalog version.
    pub upgraded: Vec<(String, String, String)>,
    /// `(name, version)` for plugins already on the version asked for.
    pub already_current: Vec<(String, String)>,
}

/// Move an organization to another catalog version of plugins it has installed: the newest
/// loaded one, or the one named (`name@version`). Pass a single spec named `all` (no version)
/// to upgrade every installed plugin, dependencies first. Nothing is copied: catalog versions
/// are already on disk, as revisions that share their unchanged files.
///
/// The target's dependencies must already be installed, and its pages must not take over a
/// route another installed plugin serves. A theme the plugin ships is updated in place and
/// stays active if it was.
pub async fn upgrade_plugins(
    db: &Surreal<Client>,
    namespace: &str,
    core_database: &str,
    org_database: &str,
    specs: &[PluginSpec],
) -> Result<UpgradeReport, CatalogError> {
    db.use_ns(namespace).await?;
    db.use_db(core_database).await?;
    let mut response = db
        .query("SELECT db_name FROM org_databases WHERE db_name = $org LIMIT 1;")
        .bind(("org", org_database.to_string()))
        .await?
        .check()?;
    let orgs: Vec<OrgDatabaseRow> = response.take(0)?;
    if orgs.is_empty() {
        return Err(CatalogError::OrganizationNotFound(org_database.to_string()));
    }

    db.use_db(org_database).await?;
    let mut response = db
        .query("SELECT plugin_name, version FROM installed_plugins;")
        .await?
        .check()?;
    let rows: Vec<InstalledRow> = response.take(0)?;
    let mut installed: HashMap<String, String> = rows
        .into_iter()
        .map(|row| (row.plugin_name, row.version))
        .collect();

    db.use_db(core_database).await?;
    let resolved = resolve_upgrade_specs(db, &installed, specs).await?;

    let mut report = UpgradeReport::default();
    for spec in &resolved {
        let Some(from) = installed.get(&spec.name).cloned() else {
            return Err(CatalogError::NotInstalled(spec.name.clone()));
        };
        db.use_db(core_database).await?;
        let target = select_catalog_version(db, spec).await?;
        if target.version == from {
            report.already_current.push((spec.name.clone(), from));
            continue;
        }
        for dependency in target.dependencies.clone().unwrap_or_default() {
            if !installed.contains_key(&dependency) {
                return Err(CatalogError::MissingDependency {
                    plugin: spec.name.clone(),
                    dependency,
                });
            }
        }
        // The plugin's own old pages do not count against its new ones.
        let others: HashMap<String, String> = installed
            .iter()
            .filter(|(name, _)| **name != spec.name)
            .map(|(name, version)| (name.clone(), version.clone()))
            .collect();
        check_route_conflicts(db, &others, std::slice::from_ref(&target)).await?;
        check_component_consumers(db, &others, &spec.name, &target).await?;
        let theme = fetch_planned_themes(db, std::slice::from_ref(&target))
            .await?
            .into_iter()
            .next();
        let planned_models = fetch_planned_models(db, std::slice::from_ref(&target)).await?;

        db.use_db(org_database).await?;
        // Make the organization's data match the new version's models first. A change that
        // cannot be applied safely stops the upgrade before the version is moved.
        let defs = models_of(&planned_models, &target.id);
        let plans = model_apply::plan_models(db, &spec.name, &defs).await?;
        model_apply::apply_plans(db, &plans).await?;
        db.query(
            r#"
            BEGIN TRANSACTION;
            UPDATE type::record('installed_plugins', $name) SET version = $version;
            IF $theme != NONE {
                -- SET replaces each field as a whole (and keeps `is_active`); MERGE would
                -- deep-merge objects and leave a removed region, token or nav item behind.
                UPSERT type::record('ui_themes', $theme.name) SET
                    name = $theme.name,
                    label = $theme.label,
                    is_system = $theme.is_system,
                    color_mode = $theme.color_mode,
                    tokens = $theme.tokens,
                    layout = $theme.layout,
                    error_pages = $theme.error_pages,
                    nav = $theme.nav,
                    layout_def = $theme.layout_def,
                    error_pages_def = $theme.error_pages_def,
                    shell = $theme.shell,
                    plugin_name = $name,
                    plugin_version = $version;
            };
            COMMIT TRANSACTION;
            "#,
        )
        .bind(("theme", theme))
        .bind(("name", spec.name.clone()))
        .bind(("version", target.version.clone()))
        .await?
        .check()?;
        sync_schedules(db, &spec.name, target.schedules.as_deref()).await?;
        crate::plugin_files::watch::sync_watches(db, &spec.name, target.watches.as_deref()).await?;
        crate::plugin_events::sync_listeners(db, &spec.name, target.event_listeners.as_deref())
            .await?;
        crate::roles::sync_roles(db, &spec.name, target.roles.as_deref()).await?;
        installed.insert(spec.name.clone(), target.version.clone());
        // Tell a separate `--serve` process to drop warm installed_plugins / pages.
        super::super::mantle_signals::emit(
            db,
            namespace,
            core_database,
            super::super::mantle_signals::SignalKind::Installed,
            Some(&spec.name),
            Some(&target.version),
            Some(org_database),
        )
        .await;
        super::super::mantle_signals::emit(
            db,
            namespace,
            core_database,
            super::super::mantle_signals::SignalKind::PluginSurface,
            Some(&spec.name),
            Some(&target.version),
            None,
        )
        .await;
        report
            .upgraded
            .push((spec.name.clone(), from, target.version));
    }
    Ok(report)
}

/// The models of the plugin versions about to be installed. Caller must be on the core database.
pub(super) async fn fetch_planned_models(
    db: &Surreal<Client>,
    order: &[PluginDbDefinition],
) -> Result<Vec<(surrealdb::types::RecordId, ModelDef)>, CatalogError> {
    if order.is_empty() {
        return Ok(Vec::new());
    }
    let ids: Vec<_> = order.iter().map(|record| record.id.clone()).collect();
    let mut response = db
        .query("SELECT plugin, definition, name FROM plugin_models WHERE plugin IN $ids ORDER BY name;")
        .bind(("ids", ids))
        .await?
        .check()?;
    let rows: Vec<PlannedModel> = response.take(0)?;
    rows.into_iter()
        .map(|row| {
            serde_json::from_value::<ModelDef>(row.definition)
                .map(|model| (row.plugin, model))
                .map_err(|source| {
                    CatalogError::ModelFile(ModelFileError::Parse {
                        path: PathBuf::from("plugin_models"),
                        source,
                    })
                })
        })
        .collect()
}

pub(super) fn models_of(
    all: &[(surrealdb::types::RecordId, ModelDef)],
    plugin: &surrealdb::types::RecordId,
) -> Vec<ModelDef> {
    all.iter()
        .filter(|(id, _)| id == plugin)
        .map(|(_, model)| model.clone())
        .collect()
}

/// The themes shipped by the plugins about to be installed. Caller must be on
/// the core database.
pub(super) async fn fetch_planned_themes(
    db: &Surreal<Client>,
    order: &[PluginDbDefinition],
) -> Result<Vec<PlannedTheme>, CatalogError> {
    if order.is_empty() {
        return Ok(Vec::new());
    }
    let ids: Vec<_> = order.iter().map(|record| record.id.clone()).collect();
    let mut response = db
        .query(
            "SELECT plugin, name, label, is_system, color_mode, tokens, layout, error_pages, nav, \
                    layout_def, error_pages_def, shell \
             FROM plugin_themes WHERE plugin IN $ids;",
        )
        .bind(("ids", ids))
        .await?
        .check()?;
    Ok(response.take(0)?)
}

/// Make an installed theme the organization's active one.
pub async fn activate_theme(
    db: &Surreal<Client>,
    namespace: &str,
    org_database: &str,
    theme_name: &str,
) -> Result<(), CatalogError> {
    db.use_ns(namespace).await?;
    db.use_db(org_database).await?;
    let mut response = db
        .query("SELECT color_mode FROM ui_themes WHERE name = $name LIMIT 1;")
        .bind(("name", theme_name.to_string()))
        .await?
        .check()?;
    let found: Vec<ThemeColorMode> = response.take(0)?;
    let Some(theme) = found.into_iter().next() else {
        return Err(CatalogError::ThemeNotInstalled(theme_name.to_string()));
    };
    db.query(
        "UPSERT ui_theme_config:org SET active_theme = type::record('ui_themes', $name), color_mode = $mode;",
    )
    .bind(("name", theme_name.to_string()))
    .bind(("mode", theme.color_mode))
    .await?
    .check()?;
    Ok(())
}

/// Dependency-ordered install plan, resolved against the core catalog.
pub(super) struct Plan<'a> {
    installed: &'a HashMap<String, String>,
    order: Vec<PluginDbDefinition>,
    visiting: HashSet<String>,
    visited: HashSet<String>,
    already_installed: Vec<(String, String)>,
}

impl Plan<'_> {
    /// Post-order DFS so dependencies precede their dependents. `requested`
    /// specs must match an existing install exactly; dependencies that are
    /// already installed are accepted at whatever version the org has.
    async fn visit(
        &mut self,
        db: &Surreal<Client>,
        spec: &PluginSpec,
        requested: bool,
    ) -> Result<(), CatalogError> {
        if self.visited.contains(&spec.name) {
            return Ok(());
        }
        if !self.visiting.insert(spec.name.clone()) {
            return Err(CatalogError::DependencyCycle(spec.name.clone()));
        }

        if let Some(installed_version) = self.installed.get(&spec.name) {
            if requested
                && let Some(wanted) = &spec.version
                && wanted != installed_version
            {
                return Err(CatalogError::VersionMismatch {
                    name: spec.name.clone(),
                    installed: installed_version.clone(),
                    requested: wanted.clone(),
                });
            }
            self.already_installed
                .push((spec.name.clone(), installed_version.clone()));
        } else {
            let record = select_catalog_version(db, spec).await?;
            let required_by = record.name.clone();
            for dependency in record.dependencies.clone().unwrap_or_default() {
                let dependency = PluginSpec {
                    name: dependency,
                    version: None,
                };
                if let Err(error) = Box::pin(self.visit(db, &dependency, false)).await {
                    return Err(match error {
                        CatalogError::PluginNotFound(missing) => {
                            CatalogError::DependencyNotInCatalog {
                                dependency: missing,
                                required_by: required_by.clone(),
                            }
                        }
                        other => other,
                    });
                }
            }
            self.order.push(record);
        }

        self.visiting.remove(&spec.name);
        self.visited.insert(spec.name.clone());
        Ok(())
    }
}

/// Whether `specs` asks to upgrade every installed plugin (`all` alone, no `@version`).
pub(super) fn upgrade_all_requested(specs: &[PluginSpec]) -> Result<bool, CatalogError> {
    if !specs.iter().any(|spec| spec.name == "all") {
        return Ok(false);
    }
    if specs.len() == 1 && specs[0].version.is_none() {
        return Ok(true);
    }
    Err(CatalogError::InvalidSpec(
        "all (use alone, without @version)".to_string(),
    ))
}

/// Expand CLI upgrade specs. `all` alone means every installed plugin (newest catalog
/// version each), ordered so dependencies precede dependents.
pub(super) async fn resolve_upgrade_specs(
    db: &Surreal<Client>,
    installed: &HashMap<String, String>,
    specs: &[PluginSpec],
) -> Result<Vec<PluginSpec>, CatalogError> {
    if !upgrade_all_requested(specs)? {
        return Ok(specs.to_vec());
    }

    let mut order = Vec::new();
    let mut visiting = HashSet::new();
    let mut visited = HashSet::new();
    let mut names: Vec<&String> = installed.keys().collect();
    names.sort();
    for name in names {
        visit_upgrade_all(db, name, installed, &mut visiting, &mut visited, &mut order).await?;
    }
    Ok(order)
}

pub(super) async fn visit_upgrade_all(
    db: &Surreal<Client>,
    name: &str,
    installed: &HashMap<String, String>,
    visiting: &mut HashSet<String>,
    visited: &mut HashSet<String>,
    order: &mut Vec<PluginSpec>,
) -> Result<(), CatalogError> {
    if visited.contains(name) {
        return Ok(());
    }
    if !visiting.insert(name.to_string()) {
        return Err(CatalogError::DependencyCycle(name.to_string()));
    }
    let target = select_catalog_version(
        db,
        &PluginSpec {
            name: name.to_string(),
            version: None,
        },
    )
    .await?;
    for dependency in target.dependencies.clone().unwrap_or_default() {
        if installed.contains_key(&dependency) {
            Box::pin(visit_upgrade_all(
                db,
                &dependency,
                installed,
                visiting,
                visited,
                order,
            ))
            .await?;
        }
    }
    visiting.remove(name);
    visited.insert(name.to_string());
    order.push(PluginSpec {
        name: name.to_string(),
        version: None,
    });
    Ok(())
}

/// Pick the active catalog row for `spec`. Without an explicit version, the one loaded
/// most recently.
pub(super) async fn select_catalog_version(
    db: &Surreal<Client>,
    spec: &PluginSpec,
) -> Result<PluginDbDefinition, CatalogError> {
    let mut response = db
        .query("SELECT * FROM plugins WHERE name = $name AND is_active = true ORDER BY date_created DESC;")
        .bind(("name", spec.name.clone()))
        .await?
        .check()?;
    let rows: Vec<PluginDbDefinition> = response.take(0)?;

    match &spec.version {
        Some(version) => rows
            .into_iter()
            .find(|row| &row.version == version)
            .ok_or_else(|| CatalogError::VersionNotFound {
                name: spec.name.clone(),
                version: version.clone(),
            }),
        None => rows
            .into_iter()
            .next()
            .ok_or_else(|| CatalogError::PluginNotFound(spec.name.clone())),
    }
}
