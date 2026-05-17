#!/bin/bash
# Quick status snapshot for llama-server: process, memory, KV cache, GPU

set -euo pipefail

PORT=${1:-8081}

echo "=== llama-server process ==="
if pgrep -x llama-server > /dev/null; then
    ps -o pid,rss,vsz,%mem,%cpu,etime,command -p "$(pgrep -x llama-server)"
else
    echo "not running"
    exit 1
fi

echo ""
echo "=== system memory ==="
vm_stat | head -10

echo ""
echo "=== GPU/Metal memory (from vmmap) ==="
vmmap "$(pgrep -x llama-server)" 2>/dev/null | grep -E "^(TOTAL|IOKit|__DATA)" | head -10

echo ""
echo "=== llama-server /metrics (KV cache) ==="
if curl -sf "http://localhost:${PORT}/metrics" > /dev/null 2>&1; then
    curl -sf "http://localhost:${PORT}/metrics" | grep -E "^(llama_kv_cache|llama_requests|llama_tokens)" | head -20
else
    echo "metrics endpoint not responding on port ${PORT}"
fi

echo ""
echo "=== memory pressure ==="
sysctl -n kern.memorystatus_level 2>/dev/null || echo "unavailable"
