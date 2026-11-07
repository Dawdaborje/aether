---
title: "Uninstalling plugins and what happens to their data"
description: "Uninstalling a plugin keeps its tables and records by default (orphaned, picked up again on reinstall). Dropping the data is a separate, confirmed, backed-up step."
section: architecture
status: stable
order: 285
---
# Uninstalling plugins and what happens to their data

An organization's data outlives the code that made it. Uninstalling a plugin therefore has three
levels, from gentlest to final:

| Level | What it does | Reversible |
|---|---|---|
| **Disable** | `installed_plugins.is_enabled = false`. Nothing is removed. | Yes |
| **Uninstall** (default) | The plugin stops being installed. Its tables, records and stored schema stay, **orphaned**. | Yes: install it again |
| **Uninstall and purge** | As above, then the plugin's tables are dropped. | No |

## Uninstall (keeping the data)

```
aether --uninstall-plugin crm --org acme
```

In one transaction the kernel removes:

- the `installed_plugins` row and its dependency edges;
- the plugin's scheduled tasks, file watches, event subscriptions and inbound hooks (manifest and
  runtime ones alike), since nothing could run them;
- the plugin's theme, and the organization's theme setting if that theme was the active one.

It keeps: the tables the plugin's models created and every record in them, the `model_schema`
rows that say which plugin owns which table, roles (people may hold them), and the audit, history
and chatter rows that mention the plugin.

Installing the plugin again reads `model_schema`, sees the tables are already there and applies
only the difference from the stored schema, exactly as an upgrade does. The records are back.

### Dependants

A plugin another installed plugin needs cannot be uninstalled. The command names the dependants;
list them in the same command to remove them together (dependants go first):

```
aether --uninstall-plugin accounting currency --org acme
```

### `on_uninstall`

A command marked `on_uninstall = true` (no required arguments) runs just before the plugin is
removed, while it can still run, for cleanup outside the database: webhooks registered with other
services, files, external subscriptions. A failure is reported and does not stop the uninstall.

## Orphaned data

```
aether --list-orphans --org acme
aether --purge-orphans crm --org acme
```

`--list-orphans` shows every plugin that is not installed but still owns tables, with record
counts. `--purge-orphans` drops one plugin's data later, once you are sure.

## Purge

`--uninstall-plugin crm --purge` and `--purge-orphans crm`:

1. print the tables and record counts that will be dropped;
2. ask you to type the organization's name (`--yes` skips this);
3. write every record to `<backup-dir>/<org>.<plugin>.<unix seconds>.json`
   (`--backup-dir`, default `purge-backups`; `--no-backup` skips it);
4. `REMOVE TABLE` for each table and for the graph-edge tables the models defined (an edge table
   another plugin's model also defined is left), and delete the `model_schema` rows.

Only tables recorded in `model_schema` for that plugin can be dropped, and only names the schema
applier would have created. Records in other plugins that link to purged records are not
changed; they point at nothing afterwards.

## Disable

A disabled plugin keeps everything; its pages, calls and listeners stop until it is enabled
again. A plugin cannot be disabled while an enabled plugin needs it, nor enabled while a plugin it
needs is disabled.

## In the desk

A developer does all of this from an organization's **Plugins** tab
(`/desk/organizations/<db>/plugins`): a switch per plugin enables or disables it, each row says
which installed plugins it needs and which need it, and **Uninstall** / **Uninstall and purge**
first show what will go (the removal order, and the tables and record counts). A purge asks for
the organization's database name to be typed. An **Orphaned data** section lists what plugins
uninstalled earlier left behind, each with a **Purge** button that asks the same way. Backups go
to `<app_dir>/purge-backups`.

The endpoints behind it (developer-only):

| Endpoint | Does |
|---|---|
| `GET /api/ui/catalog/organizations/{db}/plugins` | Installed plugins (enabled, dependencies, dependants) and orphaned data |
| `POST /api/ui/catalog/uninstall/preview` | The removal order and owned data, refused like the uninstall itself |
| `POST /api/ui/catalog/uninstall` | Uninstall, optionally with `purge` |
| `POST /api/ui/catalog/enabled` | Enable or disable one plugin |
| `POST /api/ui/catalog/purge-orphans` | Back up and drop one uninstalled plugin's data |

## Audit

Every operation writes to the plugin activity log (`uninstall`, `purge`, `enable`, `disable`),
uninstall and purge with the tables and record count. Uninstalling, enabling and disabling tell
other nodes through Mantle signals to drop their cached installed set.
