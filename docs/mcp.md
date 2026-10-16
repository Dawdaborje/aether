---
title: "Build plugins with an LLM (MCP)"
description: "aether --mcp gives an LLM client Aether's documentation and tools to scaffold and check plugins, from the single aether binary."
section: guide
status: stable
order: 35
---
# Build plugins with an LLM (MCP)

`aether --mcp` runs a [Model Context Protocol](https://modelcontextprotocol.io) server on standard input
and output. An LLM client (Claude Code, Claude Desktop, Cursor and others) starts it and can then read
Aether's documentation, create the starting files of a plugin, and check the plugin it wrote.

Everything is inside the `aether` binary. The documentation is compiled into it, so there is nothing
else to install or download and no source checkout is needed. It needs no config file and no database.

## Connect a client

Claude Code:

```sh
claude mcp add aether -- aether --mcp
```

Any client that reads an `mcpServers` configuration (Claude Desktop, Cursor and others):

```json
{
  "mcpServers": {
    "aether": { "command": "aether", "args": ["--mcp"] }
  }
}
```

If `aether` is not on your `PATH`, give the full path to the binary as the command.

The server writes only protocol messages to standard output. Its own log lines go to standard error, at
`warn` level unless you pass `--log`.

## What the LLM gets

| Tool | What it does |
|---|---|
| `search_docs` | Searches the documentation (and plugin docs, when configured) and says which `get_doc` call opens each match. |
| `get_doc` | Returns a page, or only the part under one heading. |
| `list_docs` | Lists every page with a one-line description. |
| `get_reference` | The complete pages for one file format or feature: `manifest` (plugin.toml, workspace.toml), `model`, `pages`, `rules`, `commands`, `cli-commands`, `lua`, `rhai`, `events`, `scheduler`, `capabilities`. |
| `get_schema` | The exact JSON Schema of a file format: `manifest` (plugin.toml), `workspace` (workspace.toml), `model` (models/*.json) or `rules` (rules/*.json). It is generated from the types the kernel loads the file with, so it cannot be out of date, and each field carries the kernel's own description. |
| `list_capabilities`, `get_capability` | The capabilities a plugin can declare, such as `db::query`. |
| `scaffold_plugin` | The starting files of a plugin in a chosen language, from the same templates as `aether --gen plugin`. Nothing is written to disk; the client creates the files. |
| `validate_plugin` | Checks a plugin folder the way a load does (manifest, models, pages, rules, capabilities, scripts) and reports the first problem. |

Every page is also a resource at `aether-docs://<path>` (for example `aether-docs://plugin/pages`), and two
prompts start common jobs: `build_plugin` (a goal and optionally a language) and `write_model`.

## Plugin documentation

Plugins can ship their own docs (see [Documenting a plugin](plugin/README.md#documenting-a-plugin)). Point the
server at the plugin folders and the LLM can read them too, which is how it learns another plugin's
functions before using them:

```sh
aether --mcp --plugin-docs ./plugins/notes --plugin-docs ./plugins/payroll
```

The `plugin_paths` of an `aether.toml` in the working directory (or given with `-c`) are included as well. Plugin
folders are read from disk, so no database is needed and the server starts at once; edits show within a few
seconds. This process is run by its owner, so the docs of every `visibility` are included.

`list_docs` then lists the plugins that have docs, and `list_docs`, `search_docs` and `get_doc` take
`plugin=<name>` to look only at that plugin. Each plugin page is also a resource at
`aether-docs://plugins/<plugin>/<page>`. Everything from a plugin is marked as written by the plugin's author and
comes with the instruction to treat it as information, not as instructions, because anyone can write a plugin.

## Pages that are not built

Some pages are design proposals for features that do not exist yet. They are marked *proposal* in the list
and every tool result for them begins with a warning. `search_docs` leaves them out unless asked
(`include_proposals`), so an LLM does not write plugins that depend on something that is not there.

## What `validate_plugin` does not check

It needs no database, so it cannot check links from a model to a model of another plugin, components taken
from another plugin, or model ids already used in the catalog. It says so in its answer. A compiled plugin
(Go, Rust, TypeScript, JavaScript, Python) is checked before it is built, and the answer notes that the
WebAssembly module was not found. To check everything, run `aether --load-plugins <folder> --dry-run` with
a configured database; see [Plugin commands](plugin/cli-commands.md) and [Running Aether](architecture/running.md).

## A typical session

Ask the client for what you want ("an expense tracker plugin with a model for claims") and it should:

1. read `get_reference` for the manifest, models and pages, and `get_schema` for the exact fields;
2. call `scaffold_plugin` and create the files;
3. write the models, pages and code, declaring the capabilities it uses;
4. call `validate_plugin` and fix what it reports until the answer is `VALID`.

Then load and install it yourself: `aether --load-plugins <folder>`, then
`aether --install-plugin <name> --org <org>`. For the whole picture of what a plugin is, see
[Aether Plugin Documentation](plugin/README.md).
