//! The chatter endpoints, all under `/api/chatter`.
//!
//! * `GET    /{plugin}/{model}/{key}`                          the thread, followers and what the caller may do
//! * `POST   /{plugin}/{model}/{key}/messages`                 post a message or a note
//! * `PATCH  /{plugin}/{model}/{key}/messages/{id}`            edit your own message
//! * `DELETE /{plugin}/{model}/{key}/messages/{id}`            move a message to the trash
//! * `POST   /{plugin}/{model}/{key}/messages/{id}/restore`    take it back out (developers)
//! * `PUT    /{plugin}/{model}/{key}/follow`                   follow, unfollow or mute
//! * `GET    /people?q=`                                       members to @mention
//!
//! A model without chatter answers `{ "enabled": false }` and nothing else works for it. Members
//! need the model in the plugin's `access_models`; visitors need the model's `visitors` setting
//! and read access to the model through the plugin's public surface.

use std::collections::{BTreeSet, HashMap};
use std::{net::SocketAddr, sync::Arc};

use axum::{
    Extension, Json, Router,
    extract::{ConnectInfo, Path, Query, State},
    http::{HeaderMap, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use axum_extra::extract::CookieJar;
use serde::Deserialize;
use serde_json::{Value, json};
use surrealdb::{Surreal, engine::remote::ws::Client, types::SurrealValue};

use super::store::{self, Kind, Message, NewMessage};
use crate::access::{
    audit::Actor,
    identity::{Identity, ensure_visitor, identify, with_retry_after, with_visitor_cookie},
};
use crate::data_model::{ChatterDef, ModelSchema, VisitorChatter};
use crate::notifications::{
    Audience, HubMessage, Level, NewNotification, UiEvent,
    send as send_notification,
};
use crate::plugin_manager::access::{anonymous_grants, user_grants};
use crate::state::AppState;

/// Longest message or note, in characters.
const MAX_BODY_CHARS: usize = 10_000;
/// Longest a visitor's message may be.
const MAX_VISITOR_BODY_CHARS: usize = 2_000;
const MAX_GUEST_NAME_CHARS: usize = 60;
const MAX_MENTIONS: usize = 20;
/// Lines of a thread sent at once.
const THREAD_LIMIT: u32 = 500;
const EXCERPT_CHARS: usize = 160;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/chatter/people", get(people))
        .route("/api/chatter/{plugin}/{model}/{key}", get(thread))
        .route("/api/chatter/{plugin}/{model}/{key}/messages", post(post_message))
        .route(
            "/api/chatter/{plugin}/{model}/{key}/messages/{id}",
            axum::routing::patch(edit_message).delete(delete_message),
        )
        .route("/api/chatter/{plugin}/{model}/{key}/messages/{id}/restore", post(restore_message))
        .route("/api/chatter/{plugin}/{model}/{key}/follow", put(set_following))
}

fn failure(status: StatusCode, message: impl AsRef<str>) -> Response {
    (status, Json(json!({ "error": message.as_ref() }))).into_response()
}

fn database_failure(context: &str, error: impl std::fmt::Display) -> Response {
    log::error!("chatter {context}: {error}");
    failure(StatusCode::INTERNAL_SERVER_ERROR, "database error")
}

#[derive(Deserialize)]
struct Address {
    plugin: String,
    model: String,
    key: String,
}

#[derive(Debug, Deserialize, SurrealValue)]
struct InstalledRow {
    version: String,
    is_enabled: bool,
}

#[derive(Debug, Deserialize, SurrealValue)]
struct PublicPageModels {
    models: Vec<String>,
}

/// A request resolved to one record's chatter, with what the caller may do in it.
struct Scope {
    identity: Identity,
    org: crate::state::Db,
    plugin: String,
    schema: Arc<ModelSchema>,
    chatter: ChatterDef,
    key: String,
    member: bool,
    developer: bool,
    /// May read the record, so may read its thread.
    can_read: bool,
    can_note: bool,
    can_post_message: bool,
}

impl Scope {
    fn actor(&self) -> String {
        self.identity.actor.id().unwrap_or("system").to_string()
    }

    fn table(&self) -> &str {
        &self.schema.table
    }

    /// The kinds of line the caller may see.
    fn visible_kinds(&self) -> Vec<Kind> {
        let mut kinds = Vec::new();
        if self.chatter.messages {
            kinds.push(Kind::Message);
        }
        if self.member {
            if self.chatter.notes {
                kinds.push(Kind::Note);
            }
            kinds.push(Kind::System);
            if self.chatter.track_changes {
                kinds.push(Kind::Change);
            }
        }
        kinds
    }
}

/// What resolving a request can end in besides a scope.
enum Resolved {
    Scope(Box<Scope>),
    /// Chatter is not on for this model (or for the organization).
    Off,
}

async fn who(
    state: &AppState,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: &HeaderMap,
    uri: &Uri,
) -> Result<Identity, Response> {
    let peer_ip = peer.map(|Extension(ConnectInfo(address))| address.ip());
    identify(state, headers, peer_ip, uri).await.map_err(|error| {
        let (status, message) = error.http();
        with_retry_after(failure(status, message))
    })
}

fn refuse(identity: &Identity) -> Response {
    if identity.foreign_user {
        failure(StatusCode::FORBIDDEN, "not a member of this organization")
    } else if matches!(identity.actor, Actor::User(_)) {
        failure(StatusCode::FORBIDDEN, "not allowed")
    } else {
        failure(StatusCode::UNAUTHORIZED, "not authenticated")
    }
}

async fn resolve(state: &AppState, identity: Identity, address: &Address) -> Result<Resolved, Response> {
    let member = matches!(identity.actor, Actor::User(_));
    let org = state
        .org(&identity.org_db)
        .await
        .map_err(|error| database_failure("organization", error))?;
    let core = state.core().await.map_err(|error| database_failure("core", error))?;

    let mut response = org
        .query("SELECT version, is_enabled FROM installed_plugins WHERE plugin_name = $name LIMIT 1;")
        .bind(("name", address.plugin.clone()))
        .await
        .and_then(|response| response.check())
        .map_err(|error| database_failure("installation", error))?;
    let installed: Vec<InstalledRow> =
        response.take(0).map_err(|error| database_failure("installation decode", error))?;
    let Some(installed) = installed.into_iter().next() else {
        return Err(if member {
            failure(StatusCode::NOT_FOUND, "plugin is not installed")
        } else {
            refuse(&identity)
        });
    };
    if !installed.is_enabled {
        return Err(failure(StatusCode::FORBIDDEN, "plugin is disabled"));
    }

    let loaded = state
        .plugin_runtime
        .ensure_loaded(&core, &address.plugin, &installed.version)
        .await
        .map_err(|error| {
            log::error!("chatter plugin load: {error}");
            failure(StatusCode::SERVICE_UNAVAILABLE, "plugin version is unavailable")
        })?;
    let Some(schema) = loaded.schemas.get(&address.model).cloned() else {
        return Err(failure(StatusCode::NOT_FOUND, "no such model"));
    };
    let Some(chatter) = schema.chatter.clone() else {
        return Ok(Resolved::Off);
    };
    let org_ref = super::org_ref(&identity.org_db);
    if !super::enabled_in(state, &org_ref).await {
        return Ok(Resolved::Off);
    }

    let manifest = &loaded.manifest;
    let (can_read, can_write) = if member {
        let grants = user_grants(manifest, &loaded.schemas);
        grants.get(&address.model).map_or((false, false), |grant| (grant.can_read, grant.can_write))
    } else {
        if chatter.visitors == VisitorChatter::None {
            return Err(refuse(&identity));
        }
        let mut response = core
            .query(
                "SELECT models FROM plugin_ui_pages \
                 WHERE is_public = true AND plugin.name = $name AND plugin.version = $version;",
            )
            .bind(("name", address.plugin.clone()))
            .bind(("version", installed.version.clone()))
            .await
            .and_then(|response| response.check())
            .map_err(|error| database_failure("public pages", error))?;
        let pages: Vec<PublicPageModels> =
            response.take(0).map_err(|error| database_failure("public pages decode", error))?;
        let public: BTreeSet<String> = pages.into_iter().flat_map(|page| page.models).collect();
        let grants = anonymous_grants(manifest, &loaded.schemas, &public);
        (grants.get(&address.model).is_some_and(|grant| grant.can_read), false)
    };
    if !can_read {
        return Err(refuse(&identity));
    }

    let key = address.key.strip_prefix(&format!("{}:", schema.table)).unwrap_or(&address.key).to_string();
    let mut response = org
        .query("SELECT VALUE id FROM type::record($table, $key);")
        .bind(("table", schema.table.clone()))
        .bind(("key", key.clone()))
        .await
        .and_then(|response| response.check())
        .map_err(|error| database_failure("record lookup", error))?;
    let found: Vec<surrealdb::types::RecordId> =
        response.take(0).map_err(|error| database_failure("record decode", error))?;
    if found.is_empty() {
        return Err(failure(StatusCode::NOT_FOUND, "no such record"));
    }

    let developer = identity.session.as_ref().is_some_and(|session| session.user.is_super_user);
    Ok(Resolved::Scope(Box::new(Scope {
        member,
        developer,
        can_read,
        can_note: member && can_write && chatter.notes,
        can_post_message: chatter.messages
            && if member {
                true
            } else {
                chatter.visitors == VisitorChatter::ReadWrite
            },
        identity,
        org,
        plugin: address.plugin.clone(),
        schema,
        chatter,
        key,
    })))
}

/// Resolve, or answer for a model with chatter off.
macro_rules! scope_or_return {
    ($state:expr, $identity:expr, $address:expr) => {
        match resolve(&$state, $identity, &$address).await {
            Ok(Resolved::Scope(scope)) => scope,
            Ok(Resolved::Off) => return Json(json!({ "enabled": false })).into_response(),
            Err(response) => return response,
        }
    };
}

// --- names -------------------------------------------------------------------------------

#[derive(Debug, SurrealValue)]
struct NameRow {
    id: String,
    username: Option<String>,
    display_name: Option<String>,
}

#[derive(Debug, SurrealValue)]
struct OrgNameRow {
    core_user_id: String,
    display_name: Option<String>,
}

/// Names to show for users: the organization's display name, else the account's.
async fn names(state: &AppState, org: &Surreal<Client>, actors: &[String]) -> HashMap<String, String> {
    let users: Vec<String> = actors.iter().filter(|actor| actor.starts_with("users:")).cloned().collect();
    let mut out = HashMap::new();
    if users.is_empty() {
        return out;
    }
    match state.core().await {
        Ok(core) => {
            let rows = core
                .query(
                    "LET $records = array::map($ids, |$i| type::record($i)); \
                     SELECT <string> id AS id, username, display_name FROM users WHERE id IN $records;",
                )
                .bind(("ids", users.clone()))
                .await
                .and_then(|response| response.check());
            match rows {
                Ok(mut response) => match response.take::<Vec<NameRow>>(1) {
                    Ok(rows) => {
                        for row in rows {
                            if let Some(name) = row.display_name.or(row.username) {
                                out.insert(row.id, name);
                            }
                        }
                    }
                    Err(error) => log::warn!("chatter names decode: {error}"),
                },
                Err(error) => log::warn!("chatter names: {error}"),
            }
        }
        Err(error) => log::warn!("chatter names core: {error}"),
    }
    let org_names = org
        .query("SELECT core_user_id, display_name FROM org_users WHERE core_user_id IN $ids;")
        .bind(("ids", users))
        .await
        .and_then(|response| response.check());
    match org_names {
        Ok(mut response) => match response.take::<Vec<OrgNameRow>>(0) {
            Ok(rows) => {
                for row in rows {
                    if let Some(name) = row.display_name.filter(|name| !name.trim().is_empty()) {
                        out.insert(row.core_user_id, name);
                    }
                }
            }
            Err(error) => log::warn!("chatter org names decode: {error}"),
        },
        Err(error) => log::warn!("chatter org names: {error}"),
    }
    out
}

fn message_json(message: &Message, names: &HashMap<String, String>) -> Value {
    let mut value = serde_json::to_value(message).unwrap_or(Value::Null);
    if let Value::Object(object) = &mut value {
        let author = names
            .get(&message.author)
            .cloned()
            .or_else(|| message.author_name.clone())
            .unwrap_or_else(|| if message.author == "system" { "System".into() } else { "Someone".into() });
        object.insert("author_label".into(), Value::String(author));
    }
    value
}

// --- reading -----------------------------------------------------------------------------

#[derive(Deserialize)]
struct ThreadQuery {
    #[serde(default)]
    trash: bool,
}

async fn thread(
    State(state): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    Path(address): Path<Address>,
    Query(query): Query<ThreadQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    let identity = match who(&state, peer, &headers, &uri).await {
        Ok(identity) => identity,
        Err(response) => return response,
    };
    let scope = scope_or_return!(state, identity, address);
    let with_trash = query.trash && scope.developer;
    let (table, key) = (scope.table().to_string(), scope.key.clone());

    let messages = match store::thread(&scope.org, &table, &key, &scope.visible_kinds(), with_trash, THREAD_LIMIT).await {
        Ok(messages) => messages,
        Err(error) => return database_failure("thread", error),
    };
    let followers = if scope.member && scope.chatter.followers {
        match store::followers(&scope.org, &table, &key).await {
            Ok(followers) => followers,
            Err(error) => return database_failure("followers", error),
        }
    } else {
        Vec::new()
    };
    let trashed = if scope.developer {
        store::trashed_count(&scope.org, &table, &key).await.unwrap_or(0)
    } else {
        0
    };

    let mut actors: Vec<String> = messages.iter().map(|m| m.author.clone()).collect();
    actors.extend(followers.iter().map(|f| f.actor.clone()));
    actors.sort();
    actors.dedup();
    let names = names(&state, &scope.org, &actors).await;

    let me = scope.actor();
    let mine = followers.iter().find(|follower| follower.actor == me);
    Json(json!({
        "enabled": true,
        "config": {
            "messages": scope.chatter.messages,
            "notes": scope.chatter.notes,
            "followers": scope.chatter.followers,
            "track_changes": scope.chatter.track_changes,
        },
        "can": {
            "message": scope.can_post_message,
            "note": scope.can_note,
            "follow": scope.member && scope.chatter.followers,
            "trash": scope.developer,
        },
        "me": me,
        "member": scope.member,
        "tracked": scope.schema.tracked_fields(),
        "messages": messages.iter().map(|m| message_json(m, &names)).collect::<Vec<_>>(),
        "followers": followers.iter().map(|follower| json!({
            "actor": follower.actor,
            "label": names.get(&follower.actor).cloned().unwrap_or_else(|| "Someone".into()),
            "reason": follower.reason,
            "muted": follower.muted,
        })).collect::<Vec<_>>(),
        "following": mine.is_some(),
        "muted": mine.is_some_and(|follower| follower.muted),
        "trash_count": trashed,
    }))
    .into_response()
}

#[derive(Deserialize)]
struct PeopleQuery {
    #[serde(default)]
    q: String,
}

#[derive(Debug, SurrealValue)]
struct PersonRow {
    core_user_id: String,
}

/// Members whose name starts with what was typed after an `@`.
async fn people(
    State(state): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    Query(query): Query<PeopleQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    let identity = match who(&state, peer, &headers, &uri).await {
        Ok(identity) => identity,
        Err(response) => return response,
    };
    if !matches!(identity.actor, Actor::User(_)) {
        return refuse(&identity);
    }
    let org = match state.org(&identity.org_db).await {
        Ok(org) => org,
        Err(error) => return database_failure("people organization", error),
    };
    let mut response = match org
        .query("SELECT core_user_id FROM org_users WHERE is_active = true LIMIT 500;")
        .await
        .and_then(|response| response.check())
    {
        Ok(response) => response,
        Err(error) => return database_failure("people", error),
    };
    let rows: Vec<PersonRow> = match response.take(0) {
        Ok(rows) => rows,
        Err(error) => return database_failure("people decode", error),
    };
    let actors: Vec<String> = rows.into_iter().map(|row| row.core_user_id).collect();
    let names = names(&state, &org, &actors).await;
    let needle = query.q.trim().to_lowercase();
    let mut people: Vec<Value> = actors
        .iter()
        .filter_map(|actor| {
            let name = names.get(actor)?;
            name.to_lowercase().contains(&needle).then(|| json!({ "actor": actor, "name": name }))
        })
        .collect();
    people.truncate(8);
    Json(json!({ "people": people })).into_response()
}

// --- writing -----------------------------------------------------------------------------

#[derive(Deserialize)]
struct PostBody {
    #[serde(default = "default_kind")]
    kind: String,
    body: String,
    #[serde(default)]
    mentions: Vec<String>,
    /// An app path to the record's page, for the notification.
    #[serde(default)]
    link: Option<String>,
    /// Required from visitors.
    #[serde(default)]
    guest_name: Option<String>,
}

fn default_kind() -> String {
    "message".into()
}

fn clean_body(text: &str, limit: usize) -> Result<String, Response> {
    let text = text.trim();
    if text.is_empty() {
        return Err(failure(StatusCode::UNPROCESSABLE_ENTITY, "write something first"));
    }
    if text.chars().count() > limit {
        return Err(failure(StatusCode::UNPROCESSABLE_ENTITY, format!("at most {limit} characters")));
    }
    Ok(text.to_string())
}

async fn post_message(
    State(state): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    Path(address): Path<Address>,
    headers: HeaderMap,
    uri: Uri,
    jar: CookieJar,
    Json(request): Json<PostBody>,
) -> Response {
    let identity = match who(&state, peer, &headers, &uri).await {
        Ok(identity) => identity,
        Err(response) => return response,
    };
    let response = post_inner(&state, identity, &address, request, jar.clone()).await;
    with_retry_after(response)
}

async fn post_inner(
    state: &AppState,
    identity: Identity,
    address: &Address,
    request: PostBody,
    jar: CookieJar,
) -> Response {
    let mut scope = scope_or_return!(state, identity, *address);
    let Some(kind) = Kind::parse(&request.kind).filter(|kind| matches!(kind, Kind::Message | Kind::Note)) else {
        return failure(StatusCode::UNPROCESSABLE_ENTITY, "kind must be `message` or `note`");
    };
    let allowed = match kind {
        Kind::Note => scope.can_note,
        _ => scope.can_post_message,
    };
    if !allowed {
        return failure(StatusCode::FORBIDDEN, "you cannot post this here");
    }
    let limit = if scope.member { MAX_BODY_CHARS } else { MAX_VISITOR_BODY_CHARS };
    let body = match clean_body(&request.body, limit) {
        Ok(body) => body,
        Err(response) => return response,
    };

    let mut guest = None;
    if !scope.member {
        let name = request.guest_name.as_deref().map(str::trim).unwrap_or_default();
        if name.is_empty() || name.chars().count() > MAX_GUEST_NAME_CHARS {
            return failure(StatusCode::UNPROCESSABLE_ENTITY, "tell us your name (up to 60 characters)");
        }
        guest = Some(name.to_string());
        // A visitor identity is issued only now that the post is allowed.
        if let Err(error) = ensure_visitor(state, &mut scope.identity).await {
            let (status, message) = error.http();
            return failure(status, message);
        }
    }

    // Mentions are members of this organization; visitors cannot mention.
    let mentions = if scope.member { valid_mentions(&scope, &request.mentions).await } else { Vec::new() };
    let link = request.link.filter(|link| link.starts_with('/') && !link.starts_with("//") && link.len() <= 500);
    let author = scope.actor();
    let stored = match store::insert(
        &scope.org,
        &NewMessage {
            model_id: scope.table().to_string(),
            record_key: scope.key.clone(),
            kind,
            author: author.clone(),
            author_name: guest,
            body: Some(body.clone()),
            mentions: mentions.clone(),
            link: link.clone(),
            plugin: Some(scope.plugin.clone()),
        },
    )
    .await
    {
        Ok(stored) => stored,
        Err(error) => return database_failure("post", error),
    };

    if scope.member && scope.chatter.followers {
        let table = scope.table().to_string();
        let mut follows = vec![(author.clone(), "commenter")];
        follows.extend(mentions.iter().map(|actor| (actor.clone(), "mention")));
        for (actor, reason) in follows {
            if let Err(error) = store::follow(&scope.org, &table, &scope.key, &actor, reason).await {
                log::warn!("chatter follow {actor}: {error}");
            }
        }
    }

    notify(state, &scope, &stored, &mentions, link).await;
    state.notifications.publish(
        &scope.identity.org_db,
        HubMessage::Event(Arc::new(UiEvent {
            plugin: "chatter".into(),
            event: "chatter".into(),
            payload: json!({ "plugin": scope.plugin, "model": address.model, "key": scope.key }),
        })),
    );

    let names = names(state, &scope.org, std::slice::from_ref(&author)).await;
    let jar = with_visitor_cookie(state, &scope.identity, jar);
    (StatusCode::CREATED, jar, Json(json!({ "message": message_json(&stored, &names) }))).into_response()
}

/// The mentions that are active members of the organization, without repeats.
async fn valid_mentions(scope: &Scope, asked: &[String]) -> Vec<String> {
    let mut wanted: Vec<String> = asked.iter().filter(|actor| actor.starts_with("users:")).cloned().collect();
    wanted.sort();
    wanted.dedup();
    wanted.truncate(MAX_MENTIONS);
    if wanted.is_empty() {
        return wanted;
    }
    let rows = scope
        .org
        .query("SELECT VALUE core_user_id FROM org_users WHERE core_user_id IN $ids AND is_active = true;")
        .bind(("ids", wanted))
        .await
        .and_then(|response| response.check());
    match rows {
        Ok(mut response) => response.take::<Vec<String>>(0).unwrap_or_default(),
        Err(error) => {
            log::warn!("chatter mentions: {error}");
            Vec::new()
        }
    }
}

/// A message tells the record's followers; a note tells only the people it mentions.
async fn notify(state: &AppState, scope: &Scope, message: &Message, mentions: &[String], link: Option<String>) {
    let mut recipients: Vec<String> = mentions.to_vec();
    if message.kind == Kind::Message && scope.chatter.followers {
        match store::followers(&scope.org, scope.table(), &scope.key).await {
            Ok(followers) => recipients.extend(
                followers.into_iter().filter(|follower| !follower.muted).map(|follower| follower.actor),
            ),
            Err(error) => log::warn!("chatter followers for notification: {error}"),
        }
    }
    recipients.retain(|actor| *actor != message.author);
    recipients.sort();
    recipients.dedup();
    if recipients.is_empty() {
        return;
    }
    let who = names(state, &scope.org, std::slice::from_ref(&message.author)).await;
    let author = who
        .get(&message.author)
        .cloned()
        .or_else(|| message.author_name.clone())
        .unwrap_or_else(|| "Someone".into());
    let body = message.body.as_deref().unwrap_or_default();
    let excerpt: String = body.chars().take(EXCERPT_CHARS).collect();
    let what = if message.kind == Kind::Note { "noted" } else { "wrote" };
    let notification = NewNotification {
        source: "chatter".into(),
        level: Level::Info,
        title: format!("{author} {what} on {}", scope.schema.name),
        body: Some(excerpt),
        link,
        payload: Some(json!({ "plugin": scope.plugin, "model": scope.schema.name, "key": scope.key })),
        audience: Audience::Actors { actors: recipients },
        expires_in_secs: None,
    };
    if let Err(error) = send_notification(&scope.org, &state.notifications, &scope.identity.org_db, notification).await {
        log::warn!("chatter notification: {error}");
    }
}

#[derive(Deserialize)]
struct MessageAddress {
    plugin: String,
    model: String,
    key: String,
    id: String,
}

impl MessageAddress {
    fn record(&self) -> Address {
        Address { plugin: self.plugin.clone(), model: self.model.clone(), key: self.key.clone() }
    }
}

#[derive(Deserialize)]
struct EditBody {
    body: String,
}

async fn edit_message(
    State(state): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    Path(address): Path<MessageAddress>,
    headers: HeaderMap,
    uri: Uri,
    Json(request): Json<EditBody>,
) -> Response {
    let identity = match who(&state, peer, &headers, &uri).await {
        Ok(identity) => identity,
        Err(response) => return response,
    };
    let scope = scope_or_return!(state, identity, address.record());
    let body = match clean_body(&request.body, MAX_BODY_CHARS) {
        Ok(body) => body,
        Err(response) => return response,
    };
    let existing = match store::get(&scope.org, scope.table(), &scope.key, &address.id).await {
        Ok(Some(message)) => message,
        Ok(None) => return failure(StatusCode::NOT_FOUND, "no such message"),
        Err(error) => return database_failure("edit lookup", error),
    };
    if existing.author != scope.actor() || !matches!(existing.kind, Kind::Message | Kind::Note) {
        return failure(StatusCode::FORBIDDEN, "you can only edit your own messages");
    }
    match store::edit(&scope.org, &address.id, &body).await {
        Ok(Some(message)) => {
            let names = names(&state, &scope.org, std::slice::from_ref(&message.author)).await;
            Json(json!({ "message": message_json(&message, &names) })).into_response()
        }
        Ok(None) => failure(StatusCode::NOT_FOUND, "no such message"),
        Err(error) => database_failure("edit", error),
    }
}

async fn delete_message(
    State(state): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    Path(address): Path<MessageAddress>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    let identity = match who(&state, peer, &headers, &uri).await {
        Ok(identity) => identity,
        Err(response) => return response,
    };
    let scope = scope_or_return!(state, identity, address.record());
    let existing = match store::get(&scope.org, scope.table(), &scope.key, &address.id).await {
        Ok(Some(message)) => message,
        Ok(None) => return failure(StatusCode::NOT_FOUND, "no such message"),
        Err(error) => return database_failure("delete lookup", error),
    };
    if !matches!(existing.kind, Kind::Message | Kind::Note) {
        return failure(StatusCode::FORBIDDEN, "history lines cannot be deleted");
    }
    if existing.author != scope.actor() && !scope.developer {
        return failure(StatusCode::FORBIDDEN, "you can only delete your own messages");
    }
    match store::trash(&scope.org, &address.id, &scope.actor()).await {
        Ok(Some(_)) => Json(json!({ "ok": true })).into_response(),
        Ok(None) => failure(StatusCode::NOT_FOUND, "no such message"),
        Err(error) => database_failure("delete", error),
    }
}

async fn restore_message(
    State(state): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    Path(address): Path<MessageAddress>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    let identity = match who(&state, peer, &headers, &uri).await {
        Ok(identity) => identity,
        Err(response) => return response,
    };
    let scope = scope_or_return!(state, identity, address.record());
    if !scope.developer {
        return failure(StatusCode::FORBIDDEN, "only developers can restore from the trash");
    }
    match store::restore(&scope.org, &address.id).await {
        Ok(Some(_)) => Json(json!({ "ok": true })).into_response(),
        Ok(None) => failure(StatusCode::NOT_FOUND, "that message is not in the trash"),
        Err(error) => database_failure("restore", error),
    }
}

#[derive(Deserialize)]
struct FollowBody {
    following: bool,
    #[serde(default)]
    muted: Option<bool>,
}

async fn set_following(
    State(state): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    Path(address): Path<Address>,
    headers: HeaderMap,
    uri: Uri,
    Json(request): Json<FollowBody>,
) -> Response {
    let identity = match who(&state, peer, &headers, &uri).await {
        Ok(identity) => identity,
        Err(response) => return response,
    };
    let scope = scope_or_return!(state, identity, address);
    if !scope.member || !scope.chatter.followers {
        return failure(StatusCode::FORBIDDEN, "following is not available here");
    }
    let (table, key, actor) = (scope.table().to_string(), scope.key.clone(), scope.actor());
    let result = if request.following {
        match store::follow(&scope.org, &table, &key, &actor, "manual").await {
            Ok(()) => match request.muted {
                Some(muted) => store::set_muted(&scope.org, &table, &key, &actor, muted).await,
                None => Ok(()),
            },
            Err(error) => Err(error),
        }
    } else {
        store::unfollow(&scope.org, &table, &key, &actor).await
    };
    match result {
        Ok(()) => Json(json!({ "ok": true })).into_response(),
        Err(error) => database_failure("follow", error),
    }
}
