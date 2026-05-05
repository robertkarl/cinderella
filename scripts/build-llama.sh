#!/usr/bin/env bash
#
# build-llama.sh — Build a portable arm64 llama-server from pinned llama.cpp source.
#
# Output: build/llama-server (arm64, Metal, no Homebrew runtime deps)
#
# Requirements: Xcode command-line tools (cmake, clang, metal compiler).
# Does NOT require Homebrew at build time or runtime.
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
BUILD_DIR="$REPO_ROOT/build/llama-build"
OUTPUT_DIR="$REPO_ROOT/build"

# Pinned llama.cpp release tag — update deliberately, not automatically.
LLAMA_CPP_TAG="b5270"
LLAMA_CPP_REPO="https://github.com/ggml-org/llama.cpp.git"

# Target architecture
ARCH="arm64"
MACOS_MIN="15.0"

echo "=== build-llama.sh ==="
echo "Tag:    $LLAMA_CPP_TAG"
echo "Arch:   $ARCH"
echo "macOS:  >= $MACOS_MIN"
echo ""

# Clone or update pinned source
LLAMA_SRC="$BUILD_DIR/llama.cpp"
if [ -d "$LLAMA_SRC" ]; then
    echo "Source exists at $LLAMA_SRC"
    cd "$LLAMA_SRC"
    CURRENT_TAG=$(git describe --tags --exact-match 2>/dev/null || echo "none")
    if [ "$CURRENT_TAG" != "$LLAMA_CPP_TAG" ]; then
        echo "Tag mismatch ($CURRENT_TAG != $LLAMA_CPP_TAG), re-fetching..."
        git fetch --tags
        git checkout "$LLAMA_CPP_TAG"
    fi
else
    echo "Cloning llama.cpp at $LLAMA_CPP_TAG..."
    mkdir -p "$BUILD_DIR"
    git clone --depth 1 --branch "$LLAMA_CPP_TAG" "$LLAMA_CPP_REPO" "$LLAMA_SRC"
fi

cd "$LLAMA_SRC"

# Build with CMake — Metal enabled, no Homebrew, static where possible
CMAKE_BUILD="$LLAMA_SRC/build-release"
rm -rf "$CMAKE_BUILD"
mkdir -p "$CMAKE_BUILD"

echo ""
echo "=== CMake configure ==="
cmake -B "$CMAKE_BUILD" \
    -DCMAKE_BUILD_TYPE=Release \
    -DCMAKE_OSX_ARCHITECTURES="$ARCH" \
    -DCMAKE_OSX_DEPLOYMENT_TARGET="$MACOS_MIN" \
    -DGGML_METAL=ON \
    -DGGML_ACCELERATE=ON \
    -DGGML_BLAS=OFF \
    -DLLAMA_CURL=OFF \
    -DLLAMA_BUILD_TESTS=OFF \
    -DLLAMA_BUILD_EXAMPLES=OFF \
    -DLLAMA_BUILD_SERVER=ON \
    -DBUILD_SHARED_LIBS=OFF \
    -DCMAKE_INSTALL_PREFIX="$CMAKE_BUILD/install"

echo ""
echo "=== CMake build ==="
cmake --build "$CMAKE_BUILD" --target llama-server -j "$(sysctl -n hw.ncpu)"

# Copy output
BUILT_BINARY="$CMAKE_BUILD/bin/llama-server"
if [ ! -f "$BUILT_BINARY" ]; then
    echo "ERROR: llama-server not found at $BUILT_BINARY"
    echo "Checking alternative locations..."
    find "$CMAKE_BUILD" -name "llama-server" -type f 2>/dev/null
    exit 1
fi

cp "$BUILT_BINARY" "$OUTPUT_DIR/llama-server"
echo ""
echo "=== Output ==="
echo "Binary: $OUTPUT_DIR/llama-server"
ls -lh "$OUTPUT_DIR/llama-server"
file "$OUTPUT_DIR/llama-server"

# Verify: no Homebrew runtime dependencies
echo ""
echo "=== Dependency verification ==="
OTOOL_OUT=$(otool -L "$OUTPUT_DIR/llama-server")
echo "$OTOOL_OUT"

if echo "$OTOOL_OUT" | grep -q "/opt/homebrew"; then
    echo ""
    echo "FAIL: llama-server has /opt/homebrew runtime dependencies!"
    echo "These must be eliminated before the binary can ship."
    exit 1
fi

if echo "$OTOOL_OUT" | grep -q "/usr/local"; then
    echo ""
    echo "WARN: llama-server links to /usr/local — verify these are not Homebrew."
    # /usr/local/lib may be acceptable if it's system-provided, but flag it
fi

echo ""
echo "PASS: No /opt/homebrew dependencies detected."
echo ""

# Verify architecture
ARCH_CHECK=$(lipo -archs "$OUTPUT_DIR/llama-server" 2>/dev/null || file "$OUTPUT_DIR/llama-server")
echo "Architecture: $ARCH_CHECK"
if ! echo "$ARCH_CHECK" | grep -q "arm64"; then
    echo "FAIL: Binary is not arm64!"
    exit 1
fi

echo ""
echo "=== Metal shader ==="
# llama.cpp embeds Metal shaders at build time when GGML_METAL=ON + static build.
# Check if a .metal or .metallib file was produced that needs bundling.
METAL_FILE=$(find "$CMAKE_BUILD" -name "*.metallib" -o -name "default.metal" 2>/dev/null | head -1)
if [ -n "$METAL_FILE" ]; then
    cp "$METAL_FILE" "$OUTPUT_DIR/"
    echo "Metal resource: $OUTPUT_DIR/$(basename "$METAL_FILE")"
else
    echo "No separate Metal resource found (likely embedded in binary)."
fi

echo ""
echo "=== build-llama.sh complete ==="
echo "Next: embed $OUTPUT_DIR/llama-server into Cinderella.app/Contents/MacOS/"
