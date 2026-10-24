#!/usr/bin/env python3
"""Exercise explicit token budgets in an isolated fixture; no Python tokenizer dependency."""

import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("target/release/n4h"))
    parser.add_argument("--tokenizer", choices=["o200k_base", "cl100k_base"], default="o200k_base")
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    env = {k: v for k, v in os.environ.items() if k != "N4H_TEST_FAILPOINT"}
    with tempfile.TemporaryDirectory(prefix="n4h-token-demo-", dir=os.environ.get("N4H_DEMO_TMPDIR")) as folder:
        root = Path(folder)
        source = root / "client.txt"
        before = ("# preexisting developer change\n" + "old_api 日本語🙂\n" * 24).encode()
        source.write_bytes(before)

        def run(*arguments, limit=None, expected=0):
            flags = ["--max-tokens", str(limit), "--tokenizer", args.tokenizer] if limit is not None else []
            result = subprocess.run([str(binary), *arguments, *flags], cwd=root, env=env, capture_output=True, check=False)
            value = json.loads(result.stdout)
            if result.returncode != expected:
                raise RuntimeError(f"{arguments}: {result.returncode}, expected {expected}: {value}")
            assert len(result.stdout) <= 65536
            if limit is not None:
                assert value["budget"]["output_tokens"] <= limit
                assert value["budget"]["tokenizer"] == args.tokenizer
                print(f"{arguments[0]}: {value['budget']['output_tokens']}/{limit} tokens in compact CLI JSON + newline")
            return value

        run("init", limit=1200)
        first = run("find", "old_api", "--page-size", "500", limit=1200)
        assert first["total_matches"] == 24 and first["truncated"]
        result_id = first["result_id"]
        page = first
        seen = set()
        while True:
            for found in page["matches"]:
                assert found["match_id"] not in seen
                seen.add(found["match_id"])
            if not page["cursor_next"]:
                break
            page = run("inspect", result_id, "--cursor", page["cursor_next"], limit=1200)
        assert len(seen) == 24

        rejected = run("edit", "--on", result_id, "--scope", "all", "--replace", "new_api", limit=256, expected=5)
        assert rejected["error"]["code"] == "output_budget_too_small"
        assert source.read_bytes() == before
        plan = run("edit", "--on", result_id, "--scope", "all", "--replace", "new_api", limit=1800)
        assert plan["selected_matches"] == 24
        page = plan
        chunks = []
        while True:
            chunks.append(page["diff"])
            if not page["cursor_next"]:
                break
            page = run("inspect", plan["plan_id"], "--cursor", page["cursor_next"], limit=1800)
        unlimited = run("inspect", plan["plan_id"])
        assert not unlimited["diff_truncated"]
        assert "".join(chunks) == unlimited["diff"]
        applied = run("apply", plan["plan_id"], limit=1200)
        assert source.read_bytes() == before.replace(b"old_api", b"new_api")
        run("undo", applied["checkpoint_id"], limit=1200)
        assert source.read_bytes() == before

        note = run("mem", "put", "tests", "--text", "Preserve existing developer changes.", limit=800)
        assert note["note"]["origin"] == "agent_note"
        summary = run("resume", limit=800)
        assert not summary["recovery_required"]
        assert source.read_bytes() == before
        print("PASS: bounded JSON, complete match/diff pagination, explicit too-small error, unchanged edit scope, apply/undo and memory/resume.")
        print("Counts describe each compact n4h response, not these demo summaries or a provider's tool framing/billing.")


if __name__ == "__main__":
    main()
