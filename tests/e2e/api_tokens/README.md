# API tokens, the record API, webhooks and inbound hooks, end to end

Two scripts drive a running server over HTTP, as a person who makes tokens and as a script that uses them.
`api_tokens_e2e.py` (25 checks) is about tokens; `record_api_e2e.py` (about 55 checks) is about the record API
(`/api/v1/...`, CRUD, paging, filters, validation, the rules of an ordinary member, scopes, and the generated
OpenAPI). `webhooks_e2e.py` (about 70 checks) starts a receiver on this machine and checks what it gets: signature,
fields, conditions, retries, replay, switching off, rotation, a test ping, and the refusal of private addresses. `inbound_e2e.py` (about 45 checks) plays the outside system that calls a plugin's hooks: the three ways of proving who
is calling, replays, rotation, switching off, limits, and everything that must answer 404 or 401. `import_e2e.py` (about 90 checks) uploads CSV and XLSX files like a browser and checks imports and exports (dry run, run, upsert, error file, callback, undo, an ordinary member's rules, 3 000 rows, export round trip). All share `e2e_lib.py`.

`api_tokens_e2e.py`: It checks what no unit test can: that a bearer token alone (no cookie, no CSRF header) gets
through on `/api/plugins/…`; that scopes are enforced; that a token is **not** accepted on other endpoints
or to make tokens; that a browser session keeps its CSRF check even when it also sends a token; and that
revoking works at once.

## One-time setup

A tiny Lua plugin (`tokplug`: two functions that call plugins, a model `lead` offered in `[[api]]` with a rules file and a
role, a model `memo` that is not offered, and two events, one of which webhooks may send) in a throwaway instance, with an
administrator `owner` and an ordinary member `bob`. Build the binary (`cargo build -p aether`), have SurrealDB running
(user root, password root), then, from this folder:

```sh
DB_PORT=8000 ./setup.sh     # configuration, core database, organization, users, plugin (about a minute)
./serve.sh                  # the server on 127.0.0.1:17890; leave it running
```

`setup.sh` is for an empty database; with `surreal start memory` run it again after each restart of SurrealDB.

## Running

```sh
python3 api_tokens_e2e.py
python3 record_api_e2e.py     # AETHER_USER2 / AETHER_PASSWORD2 name the second member
python3 webhooks_e2e.py       # RECEIVER_PORT (default 19911) is where its receiver listens
python3 import_e2e.py         # rows accumulate in the instance, so repeated runs get slower
python3 inbound_e2e.py        # AETHER_ORG (default tok_org) is in the hook addresses
```

After changing the plugin: bump its version, `--load-plugins` it again and `--upgrade-plugin tokplug --org tok_org`.

`app_dir/` and `aether.toml` are throwaway; do not commit them.
