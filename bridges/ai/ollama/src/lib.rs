//! Ollama: language models that run on the organization's own machines. <https://github.com/ollama/ollama/blob/main/docs/api.md>
//!
//! Needs no account. `base_url` is the Ollama server (`http://localhost:11434` by default); a request
//! never leaves the network that server is on, which is why this is the provider to pick when an
//! organization allows only local AI.
//!
//! Actions (used by `ai::complete`, `ai::embed` and `ai::status`, not by `bridge::call`):
//! * `chat { messages, model?, max_tokens?, temperature?, json?, timeout_secs? }` answers an [`aether_ai::ChatResponse`].
//! * `embed { input: [text], model?, timeout_secs? }` answers `{ embeddings, model, usage }`.
//! * `models {}` answers `{ models: [name] }`.

use std::time::Duration;

use aether_ai::{ChatResponse, ToolCall, Usage};
use aether_communication::{Action, ActionBridge, ConfigError, Field, SendError, Spec, TextSink, Values};
use async_trait::async_trait;
use serde_json::{Value, json};

pub const SPEC: Spec = Spec {
    key: "ollama",
    label: "Ollama",
    account: &[],
    options: &[
        Field { name: "base_url", label: "Ollama address", secret: false, required: false, default: Some("http://localhost:11434") },
        Field { name: "default_model", label: "Model used when none is named", secret: false, required: false, default: None },
        Field { name: "default_embed_model", label: "Model used for embeddings", secret: false, required: false, default: None },
    ],
};

pub const ACTIONS: &[Action] = &[
    Action { name: "chat", help: "{ messages, model?, max_tokens?, temperature?, json?, timeout_secs? } -> { text, model, provider, finish_reason, usage }" },
    Action { name: "embed", help: "{ input: [text], model?, timeout_secs? } -> { embeddings, model, usage }" },
    Action { name: "models", help: "{} -> { models: [name] }" },
];

pub struct Ollama {
    base_url: String,
    default_model: Option<String>,
    default_embed_model: Option<String>,
}

fn setting<'a>(values: &'a Values, name: &str) -> Option<&'a str> {
    values.get(name).map(|v| v.trim()).filter(|v| !v.is_empty())
}

impl Ollama {
    pub fn new(values: &Values) -> Result<Self, ConfigError> {
        let base_url = setting(values, "base_url").unwrap_or("http://localhost:11434").trim_end_matches('/').to_string();
        if !(base_url.starts_with("http://") || base_url.starts_with("https://")) {
            return Err(ConfigError("the Ollama address starts with http:// or https://".into()));
        }
        Ok(Self {
            base_url,
            default_model: setting(values, "default_model").map(str::to_string),
            default_embed_model: setting(values, "default_embed_model").map(str::to_string),
        })
    }

    async fn send(&self, method: reqwest::Method, path: &str, body: Option<Value>, timeout: Duration) -> Result<Value, SendError> {
        let mut request = reqwest::Client::new().request(method, format!("{}{path}", self.base_url)).timeout(timeout);
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().await.map_err(|e| {
            SendError::transient(format!(
                "Ollama could not be reached at {} ({})",
                self.base_url,
                if e.is_timeout() { "timed out" } else { "network error" }
            ))
        })?;
        let status = response.status();
        let answer: Value = response.json().await.unwrap_or(Value::Null);
        if !status.is_success() {
            let detail = answer["error"].as_str().unwrap_or("the request was refused");
            return Err(SendError::from_status(status.as_u16(), detail));
        }
        Ok(answer)
    }
}

/// The conversation in Ollama's shape: an assistant turn's tool calls carry their arguments as an object, and a
/// tool turn names the tool it answers.
fn wire_messages(messages: &Value) -> Vec<Value> {
    let mut names: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut out = Vec::new();
    for message in messages.as_array().into_iter().flatten() {
        let content = message["content"].as_str().unwrap_or_default();
        match message["role"].as_str() {
            Some("assistant") if message["tool_calls"].as_array().is_some_and(|c| !c.is_empty()) => {
                let calls: Vec<Value> = message["tool_calls"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|call| {
                        names.insert(call["id"].as_str().unwrap_or_default().to_string(), call["name"].as_str().unwrap_or_default().to_string());
                        json!({ "function": { "name": call["name"], "arguments": call["arguments"] } })
                    })
                    .collect();
                out.push(json!({ "role": "assistant", "content": content, "tool_calls": calls }));
            }
            Some("tool") => {
                let name = message["tool_call_id"].as_str().and_then(|id| names.get(id)).cloned().unwrap_or_default();
                out.push(json!({ "role": "tool", "tool_name": name, "content": content }));
            }
            Some(role) => out.push(json!({ "role": role, "content": content })),
            None => {}
        }
    }
    out
}

fn wire_tools(tools: &Value) -> Vec<Value> {
    tools
        .as_array()
        .into_iter()
        .flatten()
        .map(|t| json!({ "type": "function", "function": { "name": t["name"], "description": t["description"], "parameters": t["schema"] } }))
        .collect()
}

pub fn timeout_of(params: &Value) -> Duration {
    Duration::from_secs(params["timeout_secs"].as_u64().unwrap_or(60).clamp(1, 600))
}

impl Ollama {
    /// The model and request body of a chat; `stream` asks for a body of newline-delimited JSON chunks.
    fn chat_body(&self, params: &Value, stream: bool) -> Result<(String, Value), SendError> {
        let model = params["model"]
            .as_str()
            .or(self.default_model.as_deref())
            .ok_or_else(|| SendError::permanent("no model: name one, or set `ai.model` or the Ollama default model"))?;
        let mut options = serde_json::Map::new();
        if let Some(n) = params["max_tokens"].as_u64() {
            options.insert("num_predict".into(), json!(n));
        }
        if let Some(t) = params["temperature"].as_f64() {
            options.insert("temperature".into(), json!(t));
        }
        let mut body = json!({ "model": model, "messages": wire_messages(&params["messages"]), "stream": stream, "options": options });
        let tools = wire_tools(&params["tools"]);
        if !tools.is_empty() {
            body["tools"] = json!(tools);
        }
        if params["json"].as_bool() == Some(true) {
            body["format"] = json!("json");
        }
        Ok((model.to_string(), body))
    }

    /// A chat answered chunk by chunk: `sink` gets each piece of text; the result is the whole answer.
    async fn chat_stream(&self, params: &Value, sink: &dyn TextSink) -> Result<Value, SendError> {
        let (model, body) = self.chat_body(params, true)?;
        let mut response = reqwest::Client::new()
            .post(format!("{}/api/chat", self.base_url))
            .json(&body)
            .timeout(timeout_of(params))
            .send()
            .await
            .map_err(|e| {
                SendError::transient(format!("Ollama could not be reached at {} ({})", self.base_url, if e.is_timeout() { "timed out" } else { "network error" }))
            })?;
        let status = response.status();
        if !status.is_success() {
            let answer: Value = response.json().await.unwrap_or(Value::Null);
            return Err(SendError::from_status(status.as_u16(), answer["error"].as_str().unwrap_or("the request was refused")));
        }
        let (mut lines, mut text, mut calls, mut last) = (aether_ai::LineBuffer::default(), String::new(), Vec::new(), Value::Null);
        let mut take = |line: String, text: &mut String, calls: &mut Vec<ToolCall>, last: &mut Value| {
            if let Ok(chunk) = serde_json::from_str::<Value>(&line) {
                if let Some(error) = chunk["error"].as_str() {
                    return Err(SendError::transient(format!("Ollama failed while answering: {error}")));
                }
                if let Some(piece) = chunk["message"]["content"].as_str().filter(|p| !p.is_empty()) {
                    text.push_str(piece);
                    sink.emit(piece);
                }
                calls.extend(calls_of(&chunk["message"]["tool_calls"], calls.len()));
                if chunk["done"].as_bool() == Some(true) {
                    *last = chunk;
                }
            }
            Ok(())
        };
        while let Some(bytes) = response.chunk().await.map_err(|_| SendError::transient("the answer from Ollama was cut off"))? {
            for line in lines.push(&bytes) {
                take(line, &mut text, &mut calls, &mut last)?;
            }
        }
        if let Some(line) = lines.finish() {
            take(line, &mut text, &mut calls, &mut last)?;
        }
        if last.is_null() {
            return Err(SendError::transient("the answer from Ollama stopped before it was finished"));
        }
        finish(&last, &model, text, calls)
    }
}

/// Tool calls from a message, numbered from `first` (Ollama gives them no id).
fn calls_of(calls: &Value, first: usize) -> Vec<ToolCall> {
    calls
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
        .filter_map(|(i, call)| {
            Some(ToolCall {
                id: format!("call_{}", first + i),
                name: call["function"]["name"].as_str()?.to_string(),
                // Normally an object; a model that sends text gets it parsed.
                arguments: match &call["function"]["arguments"] {
                    Value::String(text) => serde_json::from_str(text).unwrap_or_else(|_| json!({})),
                    other => other.clone(),
                },
            })
        })
        .collect()
}

/// The common answer from Ollama's final message (the whole reply, or a stream's last chunk).
fn finish(last: &Value, model: &str, text: String, tool_calls: Vec<ToolCall>) -> Result<Value, SendError> {
    let response = ChatResponse {
        text,
        model: last["model"].as_str().unwrap_or(model).to_string(),
        provider: "ollama".into(),
        finish_reason: match last["done_reason"].as_str() {
            _ if !tool_calls.is_empty() => "tool_calls".into(),
            Some("length") => "length".into(),
            Some(other) if other != "stop" => other.to_string(),
            _ => "stop".into(),
        },
        usage: Usage {
            input_tokens: last["prompt_eval_count"].as_u64().unwrap_or(0),
            output_tokens: last["eval_count"].as_u64().unwrap_or(0),
        },
        tool_calls,
    };
    serde_json::to_value(response).map_err(|e| SendError::permanent(e.to_string()))
}

#[async_trait]
impl ActionBridge for Ollama {
    async fn call(&self, action: &str, params: &Value) -> Result<Value, SendError> {
        match action {
            "chat" => {
                let (model, body) = self.chat_body(params, false)?;
                let answer = self.send(reqwest::Method::POST, "/api/chat", Some(body), timeout_of(params)).await?;
                if !answer["message"].is_object() {
                    return Err(SendError::transient("Ollama's answer has no message"));
                }
                let text = answer["message"]["content"].as_str().unwrap_or_default().to_string();
                let calls = calls_of(&answer["message"]["tool_calls"], 0);
                finish(&answer, &model, text, calls)
            }
            "embed" => {
                let model = params["model"]
                    .as_str()
                    .or(self.default_embed_model.as_deref())
                    .ok_or_else(|| SendError::permanent("no embedding model: name one, or set `ai.embed_model`"))?;
                let body = json!({ "model": model, "input": params["input"] });
                let answer = self.send(reqwest::Method::POST, "/api/embed", Some(body), timeout_of(params)).await?;
                if !answer["embeddings"].is_array() {
                    return Err(SendError::transient("Ollama's answer has no embeddings"));
                }
                Ok(json!({
                    "embeddings": answer["embeddings"], "model": answer["model"].as_str().unwrap_or(model),
                    "usage": { "input_tokens": answer["prompt_eval_count"].as_u64().unwrap_or(0), "output_tokens": 0 },
                }))
            }
            "models" => {
                let answer = self.send(reqwest::Method::GET, "/api/tags", None, Duration::from_secs(10)).await?;
                let names: Vec<Value> = answer["models"]
                    .as_array()
                    .map(|list| list.iter().filter_map(|m| m["name"].as_str().map(|n| json!(n))).collect())
                    .unwrap_or_default();
                Ok(json!({ "models": names }))
            }
            other => Err(SendError::permanent(format!(
                "ollama has no action `{other}`; it offers: {}",
                ACTIONS.iter().map(|a| a.name).collect::<Vec<_>>().join(", ")
            ))),
        }
    }

    async fn stream(&self, action: &str, params: &Value, sink: &dyn TextSink) -> Result<Value, SendError> {
        match action {
            "chat" => self.chat_stream(params, sink).await,
            other => self.call(other, params).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_communication::testing::serve_once;

    fn bridge(port: u16) -> Ollama {
        let values: Values = [
            ("base_url".to_string(), format!("http://127.0.0.1:{port}/")),
            ("default_model".to_string(), "llama3.2".to_string()),
        ]
        .into_iter()
        .collect();
        Ollama::new(&values).unwrap_or_else(|e| panic!("{e}"))
    }

    #[tokio::test]
    async fn a_chat_is_sent_non_streaming_and_answered_in_the_common_shape() {
        let reply = r#"{"model":"llama3.2","message":{"role":"assistant","content":"Hi there"},"done_reason":"stop","prompt_eval_count":11,"eval_count":3}"#;
        let (port, seen) = serve_once(200, reply).await;
        let answer = bridge(port)
            .call("chat", &json!({ "messages": [{ "role": "user", "content": "Hello" }], "max_tokens": 50, "temperature": 0.2, "json": true }))
            .await;
        assert_eq!(
            answer,
            Ok(json!({ "text": "Hi there", "model": "llama3.2", "provider": "ollama", "finish_reason": "stop",
                       "usage": { "input_tokens": 11, "output_tokens": 3 } }))
        );
        let request = seen.await.unwrap_or_default();
        assert!(request.starts_with("POST /api/chat "), "{request}");
        for wanted in ["\"stream\":false", "\"num_predict\":50", "\"format\":\"json\"", "\"model\":\"llama3.2\""] {
            assert!(request.contains(wanted), "{wanted} in {request}");
        }
    }

    #[tokio::test]
    async fn a_streamed_chat_hands_out_text_as_it_comes_and_answers_the_whole() {
        let body = concat!(
            r#"{"model":"llama3.2","message":{"role":"assistant","content":"Hel"},"done":false}"#, "\n",
            r#"{"model":"llama3.2","message":{"role":"assistant","content":"lo"},"done":false}"#, "\n",
            r#"{"model":"llama3.2","message":{"role":"assistant","content":""},"done":true,"done_reason":"stop","prompt_eval_count":7,"eval_count":2}"#, "\n",
        );
        let (port, seen) = serve_once(200, body).await;
        let pieces = std::sync::Mutex::new(Vec::<String>::new());
        let sink = |piece: &str| pieces.lock().map(|mut p| p.push(piece.to_string())).unwrap_or(());
        let answer = bridge(port).stream("chat", &json!({ "messages": [{ "role": "user", "content": "Hi" }] }), &sink).await.unwrap_or(Value::Null);
        assert_eq!(answer["text"], "Hello");
        assert_eq!(answer["usage"], json!({ "input_tokens": 7, "output_tokens": 2 }));
        assert_eq!(pieces.lock().map(|p| p.clone()).unwrap_or_default(), vec!["Hel", "lo"]);
        assert!(seen.await.unwrap_or_default().contains("\"stream\":true"));
    }

    #[tokio::test]
    async fn a_stream_that_stops_early_or_reports_an_error_is_a_failure() {
        let (port, _) = serve_once(200, "{\"message\":{\"content\":\"Hel\"},\"done\":false}\n").await;
        let sink = |_: &str| {};
        let cut = bridge(port).stream("chat", &json!({ "messages": [] }), &sink).await.unwrap_err();
        assert!(!cut.permanent && cut.message.contains("stopped"), "{cut:?}");
        let (port, _) = serve_once(200, "{\"error\":\"out of memory\"}\n").await;
        assert!(bridge(port).stream("chat", &json!({ "messages": [] }), &sink).await.unwrap_err().message.contains("out of memory"));
        let (port, _) = serve_once(404, r#"{"error":"model not found"}"#).await;
        assert!(bridge(port).stream("chat", &json!({ "messages": [] }), &sink).await.unwrap_err().permanent);
    }

    #[tokio::test]
    async fn tools_are_offered_and_a_call_comes_back_with_an_id() {
        let reply = r#"{"model":"m","message":{"role":"assistant","content":"","tool_calls":[{"function":{"name":"leave_balance","arguments":{"kind":"annual"}}}]},"done_reason":"stop"}"#;
        let (port, seen) = serve_once(200, reply).await;
        let answer = bridge(port)
            .call(
                "chat",
                &json!({
                    "messages": [
                        { "role": "user", "content": "my leave?" },
                        { "role": "assistant", "content": "", "tool_calls": [{ "id": "c1", "name": "t0", "arguments": { "a": 1 } }] },
                        { "role": "tool", "content": "{\"days\":3}", "tool_call_id": "c1" },
                    ],
                    "tools": [{ "name": "leave_balance", "description": "Days left", "schema": { "type": "object" } }],
                }),
            )
            .await
            .unwrap_or(Value::Null);
        assert_eq!(answer["finish_reason"], "tool_calls");
        assert_eq!(answer["tool_calls"], json!([{ "id": "call_0", "name": "leave_balance", "arguments": { "kind": "annual" } }]));
        let request = seen.await.unwrap_or_default();
        assert!(request.contains("\"type\":\"function\""), "{request}");
        assert!(request.contains("\"tool_name\":\"t0\"") && request.contains("\"parameters\""), "{request}");
    }

    #[tokio::test]
    async fn a_cut_answer_says_so_and_a_server_error_can_be_retried() {
        let (port, _) = serve_once(200, r#"{"message":{"content":"abc"},"done_reason":"length"}"#).await;
        let answer = bridge(port).call("chat", &json!({ "messages": [] })).await.unwrap_or(Value::Null);
        assert_eq!(answer["finish_reason"], "length");
        let (port, _) = serve_once(404, r#"{"error":"model 'x' not found"}"#).await;
        let refused = bridge(port).call("chat", &json!({ "messages": [] })).await.unwrap_err();
        assert!(refused.permanent && refused.message.contains("not found"), "{refused:?}");
        let (port, _) = serve_once(503, "{}").await;
        assert!(!bridge(port).call("chat", &json!({ "messages": [] })).await.unwrap_err().permanent);
    }

    #[tokio::test]
    async fn embeddings_and_models() {
        let (port, seen) = serve_once(200, r#"{"model":"nomic","embeddings":[[0.1,0.2],[0.3,0.4]],"prompt_eval_count":4}"#).await;
        let answer = bridge(port).call("embed", &json!({ "input": ["a", "b"], "model": "nomic" })).await.unwrap_or(Value::Null);
        assert_eq!(answer["embeddings"], json!([[0.1, 0.2], [0.3, 0.4]]));
        assert_eq!(answer["usage"]["input_tokens"], 4);
        assert!(seen.await.unwrap_or_default().starts_with("POST /api/embed "));
        let (port, _) = serve_once(200, r#"{"models":[{"name":"llama3.2:latest"},{"name":"nomic"}]}"#).await;
        assert_eq!(bridge(port).call("models", &json!({})).await, Ok(json!({ "models": ["llama3.2:latest", "nomic"] })));
    }

    #[tokio::test]
    async fn a_missing_model_and_a_bad_address_are_refused() {
        let values: Values = [("base_url".to_string(), "ftp://x".to_string())].into_iter().collect();
        assert!(Ollama::new(&values).is_err());
        let b = Ollama::new(&Values::new()).unwrap_or_else(|e| panic!("{e}"));
        assert!(b.call("chat", &json!({ "messages": [] })).await.unwrap_err().message.contains("no model"));
        assert!(b.call("embed", &json!({ "input": ["a"] })).await.unwrap_err().message.contains("no embedding model"));
        assert!(b.call("nope", &json!({})).await.unwrap_err().permanent);
    }
}
