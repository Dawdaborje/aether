//! `aether setup-mcp`: register `aether --mcp` with the LLM clients installed on this machine, once, for every
//! project. The server reads the `aether.toml` of the folder the client starts it in, so a plugin workspace gets
//! its own plugin docs without any per-project setup.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Map, Value, json};

const SERVER_NAME: &str = "aether";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Client {
    ClaudeCode,
    Cursor,
}

impl Client {
    fn label(self) -> &'static str {
        match self {
            Client::ClaudeCode => "Claude Code",
            Client::Cursor => "Cursor",
        }
    }
}

/// Which clients `--mcp-client` names: `claude`, `cursor`, or `all` (the default, only the ones found).
pub fn parse_clients(choice: &str) -> Result<Vec<(Client, bool)>, String> {
    let home = home()?;
    let found_claude = home.join(".claude.json").is_file() || home.join(".claude").is_dir();
    let found_cursor = home.join(".cursor").is_dir();
    match choice.trim().to_ascii_lowercase().as_str() {
        "" | "all" => {
            let list: Vec<_> = [(Client::ClaudeCode, found_claude), (Client::Cursor, found_cursor)].into_iter().filter(|(_, f)| *f).collect();
            if list.is_empty() {
                return Err("found neither Claude Code nor Cursor; name one with --mcp-client claude|cursor".into());
            }
            Ok(list)
        }
        "claude" | "claude-code" => Ok(vec![(Client::ClaudeCode, true)]),
        "cursor" => Ok(vec![(Client::Cursor, true)]),
        other => Err(format!("unknown client '{other}': use claude, cursor or all")),
    }
}

pub fn run(clients: Vec<(Client, bool)>, remove: bool, allow_write_hint: bool) -> Result<(), String> {
    let exe = std::env::current_exe().and_then(|p| p.canonicalize()).map_err(|e| format!("cannot find this binary: {e}"))?;
    let mut failed = false;
    for (client, _) in clients {
        let result = if remove { unregister(client) } else { register(client, &exe) };
        match result {
            Ok(message) => println!("{}: {message}", client.label()),
            Err(e) => {
                failed = true;
                eprintln!("{}: {e}", client.label());
            }
        }
    }
    if !remove && !failed {
        println!("Restart the client (or reload its MCP servers). It starts `{} --mcp` in each project folder, so the `plugin_paths` of that folder's aether.toml are read.", exe.display());
        if allow_write_hint {
            println!("Live tools (list_installed_plugins and the rest) need `--org <org>` in the server arguments; edit the entry if you want them.");
        }
    }
    if failed { Err("some clients could not be set up".into()) } else { Ok(()) }
}

fn home() -> Result<PathBuf, String> {
    directories::BaseDirs::new().map(|d| d.home_dir().to_path_buf()).ok_or_else(|| "cannot find the home directory".to_string())
}

fn register(client: Client, exe: &Path) -> Result<String, String> {
    match client {
        Client::ClaudeCode => {
            // The `claude` CLI owns ~/.claude.json, which it rewrites while running; prefer it to editing the file.
            let ran = Command::new("claude")
                .args(["mcp", "add", "--scope", "user", SERVER_NAME, "--"])
                .arg(exe)
                .arg("--mcp")
                .output();
            match ran {
                Ok(out) if out.status.success() => return Ok("added to the user scope (all projects)".into()),
                Ok(out) => {
                    let text = format!("{}{}", String::from_utf8_lossy(&out.stderr), String::from_utf8_lossy(&out.stdout));
                    if text.contains("already exists") {
                        // Replace a stale entry (the binary may have moved).
                        let _ = Command::new("claude").args(["mcp", "remove", "--scope", "user", SERVER_NAME]).output();
                        let again = Command::new("claude").args(["mcp", "add", "--scope", "user", SERVER_NAME, "--"]).arg(exe).arg("--mcp").output();
                        if matches!(&again, Ok(o) if o.status.success()) {
                            return Ok("updated in the user scope (all projects)".into());
                        }
                    }
                    log::warn!("`claude mcp add` failed ({}); editing ~/.claude.json instead", text.trim());
                }
                Err(_) => {}
            }
            edit_json(&home()?.join(".claude.json"), exe, false)
        }
        Client::Cursor => edit_json(&home()?.join(".cursor").join("mcp.json"), exe, false),
    }
}

fn unregister(client: Client) -> Result<String, String> {
    match client {
        Client::ClaudeCode => {
            if let Ok(out) = Command::new("claude").args(["mcp", "remove", "--scope", "user", SERVER_NAME]).output() {
                if out.status.success() {
                    return Ok("removed".into());
                }
            }
            edit_json(&home()?.join(".claude.json"), Path::new(""), true)
        }
        Client::Cursor => edit_json(&home()?.join(".cursor").join("mcp.json"), Path::new(""), true),
    }
}

/// Add or remove `mcpServers.aether` in a client's JSON file, leaving everything else as it was. Written through a
/// temporary file and renamed, with a `.aether-backup` copy of the original next to it.
fn edit_json(path: &Path, exe: &Path, remove: bool) -> Result<String, String> {
    let mut root: Value = match std::fs::read_to_string(path) {
        Ok(text) if !text.trim().is_empty() => serde_json::from_str(&text).map_err(|e| format!("{} is not valid JSON ({e}); not touching it", path.display()))?,
        _ => Value::Object(Map::new()),
    };
    let object = root.as_object_mut().ok_or_else(|| format!("{} is not a JSON object", path.display()))?;
    let servers = object.entry("mcpServers").or_insert_with(|| Value::Object(Map::new()));
    let servers = servers.as_object_mut().ok_or_else(|| format!("`mcpServers` in {} is not an object", path.display()))?;
    let message = if remove {
        if servers.remove(SERVER_NAME).is_none() {
            return Ok("was not set up".into());
        }
        format!("removed from {}", path.display())
    } else {
        let existed = servers.insert(SERVER_NAME.into(), json!({ "command": exe, "args": ["--mcp"] })).is_some();
        format!("{} in {} (all projects)", if existed { "updated" } else { "added" }, path.display())
    };
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    }
    if path.is_file() {
        let backup = path.with_extension("json.aether-backup");
        std::fs::copy(path, &backup).map_err(|e| format!("cannot back up {}: {e}", path.display()))?;
    }
    let tmp = path.with_extension("json.aether-tmp");
    let text = serde_json::to_string_pretty(&root).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, text + "\n").map_err(|e| format!("cannot write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("cannot replace {}: {e}", path.display()))?;
    Ok(message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adds_and_removes_keeping_other_servers() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("mcp.json");
        std::fs::write(&file, r#"{"theme":"x","mcpServers":{"other":{"command":"o"}}}"#).unwrap();
        edit_json(&file, Path::new("/bin/aether"), false).unwrap();
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(v["mcpServers"]["aether"]["args"][0], "--mcp");
        assert_eq!(v["mcpServers"]["other"]["command"], "o");
        assert_eq!(v["theme"], "x");
        edit_json(&file, Path::new(""), true).unwrap();
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        assert!(v["mcpServers"].get("aether").is_none());
        assert_eq!(v["mcpServers"]["other"]["command"], "o");
    }
}
