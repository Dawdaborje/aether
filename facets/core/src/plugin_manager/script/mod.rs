//! Script plugins: a plugin whose code is one script (`[plugin] script = "main.rhai"` or
//! `"main.lua"`) instead of a WebAssembly module. For small plugins that only read and write
//! records, validate input and send notifications; the script is read and checked when the plugin
//! loads and runs inside the same kernel commands, capability checks and model grants as a WASM
//! plugin. It cannot reach anything else: there is no file, network or process access in the
//! language, and every call has an operation, depth, size and time limit.
//!
//! Two languages are supported, picked by the file's extension: [Rhai](rhai) and [Lua](lua)
//! (Luau, the sandboxed Lua). Both take the call's JSON input (or nothing) and return JSON, and
//! both reach other plugins, in any language, through `plugins::call`.
//! `fail("message")` stops the call with a message for the caller; any other error is internal
//! and only logged.

pub mod lua;
pub mod rhai;

use std::path::Path;
use std::time::Duration;

use serde_json::Value;

use crate::kernel::{PluginHostContext, kernel_command};

use super::runtime::USER_ERROR_PREFIX;

/// Operations a single call may run (the counterpart of the WASM fuel limit).
pub const MAX_OPERATIONS: u64 = 2_000_000;
/// How long a single call may run.
pub const MAX_RUN_TIME: Duration = Duration::from_secs(10);
pub(crate) const MAX_CALL_LEVELS: usize = 32;
pub(crate) const MAX_STRING_BYTES: usize = 1024 * 1024;
pub(crate) const MAX_ITEMS: usize = 100_000;
/// Longest script accepted, in bytes.
pub const MAX_SCRIPT_BYTES: u64 = 512 * 1024;

/// Marks an error as meant for the caller.
pub(crate) const USER_MARK: &str = "\u{1}user\u{1}";

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

/// The language of a script plugin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptKind {
    Rhai,
    Lua,
}

impl ScriptKind {
    /// File extensions of script plugins.
    pub const EXTENSIONS: [&'static str; 2] = ["rhai", "lua"];

    pub fn from_extension(extension: &str) -> Option<Self> {
        match extension {
            "rhai" => Some(Self::Rhai),
            "lua" => Some(Self::Lua),
            _ => None,
        }
    }

    /// The language of the script at `path`, if it is a script at all.
    pub fn of_path(path: &Path) -> Option<Self> {
        path.extension().and_then(|extension| extension.to_str()).and_then(Self::from_extension)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Rhai => "Rhai",
            Self::Lua => "Lua",
        }
    }
}

/// A checked script, ready to run. Cheap to share; each call builds its own interpreter.
pub enum ScriptProgram {
    Rhai(rhai::RhaiProgram),
    Lua(lua::LuaProgram),
}

impl ScriptProgram {
    /// Compile a script and check that it is allowed to run.
    pub fn compile(kind: ScriptKind, source: &str) -> Result<Self, ScriptError> {
        match kind {
            ScriptKind::Rhai => rhai::RhaiProgram::compile(source).map(Self::Rhai),
            ScriptKind::Lua => lua::LuaProgram::compile(source).map(Self::Lua),
        }
    }

    pub fn kind(&self) -> ScriptKind {
        match self {
            Self::Rhai(_) => ScriptKind::Rhai,
            Self::Lua(_) => ScriptKind::Lua,
        }
    }

    pub fn has_function(&self, name: &str) -> bool {
        match self {
            Self::Rhai(program) => program.has_function(name),
            Self::Lua(program) => program.has_function(name),
        }
    }

    /// Names of the functions a caller may invoke.
    pub fn function_names(&self) -> Vec<&str> {
        match self {
            Self::Rhai(program) => program.function_names(),
            Self::Lua(program) => program.function_names(),
        }
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
        match self {
            Self::Rhai(program) => program.call(function, input, host, runtime),
            Self::Lua(program) => program.call(function, input, host, runtime),
        }
    }
}

/// Run a kernel command for a script and give back its `data` (or the whole answer when there is
/// none). The error is the text the script sees.
pub(crate) fn kernel_data(
    context: &PluginHostContext,
    runtime: &tokio::runtime::Handle,
    command: &str,
    payload: Value,
) -> Result<Value, String> {
    let answer = runtime.block_on(kernel_command(context, command, payload)).map_err(|error| error.to_string())?;
    Ok(match answer {
        Value::Object(mut object) if object.contains_key("data") => object.remove("data").unwrap_or(Value::Null),
        other => other,
    })
}
