#!/usr/bin/env python3
"""Offline installer tests: real binary/archive, mocked HTTPS transport, isolated HOME."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import pty
import select
import subprocess
import tarfile
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
ASSET = "n4h-x86_64-unknown-linux-gnu.tar.gz"
MOCK_CURL = '''#!/usr/bin/env python3
import os, sys
from pathlib import Path
args = sys.argv[1:]
url = next(a for a in args if a.startswith("https://"))
with open(os.environ["MOCK_LOG"], "a") as log: log.write(url + "\\n")
if url.endswith("/latest"):
    print("https://github.com/tomiok/n4h/releases/tag/" + os.environ["MOCK_TAG"], end="")
    raise SystemExit(0)
output = args[args.index("-o") + 1]
asset = url.rsplit("/", 1)[-1]
source = Path(os.environ["MOCK_RELEASE"]) / asset
if not source.exists(): raise SystemExit(22)
Path(output).write_bytes(source.read_bytes())
'''


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    source = parser.add_mutually_exclusive_group(required=True)
    source.add_argument("--archive", type=Path)
    source.add_argument("--binary", type=Path)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="n4h-install-tests-", dir=os.environ.get("TMPDIR")) as folder:
        base = Path(folder)
        release = base / "release"
        release.mkdir()
        archive = release / ASSET
        if args.archive:
            archive.write_bytes(args.archive.read_bytes())
        else:
            with tarfile.open(archive, "w:gz") as tar:
                tar.add(args.binary.resolve(), arcname="n4h", recursive=False)
        with tarfile.open(archive) as tar:
            member = tar.getmember("n4h")
            assert member.isfile()
            executable = base / "n4h"
            executable.write_bytes(tar.extractfile(member).read())
            executable.chmod(0o755)
        tag = "v" + subprocess.check_output([str(executable), "--version"], text=True).strip().split()[1]
        checksum = hashlib.sha256(archive.read_bytes()).hexdigest() + "  " + ASSET + "\n"
        (release / "SHA256SUMS").write_text(checksum)
        mock = base / "mock"
        mock.mkdir()
        (mock / "curl").write_text(MOCK_CURL)
        (mock / "curl").chmod(0o755)

        def fixture(name):
            root = base / name
            root.mkdir()
            for directory in ["home", "project", "config", "tmp"]:
                (root / directory).mkdir()
            env = dict(os.environ, HOME=str(root / "home"), XDG_CONFIG_HOME=str(root / "config"),
                       TMPDIR=str(root / "tmp"), PATH=str(mock) + ":" + os.environ["PATH"],
                       MOCK_RELEASE=str(release), MOCK_TAG=tag, MOCK_LOG=str(root / "requests"))
            env.pop("N4H_TEST_FAILPOINT", None)
            return root, env

        def run(root, env, *flags, success=True):
            result = subprocess.run(["bash", str(ROOT / "install.sh"), *flags], cwd=root / "project", env=env,
                                    stdin=subprocess.DEVNULL, capture_output=True, text=True, timeout=30)
            assert (result.returncode == 0) == success, (flags, result.stdout, result.stderr)
            assert not (root / "project/.n4h").exists()
            assert not list((root / "tmp").iterdir()), "Installer staging must be cleaned up"
            return result

        root, env = fixture("all-user")
        run(root, env, "--yes", "--harness", "claude-code,codex,opencode", "--scope", "user")
        for file in ["home/.claude/skills/n4h/SKILL.md", "home/.agents/skills/n4h/SKILL.md", "config/opencode/skills/n4h/SKILL.md"]:
            assert (root / file).read_bytes() == (ROOT / "skills/n4h/SKILL.md").read_bytes()
        installed = root / "home/.local/bin/n4h"
        assert installed.read_bytes() == executable.read_bytes()
        run(root, env, "--yes", "--cli-only", success=False)
        run(root, env, "--yes", "--cli-only", "--replace-binary")

        root, env = fixture("pinned-cli")
        run(root, env, "--yes", "--cli-only", "--version", tag, "--bin-dir", str(root / "custom bin"))
        assert (root / "custom bin/n4h").exists()
        assert "/latest" not in (root / "requests").read_text()
        assert not (root / "home/.agents").exists()

        for name, contents in [("bad-checksum", "0" * 64 + "  " + ASSET + "\n"),
                               ("duplicate-checksum", checksum * 2), ("missing-checksum", "")]:
            root, env = fixture(name)
            (release / "SHA256SUMS").write_text(contents)
            run(root, env, "--yes", "--cli-only", success=False)
            assert not (root / "home/.local/bin").exists()
        (release / "SHA256SUMS").write_text(checksum)

        original_archive = archive.read_bytes()
        with tarfile.open(archive, "w:gz") as tar:
            entry = tarfile.TarInfo("n4h")
            entry.type = tarfile.SYMTYPE
            entry.linkname = str(executable)
            tar.addfile(entry)
        (release / "SHA256SUMS").write_text(hashlib.sha256(archive.read_bytes()).hexdigest() + "  " + ASSET + "\n")
        root, env = fixture("archive-symlink")
        run(root, env, "--yes", "--cli-only", success=False)
        assert not (root / "home/.local/bin").exists()
        archive.write_bytes(original_archive)
        (release / "SHA256SUMS").write_text(checksum)

        root, env = fixture("wrong-binary-version")
        env["MOCK_TAG"] = "v999.0.0"
        run(root, env, "--yes", "--cli-only", success=False)
        assert not (root / "home/.local/bin").exists()

        root, env = fixture("validation")
        for flags in [("--yes",), ("--yes", "--scope", "user"), ("--yes", "--cli-only", "--harness", "codex"),
                      ("--version", "../../main"), ("--scope", "wrong"), ("--harness", "unknown"), ("--bin-dir", "relative")]:
            run(root, env, *flags, success=False)
        assert not (root / "requests").exists()

        root, env = fixture("truncated-bootstrap")
        script = (ROOT / "install.sh").read_text()
        truncated = script[:script.index("printf 'n4h: installed")]
        result = subprocess.run(["bash", "-s", "--", "--yes", "--cli-only"], input=truncated,
                                cwd=root / "project", env=env, capture_output=True, text=True, timeout=30)
        assert result.returncode != 0
        assert not (root / "requests").exists()
        assert not (root / "home/.local").exists()

        root, env = fixture("directory-symlink")
        (root / "home/.local").symlink_to(root / "config", target_is_directory=True)
        run(root, env, "--yes", "--cli-only", success=False)
        assert not (root / "requests").exists()

        root, env = fixture("binary-symlink")
        binary = root / "home/.local/bin/n4h"
        binary.parent.mkdir(parents=True)
        binary.symlink_to(executable)
        run(root, env, "--yes", "--cli-only", "--replace-binary", success=False)
        assert not (root / "requests").exists()

        root, env = fixture("unsupported-platform")
        platform_mock = root / "platform"
        platform_mock.mkdir()
        (platform_mock / "uname").write_text("#!/bin/sh\necho Darwin\n")
        (platform_mock / "uname").chmod(0o755)
        env["PATH"] = str(platform_mock) + ":" + env["PATH"]
        run(root, env, "--yes", "--cli-only", success=False)
        assert not (root / "requests").exists()

        root, env = fixture("normal-build")
        run(root, env, "--yes", "--cli-only")
        binary = root / "home/.local/bin/n4h"
        def cli(*arguments):
            result = subprocess.run([str(binary), *arguments], cwd=root / "project",
                                    env=dict(env, N4H_TEST_FAILPOINT="after_rename"), capture_output=True, text=True, check=True)
            return json.loads(result.stdout)
        cli("init")
        (root / "project/app.txt").write_text("old\n")
        found = cli("find", "old")
        plan = cli("edit", "--on", found["result_id"], "--scope", "all", "--replace", "new")
        # A fault-injection release would terminate here. Normal artifacts must
        # ignore test-only process-termination environment variables entirely.
        cli("apply", plan["plan_id"])
        assert (root / "project/app.txt").read_text() == "new\n"

        root, env = fixture("skill-conflict")
        skill = root / "project/.agents/skills/n4h/SKILL.md"
        skill.parent.mkdir(parents=True)
        skill.write_text("user work")
        result = run(root, env, "--yes", "--harness", "codex", "--scope", "project", success=False)
        assert "Binary is installed" in result.stderr
        assert skill.read_text() == "user work"

        # A controlling PTY plus script on stdin reproduces `curl ... | bash`.
        # The installer AND Rust setup must prompt via /dev/tty, not consume code.
        import fcntl
        import termios
        def interactive(root, env, steps, expected=0):
            master, slave = pty.openpty()
            def controlling_tty():
                os.setsid()
                fcntl.ioctl(slave, termios.TIOCSCTTY, 0)
            process = subprocess.Popen(["bash"], cwd=root / "project", env=env, stdin=subprocess.PIPE,
                                       stdout=subprocess.PIPE, stderr=slave, preexec_fn=controlling_tty)
            os.close(slave)
            process.stdin.write((ROOT / "install.sh").read_bytes())
            process.stdin.close()
            process.stdin = None
            transcript = b""
            deadline = time.monotonic() + 30
            try:
                while steps and time.monotonic() < deadline:
                    if select.select([master], [], [], 1)[0]:
                        transcript += os.read(master, 65536)
                        if steps[0][0] in transcript:
                            _, answer = steps.pop(0)
                            os.write(master, answer)
                assert not steps, transcript.decode(errors="replace")
                stdout, _ = process.communicate(timeout=30)
                assert process.returncode == expected, (stdout, transcript)
            finally:
                if process.poll() is None:
                    process.kill()
                    process.wait()
                os.close(master)
            assert not (root / "project/.n4h").exists()
            assert not list((root / "tmp").iterdir())
            return stdout

        root, env = fixture("interactive-pipe")
        stdout = interactive(root, env, [(b"Download and install", b"y\n"), (b"Selection:", b"1,3\n"),
                                        (b"Scope:", b"2\n"), (b"Install only these skills?", b"y\n")])
        assert (root / "project/.claude/skills/n4h/SKILL.md").exists()
        assert (root / "project/.opencode/skills/n4h/SKILL.md").exists()
        assert not (root / "project/.agents").exists()
        assert not (root / "project/.n4h").exists()
        assert not list((root / "tmp").iterdir())
        for line in stdout.splitlines():
            assert json.loads(line)["ok"]
        root, env = fixture("interactive-cli-only")
        interactive(root, env, [(b"Download and install", b"y\n"), (b"Selection:", b"5\n")])
        assert not (root / "project/.agents").exists()
        assert not (root / "home/.claude").exists()
        root, env = fixture("interactive-cancel-binary")
        interactive(root, env, [(b"Download and install", b"n\n")], expected=1)
        assert not (root / "home/.local").exists()
        root, env = fixture("interactive-cancel-skill")
        interactive(root, env, [(b"Download and install", b"y\n"), (b"Selection:", b"2\n"),
                                (b"Scope:", b"2\n"), (b"Install only these skills?", b"n\n")], expected=1)
        assert (root / "home/.local/bin/n4h").exists()
        assert not (root / "project/.agents").exists()
        print("PASS: offline installer, checksum/pinning, explicit replacement, isolated user/project skills, conflicts, symlinks, and interactive curl-pipe /dev/tty flow.")


if __name__ == "__main__":
    main()
