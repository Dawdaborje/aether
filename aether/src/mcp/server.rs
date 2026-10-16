//! The Model Context Protocol adapter: tools, resources and prompts over the functions in `tools.rs`.

use super::live::{Live, READ_TOOLS, WRITE_TOOLS};
use super::tools;
use crate::server::docs::{DocsSources, gather};
use aether_docs::plugin_docs::PluginDocs;
use aether_docs::{Status, bundle, doc};
use rmcp::{
    ErrorData, RoleServer, ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        GetPromptRequestParams, GetPromptResponse, GetPromptResult, ListPromptsResult, ListResourcesResult,
        PaginatedRequestParams, Prompt, PromptArgument, PromptMessage, ReadResourceRequestParams,
        ReadResourceResponse, ReadResourceResult, Resource, ResourceContents, Role, ServerCapabilities,
        ServerConfig,
    },
    schemars::{self, JsonSchema},
    service::RequestContext,
    tool, tool_handler, tool_router,
};
use serde::Deserialize;

const URI_PREFIX: &str = "aether-docs://";

const INSTRUCTIONS: &str = "Aether documentation and plugin tools. Aether is a plugin platform: a Rust kernel runs sandboxed \
plugins (WebAssembly, Lua, Rhai) over a multi-tenant database. Use these tools to learn it and to build plugins.\n\
\n\
Start with `get_reference` (kind `manifest`) or `search_docs`. Read the documentation instead of guessing: file formats and \
kernel commands are specific to Aether. Pages marked PROPOSAL describe designs that are NOT built; never write plugins \
that depend on them. To build a plugin: `scaffold_plugin`, write the files, then `validate_plugin` until it says VALID. \
Every page is also a resource at aether-docs://<path>.";

#[derive(Clone)]
pub struct AetherMcp {
    tool_router: ToolRouter<Self>,
    sources: std::sync::Arc<DocsSources>,
    live: Option<std::sync::Arc<Live>>,
    plugins: std::sync::Arc<tokio::sync::Mutex<Option<(std::time::Instant, std::sync::Arc<Vec<PluginDocs>>)>>>,
}

impl AetherMcp {
    /// `live` is set when the server was started with `--org`: its tools work on a real instance. Tools that do not
    /// apply are not offered at all, so the model does not try them.
    pub fn new(sources: DocsSources, live: Option<Live>) -> Self {
        let mut tool_router = Self::tool_router();
        match &live {
            None => READ_TOOLS.iter().chain(WRITE_TOOLS).for_each(|name| tool_router.remove_route(name)),
            Some(l) if !l.allow_write => WRITE_TOOLS.iter().for_each(|name| tool_router.remove_route(name)),
            Some(_) => {}
        }
        Self { tool_router, sources: std::sync::Arc::new(sources), live: live.map(std::sync::Arc::new), plugins: Default::default() }
    }

    fn live(&self) -> Result<&Live, String> {
        self.live.as_deref().ok_or_else(|| "Live mode is off: start the server with -c aether.toml --org <organization>.".to_string())
    }

    /// The docs plugins ship, read from the plugin folders; reread every few seconds so an author's edits show.
    /// This process is run by its owner, so docs of every visibility are included.
    async fn plugin_docs(&self) -> std::sync::Arc<Vec<PluginDocs>> {
        if self.sources.entries.is_empty() {
            return Default::default();
        }
        let mut cached = self.plugins.lock().await;
        if let Some((at, docs)) = cached.as_ref()
            && at.elapsed() < std::time::Duration::from_secs(5)
        {
            return docs.clone();
        }
        let fresh = std::sync::Arc::new(gather(&self.sources).await);
        *cached = Some((std::time::Instant::now(), fresh.clone()));
        fresh
    }
}

#[derive(Deserialize, JsonSchema)]
struct SearchArgs {
    /// Words to look for, such as `db::find filter` or `model field types`.
    query: String,
    /// Limit to one section: guide, plugin, architecture or capabilities.
    section: Option<String>,
    /// Also search pages that describe designs that are not built yet (default false).
    include_proposals: Option<bool>,
    /// How many results (default 8, at most 25).
    limit: Option<usize>,
    /// Search only the docs a plugin ships (its name, as `list_docs` shows). Without it Aether's pages and every plugin's are searched.
    plugin: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
struct GetDocArgs {
    /// Page path as shown by `list_docs` or `search_docs`, without `.md` (for example `plugin/pages`).
    path: String,
    /// Only the part of the page under this heading: a section id from `search_docs`, or words of the heading.
    section: Option<String>,
    /// Read a page of the docs a plugin ships (its name), instead of one of Aether's own pages.
    plugin: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
struct ListDocsArgs {
    /// Limit to one section: guide, plugin, architecture or capabilities.
    section: Option<String>,
    /// List the pages of the docs a plugin ships (its name).
    plugin: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
struct CapabilityArgs {
    /// A group such as `db`, or one capability such as `db::query`.
    code: String,
}

#[derive(Deserialize, JsonSchema)]
struct ReferenceArgs {
    /// manifest, model, pages, rules, commands, cli-commands, lua, rhai, events, scheduler or capabilities.
    kind: String,
}

#[derive(Deserialize, JsonSchema)]
struct SchemaArgs {
    /// manifest, workspace, model or rules.
    kind: String,
}

#[derive(Deserialize, JsonSchema)]
struct ModelsArgs {
    /// Only this plugin's models (its name).
    plugin: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
struct LivePathArgs {
    /// A plugin folder (the one with plugin.toml) or a workspace folder: absolute, or relative to where this server was started.
    path: String,
}

#[derive(Deserialize, JsonSchema)]
struct PluginVersionArgs {
    /// The plugin's name, as `list_catalog_plugins` shows it. For upgrades, `all` upgrades every installed plugin.
    plugin: String,
    /// A catalog version. Without it: the newest.
    version: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
struct RunCommandArgs {
    /// `<plugin>.<command>`, as `list_plugin_commands` shows it.
    command: String,
    /// The command's arguments as a JSON object, using the names `list_plugin_commands` shows.
    arguments: Option<serde_json::Value>,
}

#[derive(Deserialize, JsonSchema)]
struct ScaffoldArgs {
    /// Plugin name: lower-case letters, digits and `_`.
    name: String,
    /// go, rust, typescript, javascript, python, rhai or lua. Rhai and Lua are scripts with nothing to compile.
    language: String,
    /// Display name, for example `Expense tracker`.
    label: Option<String>,
    /// One sentence about what the plugin does.
    description: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
struct ValidateArgs {
    /// The plugin folder (the one containing plugin.toml): absolute, or relative to where this server was started.
    path: String,
}

#[tool_router]
impl AetherMcp {
    #[tool(description = "Search the Aether documentation, and the docs that plugins ship when plugin folders are configured. Returns matching sections with the get_doc call that opens each one. Hides designs that are not built unless include_proposals is set.")]
    async fn search_docs(&self, Parameters(a): Parameters<SearchArgs>) -> Result<String, String> {
        tools::search_docs(&a.query, a.section.as_deref(), a.include_proposals.unwrap_or(false), a.limit, a.plugin.as_deref(), &self.plugin_docs().await)
    }

    #[tool(description = "Read a documentation page, or one section of it (add plugin=<name> for a page of a plugin\'s own docs, which its author wrote). Pages marked PROPOSAL are designs that are not built.")]
    async fn get_doc(&self, Parameters(a): Parameters<GetDocArgs>) -> Result<String, String> {
        tools::get_doc(&a.path, a.section.as_deref(), a.plugin.as_deref(), &self.plugin_docs().await)
    }

    #[tool(description = "List every documentation page with a one-line description, optionally for one section; also lists the plugins that ship docs, and plugin=<name> lists theirs.")]
    async fn list_docs(&self, Parameters(a): Parameters<ListDocsArgs>) -> Result<String, String> {
        tools::list_docs(a.section.as_deref(), a.plugin.as_deref(), &self.plugin_docs().await)
    }

    #[tool(description = "List the capability groups a plugin can declare in plugin.toml (db, http, ai, ...).")]
    async fn list_capabilities(&self) -> String {
        tools::list_capabilities()
    }

    #[tool(description = "Describe a capability group (`db`) or one capability (`db::query`).")]
    async fn get_capability(&self, Parameters(a): Parameters<CapabilityArgs>) -> Result<String, String> {
        tools::get_capability(&a.code)
    }

    #[tool(description = "The complete reference page(s) for one file format or feature of plugins: manifest (plugin.toml, workspace.toml), model, pages, rules, commands, cli-commands, lua, rhai, events, scheduler, capabilities. Read the matching reference before writing that kind of file.")]
    async fn get_reference(&self, Parameters(a): Parameters<ReferenceArgs>) -> Result<String, String> {
        tools::get_reference(&a.kind)
    }

    #[tool(description = "Live mode. The plugins installed in the organization this server works on, with their versions.")]
    async fn list_installed_plugins(&self) -> Result<String, String> {
        self.live()?.list_installed_plugins().await
    }

    #[tool(description = "Live mode. The plugins in the instance's catalog (loaded, not necessarily installed) with their versions.")]
    async fn list_catalog_plugins(&self) -> Result<String, String> {
        self.live()?.list_catalog_plugins().await
    }

    #[tool(description = "Live mode. The models (record types) of the installed plugins, with their fields, as they are in this organization.")]
    async fn get_org_models(&self, Parameters(a): Parameters<ModelsArgs>) -> Result<String, String> {
        self.live()?.get_org_models(a.plugin.as_deref()).await
    }

    #[tool(description = "Live mode. The commands the installed plugins offer, with their arguments.")]
    async fn list_plugin_commands(&self) -> Result<String, String> {
        self.live()?.list_plugin_commands().await
    }

    #[tool(description = "Live mode. Check a plugin folder against the instance's catalog: the checks validate_plugin cannot make (links to other plugins' models, components from other plugins). Loads nothing.")]
    async fn check_plugin_against_catalog(&self, Parameters(a): Parameters<LivePathArgs>) -> Result<String, String> {
        self.live()?.check_plugin_against_catalog(&a.path).await
    }

    #[tool(description = "Live mode, writes. Register a plugin folder (or workspace) in the instance's catalog, as `aether --load-plugins`. It is not installed anywhere until install_plugin.")]
    async fn load_plugin(&self, Parameters(a): Parameters<LivePathArgs>) -> Result<String, String> {
        self.live()?.load_plugin(&a.path).await
    }

    #[tool(description = "Live mode, writes. Install a catalog plugin (with its dependencies) in the organization, as `aether --install-plugin`, and run its on_install seed commands.")]
    async fn install_plugin(&self, Parameters(a): Parameters<PluginVersionArgs>) -> Result<String, String> {
        self.live()?.install_plugin(&a.plugin, a.version.as_deref()).await
    }

    #[tool(description = "Live mode, writes. Move an installed plugin to another catalog version (the newest by default; `all` upgrades every plugin), as `aether --upgrade-plugin`, and run its on_upgrade seed commands.")]
    async fn upgrade_plugin(&self, Parameters(a): Parameters<PluginVersionArgs>) -> Result<String, String> {
        self.live()?.upgrade_plugin(&a.plugin, a.version.as_deref()).await
    }

    #[tool(description = "Live mode, writes. Run a plugin command in the organization, as `aether --command`. Commands can change data: read list_plugin_commands first.")]
    async fn run_command(&self, Parameters(a): Parameters<RunCommandArgs>) -> Result<String, String> {
        self.live()?.run_command(&a.command, a.arguments.as_ref()).await
    }

    #[tool(description = "The exact JSON Schema of a plugin file format (manifest = plugin.toml, workspace = workspace.toml, model = models/*.json, rules = rules/*.json), generated from the types the kernel loads it with. Use it to write or check a file field by field; get_reference has the explanations.")]
    async fn get_schema(&self, Parameters(a): Parameters<SchemaArgs>) -> Result<String, String> {
        tools::get_schema(&a.kind)
    }

    #[tool(description = "Generate the starting files of a new plugin (the same templates as `aether --gen plugin`) and return them as text. Nothing is written to disk: create the files yourself, then run validate_plugin.")]
    async fn scaffold_plugin(&self, Parameters(a): Parameters<ScaffoldArgs>) -> Result<String, String> {
        tools::scaffold_plugin(&a.name, &a.language, a.label.as_deref(), a.description.as_deref()).await
    }

    #[tool(description = "Check a plugin folder the way Aether does when loading it (manifest, models, pages, rules, capabilities, scripts) and report the first problem. Reads the folder only; needs no database. Run it after every change until it says VALID.")]
    async fn validate_plugin(&self, Parameters(a): Parameters<ValidateArgs>) -> Result<String, String> {
        tools::validate_plugin(&a.path).await
    }
}

// The stored router, so that tools which do not apply (see `new`) are not offered.
#[tool_handler(router = self.tool_router)]
impl ServerHandler for AetherMcp {
    fn get_info(&self) -> ServerConfig {
        let mut instructions = INSTRUCTIONS.to_string();
        if let Some(live) = &self.live {
            instructions.push_str(&format!(
                "\n\nLive mode: this server also works on a real Aether instance, in the organization `{}` only. \
                 list_installed_plugins, list_catalog_plugins, get_org_models, list_plugin_commands and check_plugin_against_catalog read from it.{}",
                live.org,
                if live.allow_write {
                    " Writing is ON: load_plugin, install_plugin, upgrade_plugin and run_command change the instance as `aether` on the command line would, and are recorded in the plugin activity log. Check with the user before changing anything they did not ask you to change."
                } else {
                    " Writing is off (it needs --mcp-allow-write), so you can look but not change."
                }
            ));
        }
        ServerConfig::new(ServerCapabilities::builder().enable_tools().enable_resources().enable_prompts().build())
            .with_instructions(instructions)
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, ErrorData> {
        let plugins = self.plugin_docs().await;
        let mut resources: Vec<Resource> = plugins
            .iter()
            .flat_map(|p| {
                p.pages.iter().map(move |d| {
                    let mut r = Resource::new(format!("{URI_PREFIX}plugins/{}/{}", p.plugin, d.slug), format!("{}/{}", p.plugin, d.slug))
                        .with_title(format!("{} (plugin {}, written by its author)", d.title, p.plugin))
                        .with_description(d.description.clone());
                    r.mime_type = Some("text/markdown".into());
                    r
                })
            })
            .collect();
        resources.extend(bundle().docs.iter().map(|d| {
                let proposal = if d.status == Status::Proposal { " [proposal, not built]" } else { "" };
                let mut r = Resource::new(format!("{URI_PREFIX}{}", d.slug), d.slug.clone())
                    .with_title(format!("{}{proposal}", d.title))
                    .with_description(d.description.clone());
                r.mime_type = Some("text/markdown".into());
                r
            }));
        Ok(ListResourcesResult::with_all_items(resources))
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, ErrorData> {
        let slug = request.uri.strip_prefix(URI_PREFIX).unwrap_or(&request.uri);
        let text = if let Some(rest) = slug.strip_prefix("plugins/") {
            let (plugin, page) = rest.split_once('/').unwrap_or((rest, "index"));
            tools::get_doc(page, None, Some(plugin), &self.plugin_docs().await).map_err(|e| ErrorData::resource_not_found(e, None))?
        } else {
            let page = doc(slug).ok_or_else(|| ErrorData::resource_not_found(format!("No documentation page `{slug}`"), None))?;
            tools::get_doc(&page.slug, None, None, &[]).map_err(|e| ErrorData::internal_error(e, None))?
        };
        Ok(ReadResourceResult::new(vec![
            ResourceContents::text(text, request.uri.clone()).with_mime_type("text/markdown"),
        ])
        .into())
    }

    async fn list_prompts(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListPromptsResult, ErrorData> {
        let arg = |name: &str, what: &str, required: bool| {
            let mut a = PromptArgument::new(name);
            a.description = Some(what.into());
            a.required = Some(required);
            a
        };
        Ok(ListPromptsResult::with_all_items(vec![
            Prompt::new(
                "build_plugin",
                Some("Build an Aether plugin step by step: read the docs, scaffold, write, validate."),
                Some(vec![arg("goal", "What the plugin should do", true), arg("language", "go, rust, typescript, javascript, python, rhai or lua (default rhai)", false)]),
            ),
            Prompt::new(
                "write_model",
                Some("Write a model (models/<name>.json) for a kind of record."),
                Some(vec![arg("description", "What the records are and which fields they need", true)]),
            ),
        ]))
    }

    async fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<GetPromptResponse, ErrorData> {
        let arg = |name: &str| request.arguments.as_ref().and_then(|a| a.get(name)).and_then(|v| v.as_str()).map(str::to_owned);
        let (description, text) = match request.name.as_str() {
            "build_plugin" => {
                let goal = arg("goal").ok_or_else(|| ErrorData::invalid_params("The argument `goal` is required", None))?;
                ("Build an Aether plugin", tools::build_plugin_prompt(&goal, arg("language").as_deref()))
            }
            "write_model" => {
                let what = arg("description").ok_or_else(|| ErrorData::invalid_params("The argument `description` is required", None))?;
                ("Write an Aether model", tools::write_model_prompt(&what))
            }
            other => return Err(ErrorData::invalid_params(format!("No prompt `{other}`"), None)),
        };
        let mut result = GetPromptResult::new(vec![PromptMessage::new_text(Role::User, text)]);
        result.description = Some(description.into());
        Ok(result.into())
    }
}
