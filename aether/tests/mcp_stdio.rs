//! Runs the real `aether --mcp` binary and talks to it the way an MCP client does.
//! stdout must carry nothing but JSON-RPC messages, one per line.
#![cfg(feature = "mcp")]

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

/// Sends `requests` (after the handshake) and returns the responses by id.
fn session(requests: Vec<Value>) -> Vec<Value> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_aether"))
        .arg("--mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .current_dir(std::env::temp_dir()) // no aether.toml, no checkout: the docs travel in the binary
        .spawn()
        .expect("start aether --mcp");
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());

    let send = |stdin: &mut std::process::ChildStdin, v: Value| writeln!(stdin, "{v}").unwrap();
    send(&mut stdin, json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
        "protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"0"}}}));
    send(&mut stdin, json!({"jsonrpc":"2.0","method":"notifications/initialized"}));
    let wanted = requests.len();
    for r in requests {
        send(&mut stdin, r);
    }
    stdin.flush().unwrap();

    let mut responses = Vec::new();
    while responses.len() < wanted + 1 {
        let mut line = String::new();
        assert!(stdout.read_line(&mut line).unwrap() > 0, "server closed early; got {responses:?}");
        let message: Value = serde_json::from_str(&line).unwrap_or_else(|e| panic!("stdout must be JSON-RPC only, got {line:?}: {e}"));
        assert_eq!(message["jsonrpc"], "2.0", "{line}");
        if message.get("id").is_some() {
            responses.push(message);
        }
    }
    drop(stdin);
    let _ = child.kill();
    let _ = child.wait();
    responses
}

fn call(id: u64, name: &str, arguments: Value) -> Value {
    json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":name,"arguments":arguments}})
}

fn text(response: &Value) -> &str {
    response["result"]["content"][0]["text"].as_str().unwrap_or_else(|| panic!("no text in {response}"))
}

fn by_id(responses: &[Value], id: u64) -> &Value {
    responses.iter().find(|r| r["id"] == id).unwrap_or_else(|| panic!("no response {id}"))
}

#[test]
fn handshake_lists_tools_resources_and_prompts() {
    let r = session(vec![
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        json!({"jsonrpc":"2.0","id":3,"method":"resources/list"}),
        json!({"jsonrpc":"2.0","id":4,"method":"prompts/list"}),
    ]);
    let init = by_id(&r, 1);
    assert!(init["result"]["instructions"].as_str().unwrap().contains("PROPOSAL"));
    for cap in ["tools", "resources", "prompts"] {
        assert!(init["result"]["capabilities"].get(cap).is_some(), "{cap}");
    }

    let tools: Vec<&str> = by_id(&r, 2)["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
    for want in ["search_docs", "get_doc", "list_docs", "list_capabilities", "get_capability", "get_reference", "get_schema", "scaffold_plugin", "validate_plugin"] {
        assert!(tools.contains(&want), "missing tool {want}: {tools:?}");
    }
    let resources = by_id(&r, 3)["result"]["resources"].as_array().unwrap();
    assert!(resources.len() > 50);
    assert!(resources.iter().any(|x| x["uri"] == "aether-docs://plugin/pages"));
    let prompts = by_id(&r, 4)["result"]["prompts"].as_array().unwrap();
    assert_eq!(prompts.len(), 2);
}

#[test]
fn reads_docs_by_tool_and_by_resource() {
    let r = session(vec![
        call(2, "search_docs", json!({"query": "scheduler recurring jobs"})),
        call(3, "get_doc", json!({"path": "architecture/history"})),
        json!({"jsonrpc":"2.0","id":4,"method":"resources/read","params":{"uri":"aether-docs://plugin/commands"}}),
        json!({"jsonrpc":"2.0","id":5,"method":"prompts/get","params":{"name":"build_plugin","arguments":{"goal":"track expenses"}}}),
        call(6, "get_doc", json!({"path": "no/such/page"})),
    ]);
    assert!(text(by_id(&r, 2)).contains("architecture/scheduler"));
    assert!(text(by_id(&r, 3)).contains("PROPOSAL, NOT BUILT"));
    let resource = &by_id(&r, 4)["result"]["contents"][0];
    assert!(resource["text"].as_str().unwrap().contains("Kernel commands"));
    assert_eq!(resource["mimeType"], "text/markdown");
    let prompt = by_id(&r, 5)["result"]["messages"][0]["content"]["text"].as_str().unwrap();
    assert!(prompt.contains("track expenses") && prompt.contains("validate_plugin"));
    let missing = by_id(&r, 6);
    assert_eq!(missing["result"]["isError"], true, "{missing}");
}

#[test]
fn builds_and_checks_a_plugin_without_any_database() {
    let dir = tempfile::tempdir().unwrap();
    let r = session(vec![call(2, "scaffold_plugin", json!({"name": "expense_tool", "language": "lua"}))]);
    let files = text(by_id(&r, 2)).to_owned();
    let root = dir.path().join("expense_tool");
    for part in files.split("\n=== FILE: ").skip(1) {
        let (path, rest) = part.split_once(" ===\n").unwrap();
        let content = rest.split("\n=== END FILE ===").next().unwrap();
        std::fs::create_dir_all(root.join(path).parent().unwrap()).unwrap();
        std::fs::write(root.join(path), content).unwrap();
    }

    let path = root.to_str().unwrap();
    let ok = session(vec![call(2, "validate_plugin", json!({"path": path}))]);
    assert!(text(by_id(&ok, 2)).starts_with("VALID"), "{}", text(by_id(&ok, 2)));

    // Break it: the manifest now names a script that does not exist.
    let manifest = std::fs::read_to_string(root.join("plugin.toml")).unwrap().replace("main.lua", "gone.lua");
    std::fs::write(root.join("plugin.toml"), manifest).unwrap();
    let bad = session(vec![call(2, "validate_plugin", json!({"path": path}))]);
    let bad = by_id(&bad, 2);
    assert_eq!(bad["result"]["isError"], true);
    assert!(text(bad).starts_with("INVALID") && text(bad).contains("gone.lua"), "{}", text(bad));
}
