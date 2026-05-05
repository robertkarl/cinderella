#!/usr/bin/env bash
#
# notarize-macos.sh — Submit, staple, and verify the Cinderella DMG.
#
# Requirements:
#   DEVELOPER_ID      — Developer ID Application identity (must match package-macos.sh)
#   NOTARY_PROFILE    — Stored notary credential profile name (from `xcrun notarytool store-credentials`)
#
# Before first use:
#   xcrun notarytool store-credentials "cinderella-notary" \
#       --apple-id "you@example.com" \
#       --team-id "YOURTEAMID" \
#       --password "@keychain:AC_PASSWORD"
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
BUILD_DIR="$REPO_ROOT/build"
APP_NAME="Cinderella"
DMG_PATH="$BUILD_DIR/$APP_NAME.dmg"
APP_BUNDLE="$BUILD_DIR/$APP_NAME.app"

NOTARY_PROFILE="${NOTARY_PROFILE:-cinderella-notary}"

echo "=== notarize-macos.sh ==="
echo "DMG:     $DMG_PATH"
echo "Profile: $NOTARY_PROFILE"
echo ""

# Pre-checks
if [ ! -f "$DMG_PATH" ]; then
    echo "FAIL: DMG not found at $DMG_PATH"
    echo "Run scripts/package-macos.sh first."
    exit 1
fi

# Verify the app is signed with a real identity (not ad-hoc)
echo "=== Pre-check: Signature ==="
SIGN_INFO=$(codesign -dvv "$APP_BUNDLE" 2>&1 || true)
if echo "$SIGN_INFO" | grep -q "Signature=adhoc"; then
    echo "FAIL: App is ad-hoc signed. Set DEVELOPER_ID and re-run package-macos.sh."
    exit 1
fi
echo "Signature looks valid (not ad-hoc)."
echo ""

# --- Step 1: Submit for notarization ---
echo "=== Step 1: Submit for notarization ==="
xcrun notarytool submit "$DMG_PATH" \
    --keychain-profile "$NOTARY_PROFILE" \
    --wait

echo ""

# --- Step 2: Staple ---
echo "=== Step 2: Staple ticket to DMG ==="
xcrun stapler staple "$DMG_PATH"
echo ""

# --- Step 3: Verify ---
echo "=== Step 3: Verification gates ==="

echo "--- codesign --verify --deep --strict ---"
codesign --verify --deep --strict "$APP_BUNDLE"
echo "PASS"

echo ""
echo "--- spctl -a -vvv -t exec ---"
spctl -a -vvv -t exec "$APP_BUNDLE"
echo ""

echo "--- xcrun stapler validate ---"
xcrun stapler validate "$DMG_PATH"
echo "PASS"

echo ""
echo "=== notarize-macos.sh complete ==="
echo "The DMG is notarized, stapled, and ready for distribution."
echo ""
echo "Distribution artifact: $DMG_PATH"
