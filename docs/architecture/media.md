---
title: "Media storage"
description: "Uploaded files go to one media backend, chosen by the operator in aether.toml under [media]. Plugins never pick or configure a backend; they ask the kernel to store and fetch files."
section: architecture
status: stable
order: 270
---
# Media storage

Uploaded files go to one **media backend**, chosen by the operator in
`aether.toml` under `[media]`. Plugins never pick or configure a backend; they
ask the kernel to store and fetch files.

```toml
[media]
backend = "local"          # "local" | "s3"

[media.local]              # default: <app_dir>/media, overridden by --media-dir
base_path = "media"

[media.s3]                 # required when backend = "s3"
bucket = "aether-media"
region = "us-east-1"
endpoint = "http://127.0.0.1:3900"   # Garage, MinIO, … (omit for AWS)
access_key_id = "..."                # or AWS_ACCESS_KEY_ID
secret_access_key = "..."            # or AWS_SECRET_ACCESS_KEY
allow_http = false
virtual_hosted_style = false
prefix = "aether"                    # every key is stored under this prefix
```

`--media-dir <PATH>` sets the local directory for one run. It is an error when
the configured backend is not `local`. Relative paths in the config file are
relative to the file.

## Integrations

`facets/storage` (`aether_storage`) defines the contract every integration
implements, `MediaBackend`:

| Method | Behaviour |
|---|---|
| `put(key, bytes)` | store, replacing any existing object |
| `get(key)` | full contents, `NotFound` if absent |
| `head(key)` | size and e-tag, `NotFound` if absent |
| `delete(key)` | idempotent |
| `list(prefix)` | objects under a prefix |
| `exists(key)` | provided, built on `head` |

Keys are validated `MediaKey`s (`/`-separated, no empty, `.` or `..` segments),
so a key cannot escape a backend's root.

`ObjectStoreBackend` adapts any `object_store` implementation, so the
`local_storage` and `s3_storage` bridges only build a store. A bridge for a
non-object-store service (Nextcloud, Google Drive) implements `MediaBackend`
directly and gets a variant in `MediaBackendKind` plus a `[media.<name>]` table.

## When the main storage is down

With a remote backend (`s3`), a network fault should not stop uploads. The kernel wraps the backend in a
**fallback**:

```toml
[media.fallback]
# enabled = true                      # default: on for every backend but "local"
base_path = "media_pending"           # default: <app_dir>/media_pending
retry_seconds = 30                    # how often waiting files are moved
write_timeout_seconds = 20            # how long S3 gets to take a file before it is kept locally
```

- **Upload.** The file goes to the main backend. If that cannot be reached (connection error, timeout), the file is
  written to local disk and noted in `media_pending` (core database), and the upload succeeds. A refusal that waiting
  cannot fix (a bad key, a missing bucket setting) is still an error.
- **Reading.** Reads try the main backend first and then the local copy, so a file is readable the moment it is stored,
  wherever it is. Listing merges both.
- **Moving.** A background job tries every `retry_seconds`: it uploads each waiting file, deletes the local copy and the
  `media_pending` row, and stops at the first outage. `attempts` and `last_error` on the row say what is stuck.
- **The reference.** A file's key names the file, not the place (`media/2026/05/<random>.pdf`), so nothing in the
  database that points at a file changes when it moves from local disk to S3.
- **Deleting** a waiting file removes the local copy and the row.

Local storage (`bridge local_storage`) is enabled from the first start (`--init` and every seeding afterwards), because
the fallback depends on it.

## Per-organization storage

Creating the organization (`aether --init`) also creates its place to keep
files, in both locations:

- **Files:** `<app_dir>/orgs/<organization>/` with a `conf/` folder.
- **Media:** `orgs/<organization>/` in the media backend. For `local` that is a
  directory under the media directory. For `s3` it is a key prefix inside the
  configured bucket. One shared bucket with a prefix per organization is the
  usual S3 layout: it needs no permission to create buckets and has no
  per-account bucket limit.

A small marker object (`.aether-org`) is written through the backend, so the
location exists (object stores have no empty folders) and a misconfigured or
read-only backend fails when the organization is created, not at the first
upload. Everything is idempotent.

Plugins and the kernel reach an organization's media through a scoped view
(`AppState::org_media`), in which every key is stored under that prefix, so one
organization cannot name, list or read another's files.

For an organization created before this existed, run
`aether --provision-org <db_name>`.

## Files a plugin guards

Every stored file is readable by every member of the organization who knows its key, as the section above describes.
A plugin that keeps files with permissions of their own (a drive, a document library) can change that for the files it
stores. It declares which of its functions decides:

```toml
[media_guard]
function = "can_read_media"
```

and, with the capability `media::protect`, puts each file's key under its guard after the upload. From then on the
download (`GET /api/media/<key>`), the extracted text (`/api/media-text/<key>`) and the thumbnail of that key are served
only when the plugin's function answers `{ "allow": true }`. The kernel calls it **as the person who asks**, with
`{ "key": "media/…" }`, so it can use that person's identity and the plugin's own records to decide. Answering anything
else, an error, or the plugin being uninstalled or switched off, means the file is not served (403).

| Command | Capability | What it does |
|---|---|---|
| `media::protect { key }` | `media::protect` | Put the key under this plugin's guard. Again for the same plugin is harmless; a key another plugin guards is refused. The plugin must declare `[media_guard]` |
| `media::store { name, base64 }` | `media::store` | Put a file the plugin holds (an attachment fetched from a mail server) into the media store. The kernel sniffs the type and refuses what an upload refuses; pictures are shrunk. The key is guarded for this plugin and readable only by the person the call runs for until the plugin `media::protect`s it. Answers `{ key, name, mime, size, sha256 }` |
| `media::read { key }` | `media::read` | The bytes (`{ base64, size }`) of a file this plugin guards: one it stored, or one the person uploaded with `guard=<plugin>` |
| `media::release { key }` | `media::release` | Take a key this plugin guards out of its guard, so every member can read it again |

Guards are rows of `media_guards` in the organization database (migrations `040` and `041`). Keys nobody guards are unchanged.

**Guarded from the first moment.** `POST /api/media?name=…&guard=<plugin>` stores the file and guards it for that plugin
(installed, switched on, and declaring `[media_guard]`; anything else is refused with `400`) before the answer is sent.
Until the plugin calls `media::protect` on the key, only the person who uploaded it can read it; `media::protect` then hands
the question over to the plugin's `[media_guard]` function. A file uploaded without `guard` is open to every member until
the plugin protects it, as before, so a plugin that keeps private files passes `guard` and no window is left. The web app's
`<files guard="true">` widget does this ([Pages](../plugin/pages.md)). A guard does not change what an upload may be (see
the upload limits above).
