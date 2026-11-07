---
title: "Streaming AI results"
description: "How the assistant streams today, and the plan to count streamed tokens, stream jobs to a person, and show growing answers on pages."
section: architecture
status: proposal
order: 31
---
# Plan: streaming AI results

Status: **plan**. Part 1 is built; parts 2 to 6 are not. See [AI](ai.md) for what exists.

## What streams today

* The three provider bridges stream a chat (`ActionBridge::stream`): Ollama's newline-delimited JSON, the OpenAI-compatible
  and Anthropic event streams, with tool calls assembled from their pieces.
* `POST /api/ui/ai/chat` with `stream: true` sends server-sent events: `step`, `delta`, `message`, `done`. The web app's
  assistant (`/assistant`) shows the words as they arrive.
* Closing the chat or pressing Stop drops the stream, which now cancels the provider request.

## What does not stream, and why

| Where | Why not |
|---|---|
| `ai::complete` in a plugin | a plugin function returns one value, and a call is cut off after 10 s |
| `ai::submit` / `ai::result` | the job stores the answer when it ends; nothing is visible before |
| Pages built from plugin XML | no field or widget shows a growing answer |
| Usage and budgets | tokens are counted when a call ends; a cancelled stream records nothing |

## 1. Done: the chat endpoint (above)

## 2. Count what was streamed

Today a stream the person cancels records no row in `ai_usage`, and a budget is only checked before a round.

* The bridge's `stream` already sees every piece. Return a partial result on cancel: `ChatResponse { finish_reason: "cancelled" }`
  with the tokens counted so far (the provider's own count where it sends one, otherwise `estimate_tokens` of the text).
* The chat backend records usage in a guard that runs when the future is dropped, so a cancelled answer is still counted.
* While streaming, add the running count to today's total and stop the stream (`finish_reason: "budget"`) when a budget is
  crossed, instead of letting one long answer overshoot it.
* Test: a fake provider that streams forever, a budget of 100 tokens, and a dropped receiver; the usage row exists in both cases.

## 3. Jobs that stream to a person: `ai::submit { stream: true }`

A plugin cannot hold a connection, but it can start a job and name who should watch it.

* `ai::submit` accepts `stream: true`. The job runs `bridge.stream` instead of `call`, and writes `partial` (the text so far)
  to its `ai_result` row at most every 250 ms (a column, not a new table; `result` stays what it is when the job ends).
* New route `GET /api/ui/ai/jobs/{job_id}/events` (server-sent events) for the **person who submitted the job**
  (`requested_by`), no one else: it replays `partial` first, so a late or reconnecting client catches up, then sends `delta`
  events until `done` or `error`. A job's events are served from the node that runs it; with several nodes the route reads the
  row and polls it every 250 ms, which is the simple version that needs no cross-node channel.
* The plugin still gets its single `ai.done` call back with the whole answer, and nothing about the existing contract changes.
* Cancel: `POST /api/ui/ai/jobs/{job_id}/cancel` by the same person, which sets the row to `cancelled` and which the job checks
  between pieces.
* Test: a fake provider streaming five pieces, a client joining after the third and receiving the replay then the last two;
  another user getting 404 for the same job.

## 4. A page widget that shows a streaming answer

Plugins write pages in XML and cannot write Svelte, so the widget is a kernel one.

* `<ai-answer job="{job_id}"/>` for a detail page: connects to the route in part 3 and shows the text as it grows, with Stop.
* `<ai-assist field="notes" prompt="Summarise the thread"/>` on a form: a button that submits a job with the prompt and the
  record's text, streams the answer into a preview, and fills the field only when the person accepts it.
* Both reuse `readEventStream` and the store in `web/src/lib/ai/chat.svelte.ts`.

## 5. Plugin-side: without holding a stream

* SDK: `ai::submit_stream(&payload)` is `submit` with `stream: true`, returning the `job_id` for the page to watch. No new
  capability: `ai::complete` already covers it.
* A plugin that wants the words as they come can name an `on_delta` function (called at most every 500 ms with
  `{ job_id, text_so_far }`). This is off by default because every call is a plugin run; it exists for plugins that must act on
  partial text, such as one that stops a generation when a keyword appears.

## 6. Hardening

* A stream holds a connection and a place in `ai.max_concurrent` for as long as the answer takes. Add `ai.max_streams_per_user`
  (default 2) so one person cannot hold all places.
* Idle limit: a stream with no piece for 60 s ends with `error` (a stalled provider), instead of holding its place until the
  120 s lease.
* The `x-accel-buffering: no` header is set; document the equivalent for other reverse proxies (response buffering off,
  read timeout above 60 s).
* Abort test in CI for each provider against its mock: a client that disconnects mid-stream leaves no task running.

## Order

2 first (it is a correctness gap: spend that is not counted), then 3 and 4 together (they are one feature seen from the server
and the page), then 5 and 6. Parts 3 and 4 need a migration (`partial` on `ai_result`) and a setting
(`ai.max_streams_per_user`); nothing else changes an existing contract.
