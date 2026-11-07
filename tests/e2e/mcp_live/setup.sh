#!/usr/bin/env bash
# Makes the throwaway instance mcp_live_e2e.py talks to: a configuration, the core database and an
# organization "mcp_org". No plugin is loaded: the script loads and installs `mcpplug` through the MCP tools.
#
#   DB_PORT=8000 ./setup.sh        # SurrealDB must be running there (user root, password root)
#
# For an empty database; run it again after `surreal` was restarted in memory. No server is needed:
# `aether --mcp` opens the database itself.
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
bin="${AETHER_BIN:-$here/../../../target/debug/aether}"
port="${DB_PORT:-8000}"
cat > "$here/aether.toml" <<TOML
app_dir = "$here/app_dir"
[core]
instance_name = "MCP live e2e"
[server]
host = "127.0.0.1"
port = 17892
[configuration]
is_development_mode = true
is_development_with_assets = false
[tenancy]
mode = "single"
single_org_name = "MCP Org"
single_org_db_name = "mcp_org"
[database]
host = "127.0.0.1"
port = $port
user = "root"
password = "root"
namespace = "mcp_e2e"
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
a --seed >/dev/null
echo "Ready. Run: python3 mcp_live_e2e.py"
