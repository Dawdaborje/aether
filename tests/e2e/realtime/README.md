# Realtime, end to end

`realtime_e2e.py` drives a running server over a real WebSocket: who may connect (session, `Origin`), joining
topics that a plugin authorizes, messages with sequence numbers, resuming after a drop, `resync`, typing events and
their rate limit, presence, being dropped from a topic, and the limits. The plugin is `chatplug`, a tiny Lua chat.

`ui_e2e.mjs` drives the chat page in a real browser (Chromium through Playwright, from `web/node_modules`): panes, the rail (groups, a live unread badge, filter, keys, folding), the virtual message list, live messages between two people, typing, the new-message button, attachments in the viewer panel, folding the rail and a narrow screen. It needs the built web app (`web/build`, from `pnpm build` in `web/`) and `chatplug`'s page `/chat/{channel?}`.

`realtime_e2e.py` needs `pip install websocket-client`. Build the binary (`cargo build -p aether`), have SurrealDB running
(user root, password root), then from this folder:

```sh
DB_PORT=8000 ./setup.sh     # configuration, core database, organization rt_org, users owner and bob, the plugin
./serve.sh                  # the server on 127.0.0.1:17891; leave it running
python3 realtime_e2e.py     # the WebSocket, 37 checks
node ui_e2e.mjs             # the page in a browser, 34 checks (CHROMIUM=/usr/bin/chromium, SHOTS=dir keeps screenshots)
```

A role is cached by the running server, so after `--grant-role` (setup does it for `bob`) start the server once more.

`setup.sh` is for an empty database; with `surreal start memory` run it again after each restart of SurrealDB.
After changing the plugin: bump its version, `--load-plugins` it again and `--upgrade-plugin chatplug --org rt_org`.

`app_dir/` and `aether.toml` are throwaway; do not commit them.
