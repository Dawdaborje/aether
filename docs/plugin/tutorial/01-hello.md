---
title: "Chapter 1: Hello, plugin"
description: "Create the plugin folder, read each file, install it and call its first function."
section: plugin
status: stable
order: 13
---
# Chapter 1: Hello, plugin

**Goal:** a plugin folder you understand file by file, installed in your organization, answering a command.

## 1. Create it

From the working folder of chapter 0:

```sh
aether --gen plugin --plugin-path plugins/tasks --plugin-language rhai
```

The last part of the path, `tasks`, is the plugin's name: lower-case letters, digits and `_`. The command writes a
folder with a `plugin.toml`, a `main.rhai`, a `README.md`, a `.gitignore` and a `LICENSE` (replace the licence with
your own).

## 2. `main.rhai`: the whole plugin

Open `plugins/tasks/main.rhai` and replace its content with this:

<!-- file: ch01/tasks/main.rhai -->
```rhai
// Tasks: the plugin author tutorial's example. This file is the whole plugin; nothing to compile.

fn hello(input) {
    #{ message: "Hello from tasks", actor: context::get().actor }
}
```

Three things to notice:

- The file *is* the plugin. Every `fn` in it can be called from outside.
- A function gets the call's input as one value (`input`) and returns what it gives back. `#{ ... }` builds a map,
  which becomes JSON.
- `context::get()` asks the kernel who is calling. The kernel offers many such commands (`db::`, `events::`,
  `scheduler::`, ...); [Rhai plugins](../rhai.md) lists them all.

## 3. `plugin.toml`: what the plugin is and may do

The manifest says who the plugin is, which functions are commands, and what it is allowed to ask the kernel. Make it:

<!-- file: ch01/tasks/plugin.toml -->
```toml
[plugin]
name = "tasks"
label = "Tasks"
version = "0.1.0"
description = "A small team task list, built step by step in the plugin author tutorial."
authors = [{ name = "Your Name", email = "your.email@example.com" }]
dependencies = []
capabilities = []
is_builtin = false
script = "main.rhai"

[[command]]
name = "hello"
function = "hello"
help = "Say hello, to check that the plugin runs."

[plugin.api]
version = "0.1"

[plugin.meta]
kind = "addon"

[communication]
channels = []
```

| Part | Meaning |
|---|---|
| `name`, `label`, `version` | The plugin's key (never changes), the name people see, and its version |
| `script` | Which file is the code. A WebAssembly plugin has `wasm_file` here instead |
| `capabilities` | What the plugin may ask the kernel to do. Empty: it can do nothing but compute. We will add database access in the next chapter |
| `[[command]]` | A function you can run from the command line. `name` is what you type, `function` is the `fn` it calls |

A plugin only ever gets what its manifest lists, so you can read `capabilities` and know what a plugin can touch.
See [Plugin documentation](../README.md) for every key.

## 4. Tell the configuration where the plugin lives

Add this line at the very top of `aether.toml`, above the first `[table]`:

```toml
plugin_paths = ["plugins/tasks"]
```

With it, `aether --serve --watch` knows which folder to watch, and `--load-plugins` with no argument loads it. Restart
the server (Ctrl+C, then the `--serve --watch` command again) so it reads the line.

## 5. Load it and install it

Two steps, on purpose. *Loading* puts the plugin in the catalog (the list of plugins the installation knows).
*Installing* turns it on for an organization.

```sh
aether -c aether.toml --load-plugins plugins/tasks
aether -c aether.toml --install-plugin tasks --org main
```

Loading checks the script compiles and the manifest is valid, and it refuses the plugin if not. If the server is
running with `--watch`, it picks the new plugin up too.

## 6. Call it

```sh
aether -c aether.toml --command tasks.hello --org main
```

You should see something like:

```json
{"actor": "...", "message": "Hello from tasks"}
```

You just ran your own code in the kernel. `tasks.hello` is `<plugin>.<command>`; `aether -c aether.toml --list-commands
--org main` lists every command that is installed.

## 7. Something going wrong, on purpose

Open `main.rhai` and break it: delete the closing `}` of `hello`. Run the load step again:

```sh
aether -c aether.toml --load-plugins plugins/tasks
```

It refuses, and the message names the file and the problem. A plugin that does not compile never reaches the
catalog, so a typo cannot take down a working install. Put the `}` back before you go on.

## What you learned

- A script plugin is a folder: a manifest and one code file.
- The manifest lists what the plugin may do; commands are functions with a name.
- Plugins are *loaded* into the catalog and then *installed* per organization.

> **Stuck?**
> - *plugin not found* at install: the load step was skipped or named a different folder.
> - *command not found*: the command must be listed under `[[command]]` and the plugin installed in that `--org`.
> - Nothing changes after an edit: load again, or run the server with `--watch`.

Previous: [Chapter 0](00-set-up.md). Next: [Chapter 2, A model](02-a-model.md).
