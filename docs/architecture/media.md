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

## Per-organization storage

Creating an organization (`aether --create-org`) also creates its place to keep
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
