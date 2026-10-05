//! Plugin commands: what an administrator runs from the command line, like Django's
//! `manage.py` commands.
//!
//! A plugin declares them in `plugin.toml`:
//!
//! ```toml
//! [[command]]
//! name = "import_rates"
//! function = "import_rates"
//! help = "Fetch the exchange rates for a day"
//! args = [{ name = "date", help = "YYYY-MM-DD, default today" }]
//! ```
//!
//! and `aether --command currency.import_rates --org acme --arg date=2026-10-05` runs the plugin's
//! function in that organization. `aether --list-commands --org acme` lists the commands of every
//! plugin installed there, so one plugin's commands are as easy to reach as another's.
//!
//! A command is an ordinary plugin function. It runs as the kernel (`system:cli:<operating system
//! user>`), through the same checks as any call (installed, enabled, the plugin's own capabilities
//! and model grants), and is recorded in the plugin call audit. Because it is a function, it can
//! also be called from code with `plugins::call` (Django's `call_command`), run by the scheduler, or
//! queued as a background job.

use serde_json::{Map, Value};
use surrealdb::types::SurrealValue;

use super::api::run_system_call_as;
use super::models::plugin_def::{CommandArgDef, PluginCommandDef};
use crate::state::AppState;

#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error("database error: {0}")]
    Db(#[from] surrealdb::Error),
    #[error("plugin `{0}` is not installed in this organization")]
    NotInstalled(String),
    #[error("plugin `{plugin}` has no command `{command}`{}", offered(.available))]
    NoSuchCommand { plugin: String, command: String, available: Vec<String> },
    #[error("{0}")]
    BadArguments(String),
    /// The plugin's function failed; the text is its message.
    #[error("{0}")]
    Failed(String),
}

fn offered(names: &[String]) -> String {
    if names.is_empty() {
        " (it offers no commands)".to_string()
    } else {
        format!("; it offers: {}", names.join(", "))
    }
}

/// A command and the plugin version it comes from.
#[derive(Debug, Clone, PartialEq)]
pub struct CommandInfo {
    pub plugin: String,
    pub version: String,
    pub enabled: bool,
    pub def: PluginCommandDef,
}

#[derive(Debug, serde::Deserialize, SurrealValue)]
struct InstalledRow {
    plugin_name: String,
    version: String,
    is_enabled: bool,
}

#[derive(Debug, serde::Deserialize, SurrealValue)]
struct CatalogRow {
    name: String,
    version: String,
    commands: Option<Vec<Value>>,
}

/// The commands of every plugin installed in `org_db`, by plugin and name.
pub async fn list(state: &AppState, org_db: &str) -> Result<Vec<CommandInfo>, CommandError> {
    let org = state.org(org_db).await?;
    let mut response = org
        .query("SELECT plugin_name, version, is_enabled FROM installed_plugins ORDER BY plugin_name;")
        .await?
        .check()?;
    let installed: Vec<InstalledRow> = response.take(0)?;
    if installed.is_empty() {
        return Ok(Vec::new());
    }
    let core = state.core().await?;
    let names: Vec<String> = installed.iter().map(|row| row.plugin_name.clone()).collect();
    let mut response = core
        .query("SELECT name, version, commands FROM plugins WHERE name IN $names;")
        .bind(("names", names))
        .await?
        .check()?;
    let catalog: Vec<CatalogRow> = response.take(0)?;
    let mut found = Vec::new();
    for row in installed {
        let Some(version) = catalog.iter().find(|c| c.name == row.plugin_name && c.version == row.version) else {
            continue;
        };
        for value in version.commands.clone().unwrap_or_default() {
            if let Ok(def) = serde_json::from_value::<PluginCommandDef>(value) {
                found.push(CommandInfo {
                    plugin: row.plugin_name.clone(),
                    version: row.version.clone(),
                    enabled: row.is_enabled,
                    def,
                });
            }
        }
    }
    found.sort_by(|a, b| (&a.plugin, &a.def.name).cmp(&(&b.plugin, &b.def.name)));
    Ok(found)
}

/// Turn `key=value` arguments (and optionally a JSON object) into the function's input, checking
/// them against the command's declaration: unknown names are refused, required ones must be there,
/// text is converted to the declared type and defaults fill the rest. A command that declares no
/// arguments takes whatever JSON it is given.
pub fn build_payload(def: &PluginCommandDef, args: &[String], json: Option<&str>) -> Result<Value, CommandError> {
    let bad = |message: String| CommandError::BadArguments(message);
    let mut given = Map::new();
    if let Some(text) = json {
        match serde_json::from_str::<Value>(text) {
            Ok(Value::Object(object)) => given = object,
            Ok(_) => return Err(bad("--json must be a JSON object".into())),
            Err(error) => return Err(bad(format!("--json is not valid JSON: {error}"))),
        }
    }
    if def.args.is_empty() && args.is_empty() {
        return Ok(Value::Object(given));
    }
    let mut from_line: Vec<(String, String)> = Vec::new();
    for arg in args {
        let (key, value) = arg
            .split_once('=')
            .ok_or_else(|| bad(format!("`{arg}` is not key=value (use --arg key=value)")))?;
        from_line.push((key.trim().to_string(), value.to_string()));
    }
    if def.args.is_empty() {
        return Err(bad(format!("`{}` takes no arguments; use --json for free-form input", def.name)));
    }
    let known: Vec<&str> = def.args.iter().map(|a| a.name.as_str()).collect();
    let unknown = |key: &str| bad(format!("`{key}` is not an argument of `{}`; it takes: {}", def.name, known.join(", ")));
    for key in given.keys() {
        if !known.contains(&key.as_str()) {
            return Err(unknown(key));
        }
    }
    for (key, text) in from_line {
        let spec: &CommandArgDef = def.args.iter().find(|a| a.name == key).ok_or_else(|| unknown(&key))?;
        if given.contains_key(&key) {
            return Err(bad(format!("`{key}` is given twice")));
        }
        given.insert(key, convert(spec, &text)?);
    }
    for spec in &def.args {
        if given.contains_key(&spec.name) {
            continue;
        }
        match (&spec.default, spec.required) {
            (Some(default), _) => {
                given.insert(spec.name.clone(), default.clone());
            }
            (None, true) => return Err(bad(format!("`{}` is required (--arg {}=…)", spec.name, spec.name))),
            (None, false) => {}
        }
    }
    Ok(Value::Object(given))
}

fn convert(spec: &CommandArgDef, text: &str) -> Result<Value, CommandError> {
    let bad = |what: &str| CommandError::BadArguments(format!("`{}` must be {what}, not `{text}`", spec.name));
    Ok(match spec.kind() {
        "int" => Value::from(text.trim().parse::<i64>().map_err(|_| bad("a whole number"))?),
        "float" => {
            let number: f64 = text.trim().parse().map_err(|_| bad("a number"))?;
            serde_json::Number::from_f64(number).map(Value::Number).ok_or_else(|| bad("a finite number"))?
        }
        "bool" => match text.trim().to_ascii_lowercase().as_str() {
            "true" | "yes" | "1" | "on" => Value::Bool(true),
            "false" | "no" | "0" | "off" => Value::Bool(false),
            _ => return Err(bad("true or false")),
        },
        "json" => serde_json::from_str(text).map_err(|_| bad("valid JSON"))?,
        _ => Value::String(text.to_string()),
    })
}

/// Run `plugin.command` in `org_db` and return the function's result. `actor` names who is
/// running it for the audit trail (`system:cli:ann`).
pub async fn run(
    state: &AppState,
    org_db: &str,
    plugin: &str,
    command: &str,
    args: &[String],
    json: Option<&str>,
    actor: &str,
) -> Result<Value, CommandError> {
    let all = list(state, org_db).await?;
    let theirs: Vec<&CommandInfo> = all.iter().filter(|info| info.plugin == plugin).collect();
    let Some(info) = theirs.iter().find(|info| info.def.name == command) else {
        let installed = state
            .org(org_db)
            .await?
            .query("SELECT VALUE plugin_name FROM installed_plugins WHERE plugin_name = $name;")
            .bind(("name", plugin.to_string()))
            .await?
            .check()?
            .take::<Vec<String>>(0)?;
        if installed.is_empty() {
            return Err(CommandError::NotInstalled(plugin.to_string()));
        }
        return Err(CommandError::NoSuchCommand {
            plugin: plugin.to_string(),
            command: command.to_string(),
            available: theirs.iter().map(|info| info.def.name.clone()).collect(),
        });
    };
    let payload = build_payload(&info.def, args, json)?;
    let request_id = format!("cmd-{}", crate::access::audit::new_request_id());
    run_system_call_as(state, org_db, actor, plugin, &info.def.function, payload, &request_id)
        .await
        .map_err(|error| CommandError::Failed(error.message))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn def() -> PluginCommandDef {
        serde_json::from_value(json!({
            "name": "import", "function": "import", "help": "h",
            "args": [
                { "name": "date" },
                { "name": "days", "type": "int", "default": 7 },
                { "name": "rate", "type": "float" },
                { "name": "force", "type": "bool" },
                { "name": "tags", "type": "json" },
                { "name": "source", "required": true }
            ]
        }))
        .unwrap_or_else(|error| panic!("test command: {error}"))
    }

    fn payload(args: &[&str], json: Option<&str>) -> Result<Value, CommandError> {
        build_payload(&def(), &args.iter().map(|a| a.to_string()).collect::<Vec<_>>(), json)
    }

    #[test]
    fn text_is_converted_to_each_declared_type_and_defaults_fill_in() -> Result<(), CommandError> {
        let value = payload(&["source=ecb", "date=2026-10-05", "days=3", "rate=1.5", "force=yes", "tags=[\"a\",\"b\"]"], None)?;
        assert_eq!(
            value,
            json!({ "source": "ecb", "date": "2026-10-05", "days": 3, "rate": 1.5, "force": true, "tags": ["a", "b"] })
        );
        // Only what is needed, with the default.
        assert_eq!(payload(&["source=ecb"], None)?, json!({ "source": "ecb", "days": 7 }));
        // A value may contain `=`.
        assert_eq!(payload(&["source=a=b"], None)?["source"], "a=b");
        Ok(())
    }

    #[test]
    fn json_and_flags_combine_but_never_overlap() -> Result<(), CommandError> {
        assert_eq!(payload(&["date=2026-10-05"], Some(r#"{"source":"ecb"}"#))?, json!({ "source": "ecb", "date": "2026-10-05", "days": 7 }));
        assert!(payload(&["source=a"], Some(r#"{"source":"b"}"#)).is_err(), "given twice");
        assert!(payload(&[], Some("[1]")).is_err());
        assert!(payload(&[], Some("{oops")).is_err());
        Ok(())
    }

    #[test]
    fn mistakes_say_what_is_wrong() {
        let message = |args: &[&str]| payload(args, None).err().map(|e| e.to_string()).unwrap_or_default();
        assert!(message(&[]).contains("`source` is required"), "{}", message(&[]));
        assert!(message(&["source=a", "colour=red"]).contains("not an argument") && message(&["source=a", "colour=red"]).contains("source"));
        assert!(message(&["source=a", "days=many"]).contains("whole number"));
        assert!(message(&["source=a", "rate=nan"]).contains("number"));
        assert!(message(&["source=a", "force=maybe"]).contains("true or false"));
        assert!(message(&["source=a", "tags={"]).contains("valid JSON"));
        assert!(message(&["nonsense"]).contains("key=value"));
    }

    #[test]
    fn a_command_without_declared_arguments_takes_free_json_only() -> Result<(), CommandError> {
        let bare: PluginCommandDef = serde_json::from_value(json!({ "name": "go", "function": "go" })).map_err(|e| CommandError::BadArguments(e.to_string()))?;
        assert_eq!(build_payload(&bare, &[], None)?, json!({}));
        assert_eq!(build_payload(&bare, &[], Some(r#"{"anything":1}"#))?, json!({ "anything": 1 }));
        assert!(build_payload(&bare, &["a=1".to_string()], None).is_err(), "flags need declared arguments");
        Ok(())
    }
}
