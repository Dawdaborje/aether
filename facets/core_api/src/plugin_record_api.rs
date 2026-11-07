//! The record API: `/api/v1/<plugin>/<model>`, a plain REST surface over the models a plugin offers
//! (see `docs/architecture/api_and_webhooks.md`).
//!
//! ```text
//! GET    /api/v1/<plugin>/<model>?filter=<json>&fields=a,b&limit=50&after=<next>
//! POST   /api/v1/<plugin>/<model>            { fields }
//! GET    /api/v1/<plugin>/<model>/<id>
//! PATCH  /api/v1/<plugin>/<model>/<id>       { fields to change }
//! DELETE /api/v1/<plugin>/<model>/<id>
//! GET    /api/v1/<plugin>/openapi.json       what the caller can do, generated from the models
//! ```
//!
//! Nothing is offered unless the plugin lists the model in an `[[api]]` block of its `plugin.toml`.
//! Every call goes through the same code as a plugin's own `db::*` calls, as the caller: the model's
//! record and field rules, workflow and checks apply, a refused write changes nothing, and every access
//! is audited. A token's scopes narrow it further; a browser session works too.
//!
//! Errors are `{ "error": { "code", "message" } }`. A list is ordered by id and paged with `after`
//! (the `next` of the previous page), so a page never skips or repeats a record that is added meanwhile.

use std::collections::HashMap;
use std::net::SocketAddr;

use axum::{
    Extension, Json, Router,
    extract::{ConnectInfo, Path, Query, State},
    http::{HeaderMap, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::get,
};
use serde_json::{Map, Value, json};

use aether_core::access::api_token::ModelOp;
use aether_core::access::audit::{Actor, record_plugin_call};
use aether_core::access::identity::{Identity, identify_api, with_retry_after};
use aether_core::kernel::{HostError, kernel_command};
use aether_core::plugin_manager::api::{CallError, Prepared, prepare_call};
use aether_core::plugin_manager::models::plugin_def::API_OPERATIONS;
use aether_core::plugin_manager::openapi::{Exposed, document};
use aether_core::state::AppState;

const DEFAULT_LIMIT: u32 = 50;
const MAX_LIMIT: u32 = 200;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/{plugin}/openapi.json", get(openapi))
        .route("/api/v1/{plugin}/{model}", get(list).post(create))
        .route(
            "/api/v1/{plugin}/{model}/{id}",
            get(read).patch(update).delete(remove),
        )
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Op {
    List,
    Get,
    Create,
    Update,
    Delete,
}

impl Op {
    /// The word an `[[api]]` block uses.
    fn api_name(self) -> &'static str {
        match self {
            Self::List => "list",
            Self::Get => "get",
            Self::Create => "create",
            Self::Update => "update",
            Self::Delete => "delete",
        }
    }

    fn model_op(self) -> ModelOp {
        match self {
            Self::List | Self::Get => ModelOp::Read,
            Self::Create => ModelOp::Create,
            Self::Update => ModelOp::Update,
            Self::Delete => ModelOp::Delete,
        }
    }

    fn writes(self) -> bool {
        !matches!(self, Self::List | Self::Get)
    }
}

fn failure(status: StatusCode, code: &str, message: impl Into<String>) -> Response {
    (
        status,
        Json(json!({ "error": { "code": code, "message": message.into() } })),
    )
        .into_response()
}

fn call_failure(error: CallError) -> Response {
    let code = match error.status {
        StatusCode::UNAUTHORIZED => "unauthenticated",
        StatusCode::FORBIDDEN => "denied",
        StatusCode::NOT_FOUND => "not_found",
        StatusCode::TOO_MANY_REQUESTS => "rate_limited",
        _ => "internal",
    };
    failure(error.status, code, error.message)
}

fn host_failure(error: HostError) -> Response {
    match error {
        HostError::Denied(message) => failure(StatusCode::FORBIDDEN, "denied", message),
        HostError::Capability(_) | HostError::ModelDenied(_) | HostError::ModelPermission(..) => {
            failure(StatusCode::FORBIDDEN, "denied", "not allowed")
        }
        HostError::InvalidPayload(message) | HostError::Refused(message) | HostError::Message(message) => {
            failure(StatusCode::UNPROCESSABLE_ENTITY, "invalid", message)
        }
        other => {
            log::error!("record api: {other}");
            failure(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal",
                "something went wrong",
            )
        }
    }
}

/// Everything a request needs once it is known to be for a model the plugin offers.
struct Ready {
    prepared: Prepared,
    identity: Identity,
    org: aether_core::databases::Db,
}

/// Identify the caller and build the context for `plugin`, then check the model is offered for `op`
/// and (for a token) that its scopes allow it. Records the call in the audit when it is refused here.
async fn ready(
    state: &AppState,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: &HeaderMap,
    uri: &Uri,
    plugin: &str,
    model: Option<&str>,
    op: Op,
) -> Result<Ready, Response> {
    let peer_ip = peer.map(|Extension(ConnectInfo(address))| address.ip());
    let mut identity = identify_api(state, headers, peer_ip, uri)
        .await
        .map_err(|error| {
            let (status, message) = error.http();
            with_retry_after(failure(
                status,
                match status {
                    StatusCode::UNAUTHORIZED => "unauthenticated",
                    StatusCode::TOO_MANY_REQUESTS => "rate_limited",
                    StatusCode::FORBIDDEN => "denied",
                    _ => "internal",
                },
                message,
            ))
        })?;
    if !matches!(identity.actor, Actor::User(_)) {
        return Err(failure(
            StatusCode::UNAUTHORIZED,
            "unauthenticated",
            "sign in or send an API token",
        ));
    }
    if let (Some(token), Some(model)) = (&identity.token, model)
        && !token.scopes.allows_model(plugin, model, op.model_op())
    {
        return Err(failure(
            StatusCode::FORBIDDEN,
            "token_scope",
            "this API token may not do that to that model",
        ));
    }
    let function = match model {
        Some(model) => format!("api:{}:{model}", op.api_name()),
        None => "api:openapi".to_string(),
    };
    let trail = vec![format!("{plugin}.{function}")];
    let prepared = prepare_call(state, &mut identity, plugin, &function, trail, 0)
        .await
        .map_err(call_failure)?;
    let org = state.org(&identity.org_db).await.map_err(|error| {
        log::error!("record api: {error}");
        failure(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal",
            "database error",
        )
    })?;
    Ok(Ready {
        prepared,
        identity,
        org,
    })
}

/// The audit row for the call. If it cannot be written the call fails, like any other.
async fn audited(ready: &Ready, plugin: &str, model: &str, op: Op, response: Response) -> Response {
    let function = format!("api:{}:{model}", op.api_name());
    match record_plugin_call(
        &ready.org,
        &ready.identity.audit,
        plugin,
        &function,
        response.status().as_u16(),
    )
    .await
    {
        Ok(()) => response,
        Err(error) => {
            log::error!("record api audit failed: {error}");
            failure(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal",
                "audit unavailable",
            )
        }
    }
}

/// `Some(host)` with the commands the operation needs, when the plugin offers the model for it.
fn offered(ready: &Ready, model: &str, op: Op) -> Option<aether_core::kernel::PluginHostContext> {
    let offered = ready
        .prepared
        .loaded
        .manifest
        .api
        .iter()
        .any(|api| api.model == model && api.allows(op.api_name()));
    if !offered || ready.prepared.host.model(model).is_none() {
        return None;
    }
    // Offering a model in `[[api]]` is what allows the commands; the rules still decide per record.
    let mut host = ready.prepared.host.clone();
    host.granted_capabilities.insert(
        if op.writes() {
            "db::mutate"
        } else {
            "db::query"
        }
        .to_string(),
    );
    if op.writes() {
        host.granted_capabilities.insert("db::query".to_string());
    }
    Some(host)
}

fn not_offered() -> Response {
    failure(StatusCode::NOT_FOUND, "not_found", "no such model")
}

fn limit_of(params: &HashMap<String, String>) -> Result<u32, Response> {
    match params.get("limit") {
        None => Ok(DEFAULT_LIMIT),
        Some(text) => match text.parse::<u32>() {
            Ok(n) if (1..=MAX_LIMIT).contains(&n) => Ok(n),
            _ => Err(failure(
                StatusCode::UNPROCESSABLE_ENTITY,
                "invalid",
                format!("`limit` is a whole number from 1 to {MAX_LIMIT}"),
            )),
        },
    }
}

fn keep_fields(record: &mut Value, fields: &Option<Vec<String>>) {
    if let (Some(names), Some(map)) = (fields, record.as_object_mut()) {
        map.retain(|key, _| key == "id" || names.iter().any(|name| name == key));
    }
}

fn fields_of(params: &HashMap<String, String>) -> Option<Vec<String>> {
    params.get("fields").map(|text| {
        text.split(',')
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_string)
            .collect()
    })
}

async fn list(
    State(state): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    Path((plugin, model)): Path<(String, String)>,
    Query(params): Query<HashMap<String, String>>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    let ready = match ready(
        &state,
        peer,
        &headers,
        &uri,
        &plugin,
        Some(&model),
        Op::List,
    )
    .await
    {
        Ok(ready) => ready,
        Err(response) => return response,
    };
    let Some(host) = offered(&ready, &model, Op::List) else {
        return audited(&ready, &plugin, &model, Op::List, not_offered()).await;
    };
    let response = async {
        let limit = limit_of(&params)?;
        let mut filter = match params.get("filter") {
            None => Map::new(),
            Some(text) => match serde_json::from_str::<Value>(text) {
                Ok(Value::Object(map)) => map,
                _ => {
                    return Err(failure(
                        StatusCode::UNPROCESSABLE_ENTITY,
                        "invalid",
                        "`filter` is a JSON object like {\"status\":\"open\"}",
                    ));
                }
            },
        };
        if let Some(after) = params.get("after") {
            let page = json!({ "id": { "gt": after } });
            filter = if filter.is_empty() {
                page.as_object().cloned().unwrap_or_default()
            } else {
                let mut both = Map::new();
                both.insert("and".into(), json!([Value::Object(filter), page]));
                both
            };
        }
        let fields = fields_of(&params);
        let reply = kernel_command(
            &host,
            "db::find",
            json!({ "model": model, "filter": filter, "limit": limit + 1, "order": "id" }),
        )
        .await
        .map_err(host_failure)?;
        let mut rows: Vec<Value> = reply
            .get("data")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let more = rows.len() > limit as usize;
        rows.truncate(limit as usize);
        let next = if more {
            rows.last().and_then(|row| row.get("id")).cloned()
        } else {
            None
        };
        for row in &mut rows {
            keep_fields(row, &fields);
        }
        Ok::<Response, Response>(Json(json!({ "data": rows, "next": next })).into_response())
    }
    .await
    .unwrap_or_else(|response| response);
    audited(&ready, &plugin, &model, Op::List, response).await
}

async fn read(
    State(state): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    Path((plugin, model, id)): Path<(String, String, String)>,
    Query(params): Query<HashMap<String, String>>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    let ready = match ready(&state, peer, &headers, &uri, &plugin, Some(&model), Op::Get).await {
        Ok(ready) => ready,
        Err(response) => return response,
    };
    let Some(host) = offered(&ready, &model, Op::Get) else {
        return audited(&ready, &plugin, &model, Op::Get, not_offered()).await;
    };
    let response = match kernel_command(&host, "db::get", json!({ "model": model, "id": id })).await
    {
        Ok(reply) => match reply.get("data").filter(|data| !data.is_null()) {
            Some(data) => {
                let mut data = data.clone();
                keep_fields(&mut data, &fields_of(&params));
                Json(json!({ "data": data })).into_response()
            }
            None => failure(StatusCode::NOT_FOUND, "not_found", "no such record"),
        },
        Err(error) => host_failure(error),
    };
    audited(&ready, &plugin, &model, Op::Get, response).await
}

fn object_body(body: Value) -> Result<Map<String, Value>, Response> {
    match body {
        Value::Object(map) if !map.is_empty() => Ok(map),
        _ => Err(failure(
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid",
            "send a JSON object with the fields",
        )),
    }
}

async fn create(
    State(state): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    Path((plugin, model)): Path<(String, String)>,
    headers: HeaderMap,
    uri: Uri,
    Json(body): Json<Value>,
) -> Response {
    let ready = match ready(
        &state,
        peer,
        &headers,
        &uri,
        &plugin,
        Some(&model),
        Op::Create,
    )
    .await
    {
        Ok(ready) => ready,
        Err(response) => return response,
    };
    let Some(host) = offered(&ready, &model, Op::Create) else {
        return audited(&ready, &plugin, &model, Op::Create, not_offered()).await;
    };
    let response = match object_body(body) {
        Err(response) => response,
        Ok(data) => {
            match kernel_command(&host, "db::create", json!({ "model": model, "data": data })).await
            {
                Ok(reply) => (
                    StatusCode::CREATED,
                    Json(json!({ "data": reply.get("data") })),
                )
                    .into_response(),
                Err(error) => host_failure(error),
            }
        }
    };
    audited(&ready, &plugin, &model, Op::Create, response).await
}

/// Whether the caller can read the record (a record they may not read is "not found", for a change too).
async fn visible(
    host: &aether_core::kernel::PluginHostContext,
    model: &str,
    id: &str,
) -> Result<bool, Response> {
    kernel_command(host, "db::get", json!({ "model": model, "id": id }))
        .await
        .map(|reply| reply.get("data").is_some_and(|data| !data.is_null()))
        .map_err(host_failure)
}

async fn update(
    State(state): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    Path((plugin, model, id)): Path<(String, String, String)>,
    headers: HeaderMap,
    uri: Uri,
    Json(body): Json<Value>,
) -> Response {
    let ready = match ready(
        &state,
        peer,
        &headers,
        &uri,
        &plugin,
        Some(&model),
        Op::Update,
    )
    .await
    {
        Ok(ready) => ready,
        Err(response) => return response,
    };
    let Some(host) = offered(&ready, &model, Op::Update) else {
        return audited(&ready, &plugin, &model, Op::Update, not_offered()).await;
    };
    let response = async {
        let data = object_body(body)?;
        if !visible(&host, &model, &id).await? {
            return Err(failure(
                StatusCode::NOT_FOUND,
                "not_found",
                "no such record",
            ));
        }
        let reply = kernel_command(
            &host,
            "db::update",
            json!({ "model": model, "id": id, "data": data }),
        )
        .await
        .map_err(host_failure)?;
        Ok::<Response, Response>(Json(json!({ "data": reply.get("data") })).into_response())
    }
    .await
    .unwrap_or_else(|response| response);
    audited(&ready, &plugin, &model, Op::Update, response).await
}

async fn remove(
    State(state): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    Path((plugin, model, id)): Path<(String, String, String)>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    let ready = match ready(
        &state,
        peer,
        &headers,
        &uri,
        &plugin,
        Some(&model),
        Op::Delete,
    )
    .await
    {
        Ok(ready) => ready,
        Err(response) => return response,
    };
    let Some(host) = offered(&ready, &model, Op::Delete) else {
        return audited(&ready, &plugin, &model, Op::Delete, not_offered()).await;
    };
    let response = async {
        if !visible(&host, &model, &id).await? {
            return Err(failure(
                StatusCode::NOT_FOUND,
                "not_found",
                "no such record",
            ));
        }
        kernel_command(&host, "db::delete", json!({ "model": model, "id": id }))
            .await
            .map_err(host_failure)?;
        Ok::<Response, Response>(Json(json!({ "deleted": id })).into_response())
    }
    .await
    .unwrap_or_else(|response| response);
    audited(&ready, &plugin, &model, Op::Delete, response).await
}

/// `GET /api/v1/<plugin>/openapi.json`: what the caller can do with this plugin's record API.
async fn openapi(
    State(state): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    Path(plugin): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    let ready = match ready(&state, peer, &headers, &uri, &plugin, None, Op::List).await {
        Ok(ready) => ready,
        Err(response) => return response,
    };
    let loaded = &ready.prepared.loaded;
    let host = &ready.prepared.host;
    let mut hidden_and_locked = Vec::new();
    for api in &loaded.manifest.api {
        let (Some(def), Some(grant)) = (
            loaded.models.iter().find(|m| m.name == api.model),
            host.model(&api.model),
        ) else {
            continue;
        };
        let mut operations: Vec<&'static str> = API_OPERATIONS
            .iter()
            .copied()
            .filter(|op| api.allows(op))
            .filter(|op| {
                let writes = !matches!(*op, "list" | "get");
                (if writes {
                    grant.can_write
                } else {
                    grant.can_read
                }) && ready.identity.token.as_ref().is_none_or(|token| {
                    let model_op = match *op {
                        "list" | "get" => ModelOp::Read,
                        "create" => ModelOp::Create,
                        "update" => ModelOp::Update,
                        _ => ModelOp::Delete,
                    };
                    token.scopes.allows_model(&plugin, &api.model, model_op)
                })
            })
            .collect();
        let (hidden, locked) =
            match aether_core::kernel::host::guard::field_limits(host, grant).await {
                Ok(limits) => limits,
                Err(error) => return host_failure(error),
            };
        if operations.is_empty() {
            operations = Vec::new();
        }
        hidden_and_locked.push((def, operations, hidden, locked));
    }
    let exposed: Vec<Exposed<'_>> = hidden_and_locked
        .iter()
        .map(|(model, operations, hidden, locked)| Exposed {
            model,
            operations: operations.clone(),
            hidden: hidden.clone(),
            locked: locked.clone(),
        })
        .collect();
    let spec = document(&plugin, &loaded.manifest.plugin.version, &exposed);
    Json(spec).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn limit_is_a_whole_number_in_range() {
        assert_eq!(limit_of(&params(&[])).ok(), Some(DEFAULT_LIMIT));
        assert_eq!(limit_of(&params(&[("limit", "1")])).ok(), Some(1));
        assert_eq!(limit_of(&params(&[("limit", "200")])).ok(), Some(200));
        for bad in ["0", "201", "-1", "ten", "", "1.5"] {
            assert!(limit_of(&params(&[("limit", bad)])).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn fields_keep_the_id_and_what_was_asked() {
        let fields = fields_of(&params(&[("fields", "title, status,,")]));
        assert_eq!(fields, Some(vec!["title".into(), "status".into()]));
        let mut record = json!({ "id": "t:1", "title": "A", "status": "new", "amount": "1.00" });
        keep_fields(&mut record, &fields);
        assert_eq!(
            record,
            json!({ "id": "t:1", "title": "A", "status": "new" })
        );
        let mut all = json!({ "id": "t:1", "title": "A" });
        keep_fields(&mut all, &fields_of(&params(&[])));
        assert_eq!(
            all,
            json!({ "id": "t:1", "title": "A" }),
            "no `fields`: everything"
        );
    }

    #[test]
    fn a_body_must_be_a_non_empty_object() {
        assert!(object_body(json!({ "a": 1 })).is_ok());
        for bad in [json!({}), json!([1]), json!("x"), json!(null), json!(3)] {
            assert!(object_body(bad).is_err());
        }
    }

    #[test]
    fn operations_map_to_what_a_token_scope_names() {
        assert_eq!(Op::List.model_op(), ModelOp::Read);
        assert_eq!(Op::Get.model_op(), ModelOp::Read);
        assert_eq!(Op::Create.model_op(), ModelOp::Create);
        assert_eq!(Op::Update.model_op(), ModelOp::Update);
        assert_eq!(Op::Delete.model_op(), ModelOp::Delete);
        assert!(!Op::List.writes() && !Op::Get.writes());
        assert!(Op::Create.writes() && Op::Update.writes() && Op::Delete.writes());
        let names: Vec<&str> = [Op::List, Op::Get, Op::Create, Op::Update, Op::Delete]
            .map(Op::api_name)
            .to_vec();
        assert_eq!(
            names,
            API_OPERATIONS.to_vec(),
            "the words an `[[api]]` block uses"
        );
    }
}
