# Lua plugins

A plugin can be one Lua script instead of a WebAssembly module. The language is
[Luau](https://luau.org), the sandboxed Lua: familiar Lua syntax, with the file, package and code-loading
parts removed.

```toml
[plugin]
name = "currency"
capabilities = ["db::query", "db::mutate"]
access_models = [{ name = "currency", permissions = ["read", "write"] }]
script = "main.lua"           # instead of wasm_file; a plugin has one or the other
```

`aether --plugin-path <dir> --plugin-language lua` (or the wizard) writes this for you. Everything else is the same as for a
[Rhai](rhai.md) or WASM plugin: `plugin.toml`, `models/*.json`, `pages/*.xml`, revisions and upgrades, the capability
and model grants, anonymous access rules, chatter. A page's `function="add_currency"` calls a global function of the
script. The engine is picked by the file's extension: `.lua` is Lua, `.rhai` is Rhai.

## Writing one

Every global function is callable. It takes the call's JSON input as one parameter (or none) and returns JSON.
A `local function`, or a function whose name starts with `_`, is a helper nobody can call.

```lua
function add_note(input)
    if input.title == nil then fail("a title is required") end
    local note = db.create("note", { title = input.title, status = "draft" })
    notify.send({ title = "Note added", level = "info" })
    return note
end
```

The top level of the script may only define things (functions, constants). It runs once when the plugin loads, to
find the functions, before any kernel command exists; a script that calls `db.get` at the top level is refused.

JSON `null` is `nil`, so a missing field and a null one look the same, and a `nil` inside an array leaves a hole.
Numbers are Lua numbers; a whole number goes back to the kernel as an integer.

| Script | Same as the kernel command |
|---|---|
| `db.get(model, id)`, `db.find(model[, query])`, `db.create(model, data)`, `db.update(model, id, data)`, `db.increment(model, id, field[, by])`, `db.delete(model, id)` | `db::*` (query: `filter`, `order`, `limit`, `offset`) |
| `db.transaction(ops)` | `db::transaction` |
| `cache.get(key)`, `cache.set(key, value[, ttl_secs])`, `cache.invalidate(key)`, `cache.invalidate_prefix(prefix)`, `cache.clear()` | `cache::*` |
| `storage.read(key)`, `storage.read_base64(key)`, `storage.write(key, text)`, `storage.write_base64(key, data)`, `storage.delete(key)`, `storage.list([prefix])` | `storage::*` |
| `http.get(url)`, `http.post(url, body)`, `http.request(table)` | `http::request` (hosts must be listed in `http_hosts`) |
| `fs.read(path)`, `fs.read_base64(path)`, `fs.write(path, text)`, `fs.write_base64(path, data)`, `fs.list([dir])`, `fs.stat(path)`, `fs.rename(from, to[, overwrite])`, `fs.delete(path)` | `fs::*`: the plugin's own folder on disk |
| `bridge.call(bridge, action[, params])` (also `bridge.invoke`) | `bridge::call` (the bridge must be in `bridges`) |
| `events.subscribe(event, function)`, `events.unsubscribe(event)`, `events.emit(table)` | `events::*` |
| `communication.send(table)` | `communication::send` |
| `scheduler.enqueue(function[, payload[, options]])`, `scheduler.job(id)`, `scheduler.cancel_job(id)`, `scheduler.register(table)`, `scheduler.cancel(name)` | `scheduler::*` |
| `plugins.call(plugin, function[, input])` (also `plugins.invoke`) | `plugins::call` (the plugin must be in `dependencies`) |
| `notify.send(table)` | `notify::send` |
| `context.get()` | `context::get` (`actor`, `organization`, `now`, ...) |
| `fail("message")` | stops the call; the message is shown to the caller |
| `log.info/warn/error(text)`, `print(...)` | the kernel log |

Each command needs its capability in `capabilities` and is limited exactly as for a WASM plugin; see
[Kernel commands](commands.md). A failed command (a model rule, a missing capability) raises an error you can catch with
`pcall`; left uncaught it is internal: logged, and the caller sees "plugin invocation failed".

## Calling other plugins, in any language

`plugins.call` reaches a plugin written in Lua, Rhai or WebAssembly, and a plugin in any of them can call yours:

```lua
-- main.lua, with dependencies = ["tax"] in plugin.toml; "tax" may be Rhai or a WASM module
function total(input)
    local tax = plugins.call("tax", "for_amount", { amount = input.amount })
    return { amount = input.amount, tax = tax.value }
end
```

The caller and the target exchange JSON and nothing else. The target runs under its own capabilities and model grants,
as the same actor; calls nest at most 4 deep, and a function cannot be called while it is already running further up the
chain.

## Limits

A script is compiled when the plugin is loaded (`--load-plugin` refuses one that does not compile) and runs with:
2 million operations (function calls and loop turns), 10 seconds, 64 MB of memory, and a 512 KB source. There is no
`io`, `package`, `require`, `loadstring`, `getfenv` or `setfenv`; the standard libraries are read-only, and what a script
defines lives only in that call's interpreter. The kernel commands above are the only way out.

Pick WASM (Rust, Go, ...) for heavy computation, third-party libraries or large plugins; Lua or Rhai for glue.
