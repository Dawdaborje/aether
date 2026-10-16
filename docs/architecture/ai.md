---
title: "AI (language models)"
description: "A plugin asks a language model a question with one command, ai::complete, and gets the same answer shape whichever provider answered."
section: architecture
status: stable
order: 30
---
# AI (language models)

A plugin asks a language model a question with one command, `ai::complete`, and gets the same answer
shape whichever provider answered. A provider is a bridge under `bridges/ai/` that runs inside the
kernel and holds the organization's credentials; Ollama, an OpenAI-compatible server and Anthropic are
built in. The plugin never holds a key or an address.

```rust
let answer = aether_sdk::ai::complete(&json!({
    "system": "Answer in one sentence.", "prompt": note, "max_tokens": 80,
}))?;
// { "text": "...", "model": "llama3.2", "provider": "ollama", "finish_reason": "stop",
//   "usage": { "input_tokens": 41, "output_tokens": 17 } }
```

Needs the capability `ai::complete` (`ai::embed` for embeddings). The conversation is `messages`
(`[{ role: "system" | "user" | "assistant", content }]`) or, as shorthand, a `prompt` and an optional
`system`. Optional: `model`, `max_tokens`, `temperature` (0 to 2), `json` (ask for one JSON object).
`finish_reason` is `length` when the answer was cut at `max_tokens`.

| Command | What it does |
|---|---|
| `ai::complete` | answer a conversation, waiting up to `ai.sync_ceiling_secs` |
| `ai::submit` / `ai::result` | the same as a queued job, with a call back (below) |
| `ai::embed` | `{ input: text \| [text], model? }` answers `{ embeddings, model, provider, usage }` (not Anthropic) |
| `ai::index` / `ai::search` / `ai::forget` | remember text and find it by meaning, see below |
| `ai::status` | `{ enabled, provider, model, local_only, limits, reachable?, models?, error? }` |

## Off until an organization turns it on

A prompt leaves the plugin and, with a remote provider, the organization's network. So `ai.enabled`
is **off** by default and every command refuses until it is on (Settings → AI assistant; the global
default can be overridden per organization). Beyond that:

* `ai.local_only` refuses any provider that is not local (only `ollama` is).
* `ai.daily_tokens_org` and `ai.daily_tokens_user` stop requests for the rest of the UTC day once that
  many tokens (in plus out) are used. The call that crosses the line is allowed: tokens are known only
  afterwards. 0 is no limit.
* Every call adds a row to the organization's `ai_usage` table: who, plugin, kind, provider, model,
  token counts, time, whether it worked. **Never the prompt or the answer**, in that table or in the log.

## Long answers: a job, and a call back

A plugin call can wait only a few seconds, so `ai::complete` gives up at `ai.sync_ceiling_secs` (at
most 9). A longer answer is a job:

```rust
ai::submit(&json!({ "prompt": text, "on_done": "summary_ready", "ref": record_id }))?;

handler! { fn summary_ready(event: Value) -> Value {
    if event["status"] == "done" {
        let answer = ai::result(&json!({ "job_id": event["job_id"] }))?;
        // answer["result"]["text"] ...
    }
    Ok(json!({}))
} }
```

`submit` answers at once with `{ job_id, status }`. The job runs in the `ai` queue (up to 2 tries; a
refusal that cannot be helped, such as a bad key, is not retried), keeps the answer in `ai_result`, and
runs the `on_done` function of the plugin that asked with `{ event: "ai.done", job_id, status, error,
usage, ref }`. The answer is not in that payload. `ai::result` answers only the plugin that submitted the
job; for any other it says `none`. Unlike OCR, an answer is never shared between requests: a model's
answer is not repeatable.

## Settings

Global defaults with an organization override, clamped to hard caps no setting can pass:

| Key | Default | Hard cap | Meaning |
|---|---|---|---|
| `ai.enabled` | off | | every command refuses when off |
| `ai.provider` | ollama | | `ollama`, `openai_compatible` or `anthropic` |
| `ai.model`, `ai.embed_model` | empty | | used when a request names none; empty uses the provider's own default |
| `ai.local_only` | off | | refuse non-local providers |
| `ai.max_tokens` | 1024 | 8192 | a request can ask for fewer, never more |
| `ai.max_prompt_kb` | 64 | 512 | longer conversations are refused |
| `ai.sync_ceiling_secs` | 8 | 9 | a plugin call is cut off after 10 s |
| `ai.job_timeout_secs` | 100 | 110 | a job is stopped when its lease runs out |
| `ai.max_concurrent` | 2 | 8 | calls at once in the process; others wait 2 s, then are told it is busy |
| `ai.max_queued` | 50 | 1000 | queued requests; more are refused until some finish |
| `ai.daily_tokens_org`, `ai.daily_tokens_user` | 0 | | token budgets per UTC day |

The provider has its own settings, `bridge.<provider>.*`: `base_url`, `default_model`,
`default_embed_model`, and `api_key` (secret, stored encrypted, never shown again) for
`openai_compatible` and `anthropic`. `openai_compatible` covers OpenAI, OpenRouter, Groq, vLLM,
LM Studio, llama.cpp's server and anything else with the same chat API; its `base_url` includes the
version (`https://api.openai.com/v1`). Anthropic has no embeddings.

## Search your own documents: `ai::index`, `ai::search`

For policy questions and the like, a plugin gives the kernel text to remember and later asks which pieces
are closest in meaning to a question (needs `ai::embed`):

```rust
let parsed = ocr::parse(&json!({ "media_key": key }))?;               // text out of an uploaded policy PDF
ai::index(&json!({ "source": "policy/leave", "text": parsed["text"] }))?;

let hits = ai::search(&json!({ "query": "How many days of carry-over?", "k": 3 }))?;
let context: Vec<_> = hits.as_array().into_iter().flatten().map(|h| h["text"].as_str().unwrap_or("")).collect();
let answer = ai::complete(&json!({ "system": "Answer only from the excerpts.", "prompt": format!("{}\n\nQuestion: ...", context.join("\n---\n")) }))?;
```

The text is cut into pieces of about 1 KB, embedded with the organization's embedding model and stored in
`ai_chunk`, per plugin: a plugin searches only what it indexed itself. Indexing a `source` again replaces its
pieces unless `append` is set. One call takes about 16 KB (a call can wait only a few seconds); feed a long
document in parts with `append`. `ai::forget { source }` removes a source. Vectors of different embedding models
do not compare, so a search finds only pieces made with the model currently set in `ai.embed_model` (or the provider's default).

## The assistant: plugin functions as tools

The web app's assistant answers questions with the person's own data by asking for **tools**: functions a plugin
declares in `plugin.toml`.

```toml
[[ai_tool]]
name = "leave_balance"
description = "How many days of each leave type the person has left this year"
function = "assistant_leave_balance"
schema = { type = "object", properties = { year = { type = "integer" } } }
writes = false
```

`POST /api/ui/ai/chat { messages, context?, stream? }` (a signed-in member; `messages` are the person's `user` and
`assistant` turns) runs the loop: the kernel offers the model the tools of the enabled plugins installed in the
organization (named `<plugin>__<tool>`), the model asks for some, the kernel runs each plugin function **as the person
chatting** (installed, enabled, the plugin's capabilities, the person's roles and record rules all apply, and the call
is in the plugin call audit), the results go back to the model, and so on until it answers in words: at most 6 rounds,
4 tool calls a round.

The answer is `{ text, model, steps: [{ tool, ok }], proposed_actions, incomplete, usage }`.

* **Writes are proposed, never run.** A tool with `writes = true` comes back in `proposed_actions` as
  `{ plugin, tool, function, arguments }`; the person is shown it and, on confirming, the web app calls that plugin
  function itself with the person's own session. The model is told it was only proposed.
* **Tool results are data.** They are cut at 8 KB and handed to the model marked as data, with a standing instruction not
  to follow instructions found in them. This reduces, and does not remove, the risk that text stored in a record
  steers the model, which is why tools are read-only by default and run with the person's rights.
* A model that does not support tools is asked again without them and answers in plain words.
* **Streaming.** With `stream: true` the answer is server-sent events: `step` as each tool is used, `delta` with the
  model's words as it writes them, then `message` (the whole answer, as above) and `done`. A round that ends in a tool
  call may have sent a few words first; the client replaces them with `message.text`. Providers stream natively (Ollama's
  newline-delimited JSON, the OpenAI-compatible and Anthropic event streams).

Plugins' own `ai::complete` calls carry no tools and stream nothing: a plugin function returns one value.

## What it does not do

* **Prompts are the plugin's responsibility.** Send the least that answers the question. The answer is
  untrusted text: text a model was shown (a resume, a note) can steer it, so never run an answer as a command
  or trust it as a permission decision.
* A queued request's prompt sits in the job queue until the queue is cleaned, as every job payload does.
* Anthropic has no embeddings, so `ai::index` and `ai::search` need Ollama or an OpenAI-compatible server.
