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

/// `db::get`, `db::find`, `db::create`, `db::update`, `db::delete`, `notify::send`,
/// `events::emit` and `context::get`, each the kernel command of the same name.
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
    engine.register_static_module("db", db.into());

    let mut notify = Module::new();
    let h = host.clone();
    notify.set_native_fn("send", move |notification: Map| {
        let payload = to_json(&Dynamic::from_map(notification))?;
        h.data("notify::send", payload)
    });
    engine.register_static_module("notify", notify.into());

    let mut events = Module::new();
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
}
