use serde::{Deserialize, Serialize};

/// Contract version the addon was authored against (`[plugin.api]`).
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct PluginApi {
    #[serde(default = "default_api_version")]
    pub version: String,
}

fn default_api_version() -> String {
    "0.1".into()
}

/// Optional metadata (`[plugin.meta]`). Prefer this over top-level kind/workspace long-term.
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct PluginMeta {
    /// `"addon"` | `"theme"` | `"bridge_pack"` | …
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub workspace: Option<String>,
}

#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
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

#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct PluginCategory {
    pub name: String,
    #[serde(default)]
    pub label: String,
}

#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct AccessModelDef {
    pub name: String,
    #[serde(default)]
    pub permissions: Vec<String>,
}

/// Core `[plugin]` table. Extra future keys are ignored (serde default).
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
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
    /// Plugins this addon uses when they are installed in the organization, and works without
    /// otherwise. They may be called with `plugins::call` and their models read as
    /// dependencies' are; ask `plugins::available` first. They are not installed with this
    /// plugin and never block it.
    #[serde(default)]
    pub optional_dependencies: Vec<String>,
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
            optional_dependencies: Vec::new(),
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
        self.wasm_file.as_deref().is_none_or(str::is_empty)
            && self.script.as_deref().is_some_and(|s| !s.is_empty())
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

#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
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

#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
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

#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
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
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
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
    /// For `emit`: the top-level fields of the event's payload that a webhook may send to an outside
    /// system. Without it, no webhook can be made for the event; only these fields ever leave.
    #[serde(default)]
    pub webhook: Option<Vec<String>>,
}

impl PluginEventDef {
    pub fn direction(&self) -> &str {
        self.direction.as_deref().unwrap_or("emit")
    }

    /// Check everything a manifest can get wrong, so a plugin is refused when it is loaded.
    pub fn validate(&self, plugin: &PluginDefinition) -> Result<(), String> {
        let plain = |part: &str| {
            plain_name(part, |c| {
                c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-'
            })
        };
        let holds = |capability: &str| plugin.capabilities.iter().any(|held| held == capability);
        if let Some(fields) = &self.webhook {
            if self.direction() != "emit" {
                return Err(format!(
                    "event `{}`: `webhook` is for events the plugin emits",
                    self.name
                ));
            }
            if fields.is_empty() || fields.len() > 50 {
                return Err(format!(
                    "event `{}`: `webhook` lists 1 to 50 payload fields",
                    self.name
                ));
            }
            if let Some(bad) = fields.iter().find(|field| {
                !plain_name(field, |c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
            }) {
                return Err(format!(
                    "event `{}`: `{bad}` is not a payload field name",
                    self.name
                ));
            }
        }
        match self.direction() {
            "emit" => {
                if !plain(&self.name) {
                    return Err(format!(
                        "emitted event `{}`: use 1 to 64 lower-case letters, digits, `_` or `-` (no dots)",
                        self.name
                    ));
                }
                if !holds("events::emit") {
                    return Err(format!(
                        "`{}` is emitted, so the plugin needs the capability `events::emit`",
                        self.name
                    ));
                }
            }
            "listen" => {
                let Some((source, event)) = self.name.split_once('.') else {
                    return Err(format!(
                        "listened event `{}` is named <plugin>.<event>, such as `chat.message_posted`",
                        self.name
                    ));
                };
                if !plain(source) || !plain(event) {
                    return Err(format!(
                        "listened event `{}`: both parts use lower-case letters, digits, `_` or `-`",
                        self.name
                    ));
                }
                if source != plugin.name
                    && !plugin
                        .dependencies
                        .iter()
                        .any(|dependency| dependency == source)
                {
                    return Err(format!(
                        "`{}` comes from `{source}`, which must be listed under `dependencies`",
                        self.name
                    ));
                }
                if self.handler.as_deref().is_none_or(str::is_empty) {
                    return Err(format!(
                        "listened event `{}` names no `handler` function",
                        self.name
                    ));
                }
                if !holds("events::subscribe") {
                    return Err(format!(
                        "`{}` is listened to, so the plugin needs the capability `events::subscribe`",
                        self.name
                    ));
                }
                if self.max_attempts.is_some_and(|n| !(1..=20).contains(&n)) {
                    return Err(format!(
                        "listened event `{}`: max_attempts is 1 to 20",
                        self.name
                    ));
                }
                if let Some(queue) = &self.queue {
                    if !plain(queue) {
                        return Err(format!(
                            "listened event `{}`: a queue name is lower-case letters, digits, `_` and `-`",
                            self.name
                        ));
                    }
                }
            }
            other => {
                return Err(format!(
                    "event `{}`: direction is `emit` or `listen`, not `{other}`",
                    self.name
                ));
            }
        }
        Ok(())
    }
}

#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct PluginPermissionDef {
    pub key: String,
    #[serde(default)]
    pub label: Option<String>,
}

#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct PluginCommunication {
    #[serde(default)]
    pub channels: Vec<String>,
}

#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
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
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct PluginI18nDef {
    pub default: String,
}

/// `[app]`: makes the plugin its own tile on the Apps launcher.
///
/// Workspace membership places the tile in a folder with siblings. Thin modules
/// (one sidebar menu or fewer) are folded into the workspace host tile unless
/// `standalone = true` (small modules live in the host sidebar; big ones
/// keep their own tile).
///
/// ```toml
/// [app]
/// label = "Chat"              # defaults to the plugin's label
/// icon = "message-square"     # a lucide icon name; the tile shows a letter if unknown
/// route = "/chat"             # a page of this plugin; what the tile opens
/// description = "Team messaging"
/// standalone = true           # keep this tile even with a single menu (optional)
/// also_in_host = true         # keep the tile and also show its menu in the host sidebar (optional)
/// folder = "Time"             # join/create a named folder (same workspace category)
/// roles = ["hr_user"]         # optional; omit = any member may see the tile
/// nav = true                  # a link beside Users / Settings instead of a tile (optional)
///
/// [app.tile]                  # a live number on the launcher tile; optional
/// function = "count_open"     # a function of this plugin answering a number
/// label = "Open requests"     # optional: what the number counts
/// unit = "%"                  # optional: shown after the number
///
/// [[app.palette]]             # an entry of the command palette (Ctrl/⌘+K); optional, repeatable
/// label = "New leave request"
/// href = "/leave/requests/new" # or `function = "start_request"` to run a function of this plugin,
///                             # or `search = "find_requests"` to add records to the palette's results
/// icon = "plus"               # optional
/// keywords = "holiday absence" # optional extra words that find it
/// shortcut = "alt+l"          # optional: `mod`, `alt` and `shift` joined by `+`, then a letter or digit
/// roles = ["leave_user"]      # optional; omit = whoever sees the app
/// ```
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct PluginAppDef {
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    pub route: String,
    #[serde(default)]
    pub description: Option<String>,
    /// Keep a launcher tile even when the plugin has only one menu entry.
    #[serde(default)]
    pub standalone: bool,
    /// Keep the launcher tile and also list this app's menu in the workspace host's sidebar.
    /// Only matters for a folder workspace; the tile is never folded away.
    #[serde(default)]
    pub also_in_host: bool,
    /// Named folder on the Apps home screen; plugins that share the same
    /// `folder` (within a workspace) appear together. Install category stays
    /// the workspace.
    #[serde(default)]
    pub folder: Option<String>,
    /// Roles that may see this app tile (OR). Short names become `<plugin>.name`.
    /// Empty = any organization member.
    #[serde(default)]
    pub roles: Vec<String>,
    /// Show `route` as a link in the home menu's Navigate section (next to Users and
    /// Settings) instead of a tile on the Apps launcher.
    #[serde(default)]
    pub nav: bool,
    /// Entries this app adds to the command palette (`[[app.palette]]`).
    #[serde(default)]
    pub palette: Vec<PluginPaletteDef>,
    /// A live number on the launcher tile (`[app.tile]`).
    #[serde(default)]
    pub tile: Option<PluginTileDef>,
}

/// `[app.tile]`: the launcher tile shows the answer of a read-only function of the plugin.
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct PluginTileDef {
    /// A function of this plugin that answers a number (or `{ "count": n }`, or a list).
    pub function: String,
    /// What the number counts, shown under it.
    #[serde(default)]
    pub label: Option<String>,
    /// Shown after the number (`%`, `h`).
    #[serde(default)]
    pub unit: Option<String>,
}

/// One `[[app.palette]]` entry: a page the command palette offers, with an optional shortcut.
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct PluginPaletteDef {
    pub label: String,
    /// An app path starting with `/`: what the entry opens. Exactly one of `href`, `function`
    /// and `search` is set.
    #[serde(default)]
    pub href: Option<String>,
    /// A function of this plugin the entry runs (with no arguments). An answer with an
    /// `href` opens that page; one with a `message` shows it.
    #[serde(default)]
    pub function: Option<String>,
    /// A function of this plugin that finds records: called with `{ "q": "<typed>", "limit": 8 }`,
    /// it answers a list of `{ "label", "href", "context"? }`. The entry itself is not listed.
    #[serde(default)]
    pub search: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    /// Extra words that find the entry.
    #[serde(default)]
    pub keywords: Option<String>,
    /// `mod`, `alt` and `shift` joined by `+`, then a letter or digit: `alt+n`, `mod+shift+e`.
    #[serde(default)]
    pub shortcut: Option<String>,
    /// Roles that see the entry (OR). Short names become `<plugin>.name`. Empty = whoever sees the app.
    #[serde(default)]
    pub roles: Vec<String>,
    /// Also list the entry under the shell's "New" button (the `create` block). Needs an `href`.
    #[serde(default)]
    pub create: bool,
}

/// Full `plugin.toml` document. Unknown top-level tables are ignored by serde.
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct PluginManifest {
    pub plugin: PluginDefinition,
    #[serde(default)]
    pub models: Vec<PluginModelRef>,
    /// Present only to reject the removed `[[pages]]` table with a clear error.
    #[serde(default, rename = "pages")]
    #[cfg_attr(feature = "schemars", schemars(skip))]
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
    /// Groups the plugin adds to Settings (`[[setting_group]]`).
    #[serde(default)]
    pub setting_group: Vec<PluginSettingGroupDef>,
    /// Settings the plugin adds (`[[setting]]`); read with `settings::get`.
    #[serde(default)]
    pub setting: Vec<PluginSettingDef>,
    /// Functions the AI assistant may call (`[[ai_tool]]`); see [`PluginAiToolDef`].
    #[serde(default)]
    pub ai_tool: Vec<PluginAiToolDef>,
    /// The plugin's own documentation (`[docs]`); see [`PluginDocsDef`]. Without this table the plugin has none.
    #[serde(default)]
    pub docs: Option<PluginDocsDef>,
    /// Models offered through the record API (`[[api]]`); see [`PluginApiDef`]. Without any, none is.
    #[serde(default)]
    pub api: Vec<PluginApiDef>,
    /// Addresses outside systems may call (`[[inbound]]`); see [`PluginInboundDef`].
    #[serde(default)]
    pub inbound: Vec<PluginInboundDef>,
    /// Mail addresses that belong to the plugin (`[[mail_routes]]`); see [`PluginMailRouteDef`].
    #[serde(default)]
    pub mail_routes: Vec<PluginMailRouteDef>,
    /// Viewers for file types the web app does not know (`[[viewer]]`); see [`PluginViewerDef`].
    #[serde(default)]
    pub viewer: Vec<PluginViewerDef>,
    /// Emoji for the message boxes of every plugin (`[[emoji_pack]]`); see [`PluginPackDef`].
    #[serde(default)]
    pub emoji_pack: Vec<PluginPackDef>,
    /// GIFs for the message boxes of every plugin (`[[gif_pack]]`); see [`PluginPackDef`].
    #[serde(default)]
    pub gif_pack: Vec<PluginPackDef>,
    /// Live topics browsers may join over the WebSocket (`[[realtime]]`); see [`PluginRealtimeDef`].
    #[serde(default)]
    pub realtime: Vec<PluginRealtimeDef>,
    /// Rooms for video meetings (`[[sfu]]`); see [`PluginSfuDef`].
    #[serde(default)]
    pub sfu: Vec<PluginSfuDef>,
    /// The function that decides who may read the files the plugin guards (`[media_guard]`); see [`PluginMediaGuardDef`].
    #[serde(default)]
    pub media_guard: Option<PluginMediaGuardDef>,
}

/// The plugin's say over who may read the stored files it has put under its guard with `media::protect`. The kernel
/// calls `function` as the person who asks, with `{ key }`, and serves the file only when it answers
/// `{ "allow": true }`. See `docs/architecture/media.md`.
///
/// ```toml
/// [media_guard]
/// function = "can_read_media"
/// ```
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginMediaGuardDef {
    pub function: String,
}

/// A live topic the plugin offers over the WebSocket (`/api/ui/realtime`). The full name on the wire is
/// `<plugin>:<topic>`. The plugin decides who may join with `subscribe`, a function of its own that the kernel
/// calls as the person who asks; messages are published with `realtime::publish`. See
/// `docs/architecture/realtime.md`.
///
/// ```toml
/// [[realtime]]
/// topic = "channel.{channel}"      # `{name}` matches one segment
/// subscribe = "can_join_channel"   # answers { "allow": true }
/// presence = true                  # tell members who is connected
/// client_events = ["typing"]       # kinds a browser may send to the others
/// history = 200                    # recent messages kept for resuming (default 100, at most 1000)
/// ```
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginRealtimeDef {
    pub topic: String,
    pub subscribe: String,
    #[serde(default)]
    pub presence: bool,
    #[serde(default)]
    pub client_events: Vec<String>,
    #[serde(default = "default_realtime_history")]
    pub history: usize,
}

fn default_realtime_history() -> usize {
    100
}

/// A kind of video-meeting room the plugin offers (`/api/ui/sfu/<plugin>:<room>`). The plugin decides who may join with
/// `join`, a function of its own that the kernel calls on every join as the person who asks, and answers
/// `{ "allow": true, "role": "host" | "member", "name": "…" }`. See `docs/architecture/meet_sfu.md`.
///
/// ```toml
/// [[sfu]]
/// room = "meeting.{meeting}"       # `{name}` matches one segment
/// join = "can_join_meeting"
/// max_participants = 50            # optional; the server's own limit still applies
/// ```
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginSfuDef {
    pub room: String,
    pub join: String,
    #[serde(default)]
    pub max_participants: Option<usize>,
}

/// The most participants a room may declare.
pub const SFU_MAX_PARTICIPANTS: usize = 500;

impl PluginSfuDef {
    fn validate(&self) -> Result<(), String> {
        let room = &self.room;
        // A room pattern follows the same rules as a realtime topic.
        let as_topic = PluginRealtimeDef {
            topic: room.clone(),
            subscribe: self.join.clone(),
            presence: false,
            client_events: Vec::new(),
            history: 0,
        };
        as_topic
            .validate()
            .map_err(|e| e.replace("`subscribe`", "`join`"))?;
        if self
            .max_participants
            .is_some_and(|n| n == 0 || n > SFU_MAX_PARTICIPANTS)
        {
            return Err(format!(
                "`{room}`: `max_participants` is between 1 and {SFU_MAX_PARTICIPANTS}"
            ));
        }
        Ok(())
    }

    /// The `{name}` values when `rest` (a room without its plugin prefix) fits this pattern.
    pub fn match_room(&self, rest: &str) -> Option<serde_json::Map<String, serde_json::Value>> {
        PluginRealtimeDef {
            topic: self.room.clone(),
            subscribe: self.join.clone(),
            presence: false,
            client_events: Vec::new(),
            history: 0,
        }
        .match_topic(rest)
    }
}

/// The most recent messages a topic may keep.
pub const REALTIME_MAX_HISTORY: usize = 1000;

fn realtime_plain(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 32
        && text
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

impl PluginRealtimeDef {
    fn validate(&self) -> Result<(), String> {
        let topic = &self.topic;
        let segments: Vec<&str> = topic.split('.').collect();
        if topic.len() > 100 || segments.len() > 6 {
            return Err(format!("`{topic}`: at most 6 segments and 100 characters"));
        }
        for segment in &segments {
            let inner = segment
                .strip_prefix('{')
                .and_then(|rest| rest.strip_suffix('}'));
            if !realtime_plain(inner.unwrap_or(segment)) {
                return Err(format!(
                    "`{topic}`: each segment is lower-case letters, digits, `_` and `-`, or `{{name}}`"
                ));
            }
        }
        if self.subscribe.is_empty()
            || !self
                .subscribe
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            return Err(format!(
                "`{topic}`: `subscribe` names one of the plugin's functions"
            ));
        }
        if self.history > REALTIME_MAX_HISTORY {
            return Err(format!(
                "`{topic}`: `history` is at most {REALTIME_MAX_HISTORY}"
            ));
        }
        if self.client_events.len() > 8
            || !self.client_events.iter().all(|kind| realtime_plain(kind))
        {
            return Err(format!(
                "`{topic}`: `client_events` is up to 8 plain names like `typing`"
            ));
        }
        Ok(())
    }

    /// The `{name}` values when `rest` (a topic without its plugin prefix) fits this pattern.
    pub fn match_topic(&self, rest: &str) -> Option<serde_json::Map<String, serde_json::Value>> {
        let mut params = serde_json::Map::new();
        let mut wanted = self.topic.split('.');
        let mut given = rest.split('.');
        loop {
            match (wanted.next(), given.next()) {
                (None, None) => return Some(params),
                (Some(pattern), Some(value)) => {
                    if let Some(name) = pattern.strip_prefix('{').and_then(|p| p.strip_suffix('}'))
                    {
                        if value.is_empty() || value.len() > 200 || value.contains(':') {
                            return None;
                        }
                        params.insert(
                            name.to_string(),
                            serde_json::Value::String(value.to_string()),
                        );
                    } else if pattern != value {
                        return None;
                    }
                }
                _ => return None,
            }
        }
    }
}

/// A viewer the plugin adds to the web app's document viewer. The plugin ships a JavaScript module under
/// `assets/`; it runs in a sandboxed frame with no access to the app, the session or the kernel API, and
/// receives only the bytes of the file being opened.
///
/// ```toml
/// [[viewer]]
/// name = "dwg"
/// label = "Drawing"
/// mimes = ["image/vnd.dwg"]     # exact types, or `image/*`
/// extensions = ["dwg", "dxf"]   # without the dot
/// module = "viewers/dwg.js"     # under assets/
/// priority = 50                 # higher wins; the built-in viewers use 100
/// hosts = []                    # https origins the module may fetch from (default none)
/// ```
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginViewerDef {
    pub name: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub mimes: Vec<String>,
    #[serde(default)]
    pub extensions: Vec<String>,
    /// Path of the module relative to the plugin's `assets/` folder.
    pub module: String,
    #[serde(default = "default_viewer_priority")]
    pub priority: i32,
    #[serde(default)]
    pub hosts: Vec<String>,
}

fn default_viewer_priority() -> i32 {
    50
}

impl PluginViewerDef {
    fn validate(&self) -> Result<(), String> {
        let name = &self.name;
        if name.is_empty()
            || name.len() > 64
            || !name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
        {
            return Err(format!(
                "`{name}`: a name is lower-case letters, digits, `_` and `-`"
            ));
        }
        if self.mimes.is_empty() && self.extensions.is_empty() {
            return Err(format!(
                "`{name}`: give `mimes` or `extensions`, or the viewer opens nothing"
            ));
        }
        for mime in &self.mimes {
            let ok = mime.split_once('/').is_some_and(|(kind, sub)| {
                !kind.is_empty()
                    && !sub.is_empty()
                    && mime.len() <= 100
                    && mime.chars().all(|c| {
                        c.is_ascii_lowercase() || c.is_ascii_digit() || "/+.-*_".contains(c)
                    })
            });
            if !ok {
                return Err(format!(
                    "`{name}`: `{mime}` is not a type like `image/png` or `image/*`"
                ));
            }
        }
        for ext in &self.extensions {
            if ext.is_empty()
                || ext.len() > 16
                || !ext
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
            {
                return Err(format!(
                    "`{name}`: `{ext}` is not an extension (lower-case, no dot)"
                ));
            }
        }
        let module = &self.module;
        let clean = !module.is_empty()
            && !module.starts_with('/')
            && !module.contains('\\')
            && module
                .split('/')
                .all(|part| !part.is_empty() && part != "." && part != "..");
        if !clean || !(module.ends_with(".js") || module.ends_with(".mjs")) {
            return Err(format!(
                "`{name}`: `module` is a .js or .mjs file inside `assets/`, like `viewers/x.js`"
            ));
        }
        if !(0..=1000).contains(&self.priority) {
            return Err(format!("`{name}`: `priority` is 0 to 1000"));
        }
        if self.hosts.len() > 8 {
            return Err(format!("`{name}`: at most 8 `hosts`"));
        }
        for host in &self.hosts {
            let ok = host.strip_prefix("https://").is_some_and(|rest| {
                !rest.is_empty()
                    && rest.len() <= 253
                    && rest
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || ".-:".contains(c))
            });
            if !ok {
                return Err(format!(
                    "`{name}`: `{host}` is not an origin like `https://tiles.example.com`"
                ));
            }
        }
        Ok(())
    }
}

/// A pack the plugin adds to a registry of the message box: emoji (`[[emoji_pack]]`) or GIFs (`[[gif_pack]]`). Any
/// message box of the organization (`<composer emoji="true" gif="true">`) offers the packs of every enabled plugin, so
/// one plugin ships them and the others only ask for them. The index file is served as it is from the plugin's
/// assets, so no plugin code runs when it is fetched.
///
/// ```toml
/// [[emoji_pack]]
/// name = "emojibase"
/// label = "Emoji"
/// file = "emoji/pack.json"      # under assets/
///
/// [[gif_pack]]
/// name = "noto-animated"
/// label = "Animated emoji"
/// file = "gif/pack.json"        # under assets/
/// ```
///
/// An emoji file is `{ "groups": [{ "key", "name" }], "emoji": [{ "c": char, "n": name, "t": [words], "s":
/// [shortcodes], "g": group index, "k": [five skin tones] }] }`; `t`, `s` and `k` are optional. A GIF file is
/// `{ "gifs": [{ "n": name, "f": path under assets/, "t": [words], "s": [shortcodes] }] }`; `t` and `s` are optional.
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginPackDef {
    pub name: String,
    #[serde(default)]
    pub label: Option<String>,
    /// Path of the JSON file relative to the plugin's `assets/` folder.
    pub file: String,
}

impl PluginPackDef {
    fn validate(&self) -> Result<(), String> {
        let name = &self.name;
        if name.is_empty()
            || name.len() > 64
            || !name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
        {
            return Err(format!(
                "`{name}`: a name is lower-case letters, digits, `_` and `-`"
            ));
        }
        let file = &self.file;
        let clean = !file.is_empty()
            && !file.starts_with('/')
            && !file.contains('\\')
            && file
                .split('/')
                .all(|part| !part.is_empty() && part != "." && part != "..");
        if !clean || !file.ends_with(".json") {
            return Err(format!(
                "`{name}`: `file` is a .json file inside `assets/`, like `emoji/pack.json`"
            ));
        }
        Ok(())
    }
}

/// How an inbound hook proves that a request comes from who it should.
pub const INBOUND_VERIFY: [&str; 3] = ["aether", "hmac-sha256", "token"];

/// An address that an outside system (a payment provider, a form service) calls to tell the plugin
/// something: `POST /api/hooks/<organization>/<plugin>/<name>`. The kernel checks who is calling
/// **before** any plugin code runs, then calls `function` with the body.
///
/// ```toml
/// [[inbound]]
/// name = "payments"
/// function = "on_payment"
/// verify = "hmac-sha256"       # "aether" (t= and v1= like our webhooks), "hmac-sha256" or "token"
/// header = "x-signature"       # the header with the signature or the token
/// ```
///
/// An administrator makes the hook's secret and gives it to the other system (see the API page). Until
/// then, and while the hook is switched off, the address answers 404.
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginInboundDef {
    pub name: String,
    pub function: String,
    #[serde(default = "default_inbound_verify")]
    pub verify: String,
    /// The header that carries the signature or token. Default for `aether`: `x-aether-signature`.
    #[serde(default)]
    pub header: Option<String>,
    /// For `aether`: how far the signed time may be from now, in seconds (default 300).
    #[serde(default)]
    pub tolerance_secs: Option<u64>,
}

/// A mail address that belongs to one plugin function: mail sent to `<alias>@<the organization's mail domain>`
/// (or `<alias>+<anything>@...`) is handed to `function` as a clean structure, not raw MIME.
///
/// ```toml
/// [[mail_routes]]
/// alias = "support"
/// function = "on_mail"
/// model = "ticket"             # the model new records land in (shown to administrators)
/// require = ["dkim"]           # optional: results the sender's domain must pass ("spf", "dkim")
/// ```
///
/// A reply to mail the plugin sent (its address or `Message-ID` carries a thread token) is not handed to the
/// function: the kernel appends it to the record's chatter.
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginMailRouteDef {
    pub alias: String,
    pub function: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub require: Vec<String>,
}

impl PluginMailRouteDef {
    fn validate(&self) -> Result<(), String> {
        let ok = !self.alias.is_empty()
            && self.alias.len() <= 64
            && self.alias.chars().all(|c| {
                c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-' || c == '.'
            });
        if !ok {
            return Err(format!(
                "`{}`: an alias is lower-case letters, digits, `.`, `_` and `-`",
                self.alias
            ));
        }
        if self.function.is_empty()
            || !self
                .function
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            return Err(format!(
                "`{}`: `function` names one of the plugin's functions",
                self.alias
            ));
        }
        if let Some(bad) = self
            .require
            .iter()
            .find(|r| !["spf", "dkim"].contains(&r.as_str()))
        {
            return Err(format!(
                "`{}`: `{bad}` is not a result to require (use spf or dkim)",
                self.alias
            ));
        }
        Ok(())
    }
}

fn default_inbound_verify() -> String {
    "aether".into()
}

impl PluginInboundDef {
    /// The header to read, with the default for `aether`.
    pub fn header_name(&self) -> String {
        self.header
            .clone()
            .unwrap_or_else(|| "x-aether-signature".to_string())
            .to_ascii_lowercase()
    }

    fn validate(&self) -> Result<(), String> {
        let plain = |text: &str| {
            !text.is_empty()
                && text.len() <= 64
                && text
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
        };
        if !plain(&self.name) {
            return Err(format!(
                "`{}`: a name is lower-case letters, digits, `_` and `-`",
                self.name
            ));
        }
        if self.function.is_empty()
            || !self
                .function
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            return Err(format!(
                "`{}`: `function` names one of the plugin's functions",
                self.name
            ));
        }
        if !INBOUND_VERIFY.contains(&self.verify.as_str()) {
            return Err(format!(
                "`{}`: `verify` is aether, hmac-sha256 or token, not `{}`",
                self.name, self.verify
            ));
        }
        if self.verify != "aether" && self.header.is_none() {
            return Err(format!(
                "`{}`: `{}` needs `header`, the header that carries it",
                self.name, self.verify
            ));
        }
        if let Some(header) = &self.header
            && (header.is_empty()
                || !header
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-')
                || [
                    "authorization",
                    "cookie",
                    "host",
                    "content-type",
                    "content-length",
                ]
                .contains(&header.to_ascii_lowercase().as_str()))
        {
            return Err(format!(
                "`{}`: `{header}` cannot carry the signature",
                self.name
            ));
        }
        if self.tolerance_secs.is_some() && self.verify != "aether" {
            return Err(format!(
                "`{}`: `tolerance_secs` is for `verify = \"aether\"`",
                self.name
            ));
        }
        Ok(())
    }
}

/// The operations a record API can offer on a model.
pub const API_OPERATIONS: [&str; 5] = ["list", "get", "create", "update", "delete"];

/// A model the plugin offers through the record API (`/api/v1/<plugin>/<model>`), and what may be done to
/// it. Nothing is offered unless the plugin says so. The caller's own rights (record and field rules, roles)
/// apply on top, and an API token's scopes narrow it further.
///
/// ```toml
/// [[api]]
/// model = "lead"
/// operations = ["list", "get", "create", "update"]   # default: ["list", "get"]
/// ```
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginApiDef {
    pub model: String,
    #[serde(default = "default_api_operations")]
    pub operations: Vec<String>,
}

fn default_api_operations() -> Vec<String> {
    vec!["list".into(), "get".into()]
}

impl PluginApiDef {
    /// Whether the block allows `operation` (one of [`API_OPERATIONS`]).
    pub fn allows(&self, operation: &str) -> bool {
        self.operations.iter().any(|o| o == operation)
    }
}

/// The plugin's documentation: markdown files that Aether collects when the plugin is loaded and shows on its
/// documentation website (`aether --serve-docs`, `/docs`) and to LLM tools (`aether --mcp`).
///
/// ```toml
/// [docs]
/// dir = "docs"              # the folder with the .md files (default "docs")
/// visibility = "public"     # public | authenticated | org (default "authenticated")
/// ```
///
/// `README.md` is the landing page; other files are pages at their path (`guide/setup.md` is `guide/setup`).
/// Pages may start with a `---` block of `title`, `description` and `order`. Links between pages are relative
/// `.md` links; images must be `https://` addresses. Raw HTML is shown as text.
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginDocsDef {
    /// The folder, relative to the plugin, holding the markdown files. Default `docs`.
    #[serde(default = "default_docs_dir")]
    pub dir: String,
    /// Who may read the docs: `public` (anyone who can reach the site), `authenticated` (any signed-in user; the
    /// default) or `org` (members of an organization that has the plugin installed).
    #[serde(default)]
    pub visibility: aether_docs::plugin_docs::Visibility,
}

fn default_docs_dir() -> String {
    "docs".to_string()
}

impl Default for PluginDocsDef {
    fn default() -> Self {
        Self {
            dir: default_docs_dir(),
            visibility: Default::default(),
        }
    }
}

/// A function of the plugin the AI assistant may ask for, to answer a question with the person's own data.
///
/// ```toml
/// [[ai_tool]]
/// name = "leave_balance"
/// description = "How many days of each leave type the person has left"
/// function = "assistant_leave_balance"
/// schema = { type = "object", properties = { year = { type = "integer" } } }
/// writes = false
/// ```
///
/// The kernel runs the function **as the person who is chatting**, so roles and record rules apply as in
/// the rest of the app. A tool with `writes = true` is never run by the assistant: the person is shown
/// the proposed call and confirms it. What a tool returns is shown to the model as data, never as instructions.
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct PluginAiToolDef {
    pub name: String,
    pub description: String,
    pub function: String,
    /// JSON Schema of the arguments, an object schema. Left out, the tool takes none.
    #[serde(default = "no_arguments")]
    pub schema: serde_json::Value,
    #[serde(default)]
    pub writes: bool,
}

fn no_arguments() -> serde_json::Value {
    serde_json::json!({ "type": "object", "properties": {} })
}

impl PluginAiToolDef {
    pub fn validate(&self, plugin: &str) -> Result<(), String> {
        if !plain_name(&self.name, |c| {
            c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'
        }) {
            return Err(format!(
                "the name `{}` is lowercase letters, digits and `_`, at most 64 characters",
                self.name
            ));
        }
        // The model sees `<plugin>__<tool>`, and providers allow 64 characters.
        if aether_ai::tool_name(plugin, &self.name).len() > 64 {
            return Err(format!(
                "`{}` is too long: `{plugin}__{}` must be at most 64 characters",
                self.name, self.name
            ));
        }
        let described = self.description.trim();
        if described.is_empty() || described.len() > 500 {
            return Err(format!(
                "`{}` needs a description of at most 500 characters: it is what the model reads",
                self.name
            ));
        }
        if !plain_name(&self.function, |c| c.is_ascii_alphanumeric() || c == '_') {
            return Err(format!(
                "`{}`: `function` names one of the plugin's functions",
                self.name
            ));
        }
        if self.schema.get("type").and_then(serde_json::Value::as_str) != Some("object") {
            return Err(format!(
                "`{}`: `schema` is a JSON Schema with `type = \"object\"`",
                self.name
            ));
        }
        if self.schema.to_string().len() > 8 * 1024 {
            return Err(format!("`{}`: `schema` is over 8 KB", self.name));
        }
        Ok(())
    }
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
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
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
        if !plain_name(&self.name, |c| {
            c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-'
        }) {
            return Err(format!(
                "watch name `{}`: use 1 to 64 lower-case letters, digits, `_` or `-`",
                self.name
            ));
        }
        if self.function.is_empty() {
            return Err(format!("watch `{}` names no function", self.name));
        }
        crate::plugin_files::safe_relative(&self.path)
            .map_err(|e| format!("watch `{}`: {e}", self.name))?;
        if self.events.is_empty() {
            return Err(format!(
                "watch `{}` lists no events; use some of {WATCH_EVENTS:?}",
                self.name
            ));
        }
        if let Some(bad) = self
            .events
            .iter()
            .find(|event| !WATCH_EVENTS.contains(&event.as_str()))
        {
            return Err(format!(
                "watch `{}`: `{bad}` is not an event; use some of {WATCH_EVENTS:?}",
                self.name
            ));
        }
        if let Some(pattern) = &self.pattern {
            globset::Glob::new(pattern)
                .map_err(|e| format!("watch `{}`: pattern `{pattern}`: {e}", self.name))?;
        }
        if !(50..=60_000).contains(&self.debounce_ms) {
            return Err(format!("watch `{}`: debounce_ms is 50 to 60000", self.name));
        }
        if self
            .poll_secs
            .is_some_and(|secs| !(1..=3600).contains(&secs))
        {
            return Err(format!("watch `{}`: poll_secs is 1 to 3600", self.name));
        }
        if self.max_attempts.is_some_and(|n| !(1..=20).contains(&n)) {
            return Err(format!("watch `{}`: max_attempts is 1 to 20", self.name));
        }
        if let Some(queue) = &self.queue {
            let ok = !queue.is_empty()
                && queue.len() <= 40
                && queue
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-');
            if !ok {
                return Err(format!(
                    "watch `{}`: a queue name is lower-case letters, digits, `_` and `-`",
                    self.name
                ));
            }
        }
        Ok(())
    }
}

/// The types an argument can have. A command line gives text; the kernel converts it.
const ARG_TYPES: &[&str] = &["string", "int", "float", "bool", "json"];

/// One argument of a command.
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
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
///
/// # Master data: the kernel runs these after
/// # install / upgrade into an organization. Prefer idempotent seed functions.
/// [[command]]
/// name = "seed"
/// function = "seed_currencies"
/// on_install = true
/// on_upgrade = true
///
/// # Desk demo data for one model (not run on install). Desk passes `count`.
/// [[command]]
/// name = "demo_currency"
/// function = "generate_demo_currency"
/// demo_model = "currency"
/// args = [{ name = "count", type = "int", default = 10 }]
/// ```
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct PluginCommandDef {
    pub name: String,
    pub function: String,
    #[serde(default)]
    pub help: String,
    #[serde(default)]
    pub args: Vec<CommandArgDef>,
    /// Run this command (empty args) when the plugin is first installed in an organization.
    #[serde(default)]
    pub on_install: bool,
    /// Run this command (empty args) when the plugin is upgraded in an organization.
    #[serde(default)]
    pub on_upgrade: bool,
    /// Run this command (empty args) just before the plugin is uninstalled from an organization,
    /// to clean up outside the database (webhooks registered elsewhere, files, ...). A failure is
    /// reported but does not stop the uninstall.
    #[serde(default)]
    pub on_uninstall: bool,
    /// When set, Desk lists this command as a demo generator for that model name.
    /// Not a lifecycle seed: do not combine with `on_install` / `on_upgrade`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub demo_model: Option<String>,
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
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
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
        if !plain_name(&self.name, |c| {
            c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'
        }) {
            return Err(format!(
                "role name `{}`: use 1 to 64 lower-case letters, digits or `_`",
                self.name
            ));
        }
        Ok(())
    }
}

/// A group a plugin adds to Settings (`[[setting_group]]`). A setting whose `group` is a group of
/// the kernel (or another plugin) needs no declaration here.
///
/// ```toml
/// [[setting_group]]
/// id = "country"
/// label = "Country"
/// icon = "globe"
///
/// [[setting]]
/// group = "country"
/// key = "country.organization"   # must start with the plugin's name and a dot
/// label = "Country"
/// value = ""                     # the default; an organization overrides it
/// description = "The country this organization operates in."
/// ```
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct PluginSettingGroupDef {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
}

/// One setting a plugin adds (`[[setting]]`); see [`PluginSettingGroupDef`].
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct PluginSettingDef {
    pub group: String,
    pub key: String,
    pub label: String,
    /// The default value (text, number or boolean). Empty when left out.
    #[serde(default)]
    pub value: serde_json::Value,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub long_description: Option<String>,
}

fn setting_id(key: &str) -> String {
    key.replace('.', "_")
}

impl PluginSettingDef {
    /// The id of the settings row, derived from the key.
    pub fn row_id(&self) -> String {
        setting_id(&self.key)
    }
}

fn plain_name(name: &str, allowed: fn(char) -> bool) -> bool {
    !name.is_empty() && name.len() <= 64 && name.chars().all(allowed)
}

impl PluginCommandDef {
    /// Check everything a manifest can get wrong, so a plugin is refused when it is loaded.
    pub fn validate(&self) -> Result<(), String> {
        if !plain_name(&self.name, |c| {
            c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-'
        }) {
            return Err(format!(
                "command name `{}`: use 1 to 64 lower-case letters, digits, `_` or `-`",
                self.name
            ));
        }
        if self.function.is_empty() {
            return Err(format!("command `{}` names no function", self.name));
        }
        if let Some(model) = &self.demo_model {
            if !plain_name(model, |c| {
                c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'
            }) {
                return Err(format!(
                    "command `{}`: demo_model `{}` uses 1 to 64 lower-case letters, digits or `_`",
                    self.name, model
                ));
            }
            if self.on_install || self.on_upgrade || self.on_uninstall {
                return Err(format!(
                    "command `{}`: demo_model cannot be combined with on_install / on_upgrade (demo data is not master-data seed)",
                    self.name
                ));
            }
        }
        let mut seen = std::collections::HashSet::new();
        for arg in &self.args {
            if !plain_name(&arg.name, |c| {
                c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'
            }) {
                return Err(format!(
                    "command `{}`: argument name `{}` uses 1 to 64 lower-case letters, digits or `_`",
                    self.name, arg.name
                ));
            }
            if !seen.insert(arg.name.clone()) {
                return Err(format!(
                    "command `{}`: two arguments are named `{}`",
                    self.name, arg.name
                ));
            }
            if !ARG_TYPES.contains(&arg.kind()) {
                return Err(format!(
                    "command `{}`: argument `{}` has type `{}`; use one of {ARG_TYPES:?}",
                    self.name,
                    arg.name,
                    arg.kind()
                ));
            }
            if arg.required && arg.default.is_some() {
                return Err(format!(
                    "command `{}`: argument `{}` is required, so it needs no default",
                    self.name, arg.name
                ));
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
                    return Err(format!(
                        "command `{}`: the default of `{}` is not a {}",
                        self.name,
                        arg.name,
                        arg.kind()
                    ));
                }
            }
            if arg.name == "count" && self.demo_model.is_some() && arg.kind() != "int" {
                return Err(format!(
                    "command `{}`: demo generator argument `count` must be type `int`",
                    self.name
                ));
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

    #[error("`[[setting]]`: {0}")]
    Setting(String),

    #[error("`[[ai_tool]]`: {0}")]
    AiTool(String),

    #[error("`[[api]]`: {0}")]
    Api(String),

    #[error("`[[inbound]]`: {0}")]
    Inbound(String),

    #[error("`[[mail_routes]]`: {0}")]
    MailRoute(String),

    #[error("`[[viewer]]`: {0}")]
    Viewer(String),

    #[error("`[[emoji_pack]]`: {0}")]
    EmojiPack(String),

    #[error("`[[gif_pack]]`: {0}")]
    GifPack(String),

    #[error("`[[realtime]]`: {0}")]
    Realtime(String),
    #[error("`[[sfu]]`: {0}")]
    Sfu(String),

    #[error("`[media_guard]`: {0}")]
    MediaGuard(String),

    #[error(
        "`[[models]]` is no longer supported: define each model in `models/<name>.json` \
         (`aether --sync-models` assigns the ids)"
    )]
    LegacyModels,
}

impl PluginManifest {
    /// Settings a plugin declares: keys under its own name, unique, in a group that exists here
    /// or (checked when the plugin is loaded) in the catalog already.
    pub fn validate_settings(&self) -> Result<(), String> {
        let plugin = self.plugin.name.as_str();
        let mut groups = std::collections::BTreeSet::new();
        for group in &self.setting_group {
            if !plain_name(&group.id, |c| {
                c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-'
            }) {
                return Err(format!(
                    "group id `{}`: use 1 to 64 lower-case letters, digits, `_` or `-`",
                    group.id
                ));
            }
            if group.label.trim().is_empty() {
                return Err(format!("group `{}` needs a label", group.id));
            }
            if !groups.insert(group.id.as_str()) {
                return Err(format!("two groups are named `{}`", group.id));
            }
        }
        let mut keys = std::collections::BTreeSet::new();
        for setting in &self.setting {
            let prefix = format!("{plugin}.");
            let rest = setting.key.strip_prefix(prefix.as_str()).unwrap_or("");
            if rest.is_empty()
                || setting.key.len() > 80
                || !rest.chars().all(|c| {
                    c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '.' || c == '-'
                })
            {
                return Err(format!(
                    "key `{}`: start with `{prefix}` and use lower-case letters, digits, `_`, `-` or `.` after it (at most 80 characters)",
                    setting.key
                ));
            }
            if setting.label.trim().is_empty() {
                return Err(format!("setting `{}` needs a label", setting.key));
            }
            if setting.group.trim().is_empty() {
                return Err(format!("setting `{}` needs a group", setting.key));
            }
            if !setting.value.is_null()
                && !(setting.value.is_string()
                    || setting.value.is_number()
                    || setting.value.is_boolean())
            {
                return Err(format!(
                    "setting `{}`: the value is text, a number or true/false",
                    setting.key
                ));
            }
            if !keys.insert(setting.key.as_str()) {
                return Err(format!("the key `{}` is declared twice", setting.key));
            }
        }
        Ok(())
    }

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
        if manifest
            .plugin
            .public_capabilities
            .iter()
            .any(|c| c == REMOVED)
        {
            return Err(ManifestError::RawSurql("public_capabilities"));
        }
        let mut seen = std::collections::HashSet::new();
        for task in &manifest.schedule {
            task.validate()
                .map_err(|error| ManifestError::Schedule(error.to_string()))?;
            if !seen.insert(task.name.clone()) {
                return Err(ManifestError::Schedule(format!(
                    "two tasks are named `{}`",
                    task.name
                )));
            }
        }
        let mut names = std::collections::HashSet::new();
        let mut demo_models = std::collections::HashSet::new();
        for command in &manifest.command {
            command.validate().map_err(ManifestError::Command)?;
            if !names.insert(command.name.clone()) {
                return Err(ManifestError::Command(format!(
                    "two commands are named `{}`",
                    command.name
                )));
            }
            if let Some(model) = &command.demo_model {
                if !demo_models.insert(model.clone()) {
                    return Err(ManifestError::Command(format!(
                        "two commands claim demo_model `{model}`"
                    )));
                }
            }
        }
        let mut watches = std::collections::HashSet::new();
        for watch in &manifest.watch {
            watch.validate().map_err(ManifestError::Watch)?;
            if !watches.insert(watch.name.clone()) {
                return Err(ManifestError::Watch(format!(
                    "two watches are named `{}`",
                    watch.name
                )));
            }
        }
        let mut roles = std::collections::HashSet::new();
        manifest
            .validate_settings()
            .map_err(ManifestError::Setting)?;
        for role in &manifest.roles {
            role.validate().map_err(ManifestError::Role)?;
            if !roles.insert(role.name.clone()) {
                return Err(ManifestError::Role(format!(
                    "two roles are named `{}`",
                    role.name
                )));
            }
        }
        let mut exposed = std::collections::HashSet::new();
        for api in &manifest.api {
            if api.model.trim().is_empty() {
                return Err(ManifestError::Api("name the model".into()));
            }
            if !exposed.insert(api.model.clone()) {
                return Err(ManifestError::Api(format!(
                    "`{}` is listed twice",
                    api.model
                )));
            }
            if api.operations.is_empty() {
                return Err(ManifestError::Api(format!(
                    "`{}` offers no operation (list, get, create, update, delete)",
                    api.model
                )));
            }
            if let Some(bad) = api
                .operations
                .iter()
                .find(|o| !API_OPERATIONS.contains(&o.as_str()))
            {
                return Err(ManifestError::Api(format!(
                    "`{}`: `{bad}` is not an operation (use list, get, create, update or delete)",
                    api.model
                )));
            }
        }
        let mut hooks = std::collections::HashSet::new();
        for hook in &manifest.inbound {
            hook.validate().map_err(ManifestError::Inbound)?;
            if !hooks.insert(hook.name.clone()) {
                return Err(ManifestError::Inbound(format!(
                    "two hooks are named `{}`",
                    hook.name
                )));
            }
        }
        let mut aliases = std::collections::HashSet::new();
        for route in &manifest.mail_routes {
            route.validate().map_err(ManifestError::MailRoute)?;
            if !aliases.insert(route.alias.clone()) {
                return Err(ManifestError::MailRoute(format!(
                    "two routes use the alias `{}`",
                    route.alias
                )));
            }
        }
        if let Some(guard) = &manifest.media_guard
            && (guard.function.is_empty()
                || !guard
                    .function
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_'))
        {
            return Err(ManifestError::MediaGuard(
                "`function` names one of the plugin's functions".into(),
            ));
        }
        let mut topics = std::collections::HashSet::new();
        for room in &manifest.sfu {
            room.validate().map_err(ManifestError::Sfu)?;
            if manifest
                .sfu
                .iter()
                .filter(|other| other.room == room.room)
                .count()
                > 1
            {
                return Err(ManifestError::Sfu(format!(
                    "`{}` is declared twice",
                    room.room
                )));
            }
        }
        for topic in &manifest.realtime {
            topic.validate().map_err(ManifestError::Realtime)?;
            if !topics.insert(topic.topic.clone()) {
                return Err(ManifestError::Realtime(format!(
                    "`{}` is declared twice",
                    topic.topic
                )));
            }
        }
        let mut viewers = std::collections::HashSet::new();
        for viewer in &manifest.viewer {
            viewer.validate().map_err(ManifestError::Viewer)?;
            if !viewers.insert(viewer.name.clone()) {
                return Err(ManifestError::Viewer(format!(
                    "two viewers are named `{}`",
                    viewer.name
                )));
            }
        }
        for (packs, wrap) in [
            (
                &manifest.emoji_pack,
                ManifestError::EmojiPack as fn(String) -> ManifestError,
            ),
            (&manifest.gif_pack, ManifestError::GifPack),
        ] {
            let mut names = std::collections::HashSet::new();
            for pack in packs {
                pack.validate().map_err(wrap)?;
                if !names.insert(pack.name.clone()) {
                    return Err(wrap(format!("two packs are named `{}`", pack.name)));
                }
            }
        }
        let mut tools = std::collections::HashSet::new();
        for tool in &manifest.ai_tool {
            tool.validate(&manifest.plugin.name)
                .map_err(ManifestError::AiTool)?;
            if !tools.insert(tool.name.clone()) {
                return Err(ManifestError::AiTool(format!(
                    "two tools are named `{}`",
                    tool.name
                )));
            }
        }
        if let Some(unknown) = manifest
            .plugin
            .bridges
            .iter()
            .find(|name| crate::bridges::find(name).is_none())
        {
            return Err(ManifestError::Bridge(format!(
                "`{unknown}` is not a bridge plugins can call; use some of: {}",
                crate::bridges::names()
            )));
        }
        let mut events = std::collections::HashSet::new();
        for event in &manifest.events {
            event
                .validate(&manifest.plugin)
                .map_err(ManifestError::Event)?;
            if !events.insert((event.direction().to_string(), event.name.clone())) {
                return Err(ManifestError::Event(format!(
                    "`{}` is declared twice",
                    event.name
                )));
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
        assert_eq!(
            (command.name.as_str(), command.function.as_str()),
            ("import_rates", "import")
        );
        assert_eq!(command.args[0].kind(), "string");
        assert_eq!(command.args[1].kind(), "int");
        assert!(command.args[2].required);
        Ok(())
    }

    #[test]
    fn api_blocks_offer_models_read_only_unless_more_is_asked() -> Result<(), ManifestError> {
        let manifest = parse(
            "[[api]]\nmodel = \"lead\"\n\
             [[api]]\nmodel = \"deal\"\noperations = [\"list\", \"get\", \"create\", \"update\", \"delete\"]\n",
        )?;
        let lead = &manifest.api[0];
        assert!(lead.allows("list") && lead.allows("get"));
        assert!(!lead.allows("create") && !lead.allows("update") && !lead.allows("delete"));
        let deal = &manifest.api[1];
        assert!(API_OPERATIONS.iter().all(|op| deal.allows(op)));
        // Without any `[[api]]` nothing is offered.
        assert!(parse("")?.api.is_empty());
        for (label, text) in [
            ("no model", "[[api]]\nmodel = \" \"\n"),
            ("twice", "[[api]]\nmodel = \"a\"\n[[api]]\nmodel = \"a\"\n"),
            ("no operations", "[[api]]\nmodel = \"a\"\noperations = []\n"),
            (
                "unknown operation",
                "[[api]]\nmodel = \"a\"\noperations = [\"list\", \"drop\"]\n",
            ),
        ] {
            assert!(matches!(parse(text), Err(ManifestError::Api(_))), "{label}");
        }
        Ok(())
    }

    #[test]
    fn inbound_hooks_name_who_may_call_and_how_they_prove_it() -> Result<(), ManifestError> {
        let manifest = parse(
            "[[inbound]]\nname = \"signed\"\nfunction = \"on_signed\"\n\
             [[inbound]]\nname = \"pay\"\nfunction = \"on_pay\"\nverify = \"hmac-sha256\"\nheader = \"X-Signature\"\n\
             [[inbound]]\nname = \"key\"\nfunction = \"on_key\"\nverify = \"token\"\nheader = \"x-api-key\"\n",
        )?;
        assert_eq!(manifest.inbound[0].verify, "aether", "the default");
        assert_eq!(manifest.inbound[0].header_name(), "x-aether-signature");
        assert_eq!(
            manifest.inbound[1].header_name(),
            "x-signature",
            "lower-cased"
        );
        for (label, text) in [
            (
                "a bad name",
                "[[inbound]]\nname = \"Bad Name\"\nfunction = \"f\"\n",
            ),
            (
                "no function",
                "[[inbound]]\nname = \"a\"\nfunction = \"\"\n",
            ),
            (
                "unknown verify",
                "[[inbound]]\nname = \"a\"\nfunction = \"f\"\nverify = \"none\"\n",
            ),
            (
                "hmac without a header",
                "[[inbound]]\nname = \"a\"\nfunction = \"f\"\nverify = \"hmac-sha256\"\n",
            ),
            (
                "token without a header",
                "[[inbound]]\nname = \"a\"\nfunction = \"f\"\nverify = \"token\"\n",
            ),
            (
                "a header that cannot carry it",
                "[[inbound]]\nname = \"a\"\nfunction = \"f\"\nverify = \"token\"\nheader = \"cookie\"\n",
            ),
            (
                "a tolerance for hmac",
                "[[inbound]]\nname = \"a\"\nfunction = \"f\"\nverify = \"hmac-sha256\"\nheader = \"x-s\"\ntolerance_secs = 5\n",
            ),
            (
                "twice",
                "[[inbound]]\nname = \"a\"\nfunction = \"f\"\n[[inbound]]\nname = \"a\"\nfunction = \"g\"\n",
            ),
        ] {
            assert!(
                matches!(parse(text), Err(ManifestError::Inbound(_))),
                "{label}"
            );
        }
        Ok(())
    }

    #[test]
    fn a_media_guard_names_a_function() -> Result<(), ManifestError> {
        let manifest = parse("[media_guard]\nfunction = \"can_read_media\"\n")?;
        assert_eq!(
            manifest.media_guard.map(|g| g.function).as_deref(),
            Some("can_read_media")
        );
        assert!(parse("").is_ok_and(|m| m.media_guard.is_none()));
        assert!(matches!(
            parse("[media_guard]\nfunction = \"\"\n"),
            Err(ManifestError::MediaGuard(_))
        ));
        assert!(matches!(
            parse("[media_guard]\nfunction = \"a b\"\n"),
            Err(ManifestError::MediaGuard(_))
        ));
        Ok(())
    }

    #[test]
    fn sfu_rooms_are_read_matched_and_checked() -> Result<(), ManifestError> {
        let manifest = parse(
            "[[sfu]]\nroom = \"meeting.{meeting}\"\njoin = \"can_join_meeting\"\nmax_participants = 20\n",
        )?;
        let room = &manifest.sfu[0];
        assert_eq!(room.max_participants, Some(20));
        let params = room
            .match_room("meeting.7")
            .ok_or(ManifestError::Sfu("no match".into()))?;
        assert_eq!(params.get("meeting").and_then(|v| v.as_str()), Some("7"));
        assert!(room.match_room("meeting").is_none() && room.match_room("other.7").is_none());
        for (label, text) in [
            (
                "a bad segment",
                "[[sfu]]\nroom = \"Meet.{a}\"\njoin = \"f\"\n",
            ),
            ("no function", "[[sfu]]\nroom = \"a\"\njoin = \"\"\n"),
            (
                "no people",
                "[[sfu]]\nroom = \"a\"\njoin = \"f\"\nmax_participants = 0\n",
            ),
            (
                "too many people",
                "[[sfu]]\nroom = \"a\"\njoin = \"f\"\nmax_participants = 5000\n",
            ),
            (
                "twice",
                "[[sfu]]\nroom = \"a\"\njoin = \"f\"\n[[sfu]]\nroom = \"a\"\njoin = \"g\"\n",
            ),
        ] {
            assert!(matches!(parse(text), Err(ManifestError::Sfu(_))), "{label}");
        }
        Ok(())
    }

    #[test]
    fn realtime_topics_are_read_matched_and_checked() -> Result<(), ManifestError> {
        let manifest = parse(
            "[[realtime]]\ntopic = \"channel.{channel}\"\nsubscribe = \"can_join\"\npresence = true\nclient_events = [\"typing\"]\n\
             [[realtime]]\ntopic = \"inbox\"\nsubscribe = \"can_inbox\"\nhistory = 0\n",
        )?;
        let channel = &manifest.realtime[0];
        assert_eq!(channel.history, 100, "the default");
        let params = channel
            .match_topic("channel.7")
            .ok_or(ManifestError::Realtime("no match".into()))?;
        assert_eq!(params["channel"], "7");
        assert!(channel.match_topic("channel").is_none());
        assert!(channel.match_topic("channel.7.x").is_none());
        assert!(channel.match_topic("room.7").is_none());
        assert!(
            manifest.realtime[1]
                .match_topic("inbox")
                .is_some_and(|p| p.is_empty())
        );
        for (label, text) in [
            (
                "a bad segment",
                "[[realtime]]\ntopic = \"Chan.{a}\"\nsubscribe = \"f\"\n",
            ),
            (
                "an empty segment",
                "[[realtime]]\ntopic = \"a..b\"\nsubscribe = \"f\"\n",
            ),
            (
                "no function",
                "[[realtime]]\ntopic = \"a\"\nsubscribe = \"\"\n",
            ),
            (
                "too much history",
                "[[realtime]]\ntopic = \"a\"\nsubscribe = \"f\"\nhistory = 5000\n",
            ),
            (
                "a bad client event",
                "[[realtime]]\ntopic = \"a\"\nsubscribe = \"f\"\nclient_events = [\"Not Ok\"]\n",
            ),
            (
                "twice",
                "[[realtime]]\ntopic = \"a\"\nsubscribe = \"f\"\n[[realtime]]\ntopic = \"a\"\nsubscribe = \"g\"\n",
            ),
        ] {
            assert!(
                matches!(parse(text), Err(ManifestError::Realtime(_))),
                "{label}"
            );
        }
        Ok(())
    }

    #[test]
    fn viewers_are_read_with_defaults_and_checked() -> Result<(), ManifestError> {
        let manifest = parse(
            "[[viewer]]\nname = \"dwg\"\nmimes = [\"image/vnd.dwg\", \"model/*\"]\nextensions = [\"dwg\"]\nmodule = \"viewers/dwg.js\"\n\
             hosts = [\"https://tiles.example.com\"]\n",
        )?;
        assert_eq!(manifest.viewer[0].priority, 50, "the default");
        assert_eq!(manifest.viewer[0].label, None);
        for (label, text) in [
            (
                "a bad name",
                "[[viewer]]\nname = \"Bad\"\nextensions = [\"a\"]\nmodule = \"v.js\"\n",
            ),
            (
                "opens nothing",
                "[[viewer]]\nname = \"a\"\nmodule = \"v.js\"\n",
            ),
            (
                "a bad type",
                "[[viewer]]\nname = \"a\"\nmimes = [\"png\"]\nmodule = \"v.js\"\n",
            ),
            (
                "a dotted extension",
                "[[viewer]]\nname = \"a\"\nextensions = [\".dwg\"]\nmodule = \"v.js\"\n",
            ),
            (
                "a module outside assets",
                "[[viewer]]\nname = \"a\"\nextensions = [\"a\"]\nmodule = \"../v.js\"\n",
            ),
            (
                "a module that is not js",
                "[[viewer]]\nname = \"a\"\nextensions = [\"a\"]\nmodule = \"v.html\"\n",
            ),
            (
                "a priority out of range",
                "[[viewer]]\nname = \"a\"\nextensions = [\"a\"]\nmodule = \"v.js\"\npriority = 5000\n",
            ),
            (
                "a plain http host",
                "[[viewer]]\nname = \"a\"\nextensions = [\"a\"]\nmodule = \"v.js\"\nhosts = [\"http://x.com\"]\n",
            ),
            (
                "twice",
                "[[viewer]]\nname = \"a\"\nextensions = [\"a\"]\nmodule = \"v.js\"\n[[viewer]]\nname = \"a\"\nextensions = [\"b\"]\nmodule = \"w.js\"\n",
            ),
        ] {
            assert!(
                matches!(parse(text), Err(ManifestError::Viewer(_))),
                "{label}"
            );
        }
        Ok(())
    }

    #[test]
    fn emoji_packs_are_read_and_checked() -> Result<(), ManifestError> {
        let manifest = parse(
            "[[emoji_pack]]\nname = \"emojibase\"\nlabel = \"Emoji\"\nfile = \"emoji/pack.json\"\n",
        )?;
        assert_eq!(manifest.emoji_pack[0].file, "emoji/pack.json");
        assert_eq!(manifest.emoji_pack[0].label.as_deref(), Some("Emoji"));
        for (label, text) in [
            (
                "a bad name",
                "[[emoji_pack]]\nname = \"Bad\"\nfile = \"p.json\"\n",
            ),
            (
                "a file outside assets",
                "[[emoji_pack]]\nname = \"a\"\nfile = \"../p.json\"\n",
            ),
            (
                "an absolute file",
                "[[emoji_pack]]\nname = \"a\"\nfile = \"/p.json\"\n",
            ),
            (
                "a file that is not json",
                "[[emoji_pack]]\nname = \"a\"\nfile = \"p.js\"\n",
            ),
            (
                "twice",
                "[[emoji_pack]]\nname = \"a\"\nfile = \"p.json\"\n[[emoji_pack]]\nname = \"a\"\nfile = \"q.json\"\n",
            ),
        ] {
            assert!(
                matches!(parse(text), Err(ManifestError::EmojiPack(_))),
                "{label}"
            );
        }
        Ok(())
    }

    #[test]
    fn gif_packs_are_read_and_checked_apart_from_emoji_packs() -> Result<(), ManifestError> {
        let manifest = parse(
            "[[emoji_pack]]\nname = \"a\"\nfile = \"e.json\"\n[[gif_pack]]\nname = \"a\"\nlabel = \"Animated\"\nfile = \"gif/pack.json\"\n",
        )?;
        assert_eq!(
            manifest.gif_pack[0].file, "gif/pack.json",
            "the same name in two registries is fine"
        );
        assert_eq!(manifest.emoji_pack.len(), 1);
        for (label, text) in [
            (
                "a bad name",
                "[[gif_pack]]\nname = \"Bad\"\nfile = \"p.json\"\n",
            ),
            (
                "a file outside assets",
                "[[gif_pack]]\nname = \"a\"\nfile = \"../p.json\"\n",
            ),
            (
                "a file that is not json",
                "[[gif_pack]]\nname = \"a\"\nfile = \"p.gif\"\n",
            ),
            (
                "twice",
                "[[gif_pack]]\nname = \"a\"\nfile = \"p.json\"\n[[gif_pack]]\nname = \"a\"\nfile = \"q.json\"\n",
            ),
        ] {
            assert!(
                matches!(parse(text), Err(ManifestError::GifPack(_))),
                "{label}"
            );
        }
        Ok(())
    }

    #[test]
    fn ai_tools_are_read_with_defaults_and_checked() -> Result<(), ManifestError> {
        let manifest = parse(
            "[[ai_tool]]\nname = \"balance\"\ndescription = \"Days left\"\nfunction = \"f\"\n\
             schema = { type = \"object\", properties = { year = { type = \"integer\" } } }\n\
             [[ai_tool]]\nname = \"apply\"\ndescription = \"Ask\"\nfunction = \"g\"\nwrites = true\n",
        )?;
        assert!(!manifest.ai_tool[0].writes && manifest.ai_tool[1].writes);
        assert_eq!(
            manifest.ai_tool[0].schema["properties"]["year"]["type"],
            "integer"
        );
        // Left out, a tool takes no arguments.
        assert_eq!(
            manifest.ai_tool[1].schema,
            serde_json::json!({ "type": "object", "properties": {} })
        );
        for (label, text) in [
            (
                "name",
                "[[ai_tool]]\nname = \"Bad Name\"\ndescription = \"d\"\nfunction = \"f\"\n",
            ),
            (
                "description",
                "[[ai_tool]]\nname = \"t\"\ndescription = \" \"\nfunction = \"f\"\n",
            ),
            (
                "function",
                "[[ai_tool]]\nname = \"t\"\ndescription = \"d\"\nfunction = \"a-b\"\n",
            ),
            (
                "schema",
                "[[ai_tool]]\nname = \"t\"\ndescription = \"d\"\nfunction = \"f\"\nschema = { type = \"string\" }\n",
            ),
            (
                "twice",
                "[[ai_tool]]\nname = \"t\"\ndescription = \"d\"\nfunction = \"f\"\n[[ai_tool]]\nname = \"t\"\ndescription = \"d\"\nfunction = \"g\"\n",
            ),
        ] {
            assert!(
                matches!(parse(text), Err(ManifestError::AiTool(_))),
                "{label}"
            );
        }
        Ok(())
    }

    #[test]
    fn watches_are_read_with_defaults_and_checked() -> Result<(), ManifestError> {
        let manifest = parse(
            "[[watch]]\nname = \"invoices\"\npath = \"inbox\"\nfunction = \"import\"\npattern = \"*.csv\"\n",
        )?;
        let watch = &manifest.watch[0];
        assert_eq!(
            (watch.events.as_slice(), watch.debounce_ms, watch.recursive),
            (
                ["created".to_string(), "modified".to_string()].as_slice(),
                500,
                false
            )
        );
        for (label, text) in [
            ("name", "[[watch]]\nname = \"Bad\"\nfunction = \"f\"\n"),
            ("function", "[[watch]]\nname = \"w\"\nfunction = \"\"\n"),
            (
                "escape",
                "[[watch]]\nname = \"w\"\nfunction = \"f\"\npath = \"../elsewhere\"\n",
            ),
            (
                "absolute",
                "[[watch]]\nname = \"w\"\nfunction = \"f\"\npath = \"/etc\"\n",
            ),
            (
                "event",
                "[[watch]]\nname = \"w\"\nfunction = \"f\"\nevents = [\"opened\"]\n",
            ),
            (
                "no events",
                "[[watch]]\nname = \"w\"\nfunction = \"f\"\nevents = []\n",
            ),
            (
                "glob",
                "[[watch]]\nname = \"w\"\nfunction = \"f\"\npattern = \"[\"\n",
            ),
            (
                "debounce",
                "[[watch]]\nname = \"w\"\nfunction = \"f\"\ndebounce_ms = 5\n",
            ),
            (
                "poll",
                "[[watch]]\nname = \"w\"\nfunction = \"f\"\npoll_secs = 0\n",
            ),
            (
                "duplicate",
                "[[watch]]\nname = \"w\"\nfunction = \"f\"\n[[watch]]\nname = \"w\"\nfunction = \"g\"\n",
            ),
        ] {
            assert!(
                matches!(parse(text), Err(ManifestError::Watch(_))),
                "{label}"
            );
        }
        Ok(())
    }

    #[test]
    fn events_are_checked_against_the_plugins_dependencies_and_capabilities()
    -> Result<(), ManifestError> {
        let with = |extra: &str| {
            PluginManifest::parse(&format!(
                "[plugin]\nname = \"crm\"\nversion = \"1\"\ndependencies = [\"chat\"]\ncapabilities = [\"events::emit\", \"events::subscribe\"]\n{extra}"
            ))
        };
        let ok = with(
            "[[events]]\nname = \"lead_won\"\n[[events]]\nname = \"chat.message_posted\"\ndirection = \"listen\"\nhandler = \"on_message\"\n",
        )?;
        assert_eq!(ok.events[0].direction(), "emit");
        assert_eq!(ok.events[1].direction(), "listen");
        for (label, text) in [
            ("dot in an emitted name", "[[events]]\nname = \"a.b\"\n"),
            (
                "listen without a plugin part",
                "[[events]]\nname = \"posted\"\ndirection = \"listen\"\nhandler = \"h\"\n",
            ),
            (
                "not a dependency",
                "[[events]]\nname = \"billing.paid\"\ndirection = \"listen\"\nhandler = \"h\"\n",
            ),
            (
                "no handler",
                "[[events]]\nname = \"chat.x\"\ndirection = \"listen\"\n",
            ),
            (
                "direction",
                "[[events]]\nname = \"x\"\ndirection = \"both\"\n",
            ),
            (
                "duplicate",
                "[[events]]\nname = \"x\"\n[[events]]\nname = \"x\"\n",
            ),
        ] {
            assert!(
                matches!(with(text), Err(ManifestError::Event(_))),
                "{label}"
            );
        }
        // Without the capabilities, declaring either direction is refused.
        let bare = |extra: &str| {
            PluginManifest::parse(&format!(
                "[plugin]\nname = \"p\"\nversion = \"1\"\ndependencies = [\"chat\"]\n{extra}"
            ))
        };
        assert!(matches!(
            bare("[[events]]\nname = \"x\"\n"),
            Err(ManifestError::Event(_))
        ));
        assert!(matches!(
            bare("[[events]]\nname = \"chat.x\"\ndirection = \"listen\"\nhandler = \"h\"\n"),
            Err(ManifestError::Event(_))
        ));
        // A plugin may listen to its own events.
        let own = PluginManifest::parse(
            "[plugin]\nname = \"p\"\nversion = \"1\"\ncapabilities = [\"events::subscribe\"]\n[[events]]\nname = \"p.x\"\ndirection = \"listen\"\nhandler = \"h\"\n",
        )?;
        assert_eq!(own.events.len(), 1);
        Ok(())
    }

    #[test]
    fn a_bad_command_refuses_the_plugin_with_a_reason() {
        for (label, text) in [
            (
                "name",
                "[[command]]\nname = \"Bad Name\"\nfunction = \"f\"\n",
            ),
            ("function", "[[command]]\nname = \"x\"\nfunction = \"\"\n"),
            (
                "type",
                "[[command]]\nname = \"x\"\nfunction = \"f\"\nargs = [{ name = \"a\", type = \"date\" }]\n",
            ),
            (
                "duplicate arg",
                "[[command]]\nname = \"x\"\nfunction = \"f\"\nargs = [{ name = \"a\" }, { name = \"a\" }]\n",
            ),
            (
                "required default",
                "[[command]]\nname = \"x\"\nfunction = \"f\"\nargs = [{ name = \"a\", required = true, default = \"v\" }]\n",
            ),
            (
                "default type",
                "[[command]]\nname = \"x\"\nfunction = \"f\"\nargs = [{ name = \"a\", type = \"int\", default = \"v\" }]\n",
            ),
            (
                "duplicate",
                "[[command]]\nname = \"x\"\nfunction = \"f\"\n[[command]]\nname = \"x\"\nfunction = \"g\"\n",
            ),
            (
                "demo_model case",
                "[[command]]\nname = \"demo\"\nfunction = \"f\"\ndemo_model = \"Bad\"\n",
            ),
            (
                "demo with install",
                "[[command]]\nname = \"demo\"\nfunction = \"f\"\ndemo_model = \"currency\"\non_install = true\n",
            ),
            (
                "demo count type",
                "[[command]]\nname = \"demo\"\nfunction = \"f\"\ndemo_model = \"currency\"\nargs = [{ name = \"count\", type = \"string\" }]\n",
            ),
            (
                "duplicate demo_model",
                "[[command]]\nname = \"demo_a\"\nfunction = \"f\"\ndemo_model = \"currency\"\n\
                 [[command]]\nname = \"demo_b\"\nfunction = \"g\"\ndemo_model = \"currency\"\n",
            ),
        ] {
            assert!(
                matches!(parse(text), Err(ManifestError::Command(_))),
                "{label}"
            );
        }
    }

    #[test]
    fn demo_model_commands_are_read() -> Result<(), ManifestError> {
        let manifest = parse(
            "[[command]]\nname = \"demo_currency\"\nfunction = \"generate_demo_currency\"\n\
             demo_model = \"currency\"\nargs = [{ name = \"count\", type = \"int\", default = 10 }]\n",
        )?;
        let command = &manifest.command[0];
        assert_eq!(command.demo_model.as_deref(), Some("currency"));
        assert!(!command.on_install);
        assert_eq!(command.args[0].kind(), "int");
        Ok(())
    }
}

#[cfg(test)]
mod setting_tests {
    use super::*;

    fn parse(extra: &str) -> Result<PluginManifest, ManifestError> {
        PluginManifest::parse(&format!(
            "[plugin]\nname = \"country\"\nversion = \"1\"\n{extra}"
        ))
    }

    const GOOD: &str = "[[setting_group]]\nid = \"country\"\nlabel = \"Country\"\nicon = \"globe\"\n\
        [[setting]]\ngroup = \"country\"\nkey = \"country.organization\"\nlabel = \"Country\"\nvalue = \"\"\n";

    #[test]
    fn declared_settings_are_read() -> Result<(), ManifestError> {
        let manifest = parse(GOOD)?;
        assert_eq!(manifest.setting_group[0].id, "country");
        assert_eq!(manifest.setting[0].key, "country.organization");
        assert_eq!(manifest.setting[0].row_id(), "country_organization");
        Ok(())
    }

    #[test]
    fn a_key_must_start_with_the_plugin_name() {
        let wrong = GOOD.replace("country.organization", "payroll.country");
        assert!(
            matches!(parse(&wrong), Err(ManifestError::Setting(message)) if message.contains("start with `country.`"))
        );
        let bare = GOOD.replace("country.organization", "country.");
        assert!(parse(&bare).is_err());
    }

    #[test]
    fn duplicates_and_odd_values_are_refused() {
        let twice = format!(
            "{GOOD}[[setting]]\ngroup = \"country\"\nkey = \"country.organization\"\nlabel = \"Again\"\n"
        );
        assert!(
            matches!(parse(&twice), Err(ManifestError::Setting(message)) if message.contains("twice"))
        );
        let table = GOOD.replace("value = \"\"", "value = [1, 2]");
        assert!(parse(&table).is_err());
    }
}
