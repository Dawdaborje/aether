---
title: "Document parsing (OCR)"
description: "A plugin reads the text of a stored file with one command, ocr::parse, and gets the same result whichever engine did the work."
section: architecture
status: stable
order: 300
---
# Document parsing (OCR)

A plugin reads the text of a stored file with one command, `ocr::parse`, and gets the same result
whichever engine did the work. An engine is a bridge under `bridges/document_parsing/` that the
kernel runs in a **separate process**; LiteParse is the first.

```rust
let parsed = aether_sdk::ocr::parse(&json!({ "media_key": key, "language": "eng" }))?;
```

Needs the capability `ocr::parse`. The file is named by its media `key` (what a `file` or `image`
field holds). The kernel reads it from the organization's storage, writes it to a private work
folder, runs the engine, deletes the folder, and answers:

```json
{ "text": "...", "total_pages": 3, "language": "eng", "engine": "liteparse", "ocr_used": true,
  "truncated": false, "warnings": [],
  "pages": [{ "page": 1, "width": 612, "height": 792, "text": "...", "confidence": 0.91,
              "blocks": [{ "kind": "heading", "text": "...", "bbox": [x1, y1, x2, y2] }] }] }
```

`ocr::status` answers `{ found, version, identity_ok, enabled, limits }`.

## Long files: a job, and a call back

A plugin call can wait only a few seconds. A big or scanned file is parsed as a job:

```rust
// in the plugin, when the file is attached
ocr::submit(&json!({ "media_key": key, "on_done": "resume_read", "ref": application_id }))?;

// the plugin's own function, called when the parse ends
handler! { fn resume_read(event: Value) -> Value {
    if event["status"] == "done" {
        let parsed = ocr::result(&json!({ "cache_key": event["cache_key"] }))?;
        // parsed["result"]["text"] ...
    }
    Ok(json!({}))
} }
```

`submit` answers at once with `{ status, cache_key, cached }`. The job runs in the `ocr` queue as a worker
(up to 2 tries; a failure that cannot be helped, such as an unsupported file, is not retried), stores the
result in the organization's `ocr_result` table, and runs the `on_done` function of the plugin that asked,
with `{ event: "ocr.done", cache_key, media_key, status, error, pages, ocr_used, ref }`. The text is not in
that payload: fetch it with `ocr::result`. `ref` is returned unchanged, so the function knows which record
the file belongs to.

A result is kept for ever under `(media key, engine, options)`: a media key never holds different bytes, so
the same file with the same options is parsed once, whether it was asked for with `parse` or `submit`, and
by whichever plugin. Submitting a file that is already `done` calls `on_done` straight away. A `failed` one
is tried again when submitted again.

## Everything that bounds a parse is a setting

Global defaults with an organization override (Settings → Document parsing), clamped to hard caps no
setting can pass:

| Key | Default | Hard cap | Meaning |
|---|---|---|---|
| `ocr.enabled` | on | | `ocr::parse` refuses when off |
| `ocr.max_input_mb` | 25 | 100 | larger files are not read into memory at all |
| `ocr.max_pages` | 100 | 1000 | pages after this are ignored, and the result says so |
| `ocr.max_text_kb` | 2048 | 16384 | longer text is cut and `truncated` is set |
| `ocr.sync_ceiling_secs` | 8 | 9 | a plugin call is cut off after 10 s, so this cannot be higher |
| `ocr.max_concurrent` | 2 | 8 | parses at once in the process; others wait 2 s, then are told it is busy |
| `ocr.max_queued` | 50 | 1000 | parses waiting or running; more are refused until some finish |
| `ocr.engine` | liteparse | | which engine; an unknown name falls back to the default |

The engine has its own settings, `bridge.liteparse.*`: `binary_path`, `default_language`,
`tessdata_path`, `ocr_server_url`, `ocr_server_token` (secret), `dpi`, `max_pages`, `timeout_secs`.
The engine's `max_pages` and the kernel's `ocr.max_pages` combine as the lower of the two.

## Why a child process, and how it is contained

PDFium parses files strangers uploaded; a crash or a hang in it must not reach the kernel. The
LiteParse bridge starts its command line with an empty environment (plus `PATH`, a private `HOME`,
and `TESSDATA_PREFIX` when set), no standard input, the work folder as its directory, a time limit,
a cap on what it may print, and its own process group, which is killed as a whole when time is up
(the program starts LibreOffice for Office files). It refuses any input path outside the kernel's
work folder. `SAL_DISABLE_OPENCL=1` and `SAL_USE_VCLPLUGIN=svp` are set because LibreOffice's GPU
probe aborts the conversion on some machines.

The binary is called `liteparse`. On many systems `lit` is LLVM's test runner, so `status` checks that
`parse --help` offers `--ocr-language` (`identity_ok`).

## Not logged

Calls are logged with the plugin, the file type, the size and the time; never the key, the name or the
text, because scans hold people's papers.
