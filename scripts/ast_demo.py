#!/usr/bin/env python3
"""Demonstrate Rust AST rewrites in an isolated fixture, never the caller's code."""

import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("target/debug/n4h"))
    parser.add_argument("--check-rust", action="store_true", help="Also compile and run this known demo fixture with rustc")
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    env = {k: v for k, v in os.environ.items() if k != "N4H_TEST_FAILPOINT"}
    with tempfile.TemporaryDirectory(prefix="n4h-ast-demo-", dir=os.environ.get("N4H_DEMO_TMPDIR")) as folder:
        root = Path(folder)
        sources = {
            "main.rs": (
                '#![allow(dead_code)]\nmod worker;\n'
                'fn old_api(value: i32) -> i32 { value + 1 }\n'
                'fn new_api(value: i32) -> i32 { value + 1 }\n'
                '// old_api(leave_this_comment)\n'
                'fn main() {\n'
                '    let text = "old_api(leave_this_string)";\n'
                '    assert!(!text.is_empty());\n'
                '    let answer = old_api(1 + 2);\n'
                '    assert_eq!(answer, 4);\n'
                '    assert_eq!(worker::call(), 3);\n'
                '}\n'
            ).encode(),
            "worker.rs": (
                '#[allow(unused_imports)]\nuse super::{old_api, new_api};\n'
                'pub fn call() -> i32 { old_api(2) }\n'
            ).encode(),
        }
        for name, content in sources.items():
            (root / name).write_bytes(content)

        def run(*arguments, expected=0):
            result = subprocess.run([str(binary), *arguments], cwd=root, env=env, capture_output=True, check=False)
            value = json.loads(result.stdout)
            if result.returncode != expected:
                raise RuntimeError(f"{arguments}: exit {result.returncode}, expected {expected}: {value}")
            print("$ n4h " + " ".join(arguments))
            print(json.dumps(value, indent=2, ensure_ascii=False))
            return value

        def check_fixture():
            if args.check_rust:
                executable = root / "fixture"
                subprocess.run(["rustc", "--edition=2024", "main.rs", "-o", str(executable)], cwd=root, env=env, check=True)
                subprocess.run([str(executable)], cwd=root, env=env, check=True)

        check_fixture()
        run("init")
        found = run("ast", "find", "old_api($ARG)", "--page-size", "1")
        assert found["total_matches"] == 2 and found["truncated"]
        assert found["policy"]["symbol_resolution"] is False
        run("inspect", found["result_id"], "--cursor", found["cursor_next"])
        invalid = run("ast", "edit", "--on", found["result_id"], "--scope", "all", "--replace", "new_api($ARG", expected=3)
        assert invalid["error"]["code"] == "ast_rewrite_invalid"
        plan = run("ast", "edit", "--on", found["result_id"], "--scope", "all", "--replace", "new_api($ARG)")
        assert plan["files_total"] == 2
        for name, content in sources.items():
            assert (root / name).read_bytes() == content
        applied = run("apply", plan["plan_id"])
        assert applied["files_modified_this_call"] == 2
        assert b"new_api(1 + 2)" in (root / "main.rs").read_bytes()
        assert b"new_api(2)" in (root / "worker.rs").read_bytes()
        assert b"old_api(leave_this_comment)" in (root / "main.rs").read_bytes()
        assert b"old_api(leave_this_string)" in (root / "main.rs").read_bytes()
        check_fixture()
        run("undo", applied["checkpoint_id"])
        for name, content in sources.items():
            assert (root / name).read_bytes() == content
        check_fixture()
        found = run("ast", "find", "old_api($ARG)")
        plan = run("ast", "edit", "--on", found["result_id"], "--scope", "all", "--replace", "new_api($ARG)")
        (root / "worker.rs").write_bytes(sources["worker.rs"] + b"// external edit\n")
        rejected = run("apply", plan["plan_id"], expected=3)
        assert rejected["error"]["code"] == "file_changed"
        assert (root / "main.rs").read_bytes() == sources["main.rs"]
        assert (root / "worker.rs").read_bytes().endswith(b"// external edit\n")
        print("PASS: Rust AST captures, syntax gate, multi-file plan/apply/undo, unchanged comments/strings, and conflict protection.")
        if args.check_rust:
            print("PASS: this demo fixture compiles and its assertions pass before apply, after apply, and after undo.")


if __name__ == "__main__":
    main()
