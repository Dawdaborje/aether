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
| `notify::send(map)` | `notify::send` |
| `events::emit(map)` | `events::emit` |
| `context::get()` | `context::get` (`actor`, `organization`, `now`, …) |
| `fail("message")` | stops the call; the message is shown to the caller |
| `log::info/warn/error(text)` | the kernel log |

A failed command (a model rule, a missing capability) raises an error you can `try`/`catch`; left
uncaught it is internal: logged, and the caller sees "plugin invocation failed".

## Limits

A script is checked when the plugin is loaded (`--load-plugin` refuses one that does not compile) and
runs with: 2 million operations, 10 seconds, 32 call levels, strings up to 1 MB, collections up to
100 000 items, and a 512 KB source. There is no file, network or process access, no `eval`, and no
`import`: a plugin is one file. Models, grants and chatter work exactly as they do for WASM plugins,
because the script goes through the same kernel commands.

Pick WASM (Rust, Go, …) for heavy computation, third-party libraries or large plugins; Rhai for glue.
