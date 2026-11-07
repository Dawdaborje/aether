#!/usr/bin/env bash
# Runs the server of the throwaway instance on 127.0.0.1:17890 (see setup.sh).
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec "${AETHER_BIN:-$here/../../../target/debug/aether}" -c "$here/aether.toml" --serve 127.0.0.1:17890
