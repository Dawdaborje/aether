//! Anthropic's Claude models. <https://docs.anthropic.com/en/api/messages>
//!
//! `api_key` is the organization's own key; `base_url` is the API root (`https://api.anthropic.com`).
//! Anthropic has no embeddings, so `embed` is refused here: pick another provider for that.
//!
//! Actions (used by `ai::complete` and `ai::status`, not by `bridge::call`):
//! * `chat { messages, model?, max_tokens, temperature?, json?, timeout_secs? }` answers an [`aether_ai::ChatResponse`].
//!   `system` messages are sent in the API's separate `system` field. There is no JSON mode: `json` adds an
//!   instruction to answer with one JSON object.
//! * `models {}` answers `{ models: [id] }`.

use std::time::Duration;

use aether_ai::{ChatResponse, ToolCall, Usage};
use aether_communication::{Action, ActionBridge, ConfigError, Field, SendError, Spec, TextSink, Values};
use async_trait::async_trait;
use serde_json::{Value, json};

pub const SPEC: Spec = Spec {
    key: "anthropic",
    label: "Anthropic",
    account: &[Field { name: "api_key", label: "API key", secret: true, required: true, default: None }],
    options: &[
        Field { name: "base_url", label: "API address", secret: false, required: false, default: Some("https://api.anthropic.com") },
        Field { name: "default_model", label: "Model used when none is named", secret: false, required: false, default: None },
    ],
};

pub const ACTIONS: &[Action] = &[
    Action { name: "chat", help: "{ messages, model?, max_tokens, temperature?, json?, timeout_secs? } -> { text, model, provider, finish_reason, usage }" },
    Action { name: "models", help: "{} -> { models: [id] }" },
];

const API_VERSION: &str = "2023-06-01";
const JSON_INSTRUCTION: &str = "Answer with one valid JSON object and nothing else: no prose, no code fence.";

pub struct Anthropic {
    base_url: String,
    api_key: String,
    default_model: Option<String>,
}

fn setting<'a>(values: &'a Values, name: &str) -> Option<&'a str> {
    values.get(name).map(|v| v.trim()).filter(|v| !v.is_empty())
}

impl Anthropic {
    pub fn new(values: &Values) -> Result<Self, ConfigError> {
        let base_url = setting(values, "base_url").unwrap_or("https://api.anthropic.com").trim_end_matches('/').to_string();
        if !(base_url.starts_with("http://") || base_url.starts_with("https://")) {
            return Err(ConfigError("the API address starts with http:// or https://".into()));
        }
        let api_key = setting(values, "api_key").ok_or_else(|| ConfigError("the Anthropic API key is not set".into()))?.to_string();
        Ok(Self { base_url, api_key, default_model: setting(values, "default_model").map(str::to_string) })
    }

    async fn send(&self, method: reqwest::Method, path: &str, body: Option<Value>, timeout: Duration) -> Result<Value, SendError> {
        let mut request = reqwest::Client::new()
            .request(method, format!("{}{path}", self.base_url))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", API_VERSION)
            .timeout(timeout);
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().await.map_err(|e| {
            SendError::transient(format!("Anthropic could not be reached ({})", if e.is_timeout() { "timed out" } else { "network error" }))
        })?;
        let status = response.status();
        let answer: Value = response.json().await.unwrap_or(Value::Null);
        if !status.is_success() {
            // 529 is "overloaded": worth another try, unlike the other 4xx.
            let detail = answer["error"]["message"].as_str().unwrap_or("the request was refused");
            return Err(if status.as_u16() == 529 {
                SendError::transient(format!("Anthropic is overloaded: {detail}"))
            } else {
                SendError::from_status(status.as_u16(), detail)
            });
        }
        Ok(answer)
    }
}

fn timeout_of(params: &Value) -> Duration {
    Duration::from_secs(params["timeout_secs"].as_u64().unwrap_or(60).clamp(1, 600))
}

impl Anthropic {
    /// The model and request body of a chat; `stream` asks for server-sent events.
    fn chat_body(&self, params: &Value, stream: bool) -> Result<(String, Value), SendError> {
        let model = params["model"]
            .as_str()
            .or(self.default_model.as_deref())
            .ok_or_else(|| SendError::permanent("no model: name one, or set `ai.model`"))?;
        let mut system: Vec<String> = Vec::new();
        let mut messages: Vec<Value> = Vec::new();
        for message in params["messages"].as_array().into_iter().flatten() {
            let content = message["content"].as_str().unwrap_or_default();
            match message["role"].as_str() {
                Some("system") => system.push(content.to_string()),
                Some("assistant") if message["tool_calls"].as_array().is_some_and(|c| !c.is_empty()) => {
                    let mut blocks = Vec::new();
                    if !content.trim().is_empty() {
                        blocks.push(json!({ "type": "text", "text": content }));
                    }
                    for call in message["tool_calls"].as_array().into_iter().flatten() {
                        blocks.push(json!({ "type": "tool_use", "id": call["id"], "name": call["name"], "input": call["arguments"] }));
                    }
                    messages.push(json!({ "role": "assistant", "content": blocks }));
                }
                // Results of one turn's tool calls go back together, in one user message.
                Some("tool") => {
                    let block = json!({ "type": "tool_result", "tool_use_id": message["tool_call_id"], "content": content });
                    match messages.last_mut() {
                        Some(last) if last["role"] == "user" && last["content"].is_array() => {
                            if let Some(list) = last["content"].as_array_mut() {
                                list.push(block);
                            }
                        }
                        _ => messages.push(json!({ "role": "user", "content": [block] })),
                    }
                }
                Some(role @ ("user" | "assistant")) if !content.trim().is_empty() => {
                    messages.push(json!({ "role": role, "content": content }));
                }
                _ => {}
            }
        }
        if messages.first().is_none_or(|m| m["role"] != "user") {
            return Err(SendError::permanent("the conversation must begin with a user message"));
        }
        if params["json"].as_bool() == Some(true) {
            system.push(JSON_INSTRUCTION.to_string());
        }
        let tools: Vec<Value> = params["tools"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|t| json!({ "name": t["name"], "description": t["description"], "input_schema": t["schema"] }))
            .collect();
        // The API requires a limit; the kernel always sends one.
        let mut body = json!({ "model": model, "max_tokens": params["max_tokens"].as_u64().unwrap_or(1024), "messages": messages });
        if !tools.is_empty() {
            body["tools"] = json!(tools);
        }
        if !system.is_empty() {
            body["system"] = json!(system.join("\n\n"));
        }
        if let Some(t) = params["temperature"].as_f64() {
            body["temperature"] = json!(t.min(1.0));
        }
        if stream {
            body["stream"] = json!(true);
        }
        Ok((model.to_string(), body))
    }

    async fn chat_stream(&self, params: &Value, sink: &dyn TextSink) -> Result<Value, SendError> {
        let (model, body) = self.chat_body(params, true)?;
        let mut response = reqwest::Client::new()
            .post(format!("{}/v1/messages", self.base_url))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", API_VERSION)
            .json(&body)
            .timeout(timeout_of(params))
            .send()
            .await
            .map_err(|e| SendError::transient(format!("Anthropic could not be reached ({})", if e.is_timeout() { "timed out" } else { "network error" })))?;
        let status = response.status();
        if !status.is_success() {
            let answer: Value = response.json().await.unwrap_or(Value::Null);
            let detail = answer["error"]["message"].as_str().unwrap_or("the request was refused");
            return Err(if status.as_u16() == 529 {
                SendError::transient(format!("Anthropic is overloaded: {detail}"))
            } else {
                SendError::from_status(status.as_u16(), detail)
            });
        }
        let mut state = StreamState { model: model.clone(), ..StreamState::default() };
        let mut lines = aether_ai::LineBuffer::default();
        while let Some(bytes) = response.chunk().await.map_err(|_| SendError::transient("the answer from Anthropic was cut off"))? {
            for line in lines.push(&bytes) {
                state.take(&line, sink)?;
            }
        }
        if let Some(line) = lines.finish() {
            state.take(&line, sink)?;
        }
        if !state.stopped {
            return Err(SendError::transient("the answer from Anthropic stopped before it was finished"));
        }
        Ok(serde_json::to_value(state.into_response()).map_err(|e| SendError::permanent(e.to_string()))?)
    }
}

/// A streamed answer being put together from its events.
#[derive(Default)]
struct StreamState {
    model: String,
    text: String,
    input: u64,
    output: u64,
    stop_reason: Option<String>,
    stopped: bool,
    /// The tool-use block being filled: `(id, name, arguments text)`, and the finished ones.
    open: Option<(String, String, String)>,
    calls: Vec<ToolCall>,
}

impl StreamState {
    fn take(&mut self, line: &str, sink: &dyn TextSink) -> Result<(), SendError> {
        let Some(data) = line.strip_prefix("data:").map(str::trim) else { return Ok(()) };
        let Ok(event) = serde_json::from_str::<Value>(data) else { return Ok(()) };
        match event["type"].as_str() {
            Some("message_start") => {
                if let Some(model) = event["message"]["model"].as_str() {
                    self.model = model.to_string();
                }
                self.input = event["message"]["usage"]["input_tokens"].as_u64().unwrap_or(0);
            }
            Some("content_block_start") if event["content_block"]["type"] == "tool_use" => {
                self.open = Some((
                    event["content_block"]["id"].as_str().unwrap_or_default().to_string(),
                    event["content_block"]["name"].as_str().unwrap_or_default().to_string(),
                    String::new(),
                ));
            }
            Some("content_block_delta") => match event["delta"]["type"].as_str() {
                Some("text_delta") => {
                    if let Some(piece) = event["delta"]["text"].as_str().filter(|p| !p.is_empty()) {
                        self.text.push_str(piece);
                        sink.emit(piece);
                    }
                }
                Some("input_json_delta") => {
                    if let (Some((_, _, arguments)), Some(piece)) = (self.open.as_mut(), event["delta"]["partial_json"].as_str()) {
                        arguments.push_str(piece);
                    }
                }
                _ => {}
            },
            Some("content_block_stop") => {
                if let Some((id, name, arguments)) = self.open.take() {
                    let arguments = if arguments.trim().is_empty() { json!({}) } else { serde_json::from_str(&arguments).unwrap_or_else(|_| json!({})) };
                    self.calls.push(ToolCall { id, name, arguments });
                }
            }
            Some("message_delta") => {
                if let Some(reason) = event["delta"]["stop_reason"].as_str() {
                    self.stop_reason = Some(reason.to_string());
                }
                if let Some(n) = event["usage"]["output_tokens"].as_u64() {
                    self.output = n;
                }
            }
            Some("message_stop") => self.stopped = true,
            Some("error") => {
                let message = event["error"]["message"].as_str().unwrap_or("an error");
                return Err(SendError::transient(format!("Anthropic failed while answering: {message}")));
            }
            _ => {}
        }
        Ok(())
    }

    fn into_response(self) -> ChatResponse {
        ChatResponse {
            text: self.text,
            model: self.model,
            provider: "anthropic".into(),
            finish_reason: match self.stop_reason.as_deref() {
                Some("tool_use") => "tool_calls".into(),
                Some("max_tokens") => "length".into(),
                Some("end_turn") | Some("stop_sequence") | None => "stop".into(),
                Some(other) => other.to_string(),
            },
            usage: Usage { input_tokens: self.input, output_tokens: self.output },
            tool_calls: self.calls,
        }
    }
}

#[async_trait]
impl ActionBridge for Anthropic {
    async fn call(&self, action: &str, params: &Value) -> Result<Value, SendError> {
        match action {
            "chat" => {
                let (model, body) = self.chat_body(params, false)?;
                let answer = self.send(reqwest::Method::POST, "/v1/messages", Some(body), timeout_of(params)).await?;
                let text: String = answer["content"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|part| part["type"] == "text")
                    .filter_map(|part| part["text"].as_str())
                    .collect();
                let tool_calls: Vec<ToolCall> = answer["content"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|part| part["type"] == "tool_use")
                    .filter_map(|part| {
                        Some(ToolCall { id: part["id"].as_str()?.to_string(), name: part["name"].as_str()?.to_string(), arguments: part["input"].clone() })
                    })
                    .collect();
                if answer["content"].as_array().is_none_or(Vec::is_empty) {
                    return Err(SendError::transient("Anthropic's answer has no content"));
                }
                let response = ChatResponse {
                    text,
                    model: answer["model"].as_str().unwrap_or(&model).to_string(),
                    provider: "anthropic".into(),
                    tool_calls,
                    finish_reason: match answer["stop_reason"].as_str() {
                        Some("tool_use") => "tool_calls".into(),
                        Some("max_tokens") => "length".into(),
                        Some("end_turn") | Some("stop_sequence") | None => "stop".into(),
                        Some(other) => other.to_string(),
                    },
                    usage: Usage {
                        input_tokens: answer["usage"]["input_tokens"].as_u64().unwrap_or(0),
                        output_tokens: answer["usage"]["output_tokens"].as_u64().unwrap_or(0),
                    },
                };
                serde_json::to_value(response).map_err(|e| SendError::permanent(e.to_string()))
            }
            "models" => {
                let answer = self.send(reqwest::Method::GET, "/v1/models", None, Duration::from_secs(10)).await?;
                let ids: Vec<Value> = answer["data"]
                    .as_array()
                    .map(|list| list.iter().filter_map(|m| m["id"].as_str().map(|n| json!(n))).collect())
                    .unwrap_or_default();
                Ok(json!({ "models": ids }))
            }
            "embed" => Err(SendError::permanent("Anthropic has no embeddings: use another provider for `ai::embed`")),
            other => Err(SendError::permanent(format!(
                "anthropic has no action `{other}`; it offers: {}",
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

    fn bridge(port: u16) -> Anthropic {
        let values: Values = [
            ("base_url".to_string(), format!("http://127.0.0.1:{port}")),
            ("api_key".to_string(), "sk-ant-test".to_string()),
            ("default_model".to_string(), "claude-x".to_string()),
        ]
        .into_iter()
        .collect();
        Anthropic::new(&values).unwrap_or_else(|e| panic!("{e}"))
    }

    #[tokio::test]
    async fn system_text_moves_to_its_own_field_and_the_answer_is_normalised() {
        let reply = r#"{"model":"claude-x","content":[{"type":"text","text":"Hello"},{"type":"text","text":" world"}],"stop_reason":"max_tokens","usage":{"input_tokens":12,"output_tokens":5}}"#;
        let (port, seen) = serve_once(200, reply).await;
        let answer = bridge(port)
            .call(
                "chat",
                &json!({ "messages": [{ "role": "system", "content": "Be brief." }, { "role": "user", "content": "Hi" }], "max_tokens": 30, "json": true }),
            )
            .await;
        assert_eq!(
            answer,
            Ok(json!({ "text": "Hello world", "model": "claude-x", "provider": "anthropic", "finish_reason": "length",
                       "usage": { "input_tokens": 12, "output_tokens": 5 } }))
        );
        let request = seen.await.unwrap_or_default();
        assert!(request.starts_with("POST /v1/messages "), "{request}");
        let lower = request.to_ascii_lowercase();
        assert!(lower.contains("x-api-key: sk-ant-test") && lower.contains("anthropic-version: 2023-06-01"), "{request}");
        assert!(request.contains("\"system\":\"Be brief.") && request.contains("valid JSON object"), "{request}");
        assert!(request.contains("\"max_tokens\":30"));
        // The system message is not also sent as a turn.
        assert!(!request.contains("\"role\":\"system\""), "{request}");
    }

    #[tokio::test]
    async fn tool_use_blocks_become_calls_and_results_go_back_in_one_user_message() {
        let reply = r#"{"model":"claude-x","content":[{"type":"text","text":"Checking"},{"type":"tool_use","id":"tu_1","name":"balance","input":{"kind":"sick"}}],"stop_reason":"tool_use","usage":{"input_tokens":1,"output_tokens":1}}"#;
        let (port, seen) = serve_once(200, reply).await;
        let answer = bridge(port)
            .call(
                "chat",
                &json!({
                    "messages": [
                        { "role": "user", "content": "q" },
                        { "role": "assistant", "content": "", "tool_calls": [{ "id": "a", "name": "t", "arguments": {} }, { "id": "b", "name": "u", "arguments": {} }] },
                        { "role": "tool", "content": "1", "tool_call_id": "a" },
                        { "role": "tool", "content": "2", "tool_call_id": "b" },
                    ],
                    "tools": [{ "name": "balance", "description": "d", "schema": { "type": "object" } }],
                }),
            )
            .await
            .unwrap_or(Value::Null);
        assert_eq!(answer["finish_reason"], "tool_calls");
        assert_eq!(answer["tool_calls"], json!([{ "id": "tu_1", "name": "balance", "arguments": { "kind": "sick" } }]));
        assert_eq!(answer["text"], "Checking");
        let request = seen.await.unwrap_or_default();
        assert!(request.contains("\"input_schema\""), "{request}");
        // Two results, one user message.
        assert_eq!(request.matches("tool_result").count(), 2, "{request}");
        assert_eq!(request.matches("\"role\":\"user\"").count(), 2, "{request}");
    }

    #[tokio::test]
    async fn a_streamed_chat_gives_text_and_assembles_a_tool_use_block() {
        let body = concat!(
            "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"model\":\"claude-y\",\"usage\":{\"input_tokens\":14}}}\n\n",
            "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
            "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Let me \"}}\n\n",
            "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"check.\"}}\n\n",
            "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
            "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"tool_use\",\"id\":\"tu_2\",\"name\":\"balance\",\"input\":{}}}\n\n",
            "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"kind\\\":\"}}\n\n",
            "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"\\\"sick\\\"}\"}}\n\n",
            "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":1}\n\n",
            "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\"},\"usage\":{\"output_tokens\":9}}\n\n",
            "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
        );
        let (port, seen) = serve_once(200, body).await;
        let pieces = std::sync::Mutex::new(Vec::<String>::new());
        let sink = |piece: &str| pieces.lock().map(|mut p| p.push(piece.to_string())).unwrap_or(());
        let answer = bridge(port).stream("chat", &json!({ "messages": [{ "role": "user", "content": "q" }] }), &sink).await.unwrap_or(Value::Null);
        assert_eq!(answer["text"], "Let me check.");
        assert_eq!(answer["model"], "claude-y");
        assert_eq!(answer["finish_reason"], "tool_calls");
        assert_eq!(answer["tool_calls"], json!([{ "id": "tu_2", "name": "balance", "arguments": { "kind": "sick" } }]));
        assert_eq!(answer["usage"], json!({ "input_tokens": 14, "output_tokens": 9 }));
        assert_eq!(pieces.lock().map(|p| p.clone()).unwrap_or_default(), vec!["Let me ", "check."]);
        assert!(seen.await.unwrap_or_default().contains("\"stream\":true"));
    }

    #[tokio::test]
    async fn a_stream_without_its_end_or_with_an_error_event_fails() {
        let sink = |_: &str| {};
        let (port, _) = serve_once(200, "data: {\"type\":\"message_start\",\"message\":{\"usage\":{}}}\n\n").await;
        assert!(bridge(port).stream("chat", &json!({ "messages": [{ "role": "user", "content": "q" }] }), &sink).await.unwrap_err().message.contains("stopped"));
        let (port, _) = serve_once(200, "data: {\"type\":\"error\",\"error\":{\"message\":\"Overloaded\"}}\n\n").await;
        assert!(!bridge(port).stream("chat", &json!({ "messages": [{ "role": "user", "content": "q" }] }), &sink).await.unwrap_err().permanent);
    }

    #[tokio::test]
    async fn the_conversation_must_start_with_the_user_and_errors_are_classified() {
        let b = bridge(1);
        let refused = b.call("chat", &json!({ "messages": [{ "role": "assistant", "content": "hi" }] })).await.unwrap_err();
        assert!(refused.permanent && refused.message.contains("begin with a user"));
        let (port, _) = serve_once(529, r#"{"error":{"message":"Overloaded"}}"#).await;
        let overloaded = bridge(port).call("chat", &json!({ "messages": [{ "role": "user", "content": "x" }] })).await.unwrap_err();
        assert!(!overloaded.permanent, "{overloaded:?}");
        let (port, _) = serve_once(401, r#"{"error":{"message":"invalid x-api-key"}}"#).await;
        assert!(bridge(port).call("chat", &json!({ "messages": [{ "role": "user", "content": "x" }] })).await.unwrap_err().permanent);
    }

    #[tokio::test]
    async fn the_key_is_required_and_embeddings_are_refused() {
        assert!(Anthropic::new(&Values::new()).is_err());
        assert!(bridge(1).call("embed", &json!({ "input": ["a"] })).await.unwrap_err().message.contains("no embeddings"));
    }
}
