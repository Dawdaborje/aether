//! The Model Context Protocol adapter: tools, resources and prompts over the functions in `tools.rs`.

use super::tools;
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
}

impl AetherMcp {
    pub fn new() -> Self {
        Self { tool_router: Self::tool_router() }
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
}

#[derive(Deserialize, JsonSchema)]
struct GetDocArgs {
    /// Page path as shown by `list_docs` or `search_docs`, without `.md` (for example `plugin/pages`).
    path: String,
    /// Only the part of the page under this heading: a section id from `search_docs`, or words of the heading.
    section: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
struct ListDocsArgs {
    /// Limit to one section: guide, plugin, architecture or capabilities.
    section: Option<String>,
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
    #[tool(description = "Search the Aether documentation. Returns matching sections with the get_doc call that opens each one. Hides designs that are not built unless include_proposals is set.")]
    async fn search_docs(&self, Parameters(a): Parameters<SearchArgs>) -> Result<String, String> {
        tools::search_docs(&a.query, a.section.as_deref(), a.include_proposals.unwrap_or(false), a.limit)
    }

    #[tool(description = "Read a documentation page, or one section of it. Pages marked PROPOSAL are designs that are not built.")]
    async fn get_doc(&self, Parameters(a): Parameters<GetDocArgs>) -> Result<String, String> {
        tools::get_doc(&a.path, a.section.as_deref())
    }

    #[tool(description = "List every documentation page with a one-line description, optionally for one section.")]
    async fn list_docs(&self, Parameters(a): Parameters<ListDocsArgs>) -> Result<String, String> {
        tools::list_docs(a.section.as_deref())
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

    #[tool(description = "Generate the starting files of a new plugin (the same templates as `aether --gen plugin`) and return them as text. Nothing is written to disk: create the files yourself, then run validate_plugin.")]
    async fn scaffold_plugin(&self, Parameters(a): Parameters<ScaffoldArgs>) -> Result<String, String> {
        tools::scaffold_plugin(&a.name, &a.language, a.label.as_deref(), a.description.as_deref()).await
    }

    #[tool(description = "Check a plugin folder the way Aether does when loading it (manifest, models, pages, rules, capabilities, scripts) and report the first problem. Reads the folder only; needs no database. Run it after every change until it says VALID.")]
    async fn validate_plugin(&self, Parameters(a): Parameters<ValidateArgs>) -> Result<String, String> {
        tools::validate_plugin(&a.path).await
    }
}

#[tool_handler]
impl ServerHandler for AetherMcp {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().enable_resources().enable_prompts().build())
            .with_instructions(INSTRUCTIONS)
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, ErrorData> {
        let resources = bundle()
            .docs
            .iter()
            .map(|d| {
                let proposal = if d.status == Status::Proposal { " [proposal, not built]" } else { "" };
                let mut r = Resource::new(format!("{URI_PREFIX}{}", d.slug), d.slug.clone())
                    .with_title(format!("{}{proposal}", d.title))
                    .with_description(d.description.clone());
                r.mime_type = Some("text/markdown".into());
                r
            })
            .collect();
        Ok(ListResourcesResult::with_all_items(resources))
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, ErrorData> {
        let slug = request.uri.strip_prefix(URI_PREFIX).unwrap_or(&request.uri);
        let page = doc(slug).ok_or_else(|| ErrorData::resource_not_found(format!("No documentation page `{slug}`"), None))?;
        let text = tools::get_doc(&page.slug, None).map_err(|e| ErrorData::internal_error(e, None))?;
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
