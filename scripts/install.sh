#!/usr/bin/env bash
#
# Builds the Aether project in release mode and installs the binary to the user's bin path.
#
# Usage:
#   ./scripts/install.sh [--web] [--app] [--bin-dir <path>]
#
# Components:
#   --web        Build only the frontend (web).
#   --app        Build only the Rust application and install the binary.
# With neither flag, both components are built (the default).
#
# Options:
#   --bin-dir <path>   Install the binary here (default: $HOME/.local/bin).
#   -h, --help         Show this help.

set -euo pipefail

BIN_NAME="aether"
BIN_DIR="${HOME}/.local/bin"
WEB_DIR="web"

BUILD_WEB=false
BUILD_APP=false

while [[ $# -gt 0 ]]; do
    case "$1" in
        --web)
            BUILD_WEB=true
            shift
            ;;
        --app)
            BUILD_APP=true
            shift
            ;;
        --bin-dir)
            BIN_DIR="$2"
            shift 2
            ;;
        -h|--help)
            awk 'NR==1 { next } /^#/ { sub(/^#[[:space:]]?/, ""); print; next } { exit }' "$0"
            exit 0
            ;;
        *)
            echo "Unknown argument: $1" >&2
            exit 1
            ;;
    esac
done

# Default to building everything when no component is explicitly selected.
if [[ "${BUILD_WEB}" == false && "${BUILD_APP}" == false ]]; then
    BUILD_WEB=true
    BUILD_APP=true
fi

# Resolve the workspace root (parent of the scripts directory).
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

if [[ "${BUILD_APP}" == true ]] && ! command -v cargo >/dev/null 2>&1; then
    echo "error: cargo not found. Install the Rust toolchain from https://rustup.rs" >&2
    exit 1
fi

if [[ "${BUILD_WEB}" == true ]] && ! command -v pnpm >/dev/null 2>&1; then
    echo "error: pnpm not found. Install it from https://pnpm.io/installation" >&2
    exit 1
fi

if [[ "${BUILD_WEB}" == true ]]; then
    echo "==> Building the frontend (${WEB_DIR})..."
    pnpm --dir "${WORKSPACE_ROOT}/${WEB_DIR}" install --frozen-lockfile
    pnpm --dir "${WORKSPACE_ROOT}/${WEB_DIR}" run build
fi

if [[ "${BUILD_APP}" == true ]]; then
    echo "==> Building ${BIN_NAME} in release mode..."
    cargo build --release --package "${BIN_NAME}" --manifest-path "${WORKSPACE_ROOT}/Cargo.toml"

    BUILT_BIN="${WORKSPACE_ROOT}/target/release/${BIN_NAME}"
    if [[ ! -f "${BUILT_BIN}" ]]; then
        echo "error: expected binary not found at ${BUILT_BIN}" >&2
        exit 1
    fi

    echo "==> Installing to ${BIN_DIR}/${BIN_NAME}"
    mkdir -p "${BIN_DIR}"
    install -m 0755 "${BUILT_BIN}" "${BIN_DIR}/${BIN_NAME}"

    # Warn if the install directory isn't on PATH.
    case ":${PATH}:" in
        *":${BIN_DIR}:"*)
            echo "${BIN_NAME} is installed and ${BIN_DIR} is on your PATH."
            echo "Run '${BIN_NAME} --help' to get started."
            ;;
        *)
            echo "note: ${BIN_DIR} is not on your PATH."
            echo "Add this to your shell profile (e.g. ~/.zshrc):"
            echo ""
            echo "    export PATH=\"${BIN_DIR}:\$PATH\""
            ;;
    esac
fi

echo "==> Done."
