#!/usr/bin/env bash
#
# package-macos.sh — Build, embed, sign, and package Glass Slipper.app for distribution.
#
# This script:
# 1. Builds the Rust glass-slipper helper (release, arm64)
# 2. Builds llama-server from pinned llama.cpp (if not already built)
# 3. Builds the Swift app via xcodebuild
# 4. Embeds helpers into the app bundle (Contents/MacOS/)
# 5. Copies model-manifest.json into the bundle (Contents/Resources/)
# 6. Verifies otool -L on all embedded binaries (hard gate)
# 7. Signs all nested code then the outer app (deep signing order)
# 8. Creates a DMG
#
# Environment variables:
#   DEVELOPER_ID  — "Developer ID Application: Name (TEAMID)" for signing
#                   If unset, falls back to ad-hoc signing (development only).
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
BUILD_DIR="$REPO_ROOT/build"
APP_NAME="Glass Slipper"
APP_BUNDLE="$BUILD_DIR/$APP_NAME.app"

# Signing identity
IDENTITY="${DEVELOPER_ID:--}"
if [ "$IDENTITY" = "-" ]; then
    echo "WARNING: No DEVELOPER_ID set. Using ad-hoc signing (not distributable)."
    echo "Set DEVELOPER_ID='Developer ID Application: Your Name (TEAMID)' for release."
    echo ""
fi

echo "=== package-macos.sh ==="
echo "Output: $APP_BUNDLE"
echo "Identity: $IDENTITY"
echo ""

# --- Step 1: Build Rust helper ---
echo "=== Step 1: Build Rust glass-slipper helper ==="
cargo build --release --manifest-path "$REPO_ROOT/Cargo.toml" --target aarch64-apple-darwin
RUST_BINARY="$REPO_ROOT/target/aarch64-apple-darwin/release/glass-slipper"
if [ ! -f "$RUST_BINARY" ]; then
    # Fallback to non-target-specific path
    RUST_BINARY="$REPO_ROOT/target/release/glass-slipper"
fi
echo "Rust helper: $RUST_BINARY"
file "$RUST_BINARY"
echo ""

# --- Step 2: Build llama-server (if needed) ---
echo "=== Step 2: Build llama-server ==="
LLAMA_BINARY="$BUILD_DIR/llama-server"
if [ -f "$LLAMA_BINARY" ]; then
    echo "llama-server already built at $LLAMA_BINARY (skipping rebuild)"
else
    "$SCRIPT_DIR/build-llama.sh"
fi
echo ""

# --- Step 3: Build Swift app via xcodebuild ---
echo "=== Step 3: Build Swift app ==="
XCODE_PROJECT="$REPO_ROOT/glass-slipper/GlassSlipper.xcodeproj"
XCODE_DERIVED="$BUILD_DIR/DerivedData"

# Build without code signing (we sign manually with proper identity below)
xcodebuild -project "$XCODE_PROJECT" \
    -scheme GlassSlipper \
    -configuration Release \
    -arch arm64 \
    -derivedDataPath "$XCODE_DERIVED" \
    CODE_SIGN_IDENTITY="" \
    CODE_SIGNING_REQUIRED=NO \
    CODE_SIGNING_ALLOWED=NO \
    ONLY_ACTIVE_ARCH=NO \
    MACOSX_DEPLOYMENT_TARGET=15.0 \
    build 2>&1 | grep -E "(error:|BUILD |warning:.*Run script)" || true

XCODE_APP="$XCODE_DERIVED/Build/Products/Release/GlassSlipper.app"
if [ ! -d "$XCODE_APP" ]; then
    echo "FAIL: Xcode build did not produce $XCODE_APP"
    exit 1
fi
echo "Xcode build: $XCODE_APP"
echo ""

# --- Step 4: Assemble final app bundle ---
echo "=== Step 4: Assemble app bundle ==="
rm -rf "$APP_BUNDLE"
cp -R "$XCODE_APP" "$APP_BUNDLE"

MACOS_DIR="$APP_BUNDLE/Contents/MacOS"

# Embed helpers
# Note: Rust helper is named "glass-slipper-agent" to avoid case-insensitive collision
# with the Swift "Glass Slipper" executable on macOS (HFS+/APFS default).
cp "$RUST_BINARY" "$MACOS_DIR/glass-slipper-agent"
cp "$LLAMA_BINARY" "$MACOS_DIR/llama-server"

# Embed MCP server
MCP_BINARY="$REPO_ROOT/target/aarch64-apple-darwin/release/glass-slipper-mcp"
if [ ! -f "$MCP_BINARY" ]; then
    MCP_BINARY="$REPO_ROOT/target/release/glass-slipper-mcp"
fi
cp "$MCP_BINARY" "$MACOS_DIR/glass-slipper-mcp"

# Copy model manifest to Resources
RESOURCES_DIR="$APP_BUNDLE/Contents/Resources"
mkdir -p "$RESOURCES_DIR"
cp "$REPO_ROOT/model-manifest.json" "$RESOURCES_DIR/model-manifest.json"

# Update Info.plist in the bundle (in case Xcode used the old one)
cp "$REPO_ROOT/glass-slipper/Info.plist" "$APP_BUNDLE/Contents/Info.plist"

echo "App bundle contents:"
ls -la "$MACOS_DIR/"
echo ""

# --- Step 5: Verify otool -L (hard gate) ---
echo "=== Step 5: Dependency verification (hard gate) ==="
GATE_PASS=true
for binary in "$MACOS_DIR/GlassSlipper" "$MACOS_DIR/glass-slipper-agent" "$MACOS_DIR/glass-slipper-mcp" "$MACOS_DIR/llama-server"; do
    if [ ! -f "$binary" ]; then
        echo "FAIL: Missing binary: $binary"
        GATE_PASS=false
        continue
    fi
    echo "--- $(basename "$binary") ---"
    DEPS=$(otool -L "$binary")
    echo "$DEPS" | head -10
    if echo "$DEPS" | grep -q "/opt/homebrew"; then
        echo "FAIL: $(basename "$binary") links to /opt/homebrew!"
        GATE_PASS=false
    fi
    echo ""
done

if [ "$GATE_PASS" = false ]; then
    echo "=== GATE FAILED: Fix dependencies before proceeding ==="
    exit 1
fi
echo "PASS: All binaries free of /opt/homebrew dependencies."
echo ""

# --- Step 6: Code signing ---
echo "=== Step 6: Code signing ==="
# Sign nested helpers first (inside-out order required by Apple)
codesign --force --options runtime --timestamp --sign "$IDENTITY" "$MACOS_DIR/glass-slipper-agent"
codesign --force --options runtime --timestamp --sign "$IDENTITY" "$MACOS_DIR/llama-server"
codesign --force --options runtime --timestamp --sign "$IDENTITY" "$MACOS_DIR/glass-slipper-mcp"

# Sign Frameworks if present
if [ -d "$APP_BUNDLE/Contents/Frameworks" ]; then
    find "$APP_BUNDLE/Contents/Frameworks" -name "*.dylib" -o -name "*.framework" | while read -r fw; do
        codesign --force --options runtime --timestamp --sign "$IDENTITY" "$fw"
    done
fi

# Sign the main app last
codesign --force --options runtime --timestamp --sign "$IDENTITY" "$APP_BUNDLE"

echo "Verifying signature..."
codesign --verify --deep --strict "$APP_BUNDLE"
echo "PASS: Signature valid."
echo ""

# --- Step 7: Create DMG ---
echo "=== Step 7: Create DMG ==="
DMG_PATH="$BUILD_DIR/$APP_NAME.dmg"
rm -f "$DMG_PATH"

# Create a temporary directory for DMG contents
DMG_STAGING="$BUILD_DIR/dmg-staging"
rm -rf "$DMG_STAGING"
mkdir -p "$DMG_STAGING"
cp -R "$APP_BUNDLE" "$DMG_STAGING/"
ln -s /Applications "$DMG_STAGING/Applications"

hdiutil create -volname "$APP_NAME" \
    -srcfolder "$DMG_STAGING" \
    -ov -format UDZO \
    "$DMG_PATH"

rm -rf "$DMG_STAGING"
echo "DMG: $DMG_PATH"
ls -lh "$DMG_PATH"
echo ""

echo "=== package-macos.sh complete ==="
echo ""
echo "Next steps:"
echo "  1. If using Developer ID: run scripts/notarize-macos.sh"
echo "  2. If ad-hoc (development): DMG is ready for local testing"
echo ""
echo "Artifacts:"
echo "  App:  $APP_BUNDLE"
echo "  DMG:  $DMG_PATH"
