//! `POST /api/ui/records/titles`: the display names of records, for showing a link as a name
//! instead of an id (a list cell, a group heading, a picker).
//!
//! The name of a record is the model's `title_field`, set in its `models/<name>.json` like
//! Django's `__str__` or Odoo's `_rec_name`. A plugin does not need to ship a lookup function
//! for it.
//!
//! The caller must be a logged-in member. A model whose `rules/` limit who may read it only
//! gives its titles to people the rules let read without a record condition (everyone, or a role
//! they hold); otherwise nothing is returned and the page keeps showing the id.

use std::collections::HashMap;

use axum::{
    Extension, Json, Router,
    extract::{ConnectInfo, State},
    http::{HeaderMap, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::post,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use surrealdb::types::{RecordId, SurrealValue};

use crate::access::{audit::Actor, identity::identify};
use crate::data_model::definition::{ModelDef, fill_template, template_fields};
use crate::data_model::rules::{Operation, RuleSet};
use crate::plugin_manager::{installed, revisions, surface};
use crate::state::AppState;
use std::net::SocketAddr;

const MAX_IDS: usize = 500;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/ui/records/titles", post(titles))
        .route("/api/ui/records/options", post(options))
}

#[derive(Debug, Deserialize)]
struct TitlesBody {
    /// The plugin of the page asking: where an unqualified `target` is looked for.
    plugin: String,
    /// The model, as a link field names it: `department` or `hr.department`.
    target: String,
    ids: Vec<String>,
}

#[derive(Debug, Serialize)]
struct TitlesAnswer {
    titles: HashMap<String, String>,
}

fn refuse(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({ "error": message }))).into_response()
}

fn empty() -> Response {
    Json(TitlesAnswer {
        titles: HashMap::new(),
    })
    .into_response()
}

/// A model this person may read titles of: where its records are, and how a title is made.
struct Readable {
    org: crate::state::Db,
    table: String,
    /// `(field name, column id)` of every field a title uses.
    columns: Vec<(String, String)>,
    template: Option<String>,
    /// The column searched for options, from `title_field` (or the template's first field).
    search_column: String,
}

impl Readable {
    /// The SELECT list: each used column under its field name.
    fn select(&self) -> String {
        self.columns
            .iter()
            .map(|(name, column)| format!("{column} AS `{name}`"))
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn title_of(&self, row: &Value) -> Option<String> {
        let text = |name: &str| match row.get(name) {
            Some(Value::String(text)) => Some(text.clone()),
            Some(Value::Null) | None => None,
            Some(other) => Some(other.to_string()),
        };
        let title = match &self.template {
            Some(template) => fill_template(template, text),
            None => self.columns.first().and_then(|(name, _)| text(name))?,
        };
        (!title.trim().is_empty()).then_some(title)
    }
}

/// Find `target` for a caller and check they may read titles of it. `Ok(None)`: not available
/// (unknown, not installed, or limited by rules): the page keeps showing the id.
async fn readable(
    state: &AppState,
    identity: &crate::access::identity::Identity,
    user_id: &str,
    plugin_of_page: &str,
    target: &str,
) -> Result<Option<Readable>, String> {
    let (plugin, model_name) = match target.split_once('.') {
        Some((plugin, model)) => (plugin.to_string(), model.to_string()),
        None => (plugin_of_page.to_string(), target.to_string()),
    };
    let org = state
        .org(&identity.org_db)
        .await
        .map_err(|e| e.to_string())?;
    let rows = installed::list_cached(&state.cache, &identity.org_db, &org)
        .await
        .map_err(|e| e.to_string())?;
    let Some(version) = rows
        .iter()
        .find(|row| row.is_enabled && row.plugin_name == plugin)
        .map(|row| row.version.clone())
    else {
        return Ok(None);
    };
    let core = state.core().await.map_err(|e| e.to_string())?;
    let models = surface::models_cached(&state.cache, &core, &plugin, &version)
        .await
        .map_err(|e| e.to_string())?;
    let Some(definition) = models
        .into_iter()
        .find(|(name, _)| *name == model_name)
        .map(|(_, definition)| definition)
    else {
        return Ok(None);
    };
    let Ok(model) = serde_json::from_value::<ModelDef>(definition) else {
        return Ok(None);
    };
    let Some(table) = model.model_id.clone() else {
        return Ok(None);
    };
    let column_of = |name: &str| {
        model
            .fields
            .iter()
            .find(|field| field.name == name)
            .and_then(|field| field.id.clone())
    };
    let used: Vec<String> = match (&model.title_template, &model.title_field) {
        (Some(template), _) => template_fields(template),
        (None, Some(title)) => vec![title.clone()],
        (None, None) => return Ok(None),
    };
    let mut columns = Vec::new();
    for name in used {
        let Some(column) = column_of(&name) else {
            return Ok(None);
        };
        columns.push((name, column));
    }
    let search_name = model
        .title_field
        .clone()
        .or_else(|| columns.first().map(|(name, _)| name.clone()));
    let Some(search_column) = search_name.and_then(|name| column_of(&name)) else {
        return Ok(None);
    };

    let is_developer = identity
        .session
        .as_ref()
        .is_some_and(|session| session.user.is_super_user);
    if !is_developer {
        let roles = crate::roles::roles_of_cached(&state.cache, &identity.org_db, &org, user_id)
            .await
            .map_err(|e| e.to_string())?;
        let layout = crate::app_dir::AppDir::new(&state.config.app_dir);
        let revision = surface_revision(&core, &plugin, &version).await;
        if !may_read_titles(
            &layout,
            &plugin,
            revision.as_deref(),
            &version,
            &model_name,
            &roles,
        )
        .await
        {
            return Ok(None);
        }
    }
    Ok(Some(Readable {
        org,
        table,
        columns,
        template: model.title_template.clone(),
        search_column,
    }))
}

async fn caller(
    state: &AppState,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: &HeaderMap,
    uri: &Uri,
) -> Result<(crate::access::identity::Identity, String), Response> {
    let peer_ip = peer.map(|Extension(ConnectInfo(address))| address.ip());
    let identity = match identify(state, headers, peer_ip, uri).await {
        Ok(identity) => identity,
        Err(error) => {
            let (status, message) = error.http();
            return Err(refuse(status, message));
        }
    };
    match &identity.actor {
        Actor::User(user_id) => {
            let user_id = user_id.clone();
            Ok((identity, user_id))
        }
        _ => Err(refuse(StatusCode::UNAUTHORIZED, "not authenticated")),
    }
}

async fn titles(
    State(state): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    uri: Uri,
    Json(body): Json<TitlesBody>,
) -> Response {
    let (identity, user_id) = match caller(&state, peer, &headers, &uri).await {
        Ok(found) => found,
        Err(response) => return response,
    };
    let wanted: Vec<RecordId> = body
        .ids
        .iter()
        .take(MAX_IDS)
        .filter_map(|id| id.split_once(':'))
        .map(|(table, key)| RecordId::new(table, key.to_string()))
        .collect();
    if wanted.is_empty() {
        return empty();
    }
    let result: Result<Option<HashMap<String, String>>, String> = async {
        let Some(model) = readable(&state, &identity, &user_id, &body.plugin, &body.target).await?
        else {
            return Ok(None);
        };
        // `table` and the columns come from the stored model definition, never from the request.
        let sql = format!(
            "SELECT <string> id AS id, {} FROM {} WHERE id IN $ids;",
            model.select(),
            model.table
        );
        let mut response = model
            .org
            .query(sql)
            .bind(("ids", wanted))
            .await
            .and_then(|r| r.check())
            .map_err(|e| e.to_string())?;
        let rows: Vec<Value> = response.take(0).map_err(|e| e.to_string())?;
        let mut out = HashMap::new();
        for row in rows {
            let Some(id) = row.get("id").and_then(Value::as_str) else {
                continue;
            };
            if let Some(title) = model.title_of(&row) {
                // Ids with unusual characters come back wrapped in angle brackets.
                out.insert(id.replace(['⟨', '⟩'], ""), title);
            }
        }
        Ok(Some(out))
    }
    .await;

    match result {
        Ok(Some(titles)) => Json(TitlesAnswer { titles }).into_response(),
        Ok(None) => empty(),
        Err(error) => {
            log::warn!("record titles for `{}`: {error}", body.target);
            refuse(
                StatusCode::INTERNAL_SERVER_ERROR,
                "could not read the titles",
            )
        }
    }
}

#[derive(Debug, Deserialize)]
struct OptionsBody {
    plugin: String,
    target: String,
    /// What the person typed; blank lists the first records.
    #[serde(default)]
    q: String,
    #[serde(default)]
    limit: Option<usize>,
}

/// `POST /api/ui/records/options`: records of a model by title, for a link picker that was given
/// no `source` function: `{ options: [{ id, title }] }`.
async fn options(
    State(state): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    uri: Uri,
    Json(body): Json<OptionsBody>,
) -> Response {
    let (identity, user_id) = match caller(&state, peer, &headers, &uri).await {
        Ok(found) => found,
        Err(response) => return response,
    };
    let limit = body.limit.unwrap_or(50).clamp(1, 200);
    let result: Result<Vec<Value>, String> = async {
        let Some(model) = readable(&state, &identity, &user_id, &body.plugin, &body.target).await?
        else {
            return Ok(Vec::new());
        };
        let needle = body.q.trim().to_lowercase();
        let filter = if needle.is_empty() {
            String::new()
        } else {
            format!(
                "WHERE string::lowercase(<string> {}) CONTAINS $q",
                model.search_column
            )
        };
        let sql = format!(
            "SELECT <string> id AS id, {} FROM {} {filter} ORDER BY {} LIMIT {limit};",
            model.select(),
            model.table,
            model.search_column
        );
        let mut query = model.org.query(sql);
        if !needle.is_empty() {
            query = query.bind(("q", needle));
        }
        let mut response = query
            .await
            .and_then(|r| r.check())
            .map_err(|e| e.to_string())?;
        let rows: Vec<Value> = response.take(0).map_err(|e| e.to_string())?;
        Ok(rows
            .into_iter()
            .filter_map(|row| {
                let id = row.get("id")?.as_str()?.replace(['⟨', '⟩'], "");
                let title = model.title_of(&row)?;
                Some(json!({ "id": id, "title": title }))
            })
            .collect())
    }
    .await;
    match result {
        Ok(options) => Json(json!({ "options": options })).into_response(),
        Err(error) => {
            log::warn!("record options for `{}`: {error}", body.target);
            refuse(
                StatusCode::INTERNAL_SERVER_ERROR,
                "could not read the records",
            )
        }
    }
}

#[derive(Debug, Deserialize, SurrealValue)]
struct RevisionRow {
    revision: Option<String>,
}

async fn surface_revision(
    core: &surrealdb::Surreal<surrealdb::engine::remote::ws::Client>,
    plugin: &str,
    version: &str,
) -> Option<String> {
    let mut response = core
        .query("SELECT revision FROM plugins WHERE name = $name AND version = $version LIMIT 1;")
        .bind(("name", plugin.to_string()))
        .bind(("version", version.to_string()))
        .await
        .ok()?;
    let rows: Vec<RevisionRow> = response.take(0).ok()?;
    rows.into_iter().next()?.revision
}

/// Whether `roles` may read this model's records without a record-level condition, according to
/// the plugin's rule files. No rules, or no rule on reading, means open.
async fn may_read_titles(
    layout: &crate::app_dir::AppDir,
    plugin: &str,
    revision: Option<&str>,
    version: &str,
    model: &str,
    roles: &[String],
) -> bool {
    let folder = revision.unwrap_or(version);
    let Ok(Some(index)) = revisions::read_index(layout, plugin, folder).await else {
        return true;
    };
    for (logical, entry) in &index.files {
        if !(logical.starts_with("rules/") && logical.ends_with(".json")) {
            continue;
        }
        let path = layout
            .root()
            .join(revisions::relative_path(plugin, entry, logical));
        let Ok(text) = tokio::fs::read_to_string(&path).await else {
            continue;
        };
        let Ok(set) = RuleSet::parse(&text) else {
            continue;
        };
        if set.model != model {
            continue;
        }
        let reads: Vec<_> = set
            .access
            .iter()
            .filter(|rule| rule.operations.contains(&Operation::Read))
            .collect();
        if reads.is_empty() {
            return true;
        }
        return reads.iter().any(|rule| {
            rule.when.is_none()
                && (rule.roles.is_empty()
                    || rule.roles.iter().any(|role| {
                        let full = crate::data_model::rules::qualify(plugin, role);
                        roles
                            .iter()
                            .any(|held| *held == full || held == "org_admin")
                    }))
        });
    }
    true
}
