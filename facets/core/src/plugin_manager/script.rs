//! Rhai plugins: a plugin whose code is one script (`[plugin] script = "main.rhai"`) instead of
//! a WebAssembly module. For small plugins that only read and write records, validate input and
//! send notifications; the script is read and checked when the plugin loads and runs inside the
//! same kernel commands, capability checks and model grants as a WASM plugin. It cannot reach
//! anything else: there is no file, network or process access in the language, `eval` and
//! `import` are off, and every call has an operation, depth, size and time limit.
//!
//! ```rhai
//! fn add(input) {
//!     let note = db::create("note", #{ title: input.title });
//!     notify::send(#{ level: "info", title: "Note added" });
//!     note
//! }
//! ```
//!
//! Functions take the call's JSON input (or nothing) and return JSON. `fail("message")` stops
//! the call with a message for the caller; any other error is internal and only logged.

use std::sync::Arc;
use std::time::{Duration, Instant};

use rhai::{
    AST, Dynamic, Engine, EvalAltResult, FnAccess, Map, Module, Position,
    Scope, packages::{Package, StandardPackage},
};
use serde_json::Value;

use crate::kernel::{PluginHostContext, kernel_command};

use super::runtime::USER_ERROR_PREFIX;

/// Operations a single call may run (Rhai's counterpart of the WASM fuel limit).
pub const MAX_OPERATIONS: u64 = 2_000_000;
/// How long a single call may run.
pub const MAX_RUN_TIME: Duration = Duration::from_secs(10);
const MAX_CALL_LEVELS: usize = 32;
const MAX_STRING_BYTES: usize = 1024 * 1024;
const MAX_ITEMS: usize = 100_000;
/// Longest script accepted, in bytes.
pub const MAX_SCRIPT_BYTES: u64 = 512 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum ScriptError {
    #[error("script does not compile: {0}")]
    Compile(String),
    #[error("function `{0}` is not in the script")]
    NoSuchFunction(String),
    /// The script called `fail`; the text is for the caller.
    #[error("{USER_ERROR_PREFIX}{0}")]
    User(String),
    #[error("script failed: {0}")]
    Failed(String),
    #[error("input or output is not valid JSON data: {0}")]
    Data(String),
}

/// A checked script, ready to run. Cheap to share; each call builds its own engine.
pub struct ScriptProgram {
    ast: AST,
    /// Public functions and how many parameters each takes.
    functions: Vec<(String, usize)>,
}

impl ScriptProgram {
    /// Compile a script and check that it is allowed to run.
    pub fn compile(source: &str) -> Result<Self, ScriptError> {
        let ast = limited_engine(None, Instant::now())
            .compile(source)
            .map_err(|error| ScriptError::Compile(error.to_string()))?;
        let functions = ast
            .iter_functions()
            .filter(|function| function.access == FnAccess::Public)
            .map(|function| (function.name.to_string(), function.params.len()))
            .collect();
        Ok(Self { ast, functions })
    }

    pub fn has_function(&self, name: &str) -> bool {
        self.functions.iter().any(|(function, _)| function == name)
    }

    /// Names of the functions a caller may invoke.
    pub fn function_names(&self) -> Vec<&str> {
        self.functions.iter().map(|(name, _)| name.as_str()).collect()
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
        let arity = self
            .functions
            .iter()
            .find(|(name, _)| name == function)
            .map(|(_, arity)| *arity)
            .ok_or_else(|| ScriptError::NoSuchFunction(function.to_string()))?;
        let engine = limited_engine(Some(Arc::new(Host { context: host, runtime })), Instant::now());
        let input = rhai::serde::to_dynamic(&input).map_err(|error| ScriptError::Data(error.to_string()))?;
        let mut scope = Scope::new();
        let result = match arity {
            0 => engine.call_fn::<Dynamic>(&mut scope, &self.ast, function, ()),
            1 => engine.call_fn::<Dynamic>(&mut scope, &self.ast, function, (input,)),
            _ => {
                return Err(ScriptError::Failed(format!(
                    "`{function}` must take one parameter (the input) or none, not {arity}"
                )));
            }
        }
        .map_err(|error| describe(*error))?;
        rhai::serde::from_dynamic::<Value>(&result).map_err(|error| ScriptError::Data(error.to_string()))
    }
}

/// How scripts reach the kernel: the same commands a WASM plugin sends.
struct Host {
    context: PluginHostContext,
    runtime: tokio::runtime::Handle,
}

impl Host {
    fn command(&self, command: &str, payload: Value) -> Result<Value, Box<EvalAltResult>> {
        match self.runtime.block_on(kernel_command(&self.context, command, payload)) {
            Ok(value) => Ok(value),
            Err(error) => Err(runtime_error(error.to_string())),
        }
    }

    /// The command's `data` (or the whole answer without one), as a script value.
    fn data(&self, command: &str, payload: Value) -> Result<Dynamic, Box<EvalAltResult>> {
        let answer = self.command(command, payload)?;
        let data = match answer {
            Value::Object(mut object) if object.contains_key("data") => object.remove("data").unwrap_or(Value::Null),
            other => other,
        };
        to_script(&data)
    }
}

fn to_script(value: &Value) -> Result<Dynamic, Box<EvalAltResult>> {
    rhai::serde::to_dynamic(value).map_err(|error| runtime_error(error.to_string()))
}

fn to_json(value: &Dynamic) -> Result<Value, Box<EvalAltResult>> {
    rhai::serde::from_dynamic::<Value>(value).map_err(|error| runtime_error(error.to_string()))
}

fn runtime_error(message: String) -> Box<EvalAltResult> {
    Box::new(EvalAltResult::ErrorRuntime(Dynamic::from(message), Position::NONE))
}

/// Marks an error as meant for the caller.
const USER_MARK: &str = "\u{1}user\u{1}";

fn describe(error: EvalAltResult) -> ScriptError {
    // An error inside a script function arrives wrapped in each call that led to it.
    let mut error = &error;
    while let EvalAltResult::ErrorInFunctionCall(_, _, inner, _) | EvalAltResult::ErrorInModule(_, inner, _) = error {
        error = inner;
    }
    match error {
        EvalAltResult::ErrorRuntime(value, _) => {
            let text = value.to_string();
            match text.strip_prefix(USER_MARK) {
                Some(message) => ScriptError::User(message.to_string()),
                None => ScriptError::Failed(text),
            }
        }
        EvalAltResult::ErrorTooManyOperations(_) => {
            ScriptError::Failed(format!("the script ran more than {MAX_OPERATIONS} operations"))
        }
        EvalAltResult::ErrorTerminated(_, _) => {
            ScriptError::Failed(format!("the script ran longer than {} s", MAX_RUN_TIME.as_secs()))
        }
        other => ScriptError::Failed(other.to_string()),
    }
}

/// An engine with the limits on. With a `host`, the kernel's commands are registered too.
fn limited_engine(host: Option<Arc<Host>>, started: Instant) -> Engine {
    let mut engine = Engine::new_raw();
    engine.register_global_module(StandardPackage::new().as_shared_module());
    engine.set_max_operations(MAX_OPERATIONS);
    engine.set_max_call_levels(MAX_CALL_LEVELS);
    engine.set_max_expr_depths(64, 32);
    engine.set_max_string_size(MAX_STRING_BYTES);
    engine.set_max_array_size(MAX_ITEMS);
    engine.set_max_map_size(MAX_ITEMS);
    engine.disable_symbol("eval");
    // No `import`: a script is one file, with nothing else to load.
    engine.set_module_resolver(rhai::module_resolvers::DummyModuleResolver::new());
    engine.on_progress(move |_| {
        if started.elapsed() > MAX_RUN_TIME {
            Some(Dynamic::UNIT)
        } else {
            None
        }
    });
    engine.on_print(|text| log::info!("[script] {text}"));
    engine.on_debug(|text, _, _| log::debug!("[script] {text}"));
    engine.register_fn("fail", |message: &str| -> Result<(), Box<EvalAltResult>> {
        Err(runtime_error(format!("{USER_MARK}{message}")))
    });
    register_log(&mut engine);
    if let Some(host) = host {
        register_commands(&mut engine, &host);
    }
    engine
}

fn register_log(engine: &mut Engine) {
    let mut log_module = Module::new();
    log_module.set_native_fn("info", |message: &str| {
        log::info!("[script] {message}");
        Ok(())
    });
    log_module.set_native_fn("warn", |message: &str| {
        log::warn!("[script] {message}");
        Ok(())
    });
    log_module.set_native_fn("error", |message: &str| {
        log::error!("[script] {message}");
        Ok(())
    });
    engine.register_static_module("log", log_module.into());
}

/// The kernel commands scripts may call, each bound under the module named like the command
/// (`db::`, `cache::`, `storage::`, `http::`, `notify::`, `events::`, `context::`).
fn register_commands(engine: &mut Engine, host: &Arc<Host>) {
    let mut db = Module::new();
    let h = host.clone();
    db.set_native_fn("get", move |model: &str, id: &str| {
        h.data("db::get", serde_json::json!({ "model": model, "id": id }))
    });
    let h = host.clone();
    db.set_native_fn("find", move |model: &str, query: Map| {
        let mut payload = to_json(&Dynamic::from_map(query))?;
        if let Value::Object(object) = &mut payload {
            object.insert("model".into(), Value::String(model.to_string()));
        }
        h.data("db::find", payload)
    });
    let h = host.clone();
    db.set_native_fn("find", move |model: &str| {
        h.data("db::find", serde_json::json!({ "model": model }))
    });
    let h = host.clone();
    db.set_native_fn("create", move |model: &str, data: Map| {
        let data = to_json(&Dynamic::from_map(data))?;
        h.data("db::create", serde_json::json!({ "model": model, "data": data }))
    });
    let h = host.clone();
    db.set_native_fn("update", move |model: &str, id: &str, data: Map| {
        let data = to_json(&Dynamic::from_map(data))?;
        h.data("db::update", serde_json::json!({ "model": model, "id": id, "data": data }))
    });
    let h = host.clone();
    db.set_native_fn("increment", move |model: &str, id: &str, field: &str, by: Dynamic| {
        let by = to_json(&by)?;
        h.data("db::increment", serde_json::json!({ "model": model, "id": id, "field": field, "by": by }))
    });
    let h = host.clone();
    db.set_native_fn("increment", move |model: &str, id: &str, field: &str| {
        h.data("db::increment", serde_json::json!({ "model": model, "id": id, "field": field }))
    });
    let h = host.clone();
    db.set_native_fn("delete", move |model: &str, id: &str| {
        h.data("db::delete", serde_json::json!({ "model": model, "id": id }))
    });
    let h = host.clone();
    db.set_native_fn("transaction", move |ops: rhai::Array| {
        let ops = to_json(&Dynamic::from_array(ops))?;
        h.data("db::transaction", serde_json::json!({ "ops": ops }))
    });
    engine.register_static_module("db", db.into());

    let mut cache = Module::new();
    let h = host.clone();
    cache.set_native_fn("get", move |key: &str| h.data("cache::get", serde_json::json!({ "key": key })));
    let h = host.clone();
    cache.set_native_fn("set", move |key: &str, value: Dynamic| {
        let value = to_json(&value)?;
        h.data("cache::set", serde_json::json!({ "key": key, "value": value }))
    });
    let h = host.clone();
    cache.set_native_fn("set", move |key: &str, value: Dynamic, ttl_secs: i64| {
        let value = to_json(&value)?;
        h.data("cache::set", serde_json::json!({ "key": key, "value": value, "ttl_secs": ttl_secs }))
    });
    let h = host.clone();
    cache.set_native_fn("invalidate", move |key: &str| {
        h.data("cache::invalidate", serde_json::json!({ "key": key }))
    });
    let h = host.clone();
    cache.set_native_fn("invalidate_prefix", move |prefix: &str| {
        h.data("cache::invalidate", serde_json::json!({ "prefix": prefix }))
    });
    let h = host.clone();
    cache.set_native_fn("clear", move || h.data("cache::clear", Value::Null));
    engine.register_static_module("cache", cache.into());

    let mut storage = Module::new();
    let h = host.clone();
    storage.set_native_fn("write", move |key: &str, text: &str| {
        h.data("storage::write", serde_json::json!({ "key": key, "text": text }))
    });
    let h = host.clone();
    storage.set_native_fn("write_base64", move |key: &str, data: &str| {
        h.data("storage::write", serde_json::json!({ "key": key, "base64": data }))
    });
    let h = host.clone();
    storage.set_native_fn("read", move |key: &str| {
        let answer = h.data("storage::read", serde_json::json!({ "key": key }))?;
        Ok(answer.try_cast::<Map>().and_then(|map| map.get("text").cloned()).unwrap_or(Dynamic::UNIT))
    });
    let h = host.clone();
    storage.set_native_fn("read_base64", move |key: &str| {
        let answer = h.data("storage::read", serde_json::json!({ "key": key, "encoding": "base64" }))?;
        Ok(answer.try_cast::<Map>().and_then(|map| map.get("base64").cloned()).unwrap_or(Dynamic::UNIT))
    });
    let h = host.clone();
    storage.set_native_fn("delete", move |key: &str| h.data("storage::delete", serde_json::json!({ "key": key })));
    let h = host.clone();
    storage.set_native_fn("list", move || h.data("storage::list", Value::Null));
    let h = host.clone();
    storage.set_native_fn("list", move |prefix: &str| h.data("storage::list", serde_json::json!({ "prefix": prefix })));
    engine.register_static_module("storage", storage.into());

    let mut plugins = Module::new();
    let h = host.clone();
    plugins.set_native_fn("invoke", move |plugin: &str, function: &str, input: Dynamic| {
        let input = to_json(&input)?;
        h.data("plugins::call", serde_json::json!({ "plugin": plugin, "function": function, "input": input }))
    });
    let h = host.clone();
    plugins.set_native_fn("invoke", move |plugin: &str, function: &str| {
        h.data("plugins::call", serde_json::json!({ "plugin": plugin, "function": function }))
    });
    engine.register_static_module("plugins", plugins.into());

    let mut bridge = Module::new();
    let h = host.clone();
    bridge.set_native_fn("invoke", move |name: &str, action: &str, params: Map| {
        let params = to_json(&Dynamic::from_map(params))?;
        h.data("bridge::call", serde_json::json!({ "bridge": name, "action": action, "params": params }))
    });
    let h = host.clone();
    bridge.set_native_fn("invoke", move |name: &str, action: &str| {
        h.data("bridge::call", serde_json::json!({ "bridge": name, "action": action }))
    });
    engine.register_static_module("bridge", bridge.into());

    let mut fs = Module::new();
    let h = host.clone();
    fs.set_native_fn("read", move |path: &str| {
        let answer = h.data("fs::read", serde_json::json!({ "path": path }))?;
        Ok(answer.try_cast::<Map>().and_then(|map| map.get("text").cloned()).unwrap_or(Dynamic::UNIT))
    });
    let h = host.clone();
    fs.set_native_fn("read_base64", move |path: &str| {
        let answer = h.data("fs::read", serde_json::json!({ "path": path, "encoding": "base64" }))?;
        Ok(answer.try_cast::<Map>().and_then(|map| map.get("base64").cloned()).unwrap_or(Dynamic::UNIT))
    });
    let h = host.clone();
    fs.set_native_fn("write", move |path: &str, text: &str| h.data("fs::write", serde_json::json!({ "path": path, "text": text })));
    let h = host.clone();
    fs.set_native_fn("write_base64", move |path: &str, data: &str| h.data("fs::write", serde_json::json!({ "path": path, "base64": data })));
    let h = host.clone();
    fs.set_native_fn("list", move || h.data("fs::list", Value::Null));
    let h = host.clone();
    fs.set_native_fn("list", move |path: &str| h.data("fs::list", serde_json::json!({ "path": path })));
    let h = host.clone();
    fs.set_native_fn("stat", move |path: &str| h.data("fs::stat", serde_json::json!({ "path": path })));
    let h = host.clone();
    fs.set_native_fn("rename", move |from: &str, to: &str| h.data("fs::rename", serde_json::json!({ "from": from, "to": to })));
    let h = host.clone();
    fs.set_native_fn("rename", move |from: &str, to: &str, overwrite: bool| {
        h.data("fs::rename", serde_json::json!({ "from": from, "to": to, "overwrite": overwrite }))
    });
    let h = host.clone();
    fs.set_native_fn("delete", move |path: &str| h.data("fs::delete", serde_json::json!({ "path": path })));
    engine.register_static_module("fs", fs.into());

    let mut communication = Module::new();
    let h = host.clone();
    communication.set_native_fn("send", move |message: Map| {
        let payload = to_json(&Dynamic::from_map(message))?;
        h.data("communication::send", payload)
    });
    engine.register_static_module("communication", communication.into());

    let mut scheduler = Module::new();
    let h = host.clone();
    scheduler.set_native_fn("enqueue", move |function: &str| {
        h.data("scheduler::enqueue", serde_json::json!({ "function": function }))
    });
    let h = host.clone();
    scheduler.set_native_fn("enqueue", move |function: &str, payload: Map| {
        let payload = to_json(&Dynamic::from_map(payload))?;
        h.data("scheduler::enqueue", serde_json::json!({ "function": function, "payload": payload }))
    });
    let h = host.clone();
    scheduler.set_native_fn("enqueue", move |function: &str, payload: Map, options: Map| {
        let mut request = to_json(&Dynamic::from_map(options))?;
        if let Value::Object(object) = &mut request {
            object.insert("function".into(), Value::String(function.to_string()));
            object.insert("payload".into(), to_json(&Dynamic::from_map(payload))?);
        }
        h.data("scheduler::enqueue", request)
    });
    let h = host.clone();
    scheduler.set_native_fn("job", move |id: &str| h.data("scheduler::job", serde_json::json!({ "id": id })));
    let h = host.clone();
    scheduler.set_native_fn("cancel_job", move |id: &str| h.data("scheduler::cancel_job", serde_json::json!({ "id": id })));
    let h = host.clone();
    scheduler.set_native_fn("register", move |task: Map| {
        let payload = to_json(&Dynamic::from_map(task))?;
        h.data("scheduler::register", payload)
    });
    let h = host.clone();
    scheduler.set_native_fn("cancel", move |name: &str| h.data("scheduler::cancel", serde_json::json!({ "name": name })));
    engine.register_static_module("scheduler", scheduler.into());

    let mut http = Module::new();
    let h = host.clone();
    http.set_native_fn("request", move |request: Map| {
        let payload = to_json(&Dynamic::from_map(request))?;
        h.data("http::request", payload)
    });
    let h = host.clone();
    http.set_native_fn("get", move |url: &str| {
        h.data("http::request", serde_json::json!({ "method": "GET", "url": url }))
    });
    let h = host.clone();
    http.set_native_fn("post", move |url: &str, body: Dynamic| {
        let body = to_json(&body)?;
        // Text goes as it is; maps and arrays are sent as JSON.
        let payload = match body {
            Value::String(text) => serde_json::json!({ "method": "POST", "url": url, "body": text }),
            other => serde_json::json!({ "method": "POST", "url": url, "json": other }),
        };
        h.data("http::request", payload)
    });
    engine.register_static_module("http", http.into());

    let mut notify = Module::new();
    let h = host.clone();
    notify.set_native_fn("send", move |notification: Map| {
        let payload = to_json(&Dynamic::from_map(notification))?;
        h.data("notify::send", payload)
    });
    engine.register_static_module("notify", notify.into());

    let mut events = Module::new();
    let h = host.clone();
    let h = host.clone();
    events.set_native_fn("subscribe", move |event: &str, function: &str| {
        h.data("events::subscribe", serde_json::json!({ "event": event, "function": function }))
    });
    let h = host.clone();
    events.set_native_fn("unsubscribe", move |event: &str| h.data("events::unsubscribe", serde_json::json!({ "event": event })));
    let h = host.clone();
    events.set_native_fn("emit", move |event: Map| {
        let payload = to_json(&Dynamic::from_map(event))?;
        h.data("events::emit", payload)
    });
    engine.register_static_module("events", events.into());

    let mut context = Module::new();
    let h = host.clone();
    context.set_native_fn("get", move || h.data("context::get", Value::Null));
    engine.register_static_module("context", context.into());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn program(source: &str) -> Result<ScriptProgram, Box<dyn std::error::Error>> {
        Ok(ScriptProgram::compile(source)?)
    }

    #[test]
    fn only_public_functions_are_callable() -> Result<(), Box<dyn std::error::Error>> {
        let script = program("fn hello(input) { input } private fn secret() { 1 } fn zero() { 2 }")?;
        assert!(script.has_function("hello") && script.has_function("zero"));
        assert!(!script.has_function("secret"));
        Ok(())
    }

    #[test]
    fn a_script_that_does_not_compile_is_refused_when_loaded() {
        assert!(matches!(ScriptProgram::compile("fn broken( {"), Err(ScriptError::Compile(_))));
    }

    #[test]
    fn eval_and_import_are_off() {
        assert!(ScriptProgram::compile(r#"fn a(i) { eval("1") }"#).is_err());
        assert!(ScriptProgram::compile(r#"import "x" as y; fn a(i) { i }"#).is_ok());
    }

    #[test]
    fn runaway_scripts_are_stopped() -> Result<(), Box<dyn std::error::Error>> {
        let script = program("fn spin(input) { let n = 0; loop { n += 1; } }")?;
        let engine = limited_engine(None, Instant::now());
        let mut scope = Scope::new();
        let result = engine.call_fn::<Dynamic>(&mut scope, &script.ast, "spin", (Dynamic::UNIT,));
        let error = result.err().map(|error| describe(*error)).ok_or("it finished")?;
        assert!(error.to_string().contains("operations"), "{error}");
        Ok(())
    }

    #[test]
    fn fail_from_a_nested_function_still_reaches_the_caller() -> Result<(), Box<dyn std::error::Error>> {
        let script = program(r#"fn inner() { fail("deep"); } fn outer(input) { inner(); }"#)?;
        let engine = limited_engine(None, Instant::now());
        let mut scope = Scope::new();
        let result = engine.call_fn::<Dynamic>(&mut scope, &script.ast, "outer", (Dynamic::UNIT,));
        let error = result.err().map(|error| describe(*error)).ok_or("it finished")?;
        assert!(matches!(&error, ScriptError::User(message) if message == "deep"), "{error}");
        Ok(())
    }

    #[test]
    fn fail_is_for_the_caller_and_other_errors_are_not() {
        let user = describe(*runtime_error(format!("{USER_MARK}no such note")));
        assert!(matches!(&user, ScriptError::User(message) if message == "no such note"));
        assert!(user.to_string().starts_with(USER_ERROR_PREFIX));
        let internal = describe(*runtime_error("db exploded".into()));
        assert!(matches!(internal, ScriptError::Failed(_)));
    }

    #[tokio::test]
    async fn plugins_invoke_reaches_the_kernel_command() -> Result<(), Box<dyn std::error::Error>> {
        // `call` is a reserved word in Rhai, so the script-side name is `invoke`. This test host
        // has no plugin caller, so the kernel's own refusal (not a compile error) must come back.
        let result = run_script(
            r#"fn run() {
                let outcome = "";
                try { plugins::invoke("dep", "double", #{ n: 1 }); outcome = "called"; } catch (e) { outcome = "" + e; }
                outcome
            }"#,
            &["plugins::call"],
        )
        .await?;
        assert!(result.as_str().is_some_and(|text| text.contains("dependencies")), "{result}");
        Ok(())
    }

    #[tokio::test]
    async fn file_commands_are_bound_and_checked() -> Result<(), Box<dyn std::error::Error>> {
        let result = run_script(
            r#"fn run() {
                let refused = [];
                try { fs::read("a"); } catch (e) { refused.push("read"); }
                try { fs::read_base64("a"); } catch (e) { refused.push("read_base64"); }
                try { fs::write("a", "x"); } catch (e) { refused.push("write"); }
                try { fs::write_base64("a", "AA=="); } catch (e) { refused.push("write_base64"); }
                try { fs::list(); } catch (e) { refused.push("list"); }
                try { fs::list("d"); } catch (e) { refused.push("list_dir"); }
                try { fs::stat("a"); } catch (e) { refused.push("stat"); }
                try { fs::rename("a", "b"); } catch (e) { refused.push("rename"); }
                try { fs::rename("a", "b", true); } catch (e) { refused.push("rename_overwrite"); }
                try { fs::delete("a"); } catch (e) { refused.push("delete"); }
                refused
            }"#,
            &[],
        )
        .await?;
        assert_eq!(result.as_array().map(Vec::len), Some(10), "{result}");
        Ok(())
    }

    #[tokio::test]
    async fn messaging_and_background_commands_are_bound_and_checked() -> Result<(), Box<dyn std::error::Error>> {
        // These reach the kernel: without the capability each is refused and the script can catch it.
        let result = run_script(
            r#"fn run() {
                let refused = [];
                try { communication::send(#{ type: "sms", to: "+2348012345678", text: "hi" }); } catch (e) { refused.push("send"); }
                try { scheduler::enqueue("export"); } catch (e) { refused.push("enqueue"); }
                try { scheduler::enqueue("export", #{ n: 1 }); } catch (e) { refused.push("enqueue_payload"); }
                try { scheduler::enqueue("export", #{ n: 1 }, #{ delay_secs: 60 }); } catch (e) { refused.push("enqueue_options"); }
                try { scheduler::job("x"); } catch (e) { refused.push("job"); }
                try { scheduler::cancel_job("x"); } catch (e) { refused.push("cancel_job"); }
                try { scheduler::register(#{ name: "t", function: "f", every: "5m" }); } catch (e) { refused.push("register"); }
                try { scheduler::cancel("t"); } catch (e) { refused.push("cancel"); }
                refused
            }"#,
            &[],
        )
        .await?;
        assert_eq!(
            result,
            serde_json::json!(["send", "enqueue", "enqueue_payload", "enqueue_options", "job", "cancel_job", "register", "cancel"])
        );
        Ok(())
    }

    /// Run `source`'s function `run` with a test host that holds `caps`, on a blocking thread
    /// the way the runtime does.
    async fn run_script(source: &str, caps: &[&str]) -> Result<Value, ScriptError> {
        use crate::kernel::host::test_support::{dummy_ctx, in_memory_media, with_services};
        let script = ScriptProgram::compile(source)?;
        let host = with_services(dummy_ctx(caps), in_memory_media());
        let runtime = tokio::runtime::Handle::current();
        tokio::task::spawn_blocking(move || script.call("run", Value::Null, host, runtime))
            .await
            .map_err(|error| ScriptError::Failed(error.to_string()))?
    }

    #[tokio::test]
    async fn scripts_can_use_the_cache() -> Result<(), Box<dyn std::error::Error>> {
        let result = run_script(
            r#"fn run() {
                cache::set("k", #{ n: 1, tags: ["a"] });
                cache::set("short", 5, 60);
                let seen = cache::get("k");
                let gone = cache::get("missing");
                cache::invalidate("short");
                #{ n: seen.n, tag: seen.tags[0], gone: gone, cleared: cache::clear().removed }
            }"#,
            &["cache::get", "cache::set", "cache::invalidate", "cache::clear"],
        )
        .await?;
        assert_eq!(result, serde_json::json!({ "n": 1, "tag": "a", "gone": null, "cleared": 1 }));
        Ok(())
    }

    #[tokio::test]
    async fn scripts_can_use_storage() -> Result<(), Box<dyn std::error::Error>> {
        let result = run_script(
            r#"fn run() {
                storage::write("notes/a.txt", "hello");
                storage::write_base64("bin", "AAEC");
                let listed = storage::list("notes");
                #{ text: storage::read("notes/a.txt"), bin: storage::read_base64("bin"),
                   listed: listed.len(), first: listed[0].key, all: storage::list().len() }
            }"#,
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
    async fn a_refused_command_can_be_caught_by_the_script() -> Result<(), Box<dyn std::error::Error>> {
        // No capability: the kernel refuses, the script sees an error it can handle.
        let result = run_script(
            r#"fn run() {
                let outcome = "allowed";
                try { cache::get("k"); } catch (e) { outcome = "refused"; }
                outcome
            }"#,
            &[],
        )
        .await?;
        assert_eq!(result, "refused");

        // A host the plugin did not list is refused even with the capability.
        let result = run_script(
            r#"fn run() {
                let outcome = "called";
                try { http::get("https://example.com/"); } catch (e) { outcome = e; }
                outcome
            }"#,
            &["http::request"],
        )
        .await?;
        assert!(result.as_str().is_some_and(|text| text.contains("http_hosts")), "{result}");
        Ok(())
    }

    #[tokio::test]
    async fn unimplemented_commands_are_not_bound_in_scripts() {
        // Not bound is a compile-time-visible gap, not a silent no-op: calling one fails.
        for call in ["bridge::invoke(\"x\", \"y\", #{})"] {
            let source = format!("fn run() {{ {call} }}");
            let result = run_script(&source, &["bridge::call"]).await;
            assert!(result.is_err(), "{call}");
        }
    }
}
