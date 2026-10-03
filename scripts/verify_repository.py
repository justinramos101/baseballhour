#!/usr/bin/env python3
from pathlib import Path
import re
import subprocess
import sys
from urllib.parse import unquote


def main():
    root = Path(__file__).resolve().parents[1]
    files = subprocess.check_output(
        ["git", "ls-files", "-z"], cwd=root
    ).decode().split("\0")
    failures = []
    for name in filter(None, files):
        path = root / name
        if any(part in {".audit", ".aws", ".codex", ".agents", ".impeccable", "target", "dist"} for part in path.relative_to(root).parts):
            failures.append(f"Private or generated path is tracked: {name}")
        if path.suffix == ".md":
            for link in re.findall(r"\]\(([^\s)]+)\)", path.read_text()):
                if "://" in link or link.startswith(("#", "mailto:")):
                    continue
                target = unquote(link.split("#", 1)[0])
                if not (path.parent / target).exists():
                    failures.append(f"Broken link in {name}: {link}")
    for path in (root / ".github" / "workflows").glob("*.yml"):
        body = path.read_text()
        for action in re.findall(r"\buses:\s*([^\s#]+)", body):
            if not re.fullmatch(r"[\w./-]+@[0-9a-f]{40}", action):
                failures.append(f"Action is not pinned in {path.name}: {action}")
        if "pull_request_target" in body:
            failures.append(f"Privileged pull_request_target trigger in {path.name}")
        if not re.search(r"^permissions:\n  contents: read\n", body, re.M):
            failures.append(f"Workflow lacks read-only default permissions: {path.name}")
    if not any(filter(None, files)):
        failures.append("No tracked files. Stage the publication set before this check.")
    if failures:
        print("\n".join(failures), file=sys.stderr)
        raise SystemExit(1)
    print("Public paths, documentation links, and GitHub Action references pass.")


if __name__ == "__main__":
    main()
