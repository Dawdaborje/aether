//! What a language-model provider is, and what the kernel asks of one.
//!
//! A plugin asks the kernel to *complete a conversation* (`ai::complete`) or *embed texts*
//! (`ai::embed`) and gets one normalised answer, whichever provider produced it. A provider is a bridge
//! under `bridges/ai/` (Ollama, OpenAI-compatible, Anthropic) that runs inside the kernel and holds the
//! organization's credentials; it knows how to talk to one vendor and nothing about plugins,
//! organizations or the database. This crate holds what every provider and the kernel share: the
//! request and answer shapes, the checks on them, the limits, and the counter that keeps the number of
//! calls in flight bounded.
//!
//! Limits are deliberate: a model call costs money or machine time, the prompt may hold people's data,
//! and a plugin call may only wait a few seconds. See `docs/architecture/ai.md`.

use std::sync::atomic::{AtomicUsize, Ordering};

use serde::{Deserialize, Serialize};

pub const DEFAULT_MAX_TOKENS: u64 = 1024;
pub const DEFAULT_MAX_PROMPT_KB: u64 = 64;
pub const DEFAULT_SYNC_CEILING_SECS: u64 = 8;
pub const DEFAULT_JOB_TIMEOUT_SECS: u64 = 100;
pub const DEFAULT_MAX_CONCURRENT: usize = 2;
pub const DEFAULT_MAX_QUEUED: u64 = 50;
pub const HARD_MAX_TOKENS: u64 = 8192;
pub const HARD_MAX_PROMPT_KB: u64 = 512;
/// A plugin call is cut off after ten seconds, so a command that waits for the model cannot wait longer.
pub const HARD_SYNC_CEILING_SECS: u64 = 9;
/// A job is stopped when its lease (120 s by default) runs out, so a call cannot be allowed to run longer.
pub const HARD_JOB_TIMEOUT_SECS: u64 = 110;
pub const HARD_MAX_CONCURRENT: usize = 8;
pub const HARD_MAX_QUEUED: u64 = 1000;
/// Most messages in one conversation, and most texts in one embedding call.
pub const MAX_MESSAGES: usize = 64;
pub const MAX_EMBED_INPUTS: usize = 64;

/// The providers the kernel has. `local` ones run on the organization's own machines, so they stay
/// available when an organization allows only those.
pub const PROVIDERS: &[Provider] = &[
    Provider { key: "ollama", local: true },
    Provider { key: "openai_compatible", local: false },
    Provider { key: "anthropic", local: false },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Provider {
    pub key: &'static str,
    pub local: bool,
}

/// The setting keys, in one place so the seed file, the kernel and the docs agree.
pub mod keys {
    pub const ENABLED: &str = "ai.enabled";
    pub const PROVIDER: &str = "ai.provider";
    pub const MODEL: &str = "ai.model";
    pub const EMBED_MODEL: &str = "ai.embed_model";
    pub const LOCAL_ONLY: &str = "ai.local_only";
    pub const MAX_TOKENS: &str = "ai.max_tokens";
    pub const MAX_PROMPT_KB: &str = "ai.max_prompt_kb";
    pub const SYNC_CEILING_SECS: &str = "ai.sync_ceiling_secs";
    pub const JOB_TIMEOUT_SECS: &str = "ai.job_timeout_secs";
    pub const MAX_CONCURRENT: &str = "ai.max_concurrent";
    pub const MAX_QUEUED: &str = "ai.max_queued";
    pub const DAILY_TOKENS_ORG: &str = "ai.daily_tokens_org";
    pub const DAILY_TOKENS_USER: &str = "ai.daily_tokens_user";
    pub const ALL: &[&str] = &[
        ENABLED, PROVIDER, MODEL, EMBED_MODEL, LOCAL_ONLY, MAX_TOKENS, MAX_PROMPT_KB, SYNC_CEILING_SECS, JOB_TIMEOUT_SECS,
        MAX_CONCURRENT, MAX_QUEUED, DAILY_TOKENS_ORG, DAILY_TOKENS_USER,
    ];
}

/// The limits in force for one organization, from its settings and clamped to the hard caps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Limits {
    /// Off until an organization turns it on: prompts leave the kernel (or at least the plugin).
    pub enabled: bool,
    pub provider: String,
    /// The model used when a request names none; empty lets the provider's own default apply.
    pub model: Option<String>,
    pub embed_model: Option<String>,
    pub local_only: bool,
    pub max_tokens: u64,
    pub max_prompt_bytes: usize,
    pub sync_ceiling_secs: u64,
    pub job_timeout_secs: u64,
    pub max_concurrent: usize,
    pub max_queued: u64,
    /// Tokens (in plus out) per UTC day; 0 means no budget.
    pub daily_tokens_org: u64,
    pub daily_tokens_user: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self::from_settings(|_| None)
    }
}

impl Limits {
    /// Build from a lookup of setting values (numbers and flags may be stored as text). An unreadable or
    /// out-of-range value falls back to the default, then is clamped to the hard cap.
    pub fn from_settings(get: impl Fn(&str) -> Option<serde_json::Value>) -> Self {
        let number = |key: &str, default: u64, low: u64, high: u64| -> u64 {
            get(key)
                .and_then(|v| v.as_u64().or_else(|| v.as_str().and_then(|s| s.trim().parse().ok())))
                .unwrap_or(default)
                .clamp(low, high)
        };
        let flag = |key: &str, default: bool| -> bool {
            get(key)
                .and_then(|v| v.as_bool().or_else(|| v.as_str().map(|s| matches!(s.trim(), "true" | "1" | "yes" | "on"))))
                .unwrap_or(default)
        };
        let text = |key: &str| -> Option<String> {
            get(key).and_then(|v| v.as_str().map(|s| s.trim().to_string())).filter(|s| !s.is_empty())
        };
        Self {
            enabled: flag(keys::ENABLED, false),
            provider: text(keys::PROVIDER).unwrap_or_else(|| PROVIDERS[0].key.to_string()),
            model: text(keys::MODEL),
            embed_model: text(keys::EMBED_MODEL),
            local_only: flag(keys::LOCAL_ONLY, false),
            max_tokens: number(keys::MAX_TOKENS, DEFAULT_MAX_TOKENS, 1, HARD_MAX_TOKENS),
            max_prompt_bytes: (number(keys::MAX_PROMPT_KB, DEFAULT_MAX_PROMPT_KB, 1, HARD_MAX_PROMPT_KB) * 1024) as usize,
            sync_ceiling_secs: number(keys::SYNC_CEILING_SECS, DEFAULT_SYNC_CEILING_SECS, 1, HARD_SYNC_CEILING_SECS),
            job_timeout_secs: number(keys::JOB_TIMEOUT_SECS, DEFAULT_JOB_TIMEOUT_SECS, 10, HARD_JOB_TIMEOUT_SECS),
            max_concurrent: number(keys::MAX_CONCURRENT, DEFAULT_MAX_CONCURRENT as u64, 1, HARD_MAX_CONCURRENT as u64) as usize,
            max_queued: number(keys::MAX_QUEUED, DEFAULT_MAX_QUEUED, 1, HARD_MAX_QUEUED),
            daily_tokens_org: number(keys::DAILY_TOKENS_ORG, 0, 0, u64::MAX / 2),
            daily_tokens_user: number(keys::DAILY_TOKENS_USER, 0, 0, u64::MAX / 2),
        }
    }

    /// The provider the settings name, when the kernel has it and the organization allows it.
    pub fn chosen_provider(&self) -> Result<&'static Provider, String> {
        let provider = PROVIDERS
            .iter()
            .find(|p| p.key == self.provider)
            .ok_or_else(|| format!("`{}` is not an AI provider this kernel has", self.provider))?;
        if self.local_only && !provider.local {
            return Err(format!("this organization allows only local AI providers, and `{}` is not one", provider.key));
        }
        Ok(provider)
    }
}

/// One turn of a conversation.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    /// `system`, `user`, `assistant`, or `tool` (the answer to a tool call; only the kernel's own tool loop uses it).
    pub role: String,
    pub content: String,
    /// Tools an `assistant` turn asked for.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
    /// For a `tool` turn: the call it answers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl Message {
    pub fn new(role: &str, content: impl Into<String>) -> Self {
        Self { role: role.into(), content: content.into(), ..Self::default() }
    }
}

/// A tool the model may ask for: a named function with a JSON Schema for its arguments.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    /// JSON Schema of the arguments (an object schema).
    pub schema: serde_json::Value,
}

/// The model asking for a tool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCall {
    /// The provider's id for the call, or one made up where it has none; the answer carries it back.
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

/// The name the model sees for a plugin's tool: unique across plugins.
pub fn tool_name(plugin: &str, tool: &str) -> String {
    format!("{plugin}__{tool}")
}

/// A tool name as providers accept it (letters, digits, `_` and `-`, at most 64).
pub fn valid_tool_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 64 && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// What to ask a model. Validated by [`ChatRequest::validate`] before a provider sees it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatRequest {
    pub messages: Vec<Message>,
    /// The model to use; the organization's `ai.model` setting (then the provider's default) when left out.
    #[serde(default)]
    pub model: Option<String>,
    /// Most tokens in the answer; the `ai.max_tokens` setting when left out, and never above it.
    #[serde(default)]
    pub max_tokens: Option<u64>,
    #[serde(default)]
    pub temperature: Option<f64>,
    /// Ask for a single JSON object as the answer (the provider's JSON mode where it has one).
    #[serde(default)]
    pub json: bool,
    /// Tools the model may ask for. Set only by the kernel's tool loop; a plugin's request carrying any is refused.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<ToolSpec>,
}

impl ChatRequest {
    /// Refuse a conversation a plugin should not send: no tools, no tool turns.
    pub fn validate(&self, limits: &Limits) -> Result<(), String> {
        if !self.tools.is_empty() || self.messages.iter().any(|m| m.role == "tool" || !m.tool_calls.is_empty() || m.tool_call_id.is_some()) {
            return Err("tools are offered by the kernel from plugins' `[[ai_tool]]`, not sent in a request".into());
        }
        self.validate_turns(limits)
    }

    /// The checks of [`ChatRequest::validate`] for the kernel's own tool loop, which may carry tools and tool turns.
    pub fn validate_turns(&self, limits: &Limits) -> Result<(), String> {
        if self.messages.is_empty() {
            return Err("`messages` needs at least one message".into());
        }
        if self.messages.len() > MAX_MESSAGES {
            return Err(format!("`messages` is at most {MAX_MESSAGES} messages"));
        }
        let mut bytes = 0usize;
        for message in &self.messages {
            if !matches!(message.role.as_str(), "system" | "user" | "assistant" | "tool") {
                return Err("a message's `role` is `system`, `user` or `assistant`".into());
            }
            bytes += message.content.len();
        }
        if self.messages.iter().all(|m| m.content.trim().is_empty() && m.tool_calls.is_empty()) {
            return Err("every message is empty".into());
        }
        if bytes > limits.max_prompt_bytes {
            return Err(format!("the conversation is over {} KB", limits.max_prompt_bytes / 1024));
        }
        if let Some(model) = &self.model {
            valid_model(model)?;
        }
        if self.max_tokens == Some(0) {
            return Err("`max_tokens` is at least 1".into());
        }
        if self.temperature.is_some_and(|t| !t.is_finite() || !(0.0..=2.0).contains(&t)) {
            return Err("`temperature` is 0 to 2".into());
        }
        Ok(())
    }

    /// The most tokens to ask for: the request's own, never above the organization's cap.
    pub fn token_cap(&self, limits: &Limits) -> u64 {
        self.max_tokens.unwrap_or(limits.max_tokens).min(limits.max_tokens)
    }
}

/// A model name goes into a URL path or a JSON field of a request to a vendor: keep it plain.
pub fn valid_model(model: &str) -> Result<(), String> {
    let ok = !model.is_empty()
        && model.len() <= 100
        && model.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | ':' | '/' | '@'));
    if ok { Ok(()) } else { Err("`model` is a plain model name such as `llama3.2` or `claude-sonnet-5-5`".into()) }
}

/// Tokens a call used, as the provider counted them (zero where it does not say).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
}

impl Usage {
    pub fn total(&self) -> u64 {
        self.input_tokens.saturating_add(self.output_tokens)
    }
}

/// The answer every provider normalises to.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatResponse {
    pub text: String,
    pub model: String,
    pub provider: String,
    /// `stop`, `length` (cut at `max_tokens`), or the provider's own word.
    pub finish_reason: String,
    pub usage: Usage,
    /// Tools the model asked for; `text` may then be empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
}

/// A rough count (four bytes a token) for budgets when a provider reports no usage.
pub fn estimate_tokens(text: &str) -> u64 {
    (text.len() as u64).div_ceil(4)
}

/// Cut `text` to at most `max` bytes on a character boundary.
pub fn clip(text: &str, max: usize) -> &str {
    if text.len() <= max {
        return text;
    }
    let mut end = max;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// Split `text` into pieces of at most `size` bytes for embedding, keeping paragraphs together where they fit
/// and cutting a longer one on character boundaries with `overlap` bytes repeated, so a sentence cut at the
/// edge is whole in one piece. Empty pieces are dropped.
pub fn chunk_text(text: &str, size: usize, overlap: usize) -> Vec<String> {
    let size = size.max(64);
    let overlap = overlap.min(size / 2);
    let mut chunks: Vec<String> = Vec::new();
    let mut current = String::new();
    let flush = |current: &mut String, chunks: &mut Vec<String>| {
        let piece = current.trim();
        if !piece.is_empty() {
            chunks.push(piece.to_string());
        }
        current.clear();
    };
    for paragraph in text.split("\n\n") {
        let paragraph = paragraph.trim();
        if paragraph.is_empty() {
            continue;
        }
        if paragraph.len() > size {
            flush(&mut current, &mut chunks);
            let mut start = 0;
            while start < paragraph.len() {
                let mut end = (start + size).min(paragraph.len());
                while !paragraph.is_char_boundary(end) {
                    end -= 1;
                }
                let piece = paragraph[start..end].trim();
                if !piece.is_empty() {
                    chunks.push(piece.to_string());
                }
                if end >= paragraph.len() {
                    break;
                }
                let mut next = end.saturating_sub(overlap);
                while !paragraph.is_char_boundary(next) {
                    next += 1;
                }
                // Always move forward.
                start = if next > start { next } else { end };
            }
        } else if current.len() + paragraph.len() + 2 > size {
            flush(&mut current, &mut chunks);
            current.push_str(paragraph);
        } else {
            if !current.is_empty() {
                current.push_str("\n\n");
            }
            current.push_str(paragraph);
        }
    }
    flush(&mut current, &mut chunks);
    chunks
}

/// Cuts a byte stream into lines as it arrives (a server-sent-events or newline-delimited JSON body), so a
/// line split across two network reads is whole when it is handed out. `\r\n` and `\n` both end a line.
#[derive(Debug, Default)]
pub struct LineBuffer(Vec<u8>);

impl LineBuffer {
    /// Add bytes; the lines they completed (without their endings).
    pub fn push(&mut self, bytes: &[u8]) -> Vec<String> {
        self.0.extend_from_slice(bytes);
        let mut lines = Vec::new();
        while let Some(end) = self.0.iter().position(|b| *b == b'\n') {
            let line: Vec<u8> = self.0.drain(..=end).collect();
            lines.push(String::from_utf8_lossy(&line).trim_end_matches(['\r', '\n']).to_string());
        }
        lines
    }

    /// What is left when the stream ends without a final newline.
    pub fn finish(&mut self) -> Option<String> {
        if self.0.is_empty() {
            return None;
        }
        let line = String::from_utf8_lossy(&self.0).trim_end_matches('\r').to_string();
        self.0.clear();
        Some(line)
    }
}

/// Counts the model calls running in this process, so the `ai.max_concurrent` setting can change without a restart.
pub struct Gate {
    running: AtomicUsize,
}

/// Held while a call runs; releases its place when dropped.
pub struct Permit(&'static Gate);

impl Drop for Permit {
    fn drop(&mut self) {
        self.0.running.fetch_sub(1, Ordering::AcqRel);
    }
}

impl Gate {
    /// A place for one more call, waiting up to `wait` for one to free up; `None` when none did.
    pub async fn acquire(&'static self, limit: usize, wait: std::time::Duration) -> Option<Permit> {
        let started = std::time::Instant::now();
        loop {
            let now = self.running.load(Ordering::Acquire);
            if now < limit && self.running.compare_exchange(now, now + 1, Ordering::AcqRel, Ordering::Acquire).is_ok() {
                return Some(Permit(self));
            }
            if started.elapsed() >= wait {
                return None;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    }
}

pub fn gate() -> &'static Gate {
    static GATE: Gate = Gate { running: AtomicUsize::new(0) };
    &GATE
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ask(content: &str) -> ChatRequest {
        ChatRequest {
            messages: vec![Message::new("user", content)],
            model: None,
            max_tokens: None,
            temperature: None,
            json: false,
            tools: vec![],
        }
    }

    #[test]
    fn ai_is_off_until_an_organization_turns_it_on() {
        let limits = Limits::default();
        assert!(!limits.enabled);
        assert_eq!(limits.provider, "ollama");
        assert_eq!(limits.max_tokens, 1024);
        let on = Limits::from_settings(|k| (k == keys::ENABLED).then(|| json!("true")));
        assert!(on.enabled);
    }

    #[test]
    fn settings_are_clamped_to_the_hard_caps() {
        let l = Limits::from_settings(|k| match k {
            keys::MAX_TOKENS => Some(json!(999999)),
            keys::SYNC_CEILING_SECS => Some(json!("60")),
            keys::MAX_CONCURRENT => Some(json!(0)),
            _ => None,
        });
        assert_eq!((l.max_tokens, l.sync_ceiling_secs, l.max_concurrent), (HARD_MAX_TOKENS, HARD_SYNC_CEILING_SECS, 1));
    }

    #[test]
    fn local_only_refuses_a_remote_provider() {
        let mut l = Limits::default();
        assert!(l.chosen_provider().is_ok());
        l.local_only = true;
        l.provider = "anthropic".into();
        assert!(l.chosen_provider().unwrap_err().contains("local"));
        l.provider = "nonsense".into();
        assert!(l.chosen_provider().unwrap_err().contains("not an AI provider"));
        l.provider = "ollama".into();
        assert!(l.chosen_provider().is_ok());
    }

    #[test]
    fn a_conversation_is_checked_before_a_provider_sees_it() {
        let l = Limits::default();
        assert!(ask("hello").validate(&l).is_ok());
        assert!(ask("   ").validate(&l).is_err());
        assert!(ask(&"x".repeat(65 * 1024)).validate(&l).unwrap_err().contains("64 KB"));
        let mut bad_role = ask("hi");
        bad_role.messages[0].role = "tool".into();
        assert!(bad_role.validate(&l).is_err());
        let mut bad_model = ask("hi");
        bad_model.model = Some("a b; drop".into());
        assert!(bad_model.validate(&l).is_err());
        let mut hot = ask("hi");
        hot.temperature = Some(5.0);
        assert!(hot.validate(&l).is_err());
        assert!(ChatRequest { messages: vec![], ..ask("x") }.validate(&l).is_err());
    }

    #[test]
    fn a_plugin_cannot_send_tools_or_tool_turns() {
        let l = Limits::default();
        let mut r = ask("hi");
        r.tools.push(ToolSpec { name: "t".into(), description: "d".into(), schema: json!({}) });
        assert!(r.validate(&l).unwrap_err().contains("[[ai_tool]]"));
        assert!(r.validate_turns(&l).is_ok());
        let mut t = ask("hi");
        t.messages.push(Message { role: "tool".into(), content: "x".into(), tool_call_id: Some("1".into()), ..Message::default() });
        assert!(t.validate(&l).is_err());
        assert!(t.validate_turns(&l).is_ok());
        assert!(valid_tool_name("hr_leave__balance") && !valid_tool_name("a b") && !valid_tool_name(""));
    }

    #[test]
    fn a_request_cannot_ask_for_more_tokens_than_the_organization_allows() {
        let l = Limits::default();
        let mut r = ask("hi");
        assert_eq!(r.token_cap(&l), 1024);
        r.max_tokens = Some(100);
        assert_eq!(r.token_cap(&l), 100);
        r.max_tokens = Some(100_000);
        assert_eq!(r.token_cap(&l), 1024);
    }

    #[test]
    fn clipping_keeps_whole_characters() {
        assert_eq!(clip("héllo", 2), "h");
        assert_eq!(clip("abc", 10), "abc");
        assert_eq!(estimate_tokens("abcdefgh"), 2);
    }

    #[test]
    fn text_is_chunked_on_paragraphs_then_on_characters() {
        let text = "First paragraph.\n\nSecond paragraph.\n\n".to_string() + &"é".repeat(300);
        let chunks = chunk_text(&text, 100, 20);
        assert_eq!(chunks[0], "First paragraph.\n\nSecond paragraph.");
        assert!(chunks.len() >= 5, "{}", chunks.len());
        assert!(chunks.iter().all(|c| c.len() <= 100), "{:?}", chunks.iter().map(String::len).collect::<Vec<_>>());
        assert!(chunk_text("  \n\n ", 100, 10).is_empty());
        // The pieces of a long run overlap.
        let long = "abcdefghij".repeat(30);
        let pieces = chunk_text(&long, 100, 20);
        assert!(pieces[1].starts_with(&pieces[0][pieces[0].len() - 20..]));
    }

    #[test]
    fn lines_split_across_reads_are_whole() {
        let mut buffer = LineBuffer::default();
        assert!(buffer.push(b"data: {\"a\"").is_empty());
        assert_eq!(buffer.push(b":1}\r\n\r\ndata: [DO"), vec!["data: {\"a\":1}".to_string(), String::new()]);
        assert_eq!(buffer.push(b"NE]"), Vec::<String>::new());
        assert_eq!(buffer.finish().as_deref(), Some("data: [DONE]"));
        assert_eq!(buffer.finish(), None);
    }

    #[tokio::test]
    async fn the_gate_holds_to_its_limit_and_releases() {
        let g = gate();
        let a = g.acquire(1, std::time::Duration::from_millis(10)).await;
        assert!(a.is_some());
        assert!(g.acquire(1, std::time::Duration::from_millis(60)).await.is_none());
        drop(a);
        assert!(g.acquire(1, std::time::Duration::from_millis(10)).await.is_some());
    }
}
