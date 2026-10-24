#!/usr/bin/env python3
"""Exercise n4h in an isolated fixture; never edit the caller's repository."""

import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("target/debug/n4h"))
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    with tempfile.TemporaryDirectory(prefix="n4h-demo-", dir=os.environ.get("N4H_DEMO_TMPDIR")) as folder:
        root = Path(folder)
        sources = {
            "client.txt": b"# existing developer change\nhttps://old.example/api\nhttps://old.example/api\n",
            "settings.txt": b"# another developer change\nhttps://old.example/api\n",
        }
        for name, before in sources.items():
            (root / name).write_bytes(before)

        def run(*arguments, expected=0):
            command = [str(binary), *arguments]
            env = {key: value for key, value in os.environ.items() if key != "N4H_TEST_FAILPOINT"}
            result = subprocess.run(command, cwd=root, env=env, capture_output=True, check=False)
            value = json.loads(result.stdout)
            if result.returncode != expected:
                raise RuntimeError(f"{arguments}: exit {result.returncode}, expected {expected}: {value}")
            print("$ n4h " + " ".join(arguments))
            print(json.dumps(value, indent=2, ensure_ascii=False))
            return value

        run("init")
        assert (root / ".n4h" / "state.redb").is_file(), "New state must use redb"
        assert not (root / ".n4h" / "state.sqlite3").exists(), "SQLite must not be initialized"
        note = run("mem", "put", "fixture.validation", "--text", "Verify both URL files and preserve existing developer changes.")
        assert note["note"]["origin"] == "agent_note"
        assert run("mem", "get", "fixture.validation")["note"] == note["note"]
        assert run("mem", "search", "developer")["total_matches"] == 1
        found = run("find", "https://old.example/api", "--page-size", "1")
        assert found["total_matches"] == 3 and found["truncated"]
        next_page = run("inspect", found["result_id"], "--cursor", found["cursor_next"])
        assert next_page["matches"][0]["match_id"] != found["matches"][0]["match_id"]
        plan = run("edit", "--on", found["result_id"], "--scope", "all", "--replace", "https://new.example/api")
        assert plan["files_total"] == 2
        for name, before in sources.items():
            assert (root / name).read_bytes() == before, "Preview must not edit sources"
        applied = run("apply", plan["plan_id"])
        assert applied["files_modified_this_call"] == 2
        for name, before in sources.items():
            assert (root / name).read_bytes() == before.replace(b"old.example", b"new.example")
        retry = run("apply", plan["plan_id"])
        assert retry["idempotent_replay"] and retry["operation_id"] == applied["operation_id"]
        run("undo", applied["checkpoint_id"])
        for name, before in sources.items():
            assert (root / name).read_bytes() == before, "Undo must preserve preexisting developer changes"

        # A fresh CLI invocation can recover factual context and separate notes.
        summary = run("resume")
        assert not summary["recovery_required"]
        assert summary["operational_facts"]["recent_operations"]["total"] == 2
        assert summary["operational_facts"]["file_state_verified"] is False
        assert summary["agent_notes"]["items"][0]["key"] == "fixture.validation"
        updated = run("mem", "put", "fixture.validation", "--text", "Apply and undo verified; next demonstrate a conflict.",
                      "--if-revision", str(note["note"]["revision"]))
        assert updated["note"]["revision"] > note["note"]["revision"]

        found = run("find", "https://old.example/api")
        plan = run("edit", "--on", found["result_id"], "--scope", "all", "--replace", "https://new.example/api")
        external = sources["settings.txt"] + b"# an external edit after preview\n"
        (root / "settings.txt").write_bytes(external)
        rejected = run("apply", plan["plan_id"], expected=3)
        assert rejected["error"]["code"] == "file_changed"
        assert (root / "settings.txt").read_bytes() == external, "Conflict must not overwrite external work"
        assert (root / "client.txt").read_bytes() == sources["client.txt"], "Whole-plan preflight must prevent partial writes"
        assert not run("status")["recovery_required"]
        run("mem", "forget", "fixture.validation", "--if-revision", str(updated["note"]["revision"]))
        assert run("mem", "list")["total_notes"] == 0
        assert run("resume")["operational_facts"]["recent_operations"]["total"] == 2
        print("PASS: redb persistence, explicit agent memory, resume, pagination, immutable multi-file preview, apply, idempotency, undo, and whole-plan conflict protection.")


if __name__ == "__main__":
    main()
