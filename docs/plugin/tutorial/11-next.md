---
title: "Chapter 11: Where next"
description: "What to read and try after the tutorial: other languages, workspaces, assistants and the reference."
section: plugin
status: stable
order: 23
---
# Chapter 11: Where next

You built a plugin with a model, pages, permissions, rules, code, events, a schedule, translations, documentation and a
test. Here is where to go from here.

## A bigger plugin

- **Several plugins that belong together** go in a *workspace*, a folder with a `workspace.toml` that lists them and shows
  them as one folder of apps. See [Plugin documentation](../README.md).
- **A plugin that uses another's models** lists it in `dependencies` and links to its models; see
  [Models](../../architecture/models.md).
- **More page types**: kanban boards, calendars, dashboards, charts, file lists, chat. [Pages and data](../pages.md) has them all.
- **Reusable page fragments**: [UI components](../components.md).
- **Who may see what, by record**: [Record and field rules](../../architecture/rules.md) and
  [Visitors, public pages and the audit trail](../../architecture/access.md).
- **Import, export and printing**: [Import and export](../../architecture/import_export.md) and
  [Printing](../../architecture/print.md).

## Another language

Rhai is the shortest path, but plugins can also be written in Lua (the same shape, see [Lua plugins](../lua.md)) or in
Rust, Go, TypeScript, JavaScript and Python, which compile to WebAssembly. Choose one of those when you need heavy
computation, a library, or a larger code base. Generate a starter with
`aether --gen plugin --plugin-path plugins/name --plugin-language rust`; the [SDK](../../sdk.md) page lists the languages.
The [appendix](12-in-rust.md) writes this very plugin in Rust.

## Working with an assistant

`aether --mcp` gives an AI assistant the documentation and tools to scaffold, write and check a plugin, using the same
checks you used in chapter 10. See [Build plugins with an LLM](../../mcp.md).

## Reference

| You want to know | Read |
|---|---|
| Every kernel command a script can call | [Kernel commands](../commands.md) |
| Every key of `plugin.toml` | [Plugin documentation](../README.md) |
| How events and jobs behave | [Events](../../architecture/events.md), [Scheduler](../../architecture/scheduler.md) |
| Command-line commands of a plugin | [Plugin commands](../cli-commands.md) |

Go back to the [map of the tutorial](README.md).
