#!/usr/bin/env python3
"""Build a normal Linux release and deterministic archive; never reuse a test binary."""

import argparse
import hashlib
import io
from pathlib import Path
import re
import subprocess
import tarfile
import tomllib

ROOT = Path(__file__).resolve().parents[1]
TARGET = "x86_64-unknown-linux-gnu"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--output", type=Path, default=ROOT / "dist")
    args = parser.parse_args()
    version = tomllib.loads((ROOT / "Cargo.toml").read_text())["package"]["version"]
    if not re.fullmatch(r"v\d+\.\d+\.\d+(?:-[0-9A-Za-z]+(?:[.-][0-9A-Za-z]+)*)?", args.tag) or args.tag != f"v{version}":
        parser.error("tag must be v + the exact Cargo.toml package version")
    target_dir = ROOT / "target"
    # Override CARGO_TARGET_DIR/config as well: never build elsewhere and then
    # accidentally package a stale binary from the usual output directory.
    subprocess.run(["cargo", "build", "--locked", "--release", "--no-default-features",
                    "--target", TARGET, "--target-dir", str(target_dir)], cwd=ROOT, check=True)
    binary = target_dir / TARGET / "release" / "n4h"
    actual = subprocess.check_output([str(binary), "--version"], text=True).strip()
    if actual != f"n4h {version}":
        raise SystemExit("Built binary version mismatch")
    args.output.mkdir(parents=True, exist_ok=True)
    archive = args.output / f"n4h-{TARGET}.tar.gz"
    # Gzip timestamp/name and tar metadata are stable too.
    import gzip
    with archive.open("wb") as raw, gzip.GzipFile(filename="", mode="wb", fileobj=raw, mtime=0) as zipped, tarfile.open(fileobj=zipped, mode="w") as tar:
        for name, path, mode in [
            ("n4h", binary, 0o755),
            ("README.md", ROOT / "README.md", 0o644),
            ("skills/n4h/SKILL.md", ROOT / "skills/n4h/SKILL.md", 0o644),
        ]:
            data = path.read_bytes()
            entry = tarfile.TarInfo(name)
            entry.size = len(data)
            entry.mode = mode
            tar.addfile(entry, io.BytesIO(data))
    digest = hashlib.sha256(archive.read_bytes()).hexdigest()
    (args.output / "SHA256SUMS").write_text(f"{digest}  {archive.name}\n")
    print(f"Packaged {args.tag}: {archive}")


if __name__ == "__main__":
    main()
