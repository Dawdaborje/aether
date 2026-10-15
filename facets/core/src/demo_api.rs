//! Desk demo data: list and run plugin commands marked with `demo_model`.
//!
//! * `GET  /api/ui/demo?org=<db>` — plugins installed in that org that offer demo generators.
//! * `POST /api/ui/demo/generate` — run the generator for one model with a `count`.
//!
//! Developer-only. The plugin owns what to create; the kernel only discovers and invokes.

use std::collections::{BTreeMap, HashMap};

use axum::{
    Json, Router,
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use surrealdb::types::SurrealValue;

use crate::application::settings_api::require_session;
use crate::plugin_manager::commands::{self, CommandError, CommandInfo};
use crate::plugin_manager::model_edit::models_of_version;
use crate::plugin_manager::models::plugin_def::PluginCommandDef;
use crate::state::AppState;

/// Hard cap so Desk cannot ask for an unbounded run.
pub const MAX_DEMO_COUNT: u32 = 10_000;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/ui/demo", get(list_demo))
        .route("/api/ui/demo/generate", post(generate_demo))
}

fn failure(status: StatusCode, message: impl AsRef<str>) -> Response {
    (status, Json(json!({ "error": message.as_ref() }))).into_response()
}

fn database_error(context: &str, error: impl std::fmt::Display) -> Response {
    log::error!("demo {context}: {error}");
    failure(StatusCode::INTERNAL_SERVER_ERROR, "database error")
}

#[derive(Debug, Deserialize)]
struct ListQuery {
    org: String,
}

#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
pub struct DemoModelEntry {
    pub name: String,
    pub label: String,
    pub command: String,
}

#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
pub struct DemoPluginEntry {
    pub name: String,
    pub label: String,
    pub version: String,
    pub models: Vec<DemoModelEntry>,
}

#[derive(Debug, Deserialize)]
struct GenerateBody {
    org: String,
    plugin: String,
    model: String,
    count: u32,
}

#[derive(Debug, Deserialize, SurrealValue)]
struct PluginLabelRow {
    name: String,
    version: String,
    label: String,
}

/// Group installed commands that declare `demo_model` into a Desk-facing list.
/// `labels` maps `(plugin, version)` → plugin label; `model_labels` maps
/// `(plugin, version, model)` → display label.
pub fn build_demo_catalog(
    commands: &[CommandInfo],
    labels: &HashMap<(String, String), String>,
    model_labels: &HashMap<(String, String, String), String>,
) -> Vec<DemoPluginEntry> {
    let mut by_plugin: BTreeMap<(String, String), Vec<DemoModelEntry>> = BTreeMap::new();
    for info in commands {
        let Some(model) = info.def.demo_model.as_ref() else {
            continue;
        };
        let key = (info.plugin.clone(), info.version.clone());
        let label = model_labels
            .get(&(info.plugin.clone(), info.version.clone(), model.clone()))
            .cloned()
            .unwrap_or_else(|| model.clone());
        by_plugin.entry(key).or_default().push(DemoModelEntry {
            name: model.clone(),
            label,
            command: info.def.name.clone(),
        });
    }
    by_plugin
        .into_iter()
        .map(|((name, version), mut models)| {
            models.sort_by(|a, b| a.name.cmp(&b.name));
            let label = labels
                .get(&(name.clone(), version.clone()))
                .cloned()
                .unwrap_or_else(|| name.clone());
            DemoPluginEntry {
                name,
                label,
                version,
                models,
            }
        })
        .collect()
}

/// Find the unique demo command for `plugin` + `model`, or an error message.
pub fn resolve_demo_command<'a>(
    commands: &'a [CommandInfo],
    plugin: &str,
    model: &str,
) -> Result<&'a CommandInfo, String> {
    let matches: Vec<&CommandInfo> = commands
        .iter()
        .filter(|info| info.plugin == plugin && info.def.demo_model.as_deref() == Some(model))
        .collect();
    match matches.as_slice() {
        [] => {
            let installed = commands.iter().any(|info| info.plugin == plugin);
            if !installed {
                Err(format!(
                    "plugin `{plugin}` is not installed in this organization"
                ))
            } else {
                Err(format!(
                    "plugin `{plugin}` has no demo generator for model `{model}`"
                ))
            }
        }
        [one] => Ok(*one),
        many => Err(format!(
            "plugin `{plugin}` has {} demo commands for model `{model}` ({})",
            many.len(),
            many.iter()
                .map(|info| info.def.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

pub fn validate_demo_count(count: u32) -> Result<(), String> {
    if count == 0 {
        return Err("count must be at least 1".into());
    }
    if count > MAX_DEMO_COUNT {
        return Err(format!("count cannot exceed {MAX_DEMO_COUNT}"));
    }
    Ok(())
}

async fn list_demo(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ListQuery>,
) -> Response {
    if let Err((status, body)) = require_session(&state, &headers).await {
        return (status, body).into_response();
    }
    let org = query.org.trim();
    if org.is_empty() {
        return failure(StatusCode::BAD_REQUEST, "org is required");
    }
    let commands = match commands::list(&state, org).await {
        Ok(commands) => commands,
        Err(CommandError::Db(error)) => return database_error("list commands", error),
        Err(error) => return failure(StatusCode::BAD_REQUEST, error.to_string()),
    };
    let demo_cmds: Vec<&CommandInfo> = commands
        .iter()
        .filter(|info| info.def.demo_model.is_some())
        .collect();
    if demo_cmds.is_empty() {
        return Json(json!({ "plugins": [] })).into_response();
    }

    let core = match state.core().await {
        Ok(core) => core,
        Err(error) => return database_error("core", error),
    };

    let mut labels: HashMap<(String, String), String> = HashMap::new();
    let mut model_labels: HashMap<(String, String, String), String> = HashMap::new();
    let mut seen_versions: HashMap<(String, String), ()> = HashMap::new();
    for info in &demo_cmds {
        let key = (info.plugin.clone(), info.version.clone());
        if seen_versions.insert(key.clone(), ()).is_some() {
            continue;
        }
        let mut response = match core
            .query("SELECT name, version, label FROM plugins WHERE name = $name AND version = $version LIMIT 1;")
            .bind(("name", info.plugin.clone()))
            .bind(("version", info.version.clone()))
            .await
        {
            Ok(response) => match response.check() {
                Ok(response) => response,
                Err(error) => return database_error("plugin label", error),
            },
            Err(error) => return database_error("plugin label", error),
        };
        let rows: Vec<PluginLabelRow> = match response.take(0) {
            Ok(rows) => rows,
            Err(error) => return database_error("plugin label take", error),
        };
        if let Some(row) = rows.into_iter().next() {
            labels.insert((row.name, row.version), row.label);
        }
        match models_of_version(&core, &info.plugin, &info.version).await {
            Ok(models) => {
                for model in models {
                    let label = model.label.clone().unwrap_or_else(|| model.name.clone());
                    model_labels.insert(
                        (info.plugin.clone(), info.version.clone(), model.name),
                        label,
                    );
                }
            }
            Err(error) => {
                log::debug!(
                    "demo: models for {}.{} unavailable: {error}",
                    info.plugin,
                    info.version
                );
            }
        }
    }

    let plugins = build_demo_catalog(&commands, &labels, &model_labels);
    Json(json!({ "plugins": plugins })).into_response()
}

async fn generate_demo(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<GenerateBody>,
) -> Response {
    if let Err((status, body)) = require_session(&state, &headers).await {
        return (status, body).into_response();
    }
    let org = body.org.trim();
    let plugin = body.plugin.trim();
    let model = body.model.trim();
    if org.is_empty() || plugin.is_empty() || model.is_empty() {
        return failure(
            StatusCode::BAD_REQUEST,
            "org, plugin and model are required",
        );
    }
    if let Err(message) = validate_demo_count(body.count) {
        return failure(StatusCode::BAD_REQUEST, message);
    }

    let commands = match commands::list(&state, org).await {
        Ok(commands) => commands,
        Err(CommandError::Db(error)) => return database_error("list commands", error),
        Err(error) => return failure(StatusCode::BAD_REQUEST, error.to_string()),
    };
    let info = match resolve_demo_command(&commands, plugin, model) {
        Ok(info) => info,
        Err(message) => return failure(StatusCode::BAD_REQUEST, message),
    };

    let args = vec![format!("count={}", body.count)];
    // Commands that declare no args take free-form JSON instead.
    let (cli_args, json): (Vec<String>, Option<String>) = if info.def.args.is_empty() {
        (Vec::new(), Some(json!({ "count": body.count }).to_string()))
    } else if info.def.args.iter().any(|arg| arg.name == "count") {
        (args, None)
    } else {
        return failure(
            StatusCode::BAD_REQUEST,
            format!(
                "command `{}.{}` does not accept a `count` argument",
                plugin, info.def.name
            ),
        );
    };

    match commands::run(
        &state,
        org,
        plugin,
        &info.def.name,
        &cli_args,
        json.as_deref(),
        "system:desk-demo",
    )
    .await
    {
        Ok(result) => Json(json!({
            "ok": true,
            "plugin": plugin,
            "command": info.def.name,
            "model": model,
            "count": body.count,
            "result": result,
        }))
        .into_response(),
        Err(CommandError::NotInstalled(name)) => failure(
            StatusCode::BAD_REQUEST,
            format!("plugin `{name}` is not installed in this organization"),
        ),
        Err(CommandError::NoSuchCommand {
            plugin, command, ..
        }) => failure(
            StatusCode::BAD_REQUEST,
            format!("plugin `{plugin}` has no command `{command}`"),
        ),
        Err(CommandError::BadArguments(message)) => failure(StatusCode::BAD_REQUEST, message),
        Err(CommandError::Failed(message)) => failure(StatusCode::BAD_REQUEST, message),
        Err(CommandError::Db(error)) => database_error("generate", error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(plugin: &str, command: &str, demo_model: Option<&str>) -> CommandInfo {
        CommandInfo {
            plugin: plugin.into(),
            version: "1.0.0".into(),
            enabled: true,
            def: PluginCommandDef {
                name: command.into(),
                function: command.into(),
                help: String::new(),
                args: Vec::new(),
                on_install: false,
                on_upgrade: false,
                demo_model: demo_model.map(str::to_string),
            },
        }
    }

    #[test]
    fn catalog_lists_only_demo_models_grouped_by_plugin() {
        let commands = vec![
            info("currency", "seed", None),
            info("currency", "demo_currency", Some("currency")),
            info("currency", "demo_rate", Some("rate")),
            info("party", "demo_partner", Some("partner")),
        ];
        let mut labels = HashMap::new();
        labels.insert(("currency".into(), "1.0.0".into()), "Currency".into());
        labels.insert(("party".into(), "1.0.0".into()), "Party".into());
        let mut model_labels = HashMap::new();
        model_labels.insert(
            ("currency".into(), "1.0.0".into(), "currency".into()),
            "Currency".into(),
        );
        model_labels.insert(
            ("currency".into(), "1.0.0".into(), "rate".into()),
            "Exchange rate".into(),
        );

        let catalog = build_demo_catalog(&commands, &labels, &model_labels);
        assert_eq!(catalog.len(), 2);
        assert_eq!(catalog[0].name, "currency");
        assert_eq!(catalog[0].label, "Currency");
        assert_eq!(
            catalog[0]
                .models
                .iter()
                .map(|m| m.name.as_str())
                .collect::<Vec<_>>(),
            vec!["currency", "rate"]
        );
        assert_eq!(catalog[0].models[1].label, "Exchange rate");
        assert_eq!(catalog[1].name, "party");
        assert_eq!(catalog[1].models[0].command, "demo_partner");
    }

    #[test]
    fn resolve_finds_unique_command_and_rejects_missing_or_ambiguous() {
        let commands = vec![
            info("currency", "demo_currency", Some("currency")),
            info("currency", "demo_dup_a", Some("dup")),
            info("currency", "demo_dup_b", Some("dup")),
        ];
        assert_eq!(
            resolve_demo_command(&commands, "currency", "currency")
                .unwrap()
                .def
                .name,
            "demo_currency"
        );
        assert!(
            resolve_demo_command(&commands, "currency", "missing")
                .unwrap_err()
                .contains("no demo generator")
        );
        assert!(
            resolve_demo_command(&commands, "missing", "currency")
                .unwrap_err()
                .contains("not installed")
        );
        assert!(
            resolve_demo_command(&commands, "currency", "dup")
                .unwrap_err()
                .contains("2 demo commands")
        );
    }

    #[test]
    fn count_bounds() {
        assert!(validate_demo_count(0).is_err());
        assert!(validate_demo_count(1).is_ok());
        assert!(validate_demo_count(MAX_DEMO_COUNT).is_ok());
        assert!(validate_demo_count(MAX_DEMO_COUNT + 1).is_err());
    }
}
