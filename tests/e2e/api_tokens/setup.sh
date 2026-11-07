#!/usr/bin/env bash
# Makes the throwaway instance the end-to-end scripts in this folder talk to: a configuration, the
# core database, an organization "tok_org" with an administrator (owner) and an ordinary member (bob),
# and the test plugin `tokplug`. Then start the server: ./serve.sh
#
#   DB_PORT=8000 ./setup.sh        # SurrealDB must be running there (user root, password root)
#
# It is meant for an empty database; run it again after `surreal` was restarted in memory.
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
bin="${AETHER_BIN:-$here/../../../target/debug/aether}"
port="${DB_PORT:-8000}"
cat > "$here/aether.toml" <<TOML
app_dir = "$here/app_dir"
[core]
instance_name = "Token e2e"
[server]
host = "127.0.0.1"
port = 17890
[configuration]
is_development_mode = true
is_development_with_assets = false
[tenancy]
mode = "single"
single_org_name = "Tok Org"
single_org_db_name = "tok_org"
[database]
host = "127.0.0.1"
port = $port
user = "root"
password = "root"
namespace = "tok_e2e"
[cache]
backend = "moka"
default_ttl_secs = 300
max_entries = 10000
max_value_bytes = 1048576
[media]
backend = "local"
TOML
cd "$here"
a() { "$bin" -c "$here/aether.toml" "$@"; }
a --init --admin-username admin --admin-email admin@test.local --admin-password Test-pass-123 >/dev/null
a --create-user --username owner --email owner@test.local --password Test-pass-123 >/dev/null
a --sync-models "$here/tokplug" >/dev/null
a --load-plugins "$here/tokplug" >/dev/null
a --install-plugin tokplug --org tok_org >/dev/null
a --create-user --username bob --email bob@test.local --password Test-pass-123 --org-db-name tok_org >/dev/null
a --seed >/dev/null
echo "Ready. Start the server with ./serve.sh, then run the *_e2e.py scripts."
