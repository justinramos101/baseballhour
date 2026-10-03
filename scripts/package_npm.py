#!/usr/bin/env python3
"""Pack verified native releases into the baseballhour npm package."""

import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tarfile
import tempfile
import tomllib
import urllib.request


ROOT = Path(__file__).resolve().parents[1]
REPOSITORY = "justinramos101/baseballhour"
UPSTREAM = f"https://github.com/{REPOSITORY}"
STABLE_VERSION = r"(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)"


def load_release(path=None):
    return validate_release(json.loads((path or ROOT / "npm/release.json").read_text()))


def validate_release(release):
    if not isinstance(release, dict) or set(release) != {"version", "tag", "platforms"}:
        raise ValueError("release must contain version, tag, and platforms")
    if not re.fullmatch(STABLE_VERSION, release["version"]):
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


def github_json(endpoint):
    result = subprocess.run(["gh", "api", f"repos/{REPOSITORY}/{endpoint}"],
                            check=True, text=True, capture_output=True)
    return json.loads(result.stdout)


def checksum(sidecar, name):
    match = re.fullmatch(r"([0-9a-f]{64})  " + re.escape(name) + r"\n", sidecar)
    if not match:
        raise ValueError(f"invalid checksum sidecar: {name}")
    return match[1]


def resolve_release(tag, assets, destination):
    if not re.fullmatch("v" + STABLE_VERSION, tag):
        raise ValueError("tag must be vMAJOR.MINOR.PATCH with no prerelease or leading zeroes")
    metadata = github_json(f"releases/tags/{tag}")
    if metadata["tag_name"] != tag or metadata["draft"] is not False or metadata["prerelease"] is not False:
        raise ValueError("expected a published stable release for the requested tag")
    commit = github_json(f"commits/{tag}")["sha"]
    if not re.fullmatch(r"[0-9a-f]{40}", commit):
        raise ValueError("invalid native source commit")
    cargo = github_json(f"contents/Cargo.toml?ref={commit}")
    version = tomllib.loads(base64.b64decode(cargo["content"]).decode())["package"]["version"]
    if tag != f"v{version}":
        raise ValueError("native Cargo version does not match the release tag")
    release = load_release()
    release.update(version=version, tag=tag)
    listed = metadata["assets"]
    names = [entry["name"] for entry in listed]
    if len(names) != len(set(names)):
        raise ValueError("duplicate release asset names")
    entries = {entry["name"]: entry for entry in listed}
    for asset in release["platforms"]:
        name = archive_name(release, asset)
        for filename in (name, name + ".sha256"):
            entry = entries.get(filename)
            if not entry or entry["state"] != "uploaded" or entry["size"] <= 0:
                raise ValueError(f"missing or incomplete release asset: {filename}")
            target = destination / filename
            if assets:
                shutil.copyfile(assets / filename, target)
            else:
                url = f"{UPSTREAM}/releases/download/{tag}/{filename}"
                with urllib.request.urlopen(url, timeout=60) as source, target.open("wb") as output:
                    shutil.copyfileobj(source, output)
            if target.stat().st_size != entry["size"]:
                raise ValueError(f"release asset size mismatch: {filename}")
        asset["sha256"] = checksum((destination / (name + ".sha256")).read_bytes().decode("utf-8"), name)
        archive = destination / name
        if hashlib.sha256(archive.read_bytes()).hexdigest() != asset["sha256"]:
            raise ValueError(f"SHA-256 mismatch: {name}")
        digest = entries[name].get("digest")
        if digest is not None and digest != "sha256:" + asset["sha256"]:
            raise ValueError(f"GitHub asset digest mismatch: {name}")
        subprocess.run([
            "gh", "attestation", "verify", str(archive), "--repo", REPOSITORY,
            "--signer-workflow", f"{REPOSITORY}/.github/workflows/release.yml",
            "--source-ref", f"refs/tags/{tag}", "--source-digest", commit,
            "--deny-self-hosted-runners",
        ], check=True)
    return validate_release(release), commit


def integrity(archive):
    return "sha512-" + base64.b64encode(hashlib.sha512(archive.read_bytes()).digest()).decode()


def verify_evidence(archive, release, path):
    evidence = json.loads(path.read_text())
    fields = {"tag", "native_commit", "package_source_commit", "filename", "integrity"}
    if not isinstance(evidence, dict) or set(evidence) != fields:
        raise ValueError("unexpected package evidence schema")
    for field in ("native_commit", "package_source_commit"):
        if field == "native_commit" and evidence[field] is None:
            continue
        if not isinstance(evidence[field], str) or not re.fullmatch(r"[0-9a-f]{40}", evidence[field]):
            raise ValueError(f"invalid evidence commit: {field}")
    source = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    if evidence["package_source_commit"] != source or source != os.environ.get("GITHUB_SHA", source):
        raise ValueError("package source commit differs from the verification checkout or run")
    if (evidence["tag"] != release["tag"] or evidence["filename"] != archive.name
            or evidence["integrity"] != integrity(archive)):
        raise ValueError("package evidence does not match the archive and release")
    return evidence


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


def build(assets, output, tag=None, release_manifest=None):
    output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="stage-", dir=output) as temporary:
        native_commit = None
        if tag:
            release, native_commit = resolve_release(tag, assets, Path(temporary))
            assets = Path(temporary)
        else:
            release = load_release(release_manifest)
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
             "--pack-destination", temporary],
            cwd=stage, text=True, capture_output=True, check=True,
        )
        packed = json.loads(result.stdout)
        expected_filename = f"baseballhour-{release['version']}.tgz"
        if len(packed) != 1 or packed[0]["filename"] != expected_filename:
            raise ValueError("npm pack returned an unexpected package")
        archive = Path(temporary) / expected_filename
        inventory = inspect_tarball(archive, release, stage)
        evidence = {
            "tag": release["tag"], "native_commit": native_commit,
            "package_source_commit": subprocess.check_output(
                ["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
            "filename": archive.name, "integrity": integrity(archive),
        }
        for filename, data in (("release.json", release), ("evidence.json", evidence)):
            (Path(temporary) / filename).write_text(json.dumps(data, indent=2) + "\n")
        for filename in (expected_filename, "release.json", "evidence.json"):
            (Path(temporary) / filename).replace(output / filename)
        archive = output / expected_filename
        print(f"Verified {len(inventory)} packed files. {archive} ({archive.stat().st_size} bytes)")
    return archive


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--assets", type=Path, help="directory of already downloaded release archives")
    parser.add_argument("--output", type=Path, default=ROOT / "dist/npm")
    selection = parser.add_mutually_exclusive_group()
    selection.add_argument("--tag", help="published stable GitHub release to resolve and attest")
    selection.add_argument("--release-manifest", type=Path, help="validated release table to package")
    args = parser.parse_args()
    try:
        build(args.assets.resolve() if args.assets else None, args.output.resolve(),
              args.tag, args.release_manifest)
    except subprocess.CalledProcessError as error:
        parser.exit(1, f"npm packaging failed: {error.stderr or error}\n")
    except (ValueError, KeyError, TypeError, OSError, tarfile.TarError, subprocess.SubprocessError) as error:
        parser.exit(1, f"npm packaging failed: {error}\n")


if __name__ == "__main__":
    main()
