//! Lua plugins: a plugin whose code is one Lua script (`[plugin] script = "main.lua"`). The
//! language is [Luau](https://luau.org), the sandboxed Lua: the standard libraries are read-only,
//! and there is no `io`, `package`, `require`, `loadstring` or `getfenv`. A script is compiled to
//! bytecode once when the plugin loads; every call runs it in a fresh interpreter with an
//! operation, memory and time limit.
//!
//! ```lua
//! function add(input)
//!     local note = db.create("note", { title = input.title })
//!     notify.send({ level = "info", title = "Note added" })
//!     return note
//! end
//! ```
//!
//! Every global function is callable, except one whose name starts with `_` and any `local
//! function`. A function takes the call's JSON input (or nothing) and returns JSON. The top level
//! of the script may only define things (functions, constants): it runs once to find the
//! functions, before any kernel command is available. `fail("message")` stops the call with a
//! message for the caller; any other error is internal and only logged.
//!
//! JSON `null` is Lua `nil`: a missing field and a null one look the same, and a `nil` inside an
//! array leaves a hole.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Instant;

use mlua::chunk::{ChunkMode, Compiler};
use mlua::serde::SerializeOptions;
use mlua::{Error as LuaError, Function, Lua, LuaSerdeExt, MultiValue, Table, Value as LuaValue, VmState};
use serde_json::Value;

use crate::kernel::PluginHostContext;

use super::{MAX_OPERATIONS, MAX_RUN_TIME, ScriptError, USER_MARK, kernel_data};

/// Memory one call's interpreter may hold.
const MAX_MEMORY_BYTES: usize = 64 * 1024 * 1024;
const OPERATIONS_MARK: &str = "\u{1}operations\u{1}";
const TIME_MARK: &str = "\u{1}time\u{1}";

/// Globals a script must not have: they load code or files, or reach into the interpreter.
const REMOVED_GLOBALS: [&str; 6] = ["require", "package", "loadstring", "getfenv", "setfenv", "newproxy"];

/// JSON `null` becomes `nil` rather than a placeholder value, so `if record == nil` works.
const TO_LUA: SerializeOptions = SerializeOptions::new().serialize_none_to_null(false).serialize_unit_to_null(false);

/// A checked Lua script, ready to run. Cheap to share; each call builds its own interpreter.
pub struct LuaProgram {
    bytecode: Vec<u8>,
    /// The global functions a caller may invoke.
    functions: Vec<String>,
}

impl LuaProgram {
    /// Compile a script and check that it is allowed to run.
    pub fn compile(source: &str) -> Result<Self, ScriptError> {
        let bytecode = Compiler::new().compile(source).map_err(|error| ScriptError::Compile(error.to_string()))?;
        let lua = limited_lua(Instant::now()).map_err(|error| ScriptError::Compile(error.to_string()))?;
        let provided = global_names(&lua)?;
        run_chunk(&lua, &bytecode).map_err(|error| {
            ScriptError::Compile(format!(
                "the top level of a Lua script may only define functions and constants: {}",
                describe(error)
            ))
        })?;
        // What the host put in the globals (`fail`, `print`, ...) is not the script's to offer.
        let mut functions = Vec::new();
        for pair in lua.globals().pairs::<String, LuaValue>() {
            let (name, value) = pair.map_err(|error| ScriptError::Compile(error.to_string()))?;
            if matches!(value, LuaValue::Function(_)) && !name.starts_with('_') && !provided.contains(&name) {
                functions.push(name);
            }
        }
        functions.sort();
        Ok(Self { bytecode, functions })
    }

    pub fn has_function(&self, name: &str) -> bool {
        self.functions.iter().any(|function| function == name)
    }

    /// Names of the functions a caller may invoke.
    pub fn function_names(&self) -> Vec<&str> {
        self.functions.iter().map(String::as_str).collect()
    }

    /// Run a function. Blocking: call it from a blocking thread. `runtime` drives the kernel's
    /// async commands.
    pub fn call(
        &self,
        function: &str,
        input: Value,
        host: PluginHostContext,
        runtime: tokio::runtime::Handle,
    ) -> Result<Value, ScriptError> {
        if !self.has_function(function) {
            return Err(ScriptError::NoSuchFunction(function.to_string()));
        }
        let lua = limited_lua(Instant::now()).map_err(describe)?;
        register_commands(&lua, &Rc::new(Host { context: host, runtime })).map_err(describe)?;
        run_chunk(&lua, &self.bytecode).map_err(describe)?;
        let target: Function = lua.globals().get(function).map_err(describe)?;
        let input = lua.to_value_with(&input, TO_LUA).map_err(|error| ScriptError::Data(error.to_string()))?;
        let result: LuaValue = target.call(input).map_err(describe)?;
        let output = lua.from_value::<Value>(result).map_err(|error| ScriptError::Data(error.to_string()))?;
        Ok(whole_numbers(output))
    }
}

/// Names in the interpreter's own globals layer.
fn global_names(lua: &Lua) -> Result<Vec<String>, ScriptError> {
    lua.globals()
        .pairs::<String, LuaValue>()
        .map(|pair| pair.map(|(name, _)| name).map_err(|error| ScriptError::Compile(error.to_string())))
        .collect()
}

/// Run compiled bytecode: it defines the script's globals.
fn run_chunk(lua: &Lua, bytecode: &[u8]) -> mlua::Result<()> {
    lua.load(bytecode).set_mode(ChunkMode::Binary).exec()
}

/// Lua has one number type, so `2` may arrive as `2.0`; a whole number goes back to the kernel
/// as an integer, as the models expect.
fn whole_numbers(value: Value) -> Value {
    match value {
        Value::Number(number) => match number.as_f64() {
            Some(float) if number.is_f64() && float.fract() == 0.0 && float.abs() < 9.0e15 => Value::from(float as i64),
            _ => Value::Number(number),
        },
        Value::Array(items) => Value::Array(items.into_iter().map(whole_numbers).collect()),
        Value::Object(map) => Value::Object(map.into_iter().map(|(key, item)| (key, whole_numbers(item))).collect()),
        other => other,
    }
}

fn describe(error: LuaError) -> ScriptError {
    let text = error.to_string();
    if let Some(start) = text.find(USER_MARK) {
        let rest = &text[start + USER_MARK.len()..];
        let message = rest.split("\nstack traceback").next().unwrap_or(rest);
        return ScriptError::User(message.trim_end().to_string());
    }
    if text.contains(OPERATIONS_MARK) {
        return ScriptError::Failed(format!("the script ran more than {MAX_OPERATIONS} operations"));
    }
    if text.contains(TIME_MARK) {
        return ScriptError::Failed(format!("the script ran longer than {} s", MAX_RUN_TIME.as_secs()));
    }
    if matches!(error, LuaError::MemoryError(_)) {
        return ScriptError::Failed(format!("the script used more than {} MB of memory", MAX_MEMORY_BYTES / (1024 * 1024)));
    }
    ScriptError::Failed(text)
}

/// An interpreter with the limits on and the escape hatches removed. Its globals are a private,
/// writable layer over the read-only standard libraries, so what a script defines is only in
/// this interpreter.
fn limited_lua(started: Instant) -> mlua::Result<Lua> {
    let lua = Lua::new();
    let globals = lua.globals();
    for name in REMOVED_GLOBALS {
        globals.set(name, LuaValue::Nil)?;
    }
    lua.sandbox(true)?;
    lua.set_memory_limit(MAX_MEMORY_BYTES)?;

    // Called at every function call and loop turn, so a script cannot run away; once over a
    // limit it keeps failing, so a `pcall` cannot swallow it.
    let operations = Cell::new(0_u64);
    lua.set_interrupt(move |_| {
        if started.elapsed() > MAX_RUN_TIME {
            return Err(LuaError::runtime(TIME_MARK));
        }
        let count = operations.get() + 1;
        operations.set(count);
        if count > MAX_OPERATIONS {
            return Err(LuaError::runtime(OPERATIONS_MARK));
        }
        Ok(VmState::Continue)
    });

    let globals = lua.globals();
    globals.set(
        "fail",
        lua.create_function(|_, message: String| -> mlua::Result<()> {
            Err(LuaError::runtime(format!("{USER_MARK}{message}")))
        })?,
    )?;
    globals.set(
        "print",
        lua.create_function(|_, args: MultiValue| {
            let text: Vec<String> = args.iter().map(|value| value.to_string().unwrap_or_default()).collect();
            log::info!("[script] {}", text.join("\t"));
            Ok(())
        })?,
    )?;
    let log_table = lua.create_table()?;
    log_table.set("info", lua.create_function(|_, message: String| {
            log::info!("[script] {message}");
            Ok(())
        })?)?;
    log_table.set("warn", lua.create_function(|_, message: String| {
            log::warn!("[script] {message}");
            Ok(())
        })?)?;
    log_table.set("error", lua.create_function(|_, message: String| {
            log::error!("[script] {message}");
            Ok(())
        })?)?;
    globals.set("log", log_table)?;
    Ok(lua)
}

/// How scripts reach the kernel: the same commands a WASM plugin sends.
struct Host {
    context: PluginHostContext,
    runtime: tokio::runtime::Handle,
}

/// Where a positional argument of a script function goes in the command's payload.
enum Arg {
    /// Under this key.
    Key(&'static str),
    /// Its fields join the payload (a table).
    Merge,
    /// Text goes under `body`; a table or number goes under `json`.
    Body,
}

/// A kernel command as a script calls it: `module.name(args...)`.
struct Command {
    module: &'static str,
    name: &'static str,
    command: &'static str,
    args: &'static [Arg],
    /// Payload fields the call always sets.
    fixed: &'static [(&'static str, &'static str)],
    /// Answer with only this field of the command's data.
    pick: Option<&'static str>,
}

const fn command(
    module: &'static str,
    name: &'static str,
    command: &'static str,
    args: &'static [Arg],
) -> Command {
    Command { module, name, command, args, fixed: &[], pick: None }
}

use Arg::{Body, Key, Merge};

/// Every kernel command a script may call, bound under the module named like the command
/// (`db.`, `cache.`, `storage.`, `http.`, ...). The same set as Rhai's.
const COMMANDS: &[Command] = &[
    command("db", "get", "db::get", &[Key("model"), Key("id"), Merge]),
    command("db", "find", "db::find", &[Key("model"), Merge]),
    command("db", "transitions", "db::transitions", &[Key("model"), Key("id")]),
    command("db", "create", "db::create", &[Key("model"), Key("data")]),
    command("db", "update", "db::update", &[Key("model"), Key("id"), Key("data")]),
    command("db", "increment", "db::increment", &[Key("model"), Key("id"), Key("field"), Key("by")]),
    command("db", "delete", "db::delete", &[Key("model"), Key("id")]),
    command("db", "transaction", "db::transaction", &[Key("ops")]),
    command("cache", "get", "cache::get", &[Key("key")]),
    command("cache", "set", "cache::set", &[Key("key"), Key("value"), Key("ttl_secs")]),
    command("cache", "invalidate", "cache::invalidate", &[Key("key")]),
    command("cache", "invalidate_prefix", "cache::invalidate", &[Key("prefix")]),
    command("cache", "clear", "cache::clear", &[]),
    command("storage", "write", "storage::write", &[Key("key"), Key("text")]),
    command("storage", "write_base64", "storage::write", &[Key("key"), Key("base64")]),
    Command { pick: Some("text"), ..command("storage", "read", "storage::read", &[Key("key")]) },
    Command {
        fixed: &[("encoding", "base64")],
        pick: Some("base64"),
        ..command("storage", "read_base64", "storage::read", &[Key("key")])
    },
    command("storage", "delete", "storage::delete", &[Key("key")]),
    command("storage", "list", "storage::list", &[Key("prefix")]),
    command("plugins", "call", "plugins::call", &[Key("plugin"), Key("function"), Key("input")]),
    command("plugins", "invoke", "plugins::call", &[Key("plugin"), Key("function"), Key("input")]),
    command("bridge", "call", "bridge::call", &[Key("bridge"), Key("action"), Key("params")]),
    command("bridge", "invoke", "bridge::call", &[Key("bridge"), Key("action"), Key("params")]),
    Command { pick: Some("text"), ..command("fs", "read", "fs::read", &[Key("path")]) },
    Command {
        fixed: &[("encoding", "base64")],
        pick: Some("base64"),
        ..command("fs", "read_base64", "fs::read", &[Key("path")])
    },
    command("fs", "write", "fs::write", &[Key("path"), Key("text")]),
    command("fs", "write_base64", "fs::write", &[Key("path"), Key("base64")]),
    command("fs", "list", "fs::list", &[Key("path")]),
    command("fs", "stat", "fs::stat", &[Key("path")]),
    command("fs", "rename", "fs::rename", &[Key("from"), Key("to"), Key("overwrite")]),
    command("fs", "delete", "fs::delete", &[Key("path")]),
    command("communication", "send", "communication::send", &[Merge]),
    command("scheduler", "enqueue", "scheduler::enqueue", &[Key("function"), Key("payload"), Merge]),
    command("scheduler", "job", "scheduler::job", &[Key("id")]),
    command("scheduler", "cancel_job", "scheduler::cancel_job", &[Key("id")]),
    command("scheduler", "register", "scheduler::register", &[Merge]),
    command("scheduler", "cancel", "scheduler::cancel", &[Key("name")]),
    command("http", "request", "http::request", &[Merge]),
    Command { fixed: &[("method", "GET")], ..command("http", "get", "http::request", &[Key("url")]) },
    Command { fixed: &[("method", "POST")], ..command("http", "post", "http::request", &[Key("url"), Body]) },
    command("notify", "send", "notify::send", &[Merge]),
    command("events", "subscribe", "events::subscribe", &[Key("event"), Key("function")]),
    command("events", "unsubscribe", "events::unsubscribe", &[Key("event")]),
    command("events", "emit", "events::emit", &[Merge]),
    command("context", "get", "context::get", &[]),
];

impl Host {
    /// Send `spec`'s command with the call's arguments and give back its data as a Lua value.
    fn run(&self, lua: &Lua, spec: &Command, args: MultiValue) -> mlua::Result<LuaValue> {
        let mut merged = serde_json::Map::new();
        let mut keyed = serde_json::Map::new();
        for (index, value) in args.into_iter().enumerate() {
            let Some(arg) = spec.args.get(index) else { break };
            if value.is_nil() {
                continue;
            }
            let json = whole_numbers(lua.from_value::<Value>(value)?);
            match arg {
                Key(key) => {
                    keyed.insert((*key).to_string(), json);
                }
                Body => {
                    let key = if json.is_string() { "body" } else { "json" };
                    keyed.insert(key.to_string(), json);
                }
                Merge => match json {
                    Value::Object(fields) => merged.extend(fields),
                    _ => {
                        return Err(LuaError::runtime(format!(
                            "argument {} of {}.{} must be a table",
                            index + 1,
                            spec.module,
                            spec.name
                        )));
                    }
                },
            }
        }
        // What a function names itself wins over what a merged table carries.
        merged.extend(keyed);
        for (key, text) in spec.fixed {
            merged.insert((*key).to_string(), Value::String((*text).to_string()));
        }
        let payload = if merged.is_empty() { Value::Null } else { Value::Object(merged) };
        let mut data = kernel_data(&self.context, &self.runtime, spec.command, payload).map_err(LuaError::runtime)?;
        if let Some(field) = spec.pick {
            data = data.get(field).cloned().unwrap_or(Value::Null);
        }
        lua.to_value_with(&data, TO_LUA)
    }
}

/// Bind every kernel command as `module.name` in the interpreter's globals.
fn register_commands(lua: &Lua, host: &Rc<Host>) -> mlua::Result<()> {
    let globals = lua.globals();
    for spec in COMMANDS {
        let module = match globals.get::<Option<Table>>(spec.module)? {
            Some(module) => module,
            None => {
                let module = lua.create_table()?;
                globals.set(spec.module, module.clone())?;
                module
            }
        };
        let host = host.clone();
        module.set(spec.name, lua.create_function(move |lua, args: MultiValue| host.run(lua, spec, args))?)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin_manager::runtime::USER_ERROR_PREFIX;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    /// Run `source`'s function `run` with a test host that holds `caps`, on a blocking thread
    /// the way the runtime does.
    async fn run_script(source: &str, caps: &[&str]) -> Result<Value, ScriptError> {
        use crate::kernel::host::test_support::{dummy_ctx, in_memory_media, with_services};
        let script = LuaProgram::compile(source)?;
        let host = with_services(dummy_ctx(caps), in_memory_media());
        let runtime = tokio::runtime::Handle::current();
        tokio::task::spawn_blocking(move || script.call("run", Value::Null, host, runtime))
            .await
            .map_err(|error| ScriptError::Failed(error.to_string()))?
    }

    #[test]
    fn only_global_public_functions_are_callable() -> TestResult {
        let script = LuaProgram::compile(
            "function hello(input) return input end\nlocal function secret() return 1 end\nfunction _helper() return 2 end\nfunction zero() return 3 end\nLIMIT = 5",
        )?;
        assert_eq!(script.function_names(), vec!["hello", "zero"]);
        assert!(!script.has_function("secret") && !script.has_function("_helper") && !script.has_function("LIMIT"));
        Ok(())
    }

    #[test]
    fn a_script_that_does_not_compile_is_refused_when_loaded() {
        assert!(matches!(LuaProgram::compile("function broken( {"), Err(ScriptError::Compile(_))));
    }

    #[test]
    fn the_top_level_may_not_call_kernel_commands() {
        let result = LuaProgram::compile(r#"local note = db.get("note", "1")"#);
        assert!(matches!(&result, Err(ScriptError::Compile(message)) if message.contains("top level")), "{:?}", result.err());
    }

    #[tokio::test]
    async fn files_code_and_internals_are_out_of_reach() -> TestResult {
        let result = run_script(
            r#"function run()
                return { require = require, io = io, loadstring = loadstring, getfenv = getfenv, package = package }
            end"#,
            &[],
        )
        .await?;
        assert_eq!(result, serde_json::json!({}));
        Ok(())
    }

    #[tokio::test]
    async fn the_standard_libraries_cannot_be_changed() -> TestResult {
        let result = run_script(
            r#"function run()
                local ok = pcall(function() string.len = nil end)
                return ok
            end"#,
            &[],
        )
        .await?;
        assert_eq!(result, serde_json::json!(false));
        Ok(())
    }

    #[tokio::test]
    async fn runaway_scripts_are_stopped_even_inside_pcall() -> TestResult {
        let error = run_script("function run() while true do pcall(function() while true do end end) end end", &[])
            .await
            .err()
            .ok_or("it finished")?;
        assert!(error.to_string().contains("operations"), "{error}");
        Ok(())
    }

    #[tokio::test]
    async fn fail_reaches_the_caller_from_a_nested_function() -> TestResult {
        let error = run_script(r#"local function inner() fail("deep") end function run() inner() end"#, &[])
            .await
            .err()
            .ok_or("it finished")?;
        assert!(matches!(&error, ScriptError::User(message) if message == "deep"), "{error}");
        assert!(error.to_string().starts_with(USER_ERROR_PREFIX));
        Ok(())
    }

    #[tokio::test]
    async fn other_errors_are_internal() -> TestResult {
        let error = run_script(r#"function run() error("db exploded") end"#, &[]).await.err().ok_or("it finished")?;
        assert!(matches!(error, ScriptError::Failed(_)), "{error}");
        Ok(())
    }

    #[tokio::test]
    async fn input_and_output_are_json() -> TestResult {
        let script = LuaProgram::compile(
            "function double(input) return { n = input.n * 2, tags = input.tags, gone = input.missing } end",
        )?;
        let host = crate::kernel::host::test_support::dummy_ctx(&[]);
        let runtime = tokio::runtime::Handle::current();
        let answer = tokio::task::spawn_blocking(move || {
            script.call("double", serde_json::json!({ "n": 21, "tags": ["a", "b"] }), host, runtime)
        })
        .await??;
        assert_eq!(answer, serde_json::json!({ "n": 42, "tags": ["a", "b"] }));
        Ok(())
    }

    #[tokio::test]
    async fn scripts_can_use_the_cache() -> TestResult {
        let result = run_script(
            r#"function run()
                cache.set("k", { n = 1, tags = { "a" } })
                cache.set("short", 5, 60)
                local seen = cache.get("k")
                local gone = cache.get("missing")
                cache.invalidate("short")
                return { n = seen.n, tag = seen.tags[1], gone = gone, cleared = cache.clear().removed }
            end"#,
            &["cache::get", "cache::set", "cache::invalidate", "cache::clear"],
        )
        .await?;
        assert_eq!(result, serde_json::json!({ "n": 1, "tag": "a", "cleared": 1 }));
        Ok(())
    }

    #[tokio::test]
    async fn scripts_can_use_storage() -> TestResult {
        let result = run_script(
            r#"function run()
                storage.write("notes/a.txt", "hello")
                storage.write_base64("bin", "AAEC")
                local listed = storage.list("notes")
                return { text = storage.read("notes/a.txt"), bin = storage.read_base64("bin"),
                         listed = #listed, first = listed[1].key, all = #storage.list() }
            end"#,
            &["storage::read", "storage::write", "storage::list"],
        )
        .await?;
        assert_eq!(
            result,
            serde_json::json!({ "text": "hello", "bin": "AAEC", "listed": 1, "first": "notes/a.txt", "all": 2 })
        );
        Ok(())
    }

    #[tokio::test]
    async fn a_refused_command_can_be_caught_by_the_script() -> TestResult {
        let refused = run_script(
            r#"function run()
                local ok = pcall(cache.get, "k")
                return ok
            end"#,
            &[],
        )
        .await?;
        assert_eq!(refused, serde_json::json!(false));

        let result = run_script(
            r#"function run()
                local ok, message = pcall(http.get, "https://example.com/")
                return tostring(message)
            end"#,
            &["http::request"],
        )
        .await?;
        assert!(result.as_str().is_some_and(|text| text.contains("http_hosts")), "{result}");
        Ok(())
    }

    #[tokio::test]
    async fn plugins_call_reaches_the_kernel_command() -> TestResult {
        // This test host has no plugin caller, so the kernel's own refusal must come back.
        let result = run_script(
            r#"function run()
                local ok, message = pcall(plugins.call, "dep", "double", { n = 1 })
                return tostring(message)
            end"#,
            &["plugins::call"],
        )
        .await?;
        assert!(result.as_str().is_some_and(|text| text.contains("dependencies")), "{result}");
        Ok(())
    }

    #[tokio::test]
    async fn every_command_is_bound() -> TestResult {
        // Each one is called with nothing it could use: it may be refused, but it must exist.
        let mut calls = String::new();
        for spec in COMMANDS {
            calls.push_str(&format!(
                "do local ok, message = pcall({0}.{1}, {{}}) if not ok and tostring(message):find(\"nil value\") then missing[#missing + 1] = \"{0}.{1}\" end end\n",
                spec.module, spec.name
            ));
        }
        let source = format!("function run()\nlocal missing = {{}}\n{calls}return missing\nend");
        let result = run_script(&source, &[]).await?;
        assert_eq!(result, serde_json::json!({}), "commands that are not bound");
        Ok(())
    }
}
