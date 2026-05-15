#!/usr/bin/env bash
#
# copy-helpers.sh — Copy Rust helpers and llama-server into the app bundle.
# Called by the "Copy Glass Slipper" Xcode build phase.
#
set -euo pipefail

MACOS_DEST="${BUILT_PRODUCTS_DIR}/${PRODUCT_NAME}.app/Contents/MacOS"
MISSING=0

# glass-slipper-agent (the Rust CLI, renamed to avoid APFS collision)
AGENT="${SRCROOT}/../target/release/glass-slipper"
if [ -x "$AGENT" ]; then
    cp -f "$AGENT" "$MACOS_DEST/glass-slipper-agent"
    echo "Copied glass-slipper-agent into app bundle"
else
    echo "warning: glass-slipper not found at $AGENT — run: cargo build --release"
    MISSING=$((MISSING + 1))
fi

# glass-slipper-mcp
MCP="${SRCROOT}/../target/release/glass-slipper-mcp"
if [ -x "$MCP" ]; then
    cp -f "$MCP" "$MACOS_DEST/glass-slipper-mcp"
    echo "Copied glass-slipper-mcp into app bundle"
else
    echo "warning: glass-slipper-mcp not found at $MCP — run: cargo build --release"
    MISSING=$((MISSING + 1))
fi

# llama-server (pre-built arm64 binary from build-llama.sh)
LLAMA="${SRCROOT}/../build/llama-server"
if [ -x "$LLAMA" ]; then
    cp -f "$LLAMA" "$MACOS_DEST/llama-server"
    echo "Copied llama-server into app bundle"
else
    echo "warning: llama-server not found at $LLAMA — run: scripts/build-llama.sh"
    MISSING=$((MISSING + 1))
fi

if [ "$MISSING" -gt 0 ]; then
    echo "error: $MISSING helper binary(ies) missing — app bundle is incomplete"
    exit 1
fi
