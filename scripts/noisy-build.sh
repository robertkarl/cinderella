#!/bin/bash
# Simulates a verbose build that dumps tons of output.
# Used to demo local_summarize token savings.

echo "=== Glass Slipper Build System v0.1.0 ==="
echo "Configuration: Release"
echo "Target: arm64-apple-macos15.0"
echo ""

# Dump shakespeare-level verbosity
for i in $(seq 1 200); do
    echo "[$(printf '%3d' $i)/200] Compiling module_${i}.swift ..."
    echo "  → Linking symbols from libFoundation.dylib"
    echo "  → Resolving type conformances for Protocol_${i}"
    echo "  → Optimizing IR: inlining 23 functions, devirtualizing 8 calls"
    echo "  → Emitting object file: .build/arm64/module_${i}.o"
    if (( i % 50 == 0 )); then
        echo ""
        echo "  ⚠ Warning: unused variable 'tempBuffer' in module_${i}.swift:142:9"
        echo "  ⚠ Warning: expression result of type 'String' is unused in module_${i}.swift:287:12"
        echo ""
    fi
done

echo ""
echo "Linking GlassSlipper..."
echo "  → Processing 200 object files"
echo "  → Dead code stripping: removed 14,283 unused symbols"
echo "  → Code signing with identity: Apple Development"
echo "  → Generating dSYM bundle"
echo ""
echo "BUILD SUCCEEDED (200 modules, 4 warnings, 0 errors)"
echo "Total time: 47.3s"
