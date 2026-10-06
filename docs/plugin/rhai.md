# Rhai plugins

For a small plugin that reads and writes records, validates input and sends a notification, a
WebAssembly build is more than it needs. A plugin can be one [Rhai](https://rhai.rs) script instead:

```toml
[plugin]
name = "currency"
capabilities = ["db::query", "db::mutate"]
access_models = [{ name = "currency", permissions = ["read", "write"] }]
script = "main.rhai"          # instead of wasm_file; a plugin has one or the other
```

Everything else is the same as for a WASM plugin: `plugin.toml`, `models/*.json`, `pages/*.xml`,
revisions and upgrades, the capability and model grants, anonymous access rules, chatter. A page's
`function="add_currency"` calls a function of the script.

## Writing one

Every public `fn` is callable. It takes the call's JSON input as one parameter (or none) and returns
JSON. `private fn` is for helpers.

```rhai
fn add_note(input) {
    if input.title == () { fail("a title is required"); }
    let note = db::create("note", #{ title: input.title, status: "draft" });
    notify::send(#{ title: "Note added", level: "info" });
    note
}
```

| Script | Same as the kernel command |
|---|---|
| `db::get(model, id)`, `db::find(model[, query])`, `db::create(model, data)`, `db::update(model, id, data)`, `db::delete(model, id)` | `db::*` (query: `filter`, `order` (`-field` for newest first), `limit`, `offset`) |
| `db::transaction(ops)` | `db::transaction`: several writes, all or none |
| `cache::get(key)`, `cache::set(key, value[, ttl_secs])`, `cache::invalidate(key)`, `cache::invalidate_prefix(prefix)`, `cache::clear()` | `cache::*` |
| `storage::read(key)`, `storage::read_base64(key)`, `storage::write(key, text)`, `storage::write_base64(key, data)`, `storage::delete(key)`, `storage::list([prefix])` | `storage::*` |
| `http::get(url)`, `http::post(url, body)`, `http::request(map)` | `http::request` (hosts must be listed in `http_hosts`) |
| `fs::read(path)`, `fs::read_base64(path)`, `fs::write(path, text)`, `fs::write_base64(path, data)`, `fs::list([dir])`, `fs::stat(path)`, `fs::rename(from, to[, overwrite])`, `fs::delete(path)` | `fs::*`: the plugin's own folder on disk |
| `bridge::invoke(bridge, action[, params])` | `bridge::call` (the bridge must be in `bridges`; `call` is reserved in Rhai) |
| `events::subscribe(event, function)`, `events::unsubscribe(event)` | `events::subscribe/unsubscribe` |
| `communication::send(map)` | `communication::send`: `#{ type: "email" \| "sms", to, … }` |
| `scheduler::enqueue(function[, payload[, options]])`, `scheduler::job(id)`, `scheduler::cancel_job(id)` | `scheduler::enqueue/job/cancel_job` |
| `scheduler::register(map)`, `scheduler::cancel(name)` | `scheduler::register/cancel` |
| `plugins::invoke(plugin, function[, input])` | `plugins::call` (the plugin must be in `dependencies`; `call` is reserved in Rhai) |
| `notify::send(map)` | `notify::send` |
| `events::emit(map)` | `events::emit` |
| `context::get()` | `context::get` (`actor`, `organization`, `now`, …) |
| `fail("message")` | stops the call; the message is shown to the caller |
| `log::info/warn/error(text)` | the kernel log |

Each command needs its capability in `capabilities` and is limited exactly as for a WASM plugin;
see [Kernel commands](commands.md) for every payload, limit and what is not implemented yet
(every kernel command is available to scripts).

A failed command (a model rule, a missing capability) raises an error you can `try`/`catch`; left
uncaught it is internal: logged, and the caller sees "plugin invocation failed".

## Limits

A script is checked when the plugin is loaded (`--load-plugin` refuses one that does not compile) and
runs with: 2 million operations, 10 seconds, 32 call levels, strings up to 1 MB, collections up to
100 000 items, and a 512 KB source. There is no file, network or process access of the script's own (the kernel commands above are the only way out), no `eval`, and no
`import`: a plugin is one file. Models, grants and chatter work exactly as they do for WASM plugins,
because the script goes through the same kernel commands.

Pick WASM (Rust, Go, …) for heavy computation, third-party libraries or large plugins; Rhai for glue.

To call another plugin from a script, or to write the plugin in Lua instead, see [Lua plugins](lua.md); `plugins::invoke` reaches
plugins in any of the three runtimes.
