use serde::{Deserialize, Serialize};

/// Contract version the addon was authored against (`[plugin.api]`).
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct PluginApi {
    #[serde(default = "default_api_version")]
    pub version: String,
}

fn default_api_version() -> String {
    "0.1".into()
}

/// Optional metadata (`[plugin.meta]`). Prefer this over top-level kind/workspace long-term.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct PluginMeta {
    /// `"addon"` | `"theme"` | `"bridge_pack"` | …
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub workspace: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct PluginAuthor {
    pub name: String,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub github: Option<String>,
    #[serde(default)]
    pub website: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct PluginCategory {
    pub name: String,
    #[serde(default)]
    pub label: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct AccessModelDef {
    pub name: String,
    #[serde(default)]
    pub permissions: Vec<String>,
}

/// Core `[plugin]` table. Extra future keys are ignored (serde default).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PluginDefinition {
    pub name: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub long_description: Option<String>,
    #[serde(default)]
    pub icon_path: Option<String>,
    #[serde(default)]
    pub authors: Vec<PluginAuthor>,
    #[serde(default)]
    pub website: Option<String>,
    #[serde(default)]
    pub categories: Vec<PluginCategory>,
    /// Plugin names this addon depends on (mirrored to `plugin_depends_on`).
    #[serde(default)]
    pub dependencies: Vec<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub access_models: Vec<AccessModelDef>,
    /// Functions anonymous visitors may call (`POST /api/plugins/{plugin}/{function}`).
    /// Empty by default: nothing is callable without logging in.
    #[serde(default)]
    pub public_functions: Vec<String>,
    /// Capabilities an anonymous visitor may use, a subset of `capabilities`.
    /// Public pages imply read access (`db::query`) to the models they show;
    /// anything beyond reading, such as `db::mutate`, must be listed here.
    #[serde(default)]
    pub public_capabilities: Vec<String>,
    /// Models anonymous visitors may access beyond what public pages show,
    /// with their permissions. A model listed with `write` is writable by
    /// anyone, so keep this list short.
    #[serde(default)]
    pub public_access_models: Vec<AccessModelDef>,
    /// Outside hosts `http::request` may call, such as `"api.stripe.com"` or `"*.example.com"`
    /// (a wildcard covers subdomains, not the bare domain). Empty by default: no access.
    #[serde(default)]
    pub http_hosts: Vec<String>,
    /// `[plugin.i18n]`: the language the plugin's own text is written in.
    #[serde(default)]
    pub i18n: Option<PluginI18nDef>,
    /// Bridges the plugin may call with `bridge::call`, such as `["paystack"]`. Empty by default.
    #[serde(default)]
    pub bridges: Vec<String>,
    /// Parent workspace name (e.g. `"base"`, `"erp"`). Prefer `[plugin.meta].workspace`.
    #[serde(default)]
    pub workspace: Option<String>,
    /// `"addon"` | `"theme"` | … Prefer `[plugin.meta].kind`.
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub is_builtin: bool,
    #[serde(default)]
    pub api: Option<PluginApi>,
    #[serde(default)]
    pub meta: Option<PluginMeta>,
    /// The plugin's WebAssembly module (any language that compiles to WASM).
    #[serde(default)]
    pub wasm_file: Option<String>,
    /// The plugin's Rhai (`.rhai`) or Lua (`.lua`) script, instead of a WASM module: for small plugins that only read and
    /// write records. A plugin has one or the other.
    #[serde(default)]
    pub script: Option<String>,
    #[serde(default)]
    pub plugin_base_path: String,
}

impl Default for PluginDefinition {
    fn default() -> Self {
        Self {
            name: String::new(),
            label: String::new(),
            version: String::new(),
            description: None,
            long_description: None,
            icon_path: None,
            authors: Vec::new(),
            website: None,
            categories: Vec::new(),
            dependencies: Vec::new(),
            capabilities: Vec::new(),
            access_models: Vec::new(),
            public_functions: Vec::new(),
            public_capabilities: Vec::new(),
            public_access_models: Vec::new(),
            http_hosts: Vec::new(),
            bridges: Vec::new(),
            i18n: None,
            workspace: None,
            kind: Some("addon".into()),
            is_builtin: false,
            api: Some(PluginApi {
                version: default_api_version(),
            }),
            meta: None,
            wasm_file: None,
            script: None,
            plugin_base_path: String::new(),
        }
    }
}

impl PluginDefinition {
    /// The file that holds the plugin's code: its WASM module or its script.
    pub fn code_file(&self) -> Option<&str> {
        self.wasm_file
            .as_deref()
            .or(self.script.as_deref())
            .filter(|file| !file.is_empty())
    }

    /// Whether the code is a script (Rhai or Lua).
    pub fn is_script(&self) -> bool {
        self.wasm_file.as_deref().is_none_or(str::is_empty) && self.script.as_deref().is_some_and(|s| !s.is_empty())
    }

    /// Copy `[plugin.meta]` into flat `kind` / `workspace` when those are unset.
    pub fn hoist_meta(&mut self) {
        let Some(meta) = self.meta.clone() else {
            return;
        };
        if self.kind.is_none() {
            self.kind = meta.kind;
        }
        if self.workspace.is_none() {
            self.workspace = meta.workspace;
        }
    }

    pub fn api_version(&self) -> &str {
        self.api
            .as_ref()
            .map(|a| a.version.as_str())
            .unwrap_or("0.1")
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct PluginModelRef {
    pub name: String,
    #[serde(default)]
    pub table: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub file: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct PluginMenuDef {
    pub name: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub order: Option<i32>,
    #[serde(default)]
    pub parent: Option<String>,
    #[serde(default = "default_true")]
    pub visible: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct PluginHookDef {
    pub name: String,
    #[serde(default)]
    pub phase: Option<String>,
    #[serde(default)]
    pub event: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub handler: Option<String>,
}

/// An event a plugin emits or listens to (`[[events]]`).
///
/// ```toml
/// [[events]]                       # something this plugin announces with events::emit
/// name = "message_posted"
/// direction = "emit"               # the default
///
/// [[events]]                       # something another plugin announces
/// name = "chat.message_posted"     # <emitting plugin>.<event>
/// direction = "listen"
/// handler = "on_message"           # the function to run
/// ```
///
/// Listening needs the emitting plugin in `dependencies` and the capability `events::subscribe`;
/// emitting needs `events::emit`. A handler runs as a background job.
#[derive(Debug, Clone, Deserialize, Serialize, Default, PartialEq)]
pub struct PluginEventDef {
    pub name: String,
    /// `"emit"` (the default) or `"listen"`.
    #[serde(default)]
    pub direction: Option<String>,
    #[serde(default)]
    pub payload: Option<String>,
    /// For `listen`: the plugin function that handles the event.
    #[serde(default)]
    pub handler: Option<String>,
    /// For `listen`: the queue of the handler's jobs.
    #[serde(default)]
    pub queue: Option<String>,
    #[serde(default)]
    pub max_attempts: Option<i64>,
}

impl PluginEventDef {
    pub fn direction(&self) -> &str {
        self.direction.as_deref().unwrap_or("emit")
    }

    /// Check everything a manifest can get wrong, so a plugin is refused when it is loaded.
    pub fn validate(&self, plugin: &PluginDefinition) -> Result<(), String> {
        let plain = |part: &str| plain_name(part, |c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-');
        let holds = |capability: &str| plugin.capabilities.iter().any(|held| held == capability);
        match self.direction() {
            "emit" => {
                if !plain(&self.name) {
                    return Err(format!("emitted event `{}`: use 1 to 64 lower-case letters, digits, `_` or `-` (no dots)", self.name));
                }
                if !holds("events::emit") {
                    return Err(format!("`{}` is emitted, so the plugin needs the capability `events::emit`", self.name));
                }
            }
            "listen" => {
                let Some((source, event)) = self.name.split_once('.') else {
                    return Err(format!("listened event `{}` is named <plugin>.<event>, such as `chat.message_posted`", self.name));
                };
                if !plain(source) || !plain(event) {
                    return Err(format!("listened event `{}`: both parts use lower-case letters, digits, `_` or `-`", self.name));
                }
                if source != plugin.name && !plugin.dependencies.iter().any(|dependency| dependency == source) {
                    return Err(format!("`{}` comes from `{source}`, which must be listed under `dependencies`", self.name));
                }
                if self.handler.as_deref().is_none_or(str::is_empty) {
                    return Err(format!("listened event `{}` names no `handler` function", self.name));
                }
                if !holds("events::subscribe") {
                    return Err(format!("`{}` is listened to, so the plugin needs the capability `events::subscribe`", self.name));
                }
                if self.max_attempts.is_some_and(|n| !(1..=20).contains(&n)) {
                    return Err(format!("listened event `{}`: max_attempts is 1 to 20", self.name));
                }
                if let Some(queue) = &self.queue {
                    if !plain(queue) {
                        return Err(format!("listened event `{}`: a queue name is lower-case letters, digits, `_` and `-`", self.name));
                    }
                }
            }
            other => return Err(format!("event `{}`: direction is `emit` or `listen`, not `{other}`", self.name)),
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct PluginPermissionDef {
    pub key: String,
    #[serde(default)]
    pub label: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct PluginCommunication {
    #[serde(default)]
    pub channels: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct PluginThemeDef {
    pub name: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub is_system: bool,
    #[serde(default)]
    pub tokens_file: Option<String>,
}

/// `[plugin.i18n]`: `default = "en"` is the language of the plugin's text, tried last when a
/// person's own language has no translation. See `docs/architecture/localization.md`.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct PluginI18nDef {
    pub default: String,
}

/// `[app]`: makes the plugin a tile on the Apps launcher.
///
/// ```toml
/// [app]
/// label = "Chat"              # defaults to the plugin's label
/// icon = "message-square"     # a lucide icon name; the tile shows a letter if unknown
/// route = "/chat"             # a page of this plugin; what the tile opens
/// description = "Team messaging"
/// ```
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct PluginAppDef {
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    pub route: String,
    #[serde(default)]
    pub description: Option<String>,
}

/// Full `plugin.toml` document. Unknown top-level tables are ignored by serde.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct PluginManifest {
    pub plugin: PluginDefinition,
    #[serde(default)]
    pub models: Vec<PluginModelRef>,
    /// Present only to reject the removed `[[pages]]` table with a clear error.
    #[serde(default, rename = "pages")]
    legacy_pages: Option<toml::Value>,
    #[serde(default)]
    pub menus: Vec<PluginMenuDef>,
    #[serde(default)]
    pub hooks: Vec<PluginHookDef>,
    #[serde(default)]
    pub events: Vec<PluginEventDef>,
    #[serde(default)]
    pub permissions: Vec<PluginPermissionDef>,
    #[serde(default)]
    pub communication: PluginCommunication,
    #[serde(default)]
    pub theme: Option<PluginThemeDef>,
    /// Present when the plugin is an app (a tile on the Apps launcher).
    #[serde(default)]
    pub app: Option<PluginAppDef>,
    /// Recurring tasks (`[[schedule]]`): each runs one of the plugin's functions as a
    /// background job on a cron expression or interval in every organization that installs it.
    #[serde(default)]
    pub schedule: Vec<crate::scheduler::TaskDef>,
    /// Commands an administrator runs from the command line (`[[command]]`), like Django's
    /// `manage.py` commands: `aether --command <plugin>.<command> --org <org>`.
    #[serde(default)]
    pub command: Vec<PluginCommandDef>,
    /// Folders on disk the plugin wants to be told about (`[[watch]]`): when a file appears or
    /// changes, the kernel runs one of the plugin's functions.
    #[serde(default)]
    pub watch: Vec<PluginWatchDef>,
    /// Roles the plugin offers (`[[roles]]`); see [`PluginRoleDef`].
    #[serde(default)]
    pub roles: Vec<PluginRoleDef>,
}

/// What can happen to a watched file.
pub const WATCH_EVENTS: &[&str] = &["created", "modified", "deleted"];

/// A folder to watch, inside the plugin's own folder for the organization
/// (`<app_dir>/orgs/<organization>/plugins/<plugin>/`).
///
/// ```toml
/// [[watch]]
/// name = "invoices"
/// path = "inbox"             # relative to the plugin's folder; "" is the folder itself
/// pattern = "*.csv"          # optional glob on the file name
/// events = ["created"]       # created, modified, deleted; default created and modified
/// function = "import_invoice"
/// ```
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct PluginWatchDef {
    pub name: String,
    #[serde(default)]
    pub path: String,
    pub function: String,
    #[serde(default = "default_watch_events")]
    pub events: Vec<String>,
    /// A glob on the path relative to `path`, such as `*.csv` or `**/*.pdf`.
    #[serde(default)]
    pub pattern: Option<String>,
    /// A file must stay quiet this long before the plugin is told (50 to 60000 ms).
    #[serde(default = "default_debounce_ms")]
    pub debounce_ms: u64,
    #[serde(default)]
    pub recursive: bool,
    /// When the scheduler starts listening, report what happened while nothing was: files that are
    /// new, changed or gone since the watch last reported (on the first start, every file already
    /// there counts as new). Default on; turn it off to react only to live changes.
    #[serde(default = "default_true", alias = "scan_on_start")]
    pub catch_up: bool,
    /// Look at the folder every so many seconds instead of listening for changes: for network
    /// file systems, where notifications do not arrive.
    #[serde(default)]
    pub poll_secs: Option<u64>,
    #[serde(default)]
    pub queue: Option<String>,
    #[serde(default)]
    pub max_attempts: Option<i64>,
}

fn default_watch_events() -> Vec<String> {
    vec!["created".into(), "modified".into()]
}

fn default_debounce_ms() -> u64 {
    500
}

impl PluginWatchDef {
    /// Check everything a manifest can get wrong, so a plugin is refused when it is loaded.
    pub fn validate(&self) -> Result<(), String> {
        if !plain_name(&self.name, |c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-') {
            return Err(format!("watch name `{}`: use 1 to 64 lower-case letters, digits, `_` or `-`", self.name));
        }
        if self.function.is_empty() {
            return Err(format!("watch `{}` names no function", self.name));
        }
        crate::plugin_files::safe_relative(&self.path).map_err(|e| format!("watch `{}`: {e}", self.name))?;
        if self.events.is_empty() {
            return Err(format!("watch `{}` lists no events; use some of {WATCH_EVENTS:?}", self.name));
        }
        if let Some(bad) = self.events.iter().find(|event| !WATCH_EVENTS.contains(&event.as_str())) {
            return Err(format!("watch `{}`: `{bad}` is not an event; use some of {WATCH_EVENTS:?}", self.name));
        }
        if let Some(pattern) = &self.pattern {
            globset::Glob::new(pattern).map_err(|e| format!("watch `{}`: pattern `{pattern}`: {e}", self.name))?;
        }
        if !(50..=60_000).contains(&self.debounce_ms) {
            return Err(format!("watch `{}`: debounce_ms is 50 to 60000", self.name));
        }
        if self.poll_secs.is_some_and(|secs| !(1..=3600).contains(&secs)) {
            return Err(format!("watch `{}`: poll_secs is 1 to 3600", self.name));
        }
        if self.max_attempts.is_some_and(|n| !(1..=20).contains(&n)) {
            return Err(format!("watch `{}`: max_attempts is 1 to 20", self.name));
        }
        if let Some(queue) = &self.queue {
            let ok = !queue.is_empty()
                && queue.len() <= 40
                && queue.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-');
            if !ok {
                return Err(format!("watch `{}`: a queue name is lower-case letters, digits, `_` and `-`", self.name));
            }
        }
        Ok(())
    }
}

/// The types an argument can have. A command line gives text; the kernel converts it.
const ARG_TYPES: &[&str] = &["string", "int", "float", "bool", "json"];

/// One argument of a command.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct CommandArgDef {
    pub name: String,
    #[serde(default)]
    pub help: String,
    /// `string` (the default), `int`, `float`, `bool` or `json`.
    #[serde(default, rename = "type")]
    pub kind: Option<String>,
    #[serde(default)]
    pub required: bool,
    /// Used when the argument is not given; in the argument's own type.
    #[serde(default)]
    pub default: Option<serde_json::Value>,
}

impl CommandArgDef {
    pub fn kind(&self) -> &str {
        self.kind.as_deref().unwrap_or("string")
    }
}

/// A command a plugin offers to the command line: a name, the function it runs and its arguments.
///
/// ```toml
/// [[command]]
/// name = "import_rates"
/// function = "import_rates"
/// help = "Fetch the exchange rates for a day"
/// args = [{ name = "date", help = "YYYY-MM-DD, default today" }]
/// ```
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct PluginCommandDef {
    pub name: String,
    pub function: String,
    #[serde(default)]
    pub help: String,
    #[serde(default)]
    pub args: Vec<CommandArgDef>,
}

/// A role a plugin offers: something an administrator gives to people, which the plugin's code
/// then checks (`hr_manager`, `approver`). In an organization it is called `<plugin>.<name>`.
///
/// ```toml
/// [[roles]]
/// name = "hr_manager"
/// label = "HR manager"
/// description = "Hires, changes and ends employment"
/// ```
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct PluginRoleDef {
    pub name: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

impl PluginRoleDef {
    pub fn validate(&self) -> Result<(), String> {
        if !plain_name(&self.name, |c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') {
            return Err(format!("role name `{}`: use 1 to 64 lower-case letters, digits or `_`", self.name));
        }
        Ok(())
    }
}

fn plain_name(name: &str, allowed: fn(char) -> bool) -> bool {
    !name.is_empty() && name.len() <= 64 && name.chars().all(allowed)
}

impl PluginCommandDef {
    /// Check everything a manifest can get wrong, so a plugin is refused when it is loaded.
    pub fn validate(&self) -> Result<(), String> {
        if !plain_name(&self.name, |c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-') {
            return Err(format!("command name `{}`: use 1 to 64 lower-case letters, digits, `_` or `-`", self.name));
        }
        if self.function.is_empty() {
            return Err(format!("command `{}` names no function", self.name));
        }
        let mut seen = std::collections::HashSet::new();
        for arg in &self.args {
            if !plain_name(&arg.name, |c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') {
                return Err(format!("command `{}`: argument name `{}` uses 1 to 64 lower-case letters, digits or `_`", self.name, arg.name));
            }
            if !seen.insert(arg.name.clone()) {
                return Err(format!("command `{}`: two arguments are named `{}`", self.name, arg.name));
            }
            if !ARG_TYPES.contains(&arg.kind()) {
                return Err(format!("command `{}`: argument `{}` has type `{}`; use one of {ARG_TYPES:?}", self.name, arg.name, arg.kind()));
            }
            if arg.required && arg.default.is_some() {
                return Err(format!("command `{}`: argument `{}` is required, so it needs no default", self.name, arg.name));
            }
            if let Some(default) = &arg.default {
                let fits = match arg.kind() {
                    "int" => default.is_i64() || default.is_u64(),
                    "float" => default.is_number(),
                    "bool" => default.is_boolean(),
                    "string" => default.is_string(),
                    _ => true,
                };
                if !fits {
                    return Err(format!("command `{}`: the default of `{}` is not a {}", self.name, arg.name, arg.kind()));
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    #[error("{0}")]
    Toml(#[from] toml::de::Error),

    #[error(
        "`[[pages]]` is no longer supported: declare each page as an XML file under `pages/` \
         with `<page route=\"/…\">` as the root element"
    )]
    LegacyPages,

    #[error(
        "`db::surql` no longer exists: plugins reach the database only through the structured \
         `db::*` commands on the models they are granted; remove it from `{0}`"
    )]
    RawSurql(&'static str),

    #[error("`[[schedule]]`: {0}")]
    Schedule(String),

    #[error("`[[command]]`: {0}")]
    Command(String),

    #[error("`[[watch]]`: {0}")]
    Watch(String),

    #[error("`[[events]]`: {0}")]
    Event(String),

    #[error("`bridges`: {0}")]
    Bridge(String),

    #[error("`[[roles]]`: {0}")]
    Role(String),

    #[error(
        "`[[models]]` is no longer supported: define each model in `models/<name>.json` \
         (`aether --sync-models` assigns the ids)"
    )]
    LegacyModels,
}

impl PluginManifest {
    /// Parse and normalise a `plugin.toml`. Unknown keys are ignored so the
    /// contract can grow; the removed `[[pages]]` table is an error.
    pub fn parse(text: &str) -> Result<Self, ManifestError> {
        let mut manifest: Self = toml::from_str(text)?;
        if manifest.legacy_pages.is_some() {
            return Err(ManifestError::LegacyPages);
        }
        if !manifest.models.is_empty() {
            return Err(ManifestError::LegacyModels);
        }
        const REMOVED: &str = "db::surql";
        if manifest.plugin.capabilities.iter().any(|c| c == REMOVED) {
            return Err(ManifestError::RawSurql("capabilities"));
        }
        if manifest.plugin.public_capabilities.iter().any(|c| c == REMOVED) {
            return Err(ManifestError::RawSurql("public_capabilities"));
        }
        let mut seen = std::collections::HashSet::new();
        for task in &manifest.schedule {
            task.validate().map_err(|error| ManifestError::Schedule(error.to_string()))?;
            if !seen.insert(task.name.clone()) {
                return Err(ManifestError::Schedule(format!("two tasks are named `{}`", task.name)));
            }
        }
        let mut names = std::collections::HashSet::new();
        for command in &manifest.command {
            command.validate().map_err(ManifestError::Command)?;
            if !names.insert(command.name.clone()) {
                return Err(ManifestError::Command(format!("two commands are named `{}`", command.name)));
            }
        }
        let mut watches = std::collections::HashSet::new();
        for watch in &manifest.watch {
            watch.validate().map_err(ManifestError::Watch)?;
            if !watches.insert(watch.name.clone()) {
                return Err(ManifestError::Watch(format!("two watches are named `{}`", watch.name)));
            }
        }
        let mut roles = std::collections::HashSet::new();
        for role in &manifest.roles {
            role.validate().map_err(ManifestError::Role)?;
            if !roles.insert(role.name.clone()) {
                return Err(ManifestError::Role(format!("two roles are named `{}`", role.name)));
            }
        }
        if let Some(unknown) = manifest.plugin.bridges.iter().find(|name| crate::bridges::find(name).is_none()) {
            return Err(ManifestError::Bridge(format!(
                "`{unknown}` is not a bridge plugins can call; use some of: {}",
                crate::bridges::names()
            )));
        }
        let mut events = std::collections::HashSet::new();
        for event in &manifest.events {
            event.validate(&manifest.plugin).map_err(ManifestError::Event)?;
            if !events.insert((event.direction().to_string(), event.name.clone())) {
                return Err(ManifestError::Event(format!("`{}` is declared twice", event.name)));
            }
        }
        manifest.normalize();
        Ok(manifest)
    }

    pub fn normalize(&mut self) {
        self.plugin.hoist_meta();
    }
}

#[cfg(test)]
mod command_tests {
    use super::*;

    fn parse(extra: &str) -> Result<PluginManifest, ManifestError> {
        PluginManifest::parse(&format!("[plugin]\nname = \"p\"\nversion = \"1\"\n{extra}"))
    }

    #[test]
    fn commands_are_read_with_typed_arguments() -> Result<(), ManifestError> {
        let manifest = parse(
            "[[command]]\nname = \"import_rates\"\nfunction = \"import\"\nhelp = \"Fetch\"\n\
             args = [{ name = \"date\" }, { name = \"days\", type = \"int\", default = 7 }, { name = \"force\", type = \"bool\", required = true }]\n",
        )?;
        let command = &manifest.command[0];
        assert_eq!((command.name.as_str(), command.function.as_str()), ("import_rates", "import"));
        assert_eq!(command.args[0].kind(), "string");
        assert_eq!(command.args[1].kind(), "int");
        assert!(command.args[2].required);
        Ok(())
    }

    #[test]
    fn watches_are_read_with_defaults_and_checked() -> Result<(), ManifestError> {
        let manifest = parse("[[watch]]\nname = \"invoices\"\npath = \"inbox\"\nfunction = \"import\"\npattern = \"*.csv\"\n")?;
        let watch = &manifest.watch[0];
        assert_eq!((watch.events.as_slice(), watch.debounce_ms, watch.recursive), (["created".to_string(), "modified".to_string()].as_slice(), 500, false));
        for (label, text) in [
            ("name", "[[watch]]\nname = \"Bad\"\nfunction = \"f\"\n"),
            ("function", "[[watch]]\nname = \"w\"\nfunction = \"\"\n"),
            ("escape", "[[watch]]\nname = \"w\"\nfunction = \"f\"\npath = \"../elsewhere\"\n"),
            ("absolute", "[[watch]]\nname = \"w\"\nfunction = \"f\"\npath = \"/etc\"\n"),
            ("event", "[[watch]]\nname = \"w\"\nfunction = \"f\"\nevents = [\"opened\"]\n"),
            ("no events", "[[watch]]\nname = \"w\"\nfunction = \"f\"\nevents = []\n"),
            ("glob", "[[watch]]\nname = \"w\"\nfunction = \"f\"\npattern = \"[\"\n"),
            ("debounce", "[[watch]]\nname = \"w\"\nfunction = \"f\"\ndebounce_ms = 5\n"),
            ("poll", "[[watch]]\nname = \"w\"\nfunction = \"f\"\npoll_secs = 0\n"),
            ("duplicate", "[[watch]]\nname = \"w\"\nfunction = \"f\"\n[[watch]]\nname = \"w\"\nfunction = \"g\"\n"),
        ] {
            assert!(matches!(parse(text), Err(ManifestError::Watch(_))), "{label}");
        }
        Ok(())
    }

    #[test]
    fn events_are_checked_against_the_plugins_dependencies_and_capabilities() -> Result<(), ManifestError> {
        let with = |extra: &str| {
            PluginManifest::parse(&format!(
                "[plugin]\nname = \"crm\"\nversion = \"1\"\ndependencies = [\"chat\"]\ncapabilities = [\"events::emit\", \"events::subscribe\"]\n{extra}"
            ))
        };
        let ok = with("[[events]]\nname = \"lead_won\"\n[[events]]\nname = \"chat.message_posted\"\ndirection = \"listen\"\nhandler = \"on_message\"\n")?;
        assert_eq!(ok.events[0].direction(), "emit");
        assert_eq!(ok.events[1].direction(), "listen");
        for (label, text) in [
            ("dot in an emitted name", "[[events]]\nname = \"a.b\"\n"),
            ("listen without a plugin part", "[[events]]\nname = \"posted\"\ndirection = \"listen\"\nhandler = \"h\"\n"),
            ("not a dependency", "[[events]]\nname = \"billing.paid\"\ndirection = \"listen\"\nhandler = \"h\"\n"),
            ("no handler", "[[events]]\nname = \"chat.x\"\ndirection = \"listen\"\n"),
            ("direction", "[[events]]\nname = \"x\"\ndirection = \"both\"\n"),
            ("duplicate", "[[events]]\nname = \"x\"\n[[events]]\nname = \"x\"\n"),
        ] {
            assert!(matches!(with(text), Err(ManifestError::Event(_))), "{label}");
        }
        // Without the capabilities, declaring either direction is refused.
        let bare = |extra: &str| PluginManifest::parse(&format!("[plugin]\nname = \"p\"\nversion = \"1\"\ndependencies = [\"chat\"]\n{extra}"));
        assert!(matches!(bare("[[events]]\nname = \"x\"\n"), Err(ManifestError::Event(_))));
        assert!(matches!(bare("[[events]]\nname = \"chat.x\"\ndirection = \"listen\"\nhandler = \"h\"\n"), Err(ManifestError::Event(_))));
        // A plugin may listen to its own events.
        let own = PluginManifest::parse("[plugin]\nname = \"p\"\nversion = \"1\"\ncapabilities = [\"events::subscribe\"]\n[[events]]\nname = \"p.x\"\ndirection = \"listen\"\nhandler = \"h\"\n")?;
        assert_eq!(own.events.len(), 1);
        Ok(())
    }

    #[test]
    fn a_bad_command_refuses_the_plugin_with_a_reason() {
        for (label, text) in [
            ("name", "[[command]]\nname = \"Bad Name\"\nfunction = \"f\"\n"),
            ("function", "[[command]]\nname = \"x\"\nfunction = \"\"\n"),
            ("type", "[[command]]\nname = \"x\"\nfunction = \"f\"\nargs = [{ name = \"a\", type = \"date\" }]\n"),
            ("duplicate arg", "[[command]]\nname = \"x\"\nfunction = \"f\"\nargs = [{ name = \"a\" }, { name = \"a\" }]\n"),
            ("required default", "[[command]]\nname = \"x\"\nfunction = \"f\"\nargs = [{ name = \"a\", required = true, default = \"v\" }]\n"),
            ("default type", "[[command]]\nname = \"x\"\nfunction = \"f\"\nargs = [{ name = \"a\", type = \"int\", default = \"v\" }]\n"),
            ("duplicate", "[[command]]\nname = \"x\"\nfunction = \"f\"\n[[command]]\nname = \"x\"\nfunction = \"g\"\n"),
        ] {
            assert!(matches!(parse(text), Err(ManifestError::Command(_))), "{label}");
        }
    }
}
