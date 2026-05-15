#!/usr/bin/env python3
"""Benchmark llama-server --parallel throughput for Glass Slipper workloads.

Measures TTFT, generation speed, and amortized throughput across input sizes
and parallel slot counts. Outputs a markdown table to stdout.
"""

try:
    import httpx
except ImportError:
    import sys
    print("ERROR: httpx is required. Install it with:\n  pip install httpx", file=sys.stderr)
    sys.exit(1)

import argparse
import asyncio
import json
import math
import os
import shutil
import subprocess
import sys
import tempfile
import time
from dataclasses import dataclass, field
from datetime import datetime
from pathlib import Path


@dataclass
class RequestResult:
    ttft_s: float
    total_time_s: float
    prompt_tokens: int
    completion_tokens: int


@dataclass
class ConfigResult:
    parallel: int
    input_lines: int
    input_tokens: int  # average prompt tokens from server
    results: list[RequestResult] = field(default_factory=list)


def parse_args() -> argparse.Namespace:
    p = argparse.ArgumentParser(
        description="Benchmark llama-server --parallel throughput",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""\
Examples:
  %(prog)s --model ~/models/Qwen3.5-9B-Q5_K_M.gguf
  %(prog)s --model ~/models/model.gguf --sizes 250,800 --no-warmup --reps 1
  %(prog)s --model ~/models/model.gguf --parallel 1,2,3,4 --ctx-size 65536
""",
    )
    p.add_argument("--model", required=True, help="Path to GGUF model file")
    p.add_argument(
        "--llama-server",
        default=None,
        help="Path to llama-server binary (default: auto-detect)",
    )
    p.add_argument(
        "--sizes",
        default="25,250,500,800",
        help="Comma-separated line counts (default: 25,250,500,800)",
    )
    p.add_argument(
        "--parallel",
        default="1,2,3",
        help="Comma-separated parallel values (default: 1,2,3)",
    )
    p.add_argument("--reps", type=int, default=3, help="Repetitions per config (default: 3)")
    p.add_argument("--no-warmup", action="store_true", help="Skip warmup request after server start")
    p.add_argument("--port", type=int, default=8787, help="llama-server port (default: 8787)")
    p.add_argument("--ctx-size", type=int, default=32768, help="Context size (default: 32768)")
    return p.parse_args()


def find_llama_server() -> str | None:
    """Auto-detect llama-server binary."""
    # Check local build output (from scripts/build-llama.sh)
    repo_root = subprocess.run(
        ["git", "rev-parse", "--show-toplevel"],
        capture_output=True, text=True,
    ).stdout.strip()
    if repo_root:
        build_path = Path(repo_root) / "build" / "llama-server"
        if build_path.is_file() and os.access(build_path, os.X_OK):
            return str(build_path)
    # Check PATH (includes /opt/homebrew/bin)
    which = shutil.which("llama-server")
    if which:
        return which
    return None


def safety_gate() -> None:
    """Exit if llama-server is already running."""
    result = subprocess.run(
        ["pgrep", "-x", "llama-server"],
        capture_output=True,
        text=True,
    )
    if result.returncode == 0:
        pids = result.stdout.strip()
        print(
            f"ERROR: llama-server is already running (PIDs: {pids}).\n"
            "Stop it before benchmarking to avoid RAM thrashing.",
            file=sys.stderr,
        )
        sys.exit(1)


def generate_test_files(sizes: list[int], repo_root: str) -> dict[int, Path]:
    """Generate test input files from real source code via git ls-files."""
    result = subprocess.run(
        ["git", "ls-files", "*.rs", "*.swift"],
        capture_output=True,
        text=True,
        cwd=repo_root,
    )
    if result.returncode != 0:
        print("ERROR: git ls-files failed", file=sys.stderr)
        sys.exit(1)

    files = sorted(result.stdout.strip().split("\n"))
    if not files or files == [""]:
        print("ERROR: no .rs or .swift files found", file=sys.stderr)
        sys.exit(1)

    # Concatenate all source files
    all_lines: list[str] = []
    for f in files:
        fpath = Path(repo_root) / f
        if fpath.is_file():
            all_lines.extend(fpath.read_text(errors="replace").splitlines())

    if not all_lines:
        print("ERROR: source files are empty", file=sys.stderr)
        sys.exit(1)

    max_size = max(sizes)
    if len(all_lines) < max_size:
        print(
            f"WARNING: only {len(all_lines)} lines available, largest size {max_size} will use all lines",
            file=sys.stderr,
        )

    tmpdir = Path(tempfile.mkdtemp(prefix="bench-parallel-"))
    test_files: dict[int, Path] = {}
    for size in sizes:
        sliced = all_lines[:size]
        outpath = tmpdir / f"input-{size}.txt"
        outpath.write_text("\n".join(sliced) + "\n")
        test_files[size] = outpath
        print(f"  Generated {outpath.name}: {len(sliced)} lines", file=sys.stderr)

    return test_files


def start_server(
    binary: str,
    model: str,
    parallel: int,
    port: int,
    ctx_size: int,
) -> subprocess.Popen:
    """Start llama-server and redirect output to a log file."""
    now = datetime.now().strftime("%Y%m%d-%H%M%S")
    log_name = f"current-run-llama-output-p{parallel}-{now}.txt"
    log_file = open(log_name, "w")

    args = [
        binary,
        "--model", model,
        "--port", str(port),
        "--host", "127.0.0.1",
        "--ctx-size", str(ctx_size),
        "--n-gpu-layers", "-1",
        "--jinja",
        "--parallel", str(parallel),
    ]
    print(f"  Starting: {' '.join(args)}", file=sys.stderr)
    print(f"  Server log: {log_name}", file=sys.stderr)

    proc = subprocess.Popen(
        args,
        stdout=log_file,
        stderr=log_file,
    )
    # Keep reference so log file stays open
    proc._log_file = log_file  # type: ignore[attr-defined]
    return proc


def wait_for_health(port: int, timeout: float = 120.0) -> bool:
    """Poll /health until 200 or timeout."""
    url = f"http://127.0.0.1:{port}/health"
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            resp = httpx.get(url, timeout=5.0)
            if resp.status_code == 200:
                return True
        except (httpx.ConnectError, httpx.ReadError, httpx.TimeoutException):
            pass
        time.sleep(0.5)
    return False


def stop_server(proc: subprocess.Popen) -> None:
    """SIGTERM, wait 15s, SIGKILL if needed."""
    proc.terminate()
    try:
        proc.wait(timeout=15)
    except subprocess.TimeoutExpired:
        proc.kill()
        proc.wait()
    if hasattr(proc, "_log_file"):
        proc._log_file.close()


def _parse_sse_line(line: str) -> dict | None:
    """Parse an SSE line into a JSON dict, or None if not a data chunk."""
    if not line.startswith("data: "):
        return None
    data_str = line[6:]
    if data_str.strip() == "[DONE]":
        return None
    try:
        return json.loads(data_str)
    except json.JSONDecodeError:
        return None


def _has_content_delta(chunk: dict) -> bool:
    """Return True if chunk contains a non-empty content delta."""
    choices = chunk.get("choices", [])
    if not choices:
        return False
    return bool(choices[0].get("delta", {}).get("content", ""))


async def send_streaming_request(
    client: httpx.AsyncClient,
    port: int,
    content: str,
) -> RequestResult:
    """Send a streaming chat completion request and measure metrics."""
    url = f"http://127.0.0.1:{port}/v1/chat/completions"
    payload = {
        "model": "default",
        "messages": [
            {"role": "user", "content": f"Summarize this code:\n\n{content}"},
        ],
        "stream": True,
        "stream_options": {"include_usage": True},
        "max_tokens": 256,
    }

    t0 = time.perf_counter()
    ttft = 0.0
    client_token_count = 0
    server_prompt_tokens = 0
    server_completion_tokens = 0
    got_first_token = False

    async with client.stream("POST", url, json=payload, timeout=120.0) as resp:
        resp.raise_for_status()
        async for line in resp.aiter_lines():
            chunk = _parse_sse_line(line)
            if chunk is None:
                continue

            usage = chunk.get("usage")
            if usage:
                server_prompt_tokens = usage.get("prompt_tokens", 0)
                server_completion_tokens = usage.get("completion_tokens", 0)

            if _has_content_delta(chunk):
                client_token_count += 1
                if not got_first_token:
                    ttft = time.perf_counter() - t0
                    got_first_token = True

    total_time = time.perf_counter() - t0
    completion_tokens = server_completion_tokens if server_completion_tokens > 0 else client_token_count

    return RequestResult(
        ttft_s=ttft,
        total_time_s=total_time,
        prompt_tokens=server_prompt_tokens,
        completion_tokens=completion_tokens,
    )


async def fire_concurrent_requests(
    port: int,
    content: str,
    n_parallel: int,
) -> list[RequestResult]:
    """Fire n_parallel simultaneous requests."""
    async with httpx.AsyncClient() as client:
        tasks = [
            send_streaming_request(client, port, content)
            for _ in range(n_parallel)
        ]
        return await asyncio.gather(*tasks)


async def warmup_request(port: int) -> None:
    """Send a throwaway request to warm up Metal/KV cache."""
    async with httpx.AsyncClient() as client:
        url = f"http://127.0.0.1:{port}/v1/chat/completions"
        payload = {
            "model": "default",
            "messages": [{"role": "user", "content": "Hello"}],
            "max_tokens": 1,
        }
        resp = await client.post(url, json=payload, timeout=60.0)
        resp.raise_for_status()


def format_table(results: list[ConfigResult]) -> str:
    """Format results as a markdown table."""
    header = (
        "| parallel | input_lines | input_tokens | ttft_avg_ms | ttft_stddev_ms "
        "| total_time_avg_s | total_time_stddev_s | gen_tok_s | amortized_tok_s |"
    )
    sep = (
        "|----------|-------------|--------------|-------------|----------------"
        "|------------------|---------------------|-----------|-----------------|"
    )
    lines = [header, sep]

    for cr in results:
        if not cr.results:
            continue

        ttfts_ms = [r.ttft_s * 1000 for r in cr.results]
        total_times = [r.total_time_s for r in cr.results]
        completion_tokens = [r.completion_tokens for r in cr.results]

        ttft_avg = sum(ttfts_ms) / len(ttfts_ms)
        ttft_std = _stddev(ttfts_ms)
        total_avg = sum(total_times) / len(total_times)
        total_std = _stddev(total_times)

        # gen_tok_s: average per-request generation speed
        gen_speeds = [
            r.completion_tokens / r.total_time_s
            for r in cr.results
            if r.total_time_s > 0 and r.completion_tokens > 0
        ]
        gen_tok_s = sum(gen_speeds) / len(gen_speeds) if gen_speeds else 0.0

        # amortized_tok_s: total completion tokens / wall clock time for the batch
        # For concurrent requests, wall clock = max(total_times) per batch
        # We group results by batch (parallel requests per rep)
        # We have reps * parallel results, grouped by rep
        n_parallel = cr.parallel
        n_reps = len(cr.results) // n_parallel if n_parallel > 0 else 1
        amortized_sum = 0.0
        for rep_idx in range(n_reps):
            batch = cr.results[rep_idx * n_parallel : (rep_idx + 1) * n_parallel]
            wall_time = max(r.total_time_s for r in batch)
            batch_tokens = sum(r.completion_tokens for r in batch)
            if wall_time > 0:
                amortized_sum += batch_tokens / wall_time
        amortized_tok_s = amortized_sum / n_reps if n_reps > 0 else 0.0

        # Average prompt tokens for input_tokens column
        avg_prompt_tokens = (
            sum(r.prompt_tokens for r in cr.results) // len(cr.results)
            if cr.results
            else 0
        )

        lines.append(
            f"| {cr.parallel:>8} | {cr.input_lines:>11} | {avg_prompt_tokens:>12} "
            f"| {ttft_avg:>11.1f} | {ttft_std:>14.1f} "
            f"| {total_avg:>16.2f} | {total_std:>19.2f} "
            f"| {gen_tok_s:>9.1f} | {amortized_tok_s:>15.1f} |"
        )

    return "\n".join(lines)


def _stddev(values: list[float]) -> float:
    """Sample standard deviation (Bessel's correction)."""
    if len(values) < 2:
        return 0.0
    mean = sum(values) / len(values)
    variance = sum((v - mean) ** 2 for v in values) / (len(values) - 1)
    return math.sqrt(variance)


async def bench_one_config(
    port: int,
    content: str,
    parallel: int,
    size: int,
    reps: int,
) -> ConfigResult:
    """Benchmark a single (parallel, size) configuration across all reps."""
    cr = ConfigResult(parallel=parallel, input_lines=size, input_tokens=0)
    for rep in range(reps):
        print(
            f"  [{parallel}x] size={size} rep={rep+1}/{reps}...",
            file=sys.stderr,
            end="",
            flush=True,
        )
        batch_results = await fire_concurrent_requests(port, content, parallel)
        cr.results.extend(batch_results)
        avg_ttft = sum(r.ttft_s for r in batch_results) / len(batch_results)
        print(f" ttft={avg_ttft*1000:.0f}ms", file=sys.stderr)
    return cr


async def bench_one_parallel(
    binary: str,
    model: str,
    par: int,
    sizes: list[int],
    test_files: dict[int, Path],
    args: argparse.Namespace,
) -> list[ConfigResult]:
    """Start server at --parallel N, run all sizes, stop server."""
    print(f"\n{'='*60}", file=sys.stderr)
    print(f"  parallel = {par}", file=sys.stderr)
    print(f"{'='*60}", file=sys.stderr)

    proc = start_server(binary, model, par, args.port, args.ctx_size)

    try:
        print("  Waiting for server health...", file=sys.stderr)
        if not wait_for_health(args.port):
            print("  ERROR: server failed to become healthy", file=sys.stderr)
            return []

        print("  Server healthy.", file=sys.stderr)

        if not args.no_warmup:
            print("  Sending warmup request...", file=sys.stderr)
            await warmup_request(args.port)
            print("  Warmup complete.", file=sys.stderr)

        results = []
        for size in sizes:
            content = test_files[size].read_text()
            cr = await bench_one_config(args.port, content, par, size, args.reps)
            results.append(cr)

        return results
    finally:
        print("  Stopping server...", file=sys.stderr)
        stop_server(proc)
        print("  Server stopped.", file=sys.stderr)


def resolve_inputs(args: argparse.Namespace) -> tuple[str, str, str, list[int], list[int]]:
    """Validate CLI inputs and return (binary, model, repo_root, sizes, parallels)."""
    sizes = [int(s) for s in args.sizes.split(",")]
    parallels = [int(p) for p in args.parallel.split(",")]

    repo_root = subprocess.run(
        ["git", "rev-parse", "--show-toplevel"],
        capture_output=True,
        text=True,
    ).stdout.strip()
    if not repo_root:
        print("ERROR: not in a git repository", file=sys.stderr)
        sys.exit(1)

    binary = args.llama_server or find_llama_server()
    if not binary:
        print("ERROR: llama-server not found. Provide --llama-server PATH.", file=sys.stderr)
        sys.exit(1)

    model = os.path.expanduser(args.model)
    if not Path(model).is_file():
        print(f"ERROR: model file not found: {model}", file=sys.stderr)
        sys.exit(1)

    return binary, model, repo_root, sizes, parallels


async def run_benchmark(args: argparse.Namespace) -> list[ConfigResult]:
    """Run the full benchmark matrix."""
    binary, model, repo_root, sizes, parallels = resolve_inputs(args)

    print(f"Binary: {binary}", file=sys.stderr)
    print(f"Model: {model}", file=sys.stderr)
    print(f"Sizes: {sizes}", file=sys.stderr)
    print(f"Parallel values: {parallels}", file=sys.stderr)
    print(f"Reps: {args.reps}", file=sys.stderr)
    print(f"Warmup: {not args.no_warmup}", file=sys.stderr)
    print(f"Port: {args.port}", file=sys.stderr)
    print(f"Context size: {args.ctx_size}", file=sys.stderr)
    print(file=sys.stderr)

    print("Generating test input files...", file=sys.stderr)
    test_files = generate_test_files(sizes, repo_root)

    all_results: list[ConfigResult] = []
    for par in parallels:
        results = await bench_one_parallel(binary, model, par, sizes, test_files, args)
        all_results.extend(results)

    # Cleanup temp files
    if test_files:
        tmpdir = list(test_files.values())[0].parent
        shutil.rmtree(tmpdir, ignore_errors=True)

    return all_results


def main() -> None:
    args = parse_args()
    safety_gate()
    results = asyncio.run(run_benchmark(args))
    print()
    print(format_table(results))
    print()


if __name__ == "__main__":
    main()
