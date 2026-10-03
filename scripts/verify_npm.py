#!/usr/bin/env python3
"""Verify a packed npm release through global installation, npx, and a real PTY."""

import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

from package_npm import ROOT, inspect_tarball, load_release, native_path


def run(command, env, cwd, success=True):
    result = subprocess.run(command, env=env, cwd=cwd, text=True, capture_output=True, timeout=120)
    if success and result.returncode != 0:
        raise AssertionError(f"{command!r} exited {result.returncode}: {result.stderr}")
    if not success and result.returncode == 0:
        raise AssertionError(f"{command!r} unexpectedly succeeded")
    return result


def check_cli(command, version, env, cwd):
    assert run(command + ["--version"], env, cwd).stdout.strip() == f"baseballhour {version}"
    result = run(command + ["--demo", "--json", "--team", "New York Yankees"], env, cwd)
    data = json.loads(result.stdout)
    assert data["demo"] is True and data["source"] == "demo", data
    assert len(data["games"]) == 1, data
    assert "New York Yankees" in [data["games"][0][side]["name"] for side in ("away", "home")]
    frame = run(command + ["--demo", "--once"], env, cwd).stdout
    assert "BASEBALL HOUR  DEMO" in frame and "\x1b" not in frame, frame
    failure = run(command + ["--baseballhour-invalid-option"], env, cwd, success=False)
    assert failure.returncode == 2 and "unexpected argument" in failure.stderr, failure


def check_links(installed, env, cwd, version):
    links = cwd / "links with spaces"
    links.mkdir()
    absolute = links / "absolute"
    relative = links / "relative"
    chained = links / "chained"
    absolute.symlink_to(installed)
    relative.symlink_to(os.path.relpath(installed, links))
    chained.symlink_to("relative")
    for link in (absolute, relative, chained):
        assert run([str(link), "--version"], env, cwd).stdout.strip() == f"baseballhour {version}"
    path_env = dict(env, PATH=str(links) + os.pathsep + env["PATH"])
    assert run(["chained", "--version"], path_env, cwd).stdout.strip() == f"baseballhour {version}"


def check_launcher_failures(installed, release, env, cwd):
    fake_package = cwd / "missing package"
    (fake_package / "bin").mkdir(parents=True)
    fake_launcher = fake_package / "bin/baseballhour"
    shutil.copyfile(installed.resolve(), fake_launcher)
    fake_launcher.chmod(0o755)
    missing = run([str(fake_launcher), "--version"], env, cwd, success=False)
    assert missing.returncode == 126 and "packaged binary is missing" in missing.stderr, missing

    commands = cwd / "platform commands"
    commands.mkdir()
    uname = commands / "uname"
    uname.write_text('#!/bin/sh\nprintf "%s\\n" "unsupported-test-host"\n')
    uname.chmod(0o755)
    unsupported = run([str(installed)], dict(env, PATH=str(commands) + os.pathsep + env["PATH"]), cwd, False)
    assert "unsupported platform" in unsupported.stderr and unsupported.stdout == "", unsupported

    for asset in release["platforms"]:
        cpu = "x86_64" if asset["cpu"] == "x64" else "aarch64"
        os_name = "Linux" if asset["os"] == "linux" else "Darwin"
        uname.write_text(f'#!/bin/sh\ncase "$1" in -s) echo {os_name};; -m) echo {cpu};; esac\n')
        selected = fake_package / native_path(asset)
        selected.parent.mkdir(parents=True, exist_ok=True)
        selected.write_text('#!/bin/sh\nprintf "%s\\n" "$PWD" "$NPM_LAUNCHER_PROBE" "$@"\nexit 23\n')
        selected.chmod(0o755)
        probe_env = dict(env, PATH=str(commands) + os.pathsep + env["PATH"], NPM_LAUNCHER_PROBE="kept value")
        result = run([str(fake_launcher), "with spaces", "", "*"], probe_env, cwd, success=False)
        assert result.returncode == 23, result
        assert result.stdout.splitlines() == [str(cwd), "kept value", "with spaces", "", "*"], result
        selected.unlink()


def terminal(command, output, env, cwd):
    output.mkdir(parents=True, exist_ok=True)
    result = run(
        ["python3", str(ROOT / "scripts/verify_terminal.py"), "--output", str(output), "--command", *command],
        env, cwd,
    )
    print(result.stdout.strip())


def verify(archive, output):
    release = load_release()
    inventory = inspect_tarball(archive, release)
    npm = shutil.which("npm")
    npx = shutil.which("npx")
    if not npm or not npx:
        raise ValueError("npm and npx must be installed")
    with tempfile.TemporaryDirectory(prefix="baseballhour npm check ") as temporary:
        work = Path(temporary).resolve()
        prefix = work / "global prefix"
        userconfig = work / "user.npmrc"
        globalconfig = work / "global.npmrc"
        userconfig.write_text("")
        globalconfig.write_text("")
        env = dict(os.environ, npm_config_userconfig=str(userconfig), npm_config_globalconfig=str(globalconfig),
                   npm_config_offline="true", npm_config_ignore_scripts="true", npm_config_audit="false",
                   npm_config_fund="false", npm_config_update_notifier="false")
        run([npm, "install", "--global", "--ignore-scripts", "--offline", "--prefix", str(prefix),
             "--cache", str(work / "global cache"), str(archive)], env, work)
        installed = prefix / "bin/baseballhour"
        check_cli([str(installed)], release["version"], env, work)
        check_links(installed, env, work, release["version"])
        check_launcher_failures(installed, release, env, work)
        terminal([str(installed)], output / "global", env, work)
        print("Global install, arguments, symlinks, platform dispatch, and failure checks passed.")

        command = [npx, "--yes", "--ignore-scripts", "--offline", "--cache", str(work / "fresh npx cache"),
                   "--package", str(archive), "--", "baseballhour"]
        check_cli(command, release["version"], env, work)
        terminal(command, output / "npx", env, work)
        run([npm, "uninstall", "--global", "--ignore-scripts", "--offline", "--prefix", str(prefix),
             "--cache", str(work / "global cache"), "baseballhour"], env, work)
        assert not installed.exists(), "global uninstall left the command installed"
    print(f"Verified {len(inventory)} packed files, offline global install, fresh-cache npx, and both PTY entry points.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("archive", nargs="?", type=Path)
    parser.add_argument("--output", type=Path, default=ROOT / "dist/npm/verification")
    args = parser.parse_args()
    try:
        archive = args.archive or ROOT / f"dist/npm/baseballhour-{load_release()['version']}.tgz"
        verify(archive.resolve(), args.output.resolve())
    except (AssertionError, ValueError, OSError, subprocess.SubprocessError) as error:
        parser.exit(1, f"npm verification failed: {error}\n")


if __name__ == "__main__":
    main()
