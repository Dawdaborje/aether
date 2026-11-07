//! API tokens: a credential for scripts and other systems (see `docs/architecture/api_and_webhooks.md`).
//!
//! A token belongs to one person in one organization. It can never do more than that person can:
//! the rules, roles and capabilities are worked out for the owner exactly as for a browser, and the
//! token's [`Scopes`] only narrow it further. It is sent as `Authorization: Bearer aet_…` and is
//! accepted on `/api/plugins/…` and `/api/v1/…` only, not on the rest of the API.
//!
//! Only the SHA-256 of a token is stored; the token itself is shown once, when it is made. The
//! tokens live in the core database (a request must find its organization from the token).

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use surrealdb::{Surreal, engine::remote::ws::Client};
use thiserror::Error;

/// What every token starts with, so a leaked one is recognisable (and a secret scanner can find it).
pub const PREFIX: &str = "aet_";
/// The longest a token may be given a name.
const MAX_NAME: usize = 80;
/// Most tokens one person may have in one organization.
pub const MAX_PER_USER: usize = 50;
/// How long a successful lookup is reused. A revoked token stops working here at once and on other
/// servers within this time.
const LOOKUP_TTL: Duration = Duration::from_secs(30);
const MISS_TTL: Duration = Duration::from_secs(5);
/// "Last used" is written at most this often per token.
const TOUCH_EVERY: Duration = Duration::from_secs(60);

#[derive(Debug, Error)]
pub enum TokenError {
    #[error("{0}")]
    Invalid(String),
    #[error("database error: {0}")]
    Database(#[from] surrealdb::Error),
}

/// What a request carries, if it looks like one of ours.
pub fn bearer(headers: &axum::http::HeaderMap) -> Option<&str> {
    let value = headers.get(axum::http::header::AUTHORIZATION)?.to_str().ok()?;
    let raw = value.strip_prefix("Bearer ").or_else(|| value.strip_prefix("bearer "))?.trim();
    (raw.starts_with(PREFIX) && raw.len() > PREFIX.len()).then_some(raw)
}

/// A new token: `aet_` and 64 hex characters.
pub fn generate() -> String {
    let mut bytes = [0u8; 32];
    rand::RngExt::fill(&mut rand::rng(), &mut bytes);
    format!("{PREFIX}{}", bytes.iter().map(|b| format!("{b:02x}")).collect::<String>())
}

pub fn hash(raw: &str) -> String {
    Sha256::digest(raw.as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
}

/// The part of a token that is kept in the clear, to tell tokens apart.
pub fn display_prefix(raw: &str) -> String {
    raw.chars().take(PREFIX.len() + 6).collect()
}

/// What a model operation is, for `models` scopes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelOp {
    Read,
    Create,
    Update,
    Delete,
}

impl ModelOp {
    fn name(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Create => "create",
            Self::Update => "update",
            Self::Delete => "delete",
        }
    }
}

/// What a token may do beyond being its owner. A token with nothing in it can do nothing.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scopes {
    /// Plugins whose functions it may call (`/api/plugins/{plugin}/{function}`).
    #[serde(default)]
    pub plugins: Vec<String>,
    /// Single functions, as `plugin.function`.
    #[serde(default)]
    pub functions: Vec<String>,
    /// Models it may use through the record API, as `plugin.model:read,create,update,delete`
    /// (`write` is create, update and delete; no `:` means read).
    #[serde(default)]
    pub models: Vec<String>,
}

fn plain(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// A model scope entry split into `(plugin, model, operations)`.
fn parse_model(entry: &str) -> Option<(&str, &str, Vec<ModelOp>)> {
    let (target, ops) = entry.split_once(':').unwrap_or((entry, "read"));
    let (plugin, model) = target.split_once('.')?;
    if !plain(plugin) || !plain(model) {
        return None;
    }
    let mut out = Vec::new();
    for op in ops.split(',').map(str::trim) {
        match op {
            "read" => out.push(ModelOp::Read),
            "create" => out.push(ModelOp::Create),
            "update" => out.push(ModelOp::Update),
            "delete" => out.push(ModelOp::Delete),
            "write" => out.extend([ModelOp::Create, ModelOp::Update, ModelOp::Delete]),
            _ => return None,
        }
    }
    (!out.is_empty()).then_some((plugin, model, out))
}

impl Scopes {
    /// Whether the scopes are well formed and give the token something to do.
    pub fn validate(&self) -> Result<(), String> {
        if self.plugins.is_empty() && self.functions.is_empty() && self.models.is_empty() {
            return Err("give the token something to do: plugins, functions or models".into());
        }
        if let Some(bad) = self.plugins.iter().find(|p| !plain(p)) {
            return Err(format!("`{bad}` is not a plugin name"));
        }
        for function in &self.functions {
            match function.split_once('.') {
                Some((plugin, name)) if plain(plugin) && plain(name) => {}
                _ => return Err(format!("`{function}` is not a function: write `plugin.function`")),
            }
        }
        for model in &self.models {
            if parse_model(model).is_none() {
                return Err(format!(
                    "`{model}` is not a model scope: write `plugin.model` or `plugin.model:read,create,update,delete`"
                ));
            }
        }
        if self.plugins.len() + self.functions.len() + self.models.len() > 100 {
            return Err("a token has at most 100 scope entries".into());
        }
        Ok(())
    }

    /// May the token call this function of this plugin?
    pub fn allows_call(&self, plugin: &str, function: &str) -> bool {
        self.plugins.iter().any(|p| p == plugin)
            || self.functions.iter().any(|f| f.split_once('.') == Some((plugin, function)))
    }

    /// May the token do `op` on this model of this plugin through the record API?
    pub fn allows_model(&self, plugin: &str, model: &str, op: ModelOp) -> bool {
        self.models.iter().filter_map(|entry| parse_model(entry)).any(|(p, m, ops)| p == plugin && m == model && ops.contains(&op))
    }

    /// The operations granted on a model, for the OpenAPI description.
    pub fn model_ops(&self, plugin: &str, model: &str) -> Vec<&'static str> {
        [ModelOp::Read, ModelOp::Create, ModelOp::Update, ModelOp::Delete]
            .into_iter()
            .filter(|op| self.allows_model(plugin, model, *op))
            .map(ModelOp::name)
            .collect()
    }
}

/// A token as a request sees it, once it has been checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenUse {
    pub id: String,
    pub name: String,
    pub scopes: Scopes,
}

/// A stored token that is currently usable, with its owner's state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenRow {
    pub id: String,
    pub name: String,
    pub user: String,
    pub org_db: String,
    pub scopes: Scopes,
    pub user_active: bool,
    pub user_is_super: bool,
}

type LookupCache = Mutex<HashMap<String, (Instant, Option<TokenRow>)>>;
static LOOKUPS: LazyLock<LookupCache> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Find the usable token for `raw`: not revoked, not expired, its owner still active.
pub async fn lookup(core: &Surreal<Client>, raw: &str) -> Result<Option<TokenRow>, TokenError> {
    let key = hash(raw);
    if let Ok(cache) = LOOKUPS.lock()
        && let Some((at, row)) = cache.get(&key)
        && at.elapsed() < if row.is_some() { LOOKUP_TTL } else { MISS_TTL }
    {
        return Ok(row.clone());
    }
    let mut response = core
        .query(
            "SELECT <string> record::id(id) AS id, name, user, org_db, scopes FROM api_tokens \
             WHERE token_hash = $hash AND revoked_at = NONE AND (expires_at = NONE OR expires_at > time::now()) LIMIT 1;",
        )
        .bind(("hash", key.clone()))
        .await?
        .check()?;
    let found: Vec<Value> = response.take(0)?;
    let row = match found.first() {
        None => None,
        Some(token) => {
            let user = token.get("user").and_then(Value::as_str).unwrap_or("").to_string();
            let mut response = core
                .query("SELECT is_active, is_super_user FROM type::record($user);")
                .bind(("user", user.clone()))
                .await?
                .check()?;
            let owner: Vec<Value> = response.take(0)?;
            let flag = |name: &str| owner.first().and_then(|o| o.get(name)).and_then(Value::as_bool).unwrap_or(false);
            let scopes: Scopes = token
                .get("scopes")
                .and_then(|s| serde_json::from_value(s.clone()).ok())
                .unwrap_or_default();
            Some(TokenRow {
                id: token.get("id").and_then(Value::as_str).unwrap_or("").to_string(),
                name: token.get("name").and_then(Value::as_str).unwrap_or("").to_string(),
                user,
                org_db: token.get("org_db").and_then(Value::as_str).unwrap_or("").to_string(),
                scopes,
                user_active: flag("is_active"),
                user_is_super: flag("is_super_user"),
            })
        }
    };
    if let Ok(mut cache) = LOOKUPS.lock() {
        if cache.len() > 10_000 {
            cache.clear();
        }
        cache.insert(key, (Instant::now(), row.clone()));
    }
    Ok(row)
}

/// Forget the lookups this server is reusing, so the next request asks the database again. Done when a
/// token is revoked; useful after deactivating a user.
pub fn forget_lookups() {
    if let Ok(mut cache) = LOOKUPS.lock() {
        cache.clear();
    }
}

static TOUCHED: LazyLock<Mutex<HashMap<String, Instant>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Note that the token was used (at most once a minute per token).
pub async fn touch(core: &Surreal<Client>, id: &str, ip: Option<&str>) {
    if let Ok(mut touched) = TOUCHED.lock() {
        if touched.get(id).is_some_and(|at| at.elapsed() < TOUCH_EVERY) {
            return;
        }
        if touched.len() > 10_000 {
            touched.clear();
        }
        touched.insert(id.to_string(), Instant::now());
    }
    let result = core
        .query("UPDATE type::record('api_tokens', $id) SET last_used_at = time::now(), last_ip = $ip;")
        .bind(("id", id.to_string()))
        .bind(("ip", ip.map(str::to_string)))
        .await;
    if let Err(error) = result {
        log::debug!("api token use not recorded: {error}");
    }
}

static WINDOWS: LazyLock<Mutex<HashMap<String, (u64, u32)>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Whether the token may make another request this minute (`per_minute`; 0 is no limit).
pub fn allow(id: &str, per_minute: u32) -> bool {
    if per_minute == 0 {
        return true;
    }
    let minute = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() / 60);
    let Ok(mut windows) = WINDOWS.lock() else {
        return true;
    };
    if windows.len() > 10_000 {
        windows.retain(|_, (m, _)| *m == minute);
    }
    let entry = windows.entry(id.to_string()).or_insert((minute, 0));
    if entry.0 != minute {
        *entry = (minute, 0);
    }
    entry.1 += 1;
    entry.1 <= per_minute
}

/// What a person sees of a token (never the token itself).
#[derive(Debug, Clone, Serialize)]
pub struct TokenInfo {
    pub id: String,
    pub name: String,
    pub prefix: String,
    pub scopes: Value,
    pub created_at: Value,
    pub expires_at: Value,
    pub last_used_at: Value,
    pub last_ip: Value,
    pub revoked_at: Value,
}

/// What to make a token from.
pub struct NewToken<'a> {
    pub user: &'a str,
    pub org_db: &'a str,
    pub name: &'a str,
    pub scopes: &'a Scopes,
    /// Days until it stops working; `None` for never.
    pub expires_days: Option<u32>,
}

/// Make a token. Returns its id and the token itself, which is not kept and cannot be shown again.
pub async fn create(core: &Surreal<Client>, new: NewToken<'_>) -> Result<(String, String), TokenError> {
    let name = new.name.trim();
    if name.is_empty() || name.chars().count() > MAX_NAME {
        return Err(TokenError::Invalid(format!("name the token, in at most {MAX_NAME} characters")));
    }
    new.scopes.validate().map_err(TokenError::Invalid)?;
    let mut response = core
        .query("SELECT count() AS count FROM api_tokens WHERE org_db = $org AND user = $user AND revoked_at = NONE GROUP ALL;")
        .bind(("org", new.org_db.to_string()))
        .bind(("user", new.user.to_string()))
        .await?
        .check()?;
    let counted: Vec<Value> = response.take(0)?;
    if counted.first().and_then(|r| r.get("count")).and_then(Value::as_u64).unwrap_or(0) >= MAX_PER_USER as u64 {
        return Err(TokenError::Invalid(format!("at most {MAX_PER_USER} tokens at a time: revoke one first")));
    }
    let raw = generate();
    let mut response = core
        .query(
            "CREATE api_tokens SET token_hash = $hash, prefix = $prefix, user = $user, org_db = $org, \
             name = $name, scopes = $scopes, \
             expires_at = IF $days != NONE THEN time::now() + <duration> string::concat($days, 'd') ELSE NONE END \
             RETURN <string> record::id(id) AS id;",
        )
        .bind(("hash", hash(&raw)))
        .bind(("prefix", display_prefix(&raw)))
        .bind(("user", new.user.to_string()))
        .bind(("org", new.org_db.to_string()))
        .bind(("name", name.to_string()))
        .bind(("scopes", json!(new.scopes)))
        .bind(("days", new.expires_days.map(i64::from)))
        .await?
        .check()?;
    let made: Vec<Value> = response.take(0)?;
    let id = made
        .first()
        .and_then(|row| row.get("id"))
        .and_then(Value::as_str)
        .ok_or_else(|| TokenError::Invalid("the token was not stored".into()))?
        .to_string();
    Ok((id, raw))
}

/// A person's tokens in an organization, newest first.
pub async fn list(core: &Surreal<Client>, user: &str, org_db: &str) -> Result<Vec<TokenInfo>, TokenError> {
    let mut response = core
        .query(
            "SELECT <string> record::id(id) AS id, name, prefix, scopes, created_at, expires_at, last_used_at, last_ip, revoked_at \
             FROM api_tokens WHERE org_db = $org AND user = $user ORDER BY created_at DESC;",
        )
        .bind(("org", org_db.to_string()))
        .bind(("user", user.to_string()))
        .await?
        .check()?;
    let rows: Vec<Value> = response.take(0)?;
    let field = |row: &Value, key: &str| row.get(key).cloned().unwrap_or(Value::Null);
    Ok(rows
        .iter()
        .map(|row| TokenInfo {
            id: field(row, "id").as_str().unwrap_or("").to_string(),
            name: field(row, "name").as_str().unwrap_or("").to_string(),
            prefix: field(row, "prefix").as_str().unwrap_or("").to_string(),
            scopes: field(row, "scopes"),
            created_at: field(row, "created_at"),
            expires_at: field(row, "expires_at"),
            last_used_at: field(row, "last_used_at"),
            last_ip: field(row, "last_ip"),
            revoked_at: field(row, "revoked_at"),
        })
        .collect())
}

/// Stop a token from working. `false` when the person has no such token.
pub async fn revoke(core: &Surreal<Client>, id: &str, user: &str, org_db: &str) -> Result<bool, TokenError> {
    let mut response = core
        .query(
            "SELECT VALUE <string> record::id(id) FROM (UPDATE api_tokens SET revoked_at = time::now() \
             WHERE id = type::record('api_tokens', $id) AND user = $user AND org_db = $org AND revoked_at = NONE);",
        )
        .bind(("id", id.to_string()))
        .bind(("user", user.to_string()))
        .bind(("org", org_db.to_string()))
        .await?
        .check()?;
    let done: Vec<Value> = response.take(0)?;
    forget_lookups();
    Ok(!done.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{HeaderMap, HeaderValue, header::AUTHORIZATION};

    fn scopes(plugins: &[&str], functions: &[&str], models: &[&str]) -> Scopes {
        let own = |list: &[&str]| list.iter().map(|s| s.to_string()).collect();
        Scopes { plugins: own(plugins), functions: own(functions), models: own(models) }
    }

    #[test]
    fn a_token_is_recognised_by_its_prefix_and_never_stored_in_full() {
        let token = generate();
        assert!(token.starts_with("aet_") && token.len() == 4 + 64);
        assert_ne!(generate(), token);
        assert_eq!(hash(&token).len(), 64);
        assert_ne!(hash(&token), token);
        assert_eq!(display_prefix(&token).len(), 10);
        assert!(token.starts_with(&display_prefix(&token)));
    }

    #[test]
    fn only_our_bearer_tokens_are_picked_up() {
        let with = |value: &str| {
            let mut h = HeaderMap::new();
            h.insert(AUTHORIZATION, HeaderValue::from_str(value).unwrap());
            h
        };
        assert_eq!(bearer(&with("Bearer aet_abc123")), Some("aet_abc123"));
        assert_eq!(bearer(&with("bearer aet_abc123")), Some("aet_abc123"));
        assert_eq!(bearer(&with("Bearer something-else")), None, "a token of another system");
        assert_eq!(bearer(&with("Basic dXNlcjpwYXNz")), None);
        assert_eq!(bearer(&with("Bearer aet_")), None);
        assert_eq!(bearer(&HeaderMap::new()), None);
    }

    #[test]
    fn scopes_must_give_the_token_something_and_be_well_formed() {
        assert!(scopes(&[], &[], &[]).validate().is_err(), "an empty token does nothing");
        assert!(scopes(&["crm"], &[], &[]).validate().is_ok());
        assert!(scopes(&[], &["crm.add_lead"], &[]).validate().is_ok());
        assert!(scopes(&[], &[], &["crm.lead:read,write"]).validate().is_ok());
        assert!(scopes(&[], &[], &["crm.lead"]).validate().is_ok(), "no operations means read");
        assert!(scopes(&["bad name"], &[], &[]).validate().is_err());
        assert!(scopes(&[], &["add_lead"], &[]).validate().is_err(), "needs plugin.function");
        assert!(scopes(&[], &[], &["lead"]).validate().is_err(), "needs plugin.model");
        assert!(scopes(&[], &[], &["crm.lead:fly"]).validate().is_err());
        assert!(scopes(&[], &[], &["crm.lead:"]).validate().is_err());
    }

    #[test]
    fn calls_are_allowed_by_plugin_or_by_function() {
        let s = scopes(&["crm"], &["hr.hire"], &[]);
        assert!(s.allows_call("crm", "anything"));
        assert!(s.allows_call("hr", "hire"));
        assert!(!s.allows_call("hr", "fire"));
        assert!(!s.allows_call("billing", "hire"));
        assert!(!scopes(&[], &[], &["crm.lead:read"]).allows_call("crm", "add_lead"), "models are not functions");
    }

    #[test]
    fn model_access_is_per_operation() {
        let s = scopes(&[], &[], &["crm.lead:read,update", "crm.deal:write", "hr.employee"]);
        assert!(s.allows_model("crm", "lead", ModelOp::Read));
        assert!(s.allows_model("crm", "lead", ModelOp::Update));
        assert!(!s.allows_model("crm", "lead", ModelOp::Delete));
        assert!(!s.allows_model("crm", "lead", ModelOp::Create));
        assert!(s.allows_model("crm", "deal", ModelOp::Create) && s.allows_model("crm", "deal", ModelOp::Delete));
        assert!(!s.allows_model("crm", "deal", ModelOp::Read), "write is not read");
        assert!(s.allows_model("hr", "employee", ModelOp::Read));
        assert!(!s.allows_model("hr", "salary", ModelOp::Read));
        assert_eq!(s.model_ops("crm", "lead"), vec!["read", "update"]);
        assert!(!scopes(&["crm"], &[], &[]).allows_model("crm", "lead", ModelOp::Read), "calling is not reading");
    }

    #[test]
    fn a_token_is_limited_to_so_many_requests_a_minute() {
        let id = format!("test-{}", generate());
        assert!(allow(&id, 3) && allow(&id, 3) && allow(&id, 3));
        assert!(!allow(&id, 3), "the fourth in the same minute");
        assert!(allow(&id, 0), "0 is no limit");
        assert!(allow(&format!("{id}-other"), 3), "another token has its own count");
    }
}
