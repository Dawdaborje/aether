//! Any server that speaks the OpenAI chat API: OpenAI itself, Azure OpenAI's compatible endpoint, OpenRouter,
//! vLLM, LM Studio, llama.cpp's server, Groq and the like. <https://platform.openai.com/docs/api-reference>
//!
//! `base_url` is the API root including its version (`https://api.openai.com/v1`). `api_key` is sent as a
//! bearer token and may be left empty for a local server that wants none.
//!
//! Actions (used by `ai::complete`, `ai::embed` and `ai::status`, not by `bridge::call`):
//! * `chat { messages, model?, max_tokens?, temperature?, json?, timeout_secs? }` answers an [`aether_ai::ChatResponse`].
//! * `embed { input: [text], model?, timeout_secs? }` answers `{ embeddings, model, usage }`.
//! * `models {}` answers `{ models: [id] }`.

use std::time::Duration;

use aether_ai::{ChatResponse, ToolCall, Usage};
use aether_communication::{Action, ActionBridge, ConfigError, Field, SendError, Spec, TextSink, Values};
use async_trait::async_trait;
use serde_json::{Value, json};

pub const SPEC: Spec = Spec {
    key: "openai_compatible",
    label: "OpenAI-compatible",
    account: &[Field { name: "api_key", label: "API key", secret: true, required: false, default: None }],
    options: &[
        Field { name: "base_url", label: "API address", secret: false, required: false, default: Some("https://api.openai.com/v1") },
        Field { name: "default_model", label: "Model used when none is named", secret: false, required: false, default: None },
        Field { name: "default_embed_model", label: "Model used for embeddings", secret: false, required: false, default: None },
    ],
};

pub const ACTIONS: &[Action] = &[
    Action { name: "chat", help: "{ messages, model?, max_tokens?, temperature?, json?, timeout_secs? } -> { text, model, provider, finish_reason, usage }" },
    Action { name: "embed", help: "{ input: [text], model?, timeout_secs? } -> { embeddings, model, usage }" },
    Action { name: "models", help: "{} -> { models: [id] }" },
];

pub struct OpenAiCompatible {
    base_url: String,
    api_key: Option<String>,
    default_model: Option<String>,
    default_embed_model: Option<String>,
}

fn setting<'a>(values: &'a Values, name: &str) -> Option<&'a str> {
    values.get(name).map(|v| v.trim()).filter(|v| !v.is_empty())
}

impl OpenAiCompatible {
    pub fn new(values: &Values) -> Result<Self, ConfigError> {
        let base_url = setting(values, "base_url").unwrap_or("https://api.openai.com/v1").trim_end_matches('/').to_string();
        if !(base_url.starts_with("http://") || base_url.starts_with("https://")) {
            return Err(ConfigError("the API address starts with http:// or https://".into()));
        }
        Ok(Self {
            base_url,
            api_key: setting(values, "api_key").map(str::to_string),
            default_model: setting(values, "default_model").map(str::to_string),
            default_embed_model: setting(values, "default_embed_model").map(str::to_string),
        })
    }

    async fn send(&self, method: reqwest::Method, path: &str, body: Option<Value>, timeout: Duration) -> Result<Value, SendError> {
        let mut request = reqwest::Client::new().request(method, format!("{}{path}", self.base_url)).timeout(timeout);
        if let Some(key) = &self.api_key {
            request = request.bearer_auth(key);
        }
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().await.map_err(|e| {
            SendError::transient(format!("the AI server could not be reached ({})", if e.is_timeout() { "timed out" } else { "network error" }))
        })?;
        let status = response.status();
        let answer: Value = response.json().await.unwrap_or(Value::Null);
        if !status.is_success() {
            // The provider's own words, never the request: it holds the prompt.
            let detail = answer["error"]["message"].as_str().unwrap_or("the request was refused");
            return Err(SendError::from_status(status.as_u16(), detail));
        }
        Ok(answer)
    }
}

/// The conversation in the API's shape: tool calls carry their arguments as JSON text, and a tool turn names the call it answers.
fn wire_messages(messages: &Value) -> Vec<Value> {
    messages
        .as_array()
        .into_iter()
        .flatten()
        .map(|message| {
            let content = message["content"].as_str().unwrap_or_default();
            match message["role"].as_str() {
                Some("assistant") if message["tool_calls"].as_array().is_some_and(|c| !c.is_empty()) => {
                    let calls: Vec<Value> = message["tool_calls"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(|c| json!({ "id": c["id"], "type": "function", "function": { "name": c["name"], "arguments": c["arguments"].to_string() } }))
                        .collect();
                    json!({ "role": "assistant", "content": if content.is_empty() { Value::Null } else { json!(content) }, "tool_calls": calls })
                }
                Some("tool") => json!({ "role": "tool", "tool_call_id": message["tool_call_id"], "content": content }),
                role => json!({ "role": role.unwrap_or("user"), "content": content }),
            }
        })
        .collect()
}

fn timeout_of(params: &Value) -> Duration {
    Duration::from_secs(params["timeout_secs"].as_u64().unwrap_or(60).clamp(1, 600))
}

impl OpenAiCompatible {
    /// The model and request body of a chat; `stream` asks for server-sent chunks (with a usage count at the end).
    fn chat_body(&self, params: &Value, stream: bool) -> Result<(String, Value), SendError> {
        let model = params["model"]
            .as_str()
            .or(self.default_model.as_deref())
            .ok_or_else(|| SendError::permanent("no model: name one, or set `ai.model`"))?;
        let mut body = json!({ "model": model, "messages": wire_messages(&params["messages"]) });
        if let Some(tools) = params["tools"].as_array().filter(|t| !t.is_empty()) {
            body["tools"] = json!(tools
                .iter()
                .map(|t| json!({ "type": "function", "function": { "name": t["name"], "description": t["description"], "parameters": t["schema"] } }))
                .collect::<Vec<_>>());
        }
        if let Some(n) = params["max_tokens"].as_u64() {
            body["max_tokens"] = json!(n);
        }
        if let Some(t) = params["temperature"].as_f64() {
            body["temperature"] = json!(t);
        }
        if params["json"].as_bool() == Some(true) {
            body["response_format"] = json!({ "type": "json_object" });
        }
        if stream {
            body["stream"] = json!(true);
            body["stream_options"] = json!({ "include_usage": true });
        }
        Ok((model.to_string(), body))
    }

    async fn chat_stream(&self, params: &Value, sink: &dyn TextSink) -> Result<Value, SendError> {
        let (model, body) = self.chat_body(params, true)?;
        let mut request = reqwest::Client::new().post(format!("{}/chat/completions", self.base_url)).json(&body).timeout(timeout_of(params));
        if let Some(key) = &self.api_key {
            request = request.bearer_auth(key);
        }
        let mut response = request.send().await.map_err(|e| {
            SendError::transient(format!("the AI server could not be reached ({})", if e.is_timeout() { "timed out" } else { "network error" }))
        })?;
        let status = response.status();
        if !status.is_success() {
            let answer: Value = response.json().await.unwrap_or(Value::Null);
            return Err(SendError::from_status(status.as_u16(), answer["error"]["message"].as_str().unwrap_or("the request was refused")));
        }
        // Tool calls arrive in pieces, keyed by `index`: the id and name first, then the arguments' text.
        let mut state = StreamState::default();
        let mut lines = aether_ai::LineBuffer::default();
        let mut done = false;
        while let Some(bytes) = response.chunk().await.map_err(|_| SendError::transient("the answer from the AI server was cut off"))? {
            for line in lines.push(&bytes) {
                done |= state.take(&line, sink)?;
            }
        }
        if let Some(line) = lines.finish() {
            done |= state.take(&line, sink)?;
        }
        if !done && state.finish_reason.is_none() {
            return Err(SendError::transient("the answer from the AI server stopped before it was finished"));
        }
        let calls = state.calls();
        finish(&model, state.model.as_deref(), state.text, calls, state.finish_reason.as_deref(), state.input, state.output)
    }
}

#[derive(Default)]
struct StreamState {
    text: String,
    model: Option<String>,
    finish_reason: Option<String>,
    input: u64,
    output: u64,
    /// `(id, name, arguments text)` by the call's index.
    pending: std::collections::BTreeMap<u64, (String, String, String)>,
}

impl StreamState {
    /// One line of the body. `true` when it was the end marker.
    fn take(&mut self, line: &str, sink: &dyn TextSink) -> Result<bool, SendError> {
        let Some(data) = line.strip_prefix("data:").map(str::trim) else { return Ok(false) };
        if data == "[DONE]" {
            return Ok(true);
        }
        let Ok(chunk) = serde_json::from_str::<Value>(data) else { return Ok(false) };
        if let Some(message) = chunk["error"]["message"].as_str() {
            return Err(SendError::transient(format!("the AI server failed while answering: {message}")));
        }
        if let Some(model) = chunk["model"].as_str() {
            self.model = Some(model.to_string());
        }
        if let Some(n) = chunk["usage"]["prompt_tokens"].as_u64() {
            self.input = n;
        }
        if let Some(n) = chunk["usage"]["completion_tokens"].as_u64() {
            self.output = n;
        }
        let choice = &chunk["choices"][0];
        if let Some(reason) = choice["finish_reason"].as_str() {
            self.finish_reason = Some(reason.to_string());
        }
        if let Some(piece) = choice["delta"]["content"].as_str().filter(|p| !p.is_empty()) {
            self.text.push_str(piece);
            sink.emit(piece);
        }
        for call in choice["delta"]["tool_calls"].as_array().into_iter().flatten() {
            let entry = self.pending.entry(call["index"].as_u64().unwrap_or(0)).or_default();
            if let Some(id) = call["id"].as_str() {
                entry.0 = id.to_string();
            }
            if let Some(name) = call["function"]["name"].as_str() {
                entry.1.push_str(name);
            }
            if let Some(piece) = call["function"]["arguments"].as_str() {
                entry.2.push_str(piece);
            }
        }
        Ok(false)
    }

    fn calls(&self) -> Vec<ToolCall> {
        self.pending
            .iter()
            .filter(|(_, (_, name, _))| !name.is_empty())
            .map(|(index, (id, name, arguments))| ToolCall {
                id: if id.is_empty() { format!("call_{index}") } else { id.clone() },
                name: name.clone(),
                arguments: arguments_of(&Value::String(arguments.clone())),
            })
            .collect()
    }
}

/// Arguments arrive as JSON text; text that is not JSON becomes no arguments.
fn arguments_of(value: &Value) -> Value {
    match value {
        Value::String(text) if text.trim().is_empty() => json!({}),
        Value::String(text) => serde_json::from_str(text).unwrap_or_else(|_| json!({})),
        other => other.clone(),
    }
}

fn finish(
    model: &str,
    answered_by: Option<&str>,
    text: String,
    tool_calls: Vec<ToolCall>,
    finish_reason: Option<&str>,
    input_tokens: u64,
    output_tokens: u64,
) -> Result<Value, SendError> {
    let response = ChatResponse {
        text,
        model: answered_by.unwrap_or(model).to_string(),
        provider: "openai_compatible".into(),
        finish_reason: match finish_reason {
            _ if !tool_calls.is_empty() => "tool_calls".into(),
            Some("length") => "length".into(),
            Some("stop") | None => "stop".into(),
            Some(other) => other.to_string(),
        },
        usage: Usage { input_tokens, output_tokens },
        tool_calls,
    };
    serde_json::to_value(response).map_err(|e| SendError::permanent(e.to_string()))
}

#[async_trait]
impl ActionBridge for OpenAiCompatible {
    async fn call(&self, action: &str, params: &Value) -> Result<Value, SendError> {
        match action {
            "chat" => {
                let (model, body) = self.chat_body(params, false)?;
                let answer = self.send(reqwest::Method::POST, "/chat/completions", Some(body), timeout_of(params)).await?;
                let choice = &answer["choices"][0];
                if !choice["message"].is_object() {
                    return Err(SendError::transient("the AI server's answer has no message"));
                }
                let text = choice["message"]["content"].as_str().unwrap_or_default().to_string();
                let calls: Vec<ToolCall> = choice["message"]["tool_calls"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .enumerate()
                    .filter_map(|(i, call)| {
                        Some(ToolCall {
                            id: call["id"].as_str().map_or_else(|| format!("call_{i}"), str::to_string),
                            name: call["function"]["name"].as_str()?.to_string(),
                            arguments: arguments_of(&call["function"]["arguments"]),
                        })
                    })
                    .collect();
                finish(
                    &model,
                    answer["model"].as_str(),
                    text,
                    calls,
                    choice["finish_reason"].as_str(),
                    answer["usage"]["prompt_tokens"].as_u64().unwrap_or(0),
                    answer["usage"]["completion_tokens"].as_u64().unwrap_or(0),
                )
            }
            "embed" => {
                let model = params["model"]
                    .as_str()
                    .or(self.default_embed_model.as_deref())
                    .ok_or_else(|| SendError::permanent("no embedding model: name one, or set `ai.embed_model`"))?;
                let body = json!({ "model": model, "input": params["input"] });
                let answer = self.send(reqwest::Method::POST, "/embeddings", Some(body), timeout_of(params)).await?;
                let vectors: Vec<Value> = answer["data"]
                    .as_array()
                    .map(|list| list.iter().map(|item| item["embedding"].clone()).collect())
                    .unwrap_or_default();
                if vectors.is_empty() || vectors.iter().any(|v| !v.is_array()) {
                    return Err(SendError::transient("the AI server's answer has no embeddings"));
                }
                Ok(json!({
                    "embeddings": vectors, "model": answer["model"].as_str().unwrap_or(model),
                    "usage": { "input_tokens": answer["usage"]["prompt_tokens"].as_u64().unwrap_or(0), "output_tokens": 0 },
                }))
            }
            "models" => {
                let answer = self.send(reqwest::Method::GET, "/models", None, Duration::from_secs(10)).await?;
                let ids: Vec<Value> = answer["data"]
                    .as_array()
                    .map(|list| list.iter().filter_map(|m| m["id"].as_str().map(|n| json!(n))).collect())
                    .unwrap_or_default();
                Ok(json!({ "models": ids }))
            }
            other => Err(SendError::permanent(format!(
                "openai_compatible has no action `{other}`; it offers: {}",
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

    fn bridge(port: u16) -> OpenAiCompatible {
        let values: Values = [
            ("base_url".to_string(), format!("http://127.0.0.1:{port}/v1")),
            ("api_key".to_string(), "sk-test-123".to_string()),
            ("default_model".to_string(), "gpt-x".to_string()),
        ]
        .into_iter()
        .collect();
        OpenAiCompatible::new(&values).unwrap_or_else(|e| panic!("{e}"))
    }

    #[tokio::test]
    async fn a_chat_carries_the_key_and_is_answered_in_the_common_shape() {
        let reply = r#"{"model":"gpt-x-2","choices":[{"message":{"role":"assistant","content":"{\"a\":1}"},"finish_reason":"stop"}],"usage":{"prompt_tokens":9,"completion_tokens":4}}"#;
        let (port, seen) = serve_once(200, reply).await;
        let answer = bridge(port)
            .call("chat", &json!({ "messages": [{ "role": "user", "content": "x" }], "max_tokens": 20, "json": true }))
            .await;
        assert_eq!(
            answer,
            Ok(json!({ "text": "{\"a\":1}", "model": "gpt-x-2", "provider": "openai_compatible", "finish_reason": "stop",
                       "usage": { "input_tokens": 9, "output_tokens": 4 } }))
        );
        let request = seen.await.unwrap_or_default();
        assert!(request.starts_with("POST /v1/chat/completions "), "{request}");
        assert!(request.to_ascii_lowercase().contains("authorization: bearer sk-test-123"), "{request}");
        assert!(request.contains("\"max_tokens\":20") && request.contains("json_object"), "{request}");
    }

    #[tokio::test]
    async fn tool_calls_arrive_parsed_and_tool_turns_are_sent_back() {
        let reply = r#"{"model":"m","choices":[{"message":{"content":null,"tool_calls":[{"id":"call_9","type":"function","function":{"name":"balance","arguments":"{\"kind\":\"sick\"}"}}]},"finish_reason":"tool_calls"}],"usage":{}}"#;
        let (port, seen) = serve_once(200, reply).await;
        let answer = bridge(port)
            .call(
                "chat",
                &json!({
                    "messages": [
                        { "role": "user", "content": "q" },
                        { "role": "assistant", "content": "", "tool_calls": [{ "id": "c1", "name": "balance", "arguments": { "a": 1 } }] },
                        { "role": "tool", "content": "3", "tool_call_id": "c1" },
                    ],
                    "tools": [{ "name": "balance", "description": "d", "schema": { "type": "object" } }],
                }),
            )
            .await
            .unwrap_or(Value::Null);
        assert_eq!(answer["tool_calls"], json!([{ "id": "call_9", "name": "balance", "arguments": { "kind": "sick" } }]));
        assert_eq!(answer["finish_reason"], "tool_calls");
        let request = seen.await.unwrap_or_default();
        assert!(request.contains("\"tool_call_id\":\"c1\"") && request.contains("\"arguments\":\"{\\\"a\\\":1}\""), "{request}");
        assert!(request.contains("\"parameters\""), "{request}");
    }

    #[tokio::test]
    async fn a_streamed_chat_gives_text_and_assembles_tool_calls_and_usage() {
        let body = concat!(
            "data: {\"model\":\"m1\",\"choices\":[{\"delta\":{\"role\":\"assistant\",\"content\":\"Hel\"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"lo\"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_7\",\"function\":{\"name\":\"bal\",\"arguments\":\"{\\\"k\\\"\"}}]}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\":1}\"}}]},\"finish_reason\":\"tool_calls\"}]}\n\n",
            "data: {\"choices\":[],\"usage\":{\"prompt_tokens\":8,\"completion_tokens\":3}}\n\n",
            "data: [DONE]\n\n",
        );
        let (port, seen) = serve_once(200, body).await;
        let pieces = std::sync::Mutex::new(Vec::<String>::new());
        let sink = |piece: &str| pieces.lock().map(|mut p| p.push(piece.to_string())).unwrap_or(());
        let answer = bridge(port).stream("chat", &json!({ "messages": [{ "role": "user", "content": "q" }] }), &sink).await.unwrap_or(Value::Null);
        assert_eq!(answer["text"], "Hello");
        assert_eq!(answer["model"], "m1");
        assert_eq!(answer["tool_calls"], json!([{ "id": "call_7", "name": "bal", "arguments": { "k": 1 } }]));
        assert_eq!(answer["usage"], json!({ "input_tokens": 8, "output_tokens": 3 }));
        assert_eq!(answer["finish_reason"], "tool_calls");
        assert_eq!(pieces.lock().map(|p| p.clone()).unwrap_or_default(), vec!["Hel", "lo"]);
        let request = seen.await.unwrap_or_default();
        assert!(request.contains("\"stream\":true") && request.contains("include_usage"), "{request}");
    }

    #[tokio::test]
    async fn a_stream_that_ends_early_is_a_failure() {
        let (port, _) = serve_once(200, "data: {\"choices\":[{\"delta\":{\"content\":\"Hel\"}}]}\n\n").await;
        let sink = |_: &str| {};
        assert!(bridge(port).stream("chat", &json!({ "messages": [] }), &sink).await.unwrap_err().message.contains("stopped"));
    }

    #[tokio::test]
    async fn a_bad_key_is_permanent_and_never_echoes_the_prompt() {
        let (port, _) = serve_once(401, r#"{"error":{"message":"Incorrect API key provided"}}"#).await;
        let refused = bridge(port).call("chat", &json!({ "messages": [{ "role": "user", "content": "secret salary data" }] })).await.unwrap_err();
        assert!(refused.permanent && refused.message.contains("Incorrect API key"), "{refused:?}");
        assert!(!refused.message.contains("salary"));
        let (port, _) = serve_once(429, "{}").await;
        assert!(!bridge(port).call("chat", &json!({ "messages": [] })).await.unwrap_err().permanent);
    }

    #[tokio::test]
    async fn embeddings_and_models() {
        let (port, seen) = serve_once(200, r#"{"model":"emb","data":[{"embedding":[1.0,2.0]},{"embedding":[3.0,4.0]}],"usage":{"prompt_tokens":6}}"#).await;
        let answer = bridge(port).call("embed", &json!({ "input": ["a", "b"], "model": "emb" })).await.unwrap_or(Value::Null);
        assert_eq!(answer["embeddings"], json!([[1.0, 2.0], [3.0, 4.0]]));
        assert!(seen.await.unwrap_or_default().starts_with("POST /v1/embeddings "));
        let (port, _) = serve_once(200, r#"{"data":[{"id":"m1"},{"id":"m2"}]}"#).await;
        assert_eq!(bridge(port).call("models", &json!({})).await, Ok(json!({ "models": ["m1", "m2"] })));
    }

    #[tokio::test]
    async fn settings_are_checked() {
        let values: Values = [("base_url".to_string(), "file:///x".to_string())].into_iter().collect();
        assert!(OpenAiCompatible::new(&values).is_err());
        let b = OpenAiCompatible::new(&Values::new()).unwrap_or_else(|e| panic!("{e}"));
        assert!(b.call("chat", &json!({ "messages": [] })).await.unwrap_err().message.contains("no model"));
    }
}
