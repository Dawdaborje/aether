---
title: "Plugin author tutorial"
description: "Build a small team task list as an Aether plugin, one short chapter at a time, from an empty folder to a working list and form."
section: plugin
status: stable
order: 11
---
# Plugin author tutorial

You will build **Tasks**, a small team task list, as a real plugin. Each chapter adds one idea and ends with
something that runs. You need about two hours for the whole path and about thirty minutes for the first four chapters.

The plugin is written in [Rhai](../rhai.md), a small scripting language. There is nothing to install or compile: the
plugin *is* a folder of text files. (Plugins can also be written in Rust, Go, TypeScript, JavaScript and Python; the
last chapter says where to look.)

## The chapters

| Chapter | You end with |
|---|---|
| [0. Set up](00-set-up.md) | `aether` installed, a database running, an administrator, and the dev server open in a browser |
| [1. Hello, plugin](01-hello.md) | A plugin folder you understand file by file, installed, and a command that answers |
| [2. A model](02-a-model.md) | A `task` model with its ids, and tasks you can add and list from the command line |
| [3. A list and a form](03-list-and-form.md) | A Tasks tile in the web app, a list of tasks and a form that adds one |
| [4. A detail page](04-detail-page.md) | Open a task, edit it, and read its history |
| [5. Permissions](05-permissions.md) | A role, a rule that only leads may delete, and what a member sees |
| [6. A rule](06-rule.md) | A workflow `open → doing → done`, and a finished task that is locked |
| [7. Code](07-code.md) | Functions that find overdue tasks, a button on each row, and a page of what is late |
| [8. Reacting](08-events-and-schedule.md) | An event when a task is done, a listener that notifies people, and a job every morning |
| [9. Translate and theme](09-translate.md) | The plugin's text in two languages, and how it follows the theme |
| [10. Check and ship](10-ship.md) | A clean check, built-in documentation, a smoke test, and a plugin you can hand over |
| [11. Where next](11-next.md) | Other languages, workspaces, assistants and the reference |
| [Appendix: the same plugin in Rust](12-in-rust.md) | The finished plugin written in Rust and compiled to WebAssembly, and what changes |

Every chapter has a folder with the plugin as it stands at the end of that chapter (`ch01`, `ch02`, ...), next to the
finished plugin. If you get lost, copy the next folder over yours and carry on. The code in these pages is taken from
those folders and checked against them, so it is the code that was run.

## How the chapters are written

- Each step says what to type and what you should see. Where something goes wrong on purpose, the page says so.
- Words in `code` are file names, commands or things you type.
- If a command fails, the box at the end of the chapter lists the usual causes.

When you want the full details of any file format, the reference pages are [Plugin documentation](../README.md),
[Pages and data](../pages.md), [Models](../../architecture/models.md) and [Plugin commands](../cli-commands.md).
