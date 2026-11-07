---
title: "Get started"
description: "The first steps for running Aether and writing a plugin."
section: guide
status: stable
order: 20
---
# Get started

Three steps from nothing to a plugin of your own.

1. **Install the prerequisites.** The `aether` binary and SurrealDB: see [Prerequisite](./prerequisite.md).
2. **Run Aether.** Write a configuration with `aether -q aether.toml`, prepare the database with
   `aether -c aether.toml --init`, and start the server with `aether -c aether.toml --serve`. See
   [Running Aether](./architecture/running.md).
   The server answers the API and these docs; the screens are drawn by the web app in `web/`
   (`npm install && npm run dev`).
3. **Write a plugin.** The [plugin author tutorial](./plugin/tutorial/README.md) builds a small task list from an empty
   folder, one short chapter at a time, and the first four chapters take about half an hour.

Prefer to have an assistant write the plugin? See [Build plugins with an LLM](./mcp.md).
