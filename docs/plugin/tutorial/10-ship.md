---
title: "Chapter 10: Check and ship"
description: "Check the plugin without a database, give it built-in documentation and a smoke test, and hand it over."
section: plugin
status: stable
order: 22
---
# Chapter 10: Check and ship

**Goal:** a plugin you would dare give to someone else.

## 1. Check it without changing anything

```sh
aether -c aether.toml --load-plugins plugins/tasks --dry-run
```

`--dry-run` checks the manifest, the models, the rules, the pages and the script, and prints every problem it finds,
without writing to the catalog. You should see:

```text
ok: tasks@1.0.0 (4 page(s); app 'Tasks' → /tasks)
1 ok, 0 failed (1)
```

Run it after every change, and always before you hand a plugin to anyone. An assistant working through
[the MCP server](../../mcp.md) uses the same check as `validate_plugin`.

## 2. Documentation that travels with the plugin

A plugin can carry its own documentation. Add a `[docs]` table to `plugin.toml` and a `docs` folder:

<!-- file: ch10/tasks/plugin.toml#70-72 -->
```toml
[docs]
dir = "docs"
visibility = "authenticated"
```

<!-- file: ch10/tasks/docs/README.md#1-14 -->
```markdown
---
title: "Tasks"
description: "A small team task list: add tasks, move them from open to doing to done, and see what is late."
---
# Tasks

A small team task list.

## Using it

- Open **Tasks** from the Apps screen. **New task** adds one; select a row to open it.
- A task moves from *open* to *doing* to *done*. Only a lead can reopen a finished task.
- **Overdue tasks** (`/tasks/overdue`) lists what is past its due date and not done.

```

It is read and checked when the plugin loads, and a broken link stops the load with a clear message. People then find it
on the documentation site under **Plugins**; `visibility = "authenticated"` means any signed-in person. To preview it while
you write: `aether --serve-docs --plugin-docs plugins/tasks`. See [Documenting a plugin](../README.md#documenting-a-plugin).

## 3. A smoke test

A test is a script that uses the plugin the way a person would and says whether it worked. Create
`plugins/tasks/tests/smoke.sh`:

<!-- file: ch10/tasks/tests/smoke.sh -->
```bash
#!/usr/bin/env bash
# A smoke test for the Tasks plugin: adds a task that is already late and checks that it is found.
# Run it against a throwaway installation, never a real one:
#   CONFIG=aether.toml ORG=main ./tests/smoke.sh
set -euo pipefail
config="${CONFIG:-aether.toml}"
org="${ORG:-main}"
run() { aether -c "$config" --command "tasks.$1" --org "$org" "${@:2}" 2>/dev/null; }

title="smoke $(date +%s)"
run add --arg title="$title" --arg due=2000-01-01 | grep -q "\"title\": \"$title\"" || { echo "FAIL: add"; exit 1; }
run overdue | grep -Eq '"overdue": [1-9]' || { echo "FAIL: the late task was not counted"; exit 1; }
run list | grep -q "$title" || { echo "FAIL: list"; exit 1; }
echo "ok"
```

It adds a task that is already late, then checks that the overdue count and the list both see it. Run it against a
throwaway installation, never one with real data:

```sh
chmod +x plugins/tasks/tests/smoke.sh
CONFIG=aether.toml ORG=main plugins/tasks/tests/smoke.sh
```

You should see `ok`. Make a habit of one such script per plugin, and add a line each time you fix a bug.

## 4. Version it

Set `version = "1.0.0"` in `plugin.toml`. Aether treats a new version as a new revision: organizations stay on the one they
have until someone upgrades them with `--upgrade-plugin`. Then:

```sh
aether -c aether.toml --load-plugins plugins/tasks
aether -c aether.toml --upgrade-plugin tasks --org main
```

## 5. Hand it over

The plugin is the folder. To give it to someone: put the folder in version control, or zip it. On their installation a
developer can **Upload zip** under **Desk → Apps**, or unpack the folder and run `--load-plugins` and
`--install-plugin` as you did in chapter 1. See [Running Aether](../../architecture/running.md).

To take a plugin off an organization:

```sh
aether -c aether.toml --uninstall-plugin tasks --org main
```

This **keeps the data**: it says how many records are left behind, and installing the plugin again picks them up.
Add `--purge` to drop the data too, which cannot be undone.

## What you learned

- `--load-plugins --dry-run` is the check to run before anything else.
- A plugin can ship its own documentation, checked when it loads.
- A small script that drives the commands is a test.
- Uninstalling keeps data unless you ask otherwise.

> **Stuck?**
> - *`--dry-run` fails on a link in `docs/`*: the file it points at does not exist or the address is misspelled.
> - *the smoke test prints FAIL*: run its `aether --command` lines by hand to see the real error.

Previous: [Chapter 9](09-translate.md). Next: [Chapter 11, Where next](11-next.md).
