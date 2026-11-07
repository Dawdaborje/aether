//! Reading parts of a plugin manifest: the app tile, the theme, its stylesheet, and the schedules.

use super::*;

/// The launcher tile for a plugin that declares `[app]`. Its `route` must be a
/// literal page route of the plugin, so the tile always opens something.
pub(super) fn read_app(
    manifest: &PluginManifest,
    manifest_path: &Path,
    pages: &[PageDocument],
) -> Result<Option<NewApp>, CatalogError> {
    let Some(app) = &manifest.app else {
        return Ok(None);
    };
    let route = super::super::pages::RoutePattern::parse(&app.route).map_err(|_| {
        CatalogError::InvalidApp {
            path: manifest_path.to_path_buf(),
            field: "route",
            reason: "must be an absolute page route such as `/chat`",
        }
    })?;
    if route.is_pattern() {
        return Err(CatalogError::InvalidApp {
            path: manifest_path.to_path_buf(),
            field: "route",
            reason: "cannot contain `{param}` segments: a tile opens one fixed page",
        });
    }
    let route = route.canonical();
    // The tile may open `/chat` when the page is `/chat/{channel?}`: its optional tail may be left out.
    let serves = |page: &PageDocument| {
        page.route == route
            || super::super::pages::RoutePattern::parse(&page.route)
                .ok()
                .and_then(|pattern| pattern.menu_route())
                .as_deref()
                == Some(route.as_str())
    };
    if !pages.iter().any(serves) {
        // Pages made up for `extends` tabs cannot be opened by a tile; do not offer them.
        let own: Vec<&str> = pages
            .iter()
            .map(|page| page.route.as_str())
            .filter(|route| !route.starts_with(super::super::pages::EXTENSION_ROUTE_PREFIX))
            .collect();
        let fix = if own.is_empty() {
            "This plugin has no page of its own (it only adds tabs to other pages), so it has \
             nothing for an app tile to open: remove the `[app]` section from plugin.toml."
                .to_string()
        } else {
            format!(
                "Set `[app].route` to one of this plugin's pages ({}), or remove the `[app]` section.",
                own.join(", ")
            )
        };
        return Err(CatalogError::AppRouteNotAPage {
            path: manifest_path.to_path_buf(),
            route,
            fix,
        });
    }
    if let Some(icon) = &app.icon
        && !super::super::themes::is_identifier(icon)
    {
        return Err(CatalogError::InvalidApp {
            path: manifest_path.to_path_buf(),
            field: "icon",
            reason: "must be a lucide icon name (lowercase letters, digits and `-`)",
        });
    }
    let label = app
        .label
        .clone()
        .filter(|label| !label.trim().is_empty())
        .or_else(|| {
            (!manifest.plugin.label.trim().is_empty()).then(|| manifest.plugin.label.clone())
        })
        .unwrap_or_else(|| manifest.plugin.name.clone());
    let folder = app
        .folder
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string);
    let roles = app
        .roles
        .iter()
        .map(|role| crate::roles::qualify_role(&manifest.plugin.name, role))
        .filter(|role| !role.is_empty())
        .collect();
    let palette = palette_entries(&manifest.plugin.name, &app.palette).map_err(|reason| {
        CatalogError::InvalidApp {
            path: manifest_path.to_path_buf(),
            field: "palette",
            reason,
        }
    })?;
    let tile = tile_entry(app.tile.as_ref()).map_err(|reason| CatalogError::InvalidApp {
        path: manifest_path.to_path_buf(),
        field: "tile",
        reason,
    })?;
    Ok(Some(NewApp {
        label,
        icon: app.icon.clone(),
        route,
        description: app
            .description
            .clone()
            .filter(|text| !text.trim().is_empty()),
        standalone: app.standalone,
        also_in_host: app.also_in_host,
        folder,
        roles,
        nav: app.nav,
        palette,
        tile,
    }))
}

/// Check `[app.tile]` and turn it into what the catalog keeps.
pub(super) fn tile_entry(
    tile: Option<&crate::plugin_manager::models::plugin_def::PluginTileDef>,
) -> Result<Option<serde_json::Value>, &'static str> {
    let Some(tile) = tile else {
        return Ok(None);
    };
    let function = tile.function.trim();
    if function.contains('.') || !super::super::pages::is_function_ref(function) {
        return Err(
            "`function` names a function of this plugin (lowercase letters, digits and `_`)",
        );
    }
    let text = |value: &Option<String>, max: usize| {
        value
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(|t| (t.chars().count() <= max).then(|| t.to_string()))
    };
    let label = text(&tile.label, 40).transpose_or("`label` is longer than 40 characters")?;
    let unit = text(&tile.unit, 6).transpose_or("`unit` is longer than 6 characters")?;
    Ok(Some(
        serde_json::json!({ "function": function, "label": label, "unit": unit }),
    ))
}

trait TransposeOr {
    fn transpose_or(self, reason: &'static str) -> Result<Option<String>, &'static str>;
}

impl TransposeOr for Option<Option<String>> {
    fn transpose_or(self, reason: &'static str) -> Result<Option<String>, &'static str> {
        match self {
            None => Ok(None),
            Some(Some(value)) => Ok(Some(value)),
            Some(None) => Err(reason),
        }
    }
}

/// Most `[[app.palette]]` entries one app may add.
const MAX_PALETTE: usize = 20;

/// Check `[[app.palette]]` and turn it into what the catalog keeps: role names qualified,
/// blank optional text dropped.
pub(super) fn palette_entries(
    plugin: &str,
    entries: &[crate::plugin_manager::models::plugin_def::PluginPaletteDef],
) -> Result<Vec<serde_json::Value>, &'static str> {
    if entries.len() > MAX_PALETTE {
        return Err("has more than 20 entries");
    }
    let mut shortcuts = std::collections::HashSet::new();
    let mut out = Vec::with_capacity(entries.len());
    for entry in entries {
        let label = entry.label.trim();
        if label.is_empty() || label.chars().count() > 80 {
            return Err("entries need a `label` of 1 to 80 characters");
        }
        let targets = [&entry.href, &entry.function, &entry.search]
            .iter()
            .filter(|target| target.as_deref().is_some_and(|t| !t.trim().is_empty()))
            .count();
        if targets != 1 {
            return Err("each entry needs exactly one of `href`, `function` and `search`");
        }
        let href = entry
            .href
            .as_deref()
            .map(str::trim)
            .filter(|h| !h.is_empty());
        if let Some(href) = href
            && (!href.starts_with('/')
                || !super::super::themes::is_safe_href(href)
                || href.len() > 300)
        {
            return Err("`href` must be an app path starting with `/`");
        }
        if entry.create && href.is_none() {
            return Err("a `create` entry needs an `href`");
        }
        // Functions are this plugin's own: a bare name, no `plugin.` prefix.
        for function in [&entry.function, &entry.search].into_iter().flatten() {
            let function = function.trim();
            if !function.is_empty()
                && (function.contains('.') || !super::super::pages::is_function_ref(function))
            {
                return Err(
                    "`function` and `search` name a function of this plugin (lowercase letters, digits and `_`)",
                );
            }
        }
        if entry
            .search
            .as_deref()
            .is_some_and(|f| !f.trim().is_empty())
            && entry.shortcut.is_some()
        {
            return Err("a `search` entry cannot have a `shortcut`");
        }
        if let Some(icon) = &entry.icon
            && !super::super::themes::is_identifier(icon)
        {
            return Err("`icon` must be an icon name (lowercase letters, digits and `-`)");
        }
        let keywords = entry
            .keywords
            .as_deref()
            .map(str::trim)
            .filter(|k| !k.is_empty());
        if keywords.is_some_and(|k| k.chars().count() > 200) {
            return Err("`keywords` must be at most 200 characters");
        }
        let shortcut = match entry
            .shortcut
            .as_deref()
            .map(str::trim)
            .filter(|k| !k.is_empty())
        {
            None => None,
            Some(shortcut) => {
                let normal = normal_shortcut(shortcut).ok_or(
                    "`shortcut` must be `mod`, `alt` or `shift` joined by `+`, then a letter or digit, with `mod` or `alt` (and not `mod+k`, which opens the palette)",
                )?;
                if !shortcuts.insert(normal.clone()) {
                    return Err("two entries use the same `shortcut`");
                }
                Some(normal)
            }
        };
        let roles: Vec<String> = entry
            .roles
            .iter()
            .map(|role| crate::roles::qualify_role(plugin, role))
            .filter(|role| !role.is_empty())
            .collect();
        out.push(serde_json::json!({
            "label": label,
            "href": href,
            "function": entry.function.as_deref().map(str::trim).filter(|f| !f.is_empty()),
            "search": entry.search.as_deref().map(str::trim).filter(|f| !f.is_empty()),
            "icon": entry.icon,
            "keywords": keywords,
            "shortcut": shortcut,
            "roles": roles,
            "create": entry.create,
        }));
    }
    Ok(out)
}

/// `Alt+N` → `alt+n`; modifiers in the order `mod`, `alt`, `shift`. `None` when it is not a
/// shortcut the palette can bind: it needs `mod` or `alt`, one letter or digit, and is not `mod+k`.
pub(super) fn normal_shortcut(shortcut: &str) -> Option<String> {
    let lower = shortcut.to_ascii_lowercase();
    let parts: Vec<&str> = lower.split('+').map(str::trim).collect();
    let (key, modifiers) = parts.split_last()?;
    let mut has = [false; 3];
    for modifier in modifiers {
        let slot = ["mod", "alt", "shift"].iter().position(|m| m == modifier)?;
        if has[slot] {
            return None;
        }
        has[slot] = true;
    }
    let key_ok = key.len() == 1 && key.chars().all(|c| c.is_ascii_alphanumeric());
    if !key_ok || !(has[0] || has[1]) {
        return None;
    }
    let normal: Vec<&str> = ["mod", "alt", "shift"]
        .iter()
        .zip(has)
        .filter(|(_, on)| *on)
        .map(|(m, _)| *m)
        .chain(std::iter::once(*key))
        .collect();
    let normal = normal.join("+");
    (normal != "mod+k").then_some(normal)
}

#[cfg(test)]
mod palette_tests {
    use super::*;
    use crate::plugin_manager::models::plugin_def::PluginPaletteDef;

    fn entry(label: &str, href: &str, shortcut: Option<&str>) -> PluginPaletteDef {
        PluginPaletteDef {
            label: label.into(),
            href: (!href.is_empty()).then(|| href.to_string()),
            shortcut: shortcut.map(str::to_string),
            roles: vec!["leave_user".into()],
            ..Default::default()
        }
    }

    #[test]
    fn shortcuts_are_normalised_and_checked() {
        assert_eq!(normal_shortcut("Alt+N").as_deref(), Some("alt+n"));
        assert_eq!(
            normal_shortcut("shift+mod+e").as_deref(),
            Some("mod+shift+e")
        );
        assert_eq!(normal_shortcut("mod+1").as_deref(), Some("mod+1"));
        for bad in [
            "n",
            "shift+n",
            "mod+k",
            "mod+mod+n",
            "ctrl+n",
            "alt+enter",
            "alt+",
            "",
        ] {
            assert_eq!(normal_shortcut(bad), None, "{bad}");
        }
    }

    #[test]
    fn tiles_are_checked() {
        use crate::plugin_manager::models::plugin_def::PluginTileDef;
        let tile = |function: &str, unit: Option<&str>| PluginTileDef {
            function: function.into(),
            label: Some("Open".into()),
            unit: unit.map(str::to_string),
        };
        let good = tile_entry(Some(&tile("count_open", Some("%"))))
            .expect("valid")
            .expect("some");
        assert_eq!(good["function"], "count_open");
        assert_eq!(good["unit"], "%");
        assert_eq!(tile_entry(None), Ok(None));
        assert!(tile_entry(Some(&tile("crm.count", None))).is_err());
        assert!(tile_entry(Some(&tile("Count", None))).is_err());
        assert!(tile_entry(Some(&tile("count", Some("far too long")))).is_err());
    }

    #[test]
    fn palette_entries_are_checked() {
        let good = palette_entries(
            "leave",
            &[entry("New request", "/leave/new", Some("Alt+L"))],
        )
        .expect("valid");
        assert_eq!(good[0]["shortcut"], "alt+l");
        assert_eq!(good[0]["roles"][0], "leave.leave_user");
        let run = PluginPaletteDef {
            label: "Start".into(),
            function: Some("start".into()),
            ..Default::default()
        };
        let find = PluginPaletteDef {
            label: "Requests".into(),
            search: Some("find_requests".into()),
            ..Default::default()
        };
        let kinds = palette_entries("leave", &[run, find]).expect("valid");
        assert_eq!(kinds[0]["function"], "start");
        assert_eq!(kinds[1]["search"], "find_requests");

        for bad in [
            vec![entry("", "/leave", None)],
            vec![entry("x", "", None)],
            vec![PluginPaletteDef {
                label: "x".into(),
                href: Some("/a".into()),
                function: Some("go".into()),
                ..Default::default()
            }],
            vec![PluginPaletteDef {
                label: "x".into(),
                function: Some("other.go".into()),
                ..Default::default()
            }],
            vec![PluginPaletteDef {
                label: "x".into(),
                search: Some("find".into()),
                shortcut: Some("alt+f".into()),
                ..Default::default()
            }],
            vec![entry("x", "https://example.com", None)],
            vec![entry("x", "javascript:alert(1)", None)],
            vec![entry("x", "/a", Some("n"))],
            vec![
                entry("a", "/a", Some("alt+n")),
                entry("b", "/b", Some("Alt+N")),
            ],
            (0..21)
                .map(|i| entry(&format!("e{i}"), "/a", None))
                .collect(),
        ] {
            assert!(palette_entries("leave", &bad).is_err());
        }
    }
}

/// The theme a plugin ships, read from `[theme].tokens_file`; `None` for a
/// plugin that is not a theme.
pub(super) async fn read_theme(
    manifest: &PluginManifest,
    manifest_path: &Path,
    package_dir: &Path,
) -> Result<Option<ThemeDocument>, CatalogError> {
    let Some(theme) = &manifest.theme else {
        return Ok(None);
    };
    let tokens_file = theme
        .tokens_file
        .as_deref()
        .filter(|file| !file.is_empty())
        .ok_or_else(|| CatalogError::ThemeWithoutTokens(manifest_path.to_path_buf()))?;
    let relative = resolve_package_file(package_dir, tokens_file).await?;
    let path = package_dir.join(relative);
    let text = tokio::fs::read_to_string(&path)
        .await
        .map_err(io_error(&path))?;
    let mut document = parse_theme(&text, &path, &theme.name, theme.label.as_deref())?;
    attach_stylesheet(&mut document, package_dir, &manifest.plugin.name, &path).await?;
    attach_icons(&mut document, package_dir, &path).await?;
    Ok(Some(document))
}

/// Read the stylesheet a theme names, check it, and keep the checked text on the theme.
pub(super) async fn attach_stylesheet(
    theme: &mut ThemeDocument,
    package_dir: &Path,
    plugin: &str,
    theme_path: &Path,
) -> Result<(), CatalogError> {
    let Some(shell) = theme.shell.as_mut().and_then(|shell| shell.as_object_mut()) else {
        return Ok(());
    };
    let Some(file) = shell
        .remove("css_file")
        .and_then(|file| file.as_str().map(str::to_string))
    else {
        return Ok(());
    };
    let relative = resolve_package_file(package_dir, &file).await?;
    let css_path = package_dir.join(relative);
    let css = tokio::fs::read_to_string(&css_path)
        .await
        .map_err(io_error(&css_path))?;
    let checked = super::super::theme_css::sanitize(&css, plugin).map_err(|reason| {
        CatalogError::ThemeStylesheet {
            path: css_path.clone(),
            theme: theme_path.to_path_buf(),
            reason,
        }
    })?;
    shell.insert("css".into(), serde_json::Value::String(checked));
    Ok(())
}

/// Read the icon sprite a theme names, check it, and keep the checked icons on the theme.
pub(super) async fn attach_icons(
    theme: &mut ThemeDocument,
    package_dir: &Path,
    theme_path: &Path,
) -> Result<(), CatalogError> {
    let Some(shell) = theme.shell.as_mut().and_then(|shell| shell.as_object_mut()) else {
        return Ok(());
    };
    let Some(file) = shell
        .remove("icons_file")
        .and_then(|file| file.as_str().map(str::to_string))
    else {
        return Ok(());
    };
    let relative = resolve_package_file(package_dir, &file).await?;
    let svg_path = package_dir.join(relative);
    let svg = tokio::fs::read_to_string(&svg_path)
        .await
        .map_err(io_error(&svg_path))?;
    let icons = super::super::theme_icons::sanitize(&svg, &svg_path).map_err(|reason| {
        CatalogError::ThemeIcons {
            path: svg_path.clone(),
            theme: theme_path.to_path_buf(),
            reason,
        }
    })?;
    shell.insert("icons".into(), icons);
    Ok(())
}

/// Attach plugin layout / error-set definitions to a theme that names them.
pub(super) fn bind_theme_shell_defs(
    theme: &mut ThemeDocument,
    layouts: &[LayoutDef],
    error_sets: &[ErrorSetDef],
    screens: &[super::super::screens::ScreenDef],
    manifest_path: &Path,
) -> Result<(), CatalogError> {
    if !screens.is_empty() {
        // The theme's sign-in screens ride along in `shell.screens`.
        let map: serde_json::Map<String, serde_json::Value> = screens
            .iter()
            .map(|screen| (screen.name.clone(), screen.definition.clone()))
            .collect();
        let shell = theme.shell.get_or_insert_with(|| serde_json::json!({}));
        if let Some(shell) = shell.as_object_mut() {
            shell.insert("screens".into(), serde_json::Value::Object(map));
        }
    }
    let layouts = layouts_by_name(layouts);
    let error_sets = error_sets_by_name(error_sets);
    if is_builtin_layout(&theme.layout) {
        theme.layout_def = None;
    } else if let Some(def) = layouts.get(theme.layout.as_str()) {
        theme.layout_def = Some(def.definition.clone());
    } else {
        return Err(CatalogError::UnknownShellDef {
            path: manifest_path.to_path_buf(),
            field: "layout",
            name: theme.layout.clone(),
            folder: "layouts",
        });
    }
    if is_builtin_error_set(&theme.error_pages) {
        theme.error_pages_def = None;
    } else if let Some(def) = error_sets.get(theme.error_pages.as_str()) {
        theme.error_pages_def = Some(def.definition.clone());
    } else {
        return Err(CatalogError::UnknownShellDef {
            path: manifest_path.to_path_buf(),
            field: "error_pages",
            name: theme.error_pages.clone(),
            folder: "errors",
        });
    }
    Ok(())
}

/// Make an organization's recurring tasks for `plugin` match the catalog version's `[[schedule]]`.
/// Caller must be on the organization's database.
pub(super) async fn sync_schedules(
    db: &Surreal<Client>,
    plugin: &str,
    schedules: Option<&[serde_json::Value]>,
) -> Result<(), CatalogError> {
    let defs: Vec<crate::scheduler::TaskDef> = schedules
        .unwrap_or_default()
        .iter()
        .filter_map(|value| serde_json::from_value(value.clone()).ok())
        .collect();
    crate::scheduler::tasks::sync_manifest(db, plugin, &defs)
        .await
        .map_err(|error| CatalogError::Schedule(plugin.to_string(), error.to_string()))
}
