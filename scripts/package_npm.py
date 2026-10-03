#!/usr/bin/env python3
"""Pack verified native releases into the baseballhour npm package."""

import argparse
import hashlib
import json
from pathlib import Path
import re
import shutil
import subprocess
import tarfile
import tempfile
import urllib.request


ROOT = Path(__file__).resolve().parents[1]
UPSTREAM = "https://github.com/justinramos101/baseballhour"


def load_release():
    release = json.loads((ROOT / "npm/release.json").read_text())
    if set(release) != {"version", "tag", "platforms"}:
        raise ValueError("release must contain version, tag, and platforms")
    if not re.fullmatch(r"(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)", release["version"]):
        raise ValueError("release version must be a stable semantic version")
    if release["tag"] != f"v{release['version']}":
        raise ValueError("release tag must match its version")
    expected = {(os, cpu) for os in ("linux", "darwin") for cpu in ("x64", "arm64")}
    seen = set()
    for asset in release["platforms"]:
        if set(asset) != {"os", "cpu", "target", "sha256"}:
            raise ValueError("platform must contain os, cpu, target, and sha256")
        platform = (asset["os"], asset["cpu"])
        if platform not in expected or platform in seen:
            raise ValueError(f"unsupported or duplicate platform: {platform}")
        seen.add(platform)
        cpu = "x86_64" if asset["cpu"] == "x64" else "aarch64"
        suffix = "unknown-linux-musl" if asset["os"] == "linux" else "apple-darwin"
        if asset["target"] != f"{cpu}-{suffix}":
            raise ValueError(f"target does not match platform: {platform}")
        if not re.fullmatch(r"[0-9a-f]{64}", asset["sha256"]):
            raise ValueError(f"invalid SHA-256 for {platform}")
    if seen != expected:
        raise ValueError("release must contain all four platforms")
    return release


def archive_name(release, asset):
    return f"baseballhour-{release['tag']}-{asset['target']}.tar.gz"


def native_path(asset):
    return f"vendor/{asset['os']}-{asset['cpu']}/baseballhour"


def launcher(release):
    cases = []
    for asset in release["platforms"]:
        os = "Linux" if asset["os"] == "linux" else "Darwin"
        arches = ("x86_64",) if asset["cpu"] == "x64" else ("arm64", "aarch64")
        patterns = "|".join(f"{os}:{arch}" for arch in arches)
        cases.append(f"  {patterns}) native={native_path(asset)} ;;")
    return (ROOT / "npm/baseballhour.sh").read_text().replace("@PLATFORMS@", "\n".join(cases))


def package_manifest(release):
    return {
        "name": "baseballhour",
        "version": release["version"],
        "description": "Baseball schedules, scores, and ballparks in your terminal",
        "keywords": ["baseball", "mlb", "milb", "tui", "terminal"],
        "license": "MIT",
        "homepage": UPSTREAM + "#readme",
        "repository": {"type": "git", "url": "git+" + UPSTREAM + ".git"},
        "bugs": {"url": UPSTREAM + "/issues"},
        "bin": {"baseballhour": "bin/baseballhour"},
        "os": sorted({asset["os"] for asset in release["platforms"]}),
        "cpu": sorted({asset["cpu"] for asset in release["platforms"]}),
        "files": ["bin/baseballhour", "README.md", "LICENSE"]
        + [native_path(asset) for asset in release["platforms"]],
    }


def extract_binary(archive, release, asset, destination):
    digest = hashlib.sha256(archive.read_bytes()).hexdigest()
    if digest != asset["sha256"]:
        raise ValueError(f"SHA-256 mismatch: {archive.name}")
    root = archive_name(release, asset).removesuffix(".tar.gz")
    expected = {f"{root}/{name}" for name in ("baseballhour", "README.md", "LICENSE")}
    with tarfile.open(archive, "r:gz") as bundle:
        members = bundle.getmembers()
        names = [member.name for member in members]
        if len(names) != len(set(names)) or set(names) != expected:
            raise ValueError(f"unexpected or duplicate archive members: {archive.name}")
        if not all(member.isfile() for member in members):
            raise ValueError(f"release archive contains a non-regular file: {archive.name}")
        binary = bundle.getmember(f"{root}/baseballhour")
        if not binary.mode & 0o111 or binary.size == 0:
            raise ValueError(f"release binary is empty or not executable: {archive.name}")
        destination.parent.mkdir(parents=True, exist_ok=True)
        with bundle.extractfile(binary) as source, destination.open("wb") as output:
            shutil.copyfileobj(source, output)
    destination.chmod(0o755)


def inspect_tarball(archive, release, stage=None):
    manifest = package_manifest(release)
    expected = {"package.json", *manifest["files"]}
    executable = {"bin/baseballhour", *(native_path(asset) for asset in release["platforms"])}
    with tarfile.open(archive, "r:gz") as bundle:
        members = bundle.getmembers()
        names = [member.name for member in members]
        if len(names) != len(set(names)) or set(names) != {f"package/{name}" for name in expected}:
            raise ValueError("npm tarball inventory does not match the whitelist")
        for member in members:
            name = member.name.removeprefix("package/")
            mode = 0o755 if name in executable else 0o644
            if not member.isfile() or member.mode != mode or member.size == 0:
                raise ValueError(f"invalid npm file type, permissions, or size: {name}")
            data = bundle.extractfile(member).read()
            if stage and data != (stage / name).read_bytes():
                raise ValueError(f"packed bytes differ from staged file: {name}")
            if name == "package.json" and json.loads(data) != manifest:
                raise ValueError("packed manifest does not match the release")
            if name == "bin/baseballhour" and data.decode() != launcher(release):
                raise ValueError("packed launcher does not match the platform table")
    return sorted(expected)


def build(assets, output):
    release = load_release()
    output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="stage-", dir=output) as temporary:
        stage = Path(temporary) / "package"
        (stage / "bin").mkdir(parents=True)
        (stage / "bin/baseballhour").write_text(launcher(release))
        (stage / "bin/baseballhour").chmod(0o755)
        (stage / "package.json").write_text(json.dumps(package_manifest(release), indent=2) + "\n")
        shutil.copyfile(ROOT / "npm/README.md", stage / "README.md")
        shutil.copyfile(ROOT / "LICENSE", stage / "LICENSE")
        for name in ("package.json", "README.md", "LICENSE"):
            (stage / name).chmod(0o644)
        for asset in release["platforms"]:
            name = archive_name(release, asset)
            if assets:
                archive = assets / name
            else:
                archive = Path(temporary) / name
                url = f"{UPSTREAM}/releases/download/{release['tag']}/{name}"
                with urllib.request.urlopen(url, timeout=60) as source, archive.open("wb") as target:
                    shutil.copyfileobj(source, target)
            extract_binary(archive, release, asset, stage / native_path(asset))
        result = subprocess.run(
            ["npm", "pack", "--ignore-scripts", "--json", "--cache", str(Path(temporary) / "cache"),
             "--pack-destination", str(output)],
            cwd=stage, text=True, capture_output=True, check=True,
        )
        packed = json.loads(result.stdout)
        expected_filename = f"baseballhour-{release['version']}.tgz"
        if len(packed) != 1 or packed[0]["filename"] != expected_filename:
            raise ValueError("npm pack returned an unexpected package")
        archive = output / expected_filename
        inventory = inspect_tarball(archive, release, stage)
        print(f"Verified {len(inventory)} packed files. {archive} ({archive.stat().st_size} bytes)")
    return archive


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--assets", type=Path, help="directory of already downloaded release archives")
    parser.add_argument("--output", type=Path, default=ROOT / "dist/npm")
    args = parser.parse_args()
    try:
        build(args.assets.resolve() if args.assets else None, args.output.resolve())
    except subprocess.CalledProcessError as error:
        parser.exit(1, f"npm packaging failed: {error.stderr or error}\n")
    except (ValueError, OSError, tarfile.TarError, subprocess.SubprocessError) as error:
        parser.exit(1, f"npm packaging failed: {error}\n")


if __name__ == "__main__":
    main()
