#!/usr/bin/env python3
"""Verify release identity and package one native binary with its checksum."""

import argparse
import hashlib
from pathlib import Path
import subprocess
import tarfile
import tomllib


TARGETS = (
    "x86_64-unknown-linux-musl",
    "aarch64-unknown-linux-musl",
    "x86_64-apple-darwin",
    "aarch64-apple-darwin",
)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--target", choices=TARGETS)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    version = tomllib.loads((root / "Cargo.toml").read_text())["package"]["version"]
    if args.tag != f"v{version}":
        parser.error(f"tag {args.tag!r} does not match Cargo.toml version v{version}")
    if not (root / "docs" / "releases" / f"{args.tag}.md").is_file():
        parser.error(f"missing release notes for {args.tag}")
    if args.check:
        print(f"Release identity verified: {args.tag}")
        return
    if not args.target:
        parser.error("--target is required when packaging")
    binary = root / "target" / args.target / "release" / "baseballhour"
    if subprocess.check_output([binary, "--version"], text=True).strip() != f"baseballhour {version}":
        parser.error("binary version does not match Cargo.toml")
    name = f"baseballhour-{args.tag}-{args.target}"
    destination = root / "dist"
    destination.mkdir(exist_ok=True)
    archive = destination / f"{name}.tar.gz"
    with tarfile.open(archive, "w:gz") as bundle:
        for source in (binary, root / "README.md", root / "LICENSE"):
            bundle.add(source, arcname=f"{name}/{source.name}", recursive=False)
    digest = hashlib.sha256(archive.read_bytes()).hexdigest()
    archive.with_suffix(".gz.sha256").write_text(f"{digest}  {archive.name}\n")
    print(f"Packaged {archive.name} ({archive.stat().st_size} bytes)")


if __name__ == "__main__":
    main()
