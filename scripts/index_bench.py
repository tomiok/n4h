#!/usr/bin/env python3
"""Reproducible synthetic index comparison; creates only an isolated temporary fixture."""

import argparse
import json
import os
from pathlib import Path
import statistics
import subprocess
import tempfile
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("target/release/n4h"))
    parser.add_argument("--files", type=int, default=128)
    parser.add_argument("--bytes-per-file", type=int, default=65536)
    parser.add_argument("--runs", type=int, default=5)
    args = parser.parse_args()
    if not (1 <= args.files <= 4096 and 256 <= args.bytes_per_file <= 2 * 1024 * 1024 and 1 <= args.runs <= 50):
        parser.error("files: 1-4096; bytes-per-file: 256-2097152; runs: 1-50")
    if args.files * args.bytes_per_file > 100 * 1024 * 1024:
        parser.error("fixture must fit the current 100 MiB source limit")
    binary = args.binary.resolve(strict=True)
    env = {k: v for k, v in os.environ.items() if k != "N4H_TEST_FAILPOINT"}
    needle = "N4H_INDEX_BENCH_NEEDLE_83ad"
    with tempfile.TemporaryDirectory(prefix="n4h-index-bench-", dir=os.environ.get("N4H_DEMO_TMPDIR")) as folder:
        root = Path(folder)
        for i in range(args.files):
            line = f"alpha beta gamma delta file_{i:04d} keeps reproducible source content\n".encode()
            content = (line * (args.bytes_per_file // len(line) + 1))[:args.bytes_per_file]
            if i == 0:
                content = needle.encode() + b"\n" + content[len(needle) + 1:]
            (root / f"f{i:04d}.txt").write_bytes(content)

        def run(*arguments):
            start = time.perf_counter_ns()
            result = subprocess.run([str(binary), *arguments], cwd=root, env=env, capture_output=True, check=False)
            elapsed = (time.perf_counter_ns() - start) / 1_000_000
            value = json.loads(result.stdout)
            if result.returncode:
                raise RuntimeError(f"{arguments}: {value}")
            return value, elapsed

        run("init")
        built, build_ms = run("index", "build")
        rows = []
        expected = None
        for repetition in range(args.runs):
            # Alternate execution order; both modes include process startup and
            # durable result creation. Do not claim cold filesystem caches.
            modes = [False, True] if repetition % 2 == 0 else [True, False]
            for use_index in modes:
                arguments = ["find", needle, *(["--indexed"] if use_index else [])]
                value, elapsed = run(*arguments)
                signature = [{k: m[k] for k in ("path", "start_byte", "end_byte", "file_hash")} for m in value["matches"]]
                if expected is None:
                    expected = signature
                assert signature == expected and value["total_matches"] == 1
                rows.append({"run": repetition + 1, "mode": "indexed" if use_index else "live", "elapsed_ms": elapsed,
                             "source_or_cached_bytes_scanned": value["bytes_scanned"], "files_scanned": value["files_scanned"]})
        noop, noop_ms = run("index", "update")
        assert noop["counters"]["files_read"] == 0
        changed_path = root / f"f{args.files - 1:04d}.txt"
        content = changed_path.read_bytes()
        changed_path.write_bytes(content[:-1] + (b"!" if content[-1:] != b"!" else b"?"))
        incremental, incremental_ms = run("index", "update")
        assert incremental["counters"]["files_read"] == 1
        verified, verified_ms = run("index", "update", "--verify")
        assert verified["counters"]["files_read"] == args.files
        live, _ = run("find", needle)
        indexed, _ = run("find", needle, "--indexed")
        assert live["total_matches"] == indexed["total_matches"] == 1
        cleared, _ = run("index", "clear")
        assert cleared["removed"] and not (root / ".n4h/index.redb").exists()

        def summary(mode):
            values = [row["elapsed_ms"] for row in rows if row["mode"] == mode]
            return {"median_ms": statistics.median(values), "min_ms": min(values), "max_ms": max(values)}

        print(json.dumps({
            "fixture": {"files": args.files, "bytes_per_file": args.bytes_per_file, "source_bytes": args.files * args.bytes_per_file,
                        "query": needle, "selectivity": "one matching file", "content": "synthetic repetitive text"},
            "method": "CLI wall time including startup, bounded JSON and persistent results; alternating order; OS caches not cleared",
            "live": summary("live"), "indexed": summary("indexed"), "runs": rows,
            "index": {"build_ms": build_ms, "physical_bytes_after_build": built["physical_bytes"], "posting_pairs": built["posting_pairs"],
                      "noop_update_ms": noop_ms, "noop_counters": noop["counters"],
                      "one_file_update_ms": incremental_ms, "one_file_counters": incremental["counters"],
                      "verified_update_ms": verified_ms, "verified_counters": verified["counters"]},
            "peak_memory": "not measured", "model_tokens": "not measured", "correctness": "passed",
            "warning": "Exploratory synthetic result, not representative proof of speedup across repositories. All updates still walk paths and ignore rules."
        }, indent=2))


if __name__ == "__main__":
    main()
