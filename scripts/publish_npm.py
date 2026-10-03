#!/usr/bin/env python3
"""Publish the verified tarball, reconciling retries by registry integrity."""

import argparse
import json
from pathlib import Path
import re
import subprocess
import time
import urllib.error
import urllib.request

from package_npm import inspect_tarball, load_release, verify_evidence


REGISTRY = "https://registry.npmjs.org"


def registry_integrity(version):
    request = urllib.request.Request(f"{REGISTRY}/baseballhour/{version}",
                                     headers={"Accept": "application/json", "Cache-Control": "no-cache"})
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            metadata = json.load(response)
    except urllib.error.HTTPError as error:
        if error.code == 404:
            return None
        raise
    if not isinstance(metadata, dict) or metadata.get("name") != "baseballhour" or metadata.get("version") != version:
        raise ValueError("registry returned unexpected package metadata")
    value = metadata.get("dist", {}).get("integrity")
    if not isinstance(value, str) or not value.startswith("sha512-"):
        raise ValueError("registry package has no SHA-512 integrity")
    return value


def publish(archive, release_manifest, evidence_path):
    release = load_release(release_manifest)
    evidence = verify_evidence(archive, release, evidence_path)
    inspect_tarball(archive, release)
    if not re.fullmatch(r"[0-9a-f]{40}", evidence.get("native_commit") or ""):
        raise ValueError("publication requires an attested native release commit")
    expected = evidence["integrity"]
    existing = registry_integrity(release["version"])
    if existing is not None:
        if existing != expected:
            raise ValueError("this npm version already exists with different bytes")
        print("The identical npm package is already published.")
        return
    subprocess.run(["npm", "publish", str(archive), "--access", "public", "--provenance",
                    "--ignore-scripts", "--registry", REGISTRY], check=True)
    for attempt in range(6):
        actual = registry_integrity(release["version"])
        if actual == expected:
            print("Published npm integrity matches the verified tarball.")
            return
        if actual is not None:
            raise ValueError("published npm integrity differs from the verified tarball")
        if attempt < 5:
            time.sleep(5)
    raise ValueError("published npm version is still absent after 25 seconds")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("archive", type=Path)
    parser.add_argument("--release-manifest", required=True, type=Path)
    parser.add_argument("--evidence", required=True, type=Path)
    args = parser.parse_args()
    try:
        publish(args.archive.resolve(), args.release_manifest, args.evidence)
    except subprocess.CalledProcessError as error:
        parser.exit(error.returncode, f"npm publication failed: {error}\n")
    except (ValueError, KeyError, TypeError, OSError, subprocess.SubprocessError) as error:
        parser.exit(1, f"npm publication failed: {error}\n")


if __name__ == "__main__":
    main()
