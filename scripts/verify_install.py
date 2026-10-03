#!/usr/bin/env python3
import hashlib
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]
INSTALLER = ROOT / "scripts" / "install.sh"
VERSION = "0.1.0"


class ReleaseFixture:
    def __init__(self, directory: Path, target: str, reported_version: str = VERSION):
        self.name = f"baseballhour-v{VERSION}-{target}"
        archive = directory / f"{self.name}.tar.gz"
        binary = directory / self.name / "baseballhour"
        binary.parent.mkdir(parents=True)
        binary.write_text(
            "#!/bin/sh\n"
            f"[ \"${{1:-}}\" = --version ] && printf '%s\\n' 'baseballhour {reported_version}'\n"
        )
        binary.chmod(0o755)
        with tarfile.open(archive, "w:gz") as output:
            output.add(binary, arcname=f"{self.name}/baseballhour")
        digest = hashlib.sha256(archive.read_bytes()).hexdigest()
        self.checksum = directory / f"{archive.name}.sha256"
        self.checksum.write_text(f"{digest}  {archive.name}\n")
        shutil.rmtree(directory / self.name)


class InstallerSandbox:
    def __init__(self, platform: str = "Linux", machine: str = "x86_64"):
        self.context = tempfile.TemporaryDirectory(prefix="baseballhour-install-test-")
        self.root = Path(self.context.name)
        self.releases = self.root / "releases"
        self.prefix = self.root / "install prefix"
        self.temp = self.root / "tmp"
        self.mock_bin = self.root / "mock-bin"
        for directory in (self.releases, self.temp, self.mock_bin):
            directory.mkdir()
        self._write_mock("uname", self._uname_mock())
        self._write_mock("curl", self._curl_mock())
        self.environment = os.environ.copy()
        self.environment.update(
            {
                "HOME": str(self.root / "home"),
                "PATH": f"{self.mock_bin}{os.pathsep}{os.environ['PATH']}",
                "TMPDIR": str(self.temp),
                "MOCK_RELEASES": str(self.releases),
                "MOCK_LATEST_VERSION": f"v{VERSION}",
                "MOCK_UNAME_S": platform,
                "MOCK_UNAME_M": machine,
            }
        )

    def close(self):
        self.context.cleanup()

    def release(self, target: str, reported_version: str = VERSION) -> ReleaseFixture:
        return ReleaseFixture(self.releases, target, reported_version)

    def run(self, *arguments: str) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            ["sh", str(INSTALLER), *arguments],
            cwd=self.root,
            env=self.environment,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            check=False,
        )

    def install(self, *arguments: str) -> subprocess.CompletedProcess[str]:
        return self.run("--prefix", str(self.prefix), *arguments)

    @property
    def binary(self) -> Path:
        return self.prefix / "bin" / "baseballhour"

    def seed_existing_binary(self) -> bytes:
        content = b"#!/bin/sh\nprintf '%s\\n' 'baseballhour old'\n"
        self.binary.parent.mkdir(parents=True)
        self.binary.write_bytes(content)
        self.binary.chmod(0o755)
        return content

    def assert_no_debris(self, case: unittest.TestCase):
        case.assertEqual(list(self.temp.iterdir()), [])
        if self.binary.parent.exists():
            case.assertEqual(list(self.binary.parent.glob(".baseballhour.*")), [])

    def _write_mock(self, name: str, content: str):
        path = self.mock_bin / name
        path.write_text(content)
        path.chmod(0o755)

    @staticmethod
    def _uname_mock() -> str:
        return """#!/bin/sh
case "${1:-}" in
  -s) printf '%s\n' "$MOCK_UNAME_S" ;;
  -m) printf '%s\n' "$MOCK_UNAME_M" ;;
  *) exit 2 ;;
esac
"""

    @staticmethod
    def _curl_mock() -> str:
        return """#!/bin/sh
output=
write_out=
url=
while [ "$#" -gt 0 ]; do
  case "$1" in
    --output|--write-out|--proto|--proto-redir|--connect-timeout|--max-time|--retry)
      option=$1
      value=$2
      [ "$option" = --output ] && output=$value
      [ "$option" = --write-out ] && write_out=$value
      shift 2 ;;
    --*) shift ;;
    *) url=$1; shift ;;
  esac
done
case "$url" in
  */latest)
    [ "$write_out" = '%{url_effective}' ] || exit 2
    printf 'https://github.com/justinramos101/baseballhour/releases/tag/%s' "$MOCK_LATEST_VERSION" ;;
  *)
    source=$MOCK_RELEASES/${url##*/}
    [ -f "$source" ] || exit 22
    cp "$source" "$output" ;;
esac
"""


class InstallVerification(unittest.TestCase):
    def test_supported_platforms_install_matching_native_archive(self):
        cases = (
            ("Linux", "x86_64", "x86_64-unknown-linux-musl"),
            ("Linux", "aarch64", "aarch64-unknown-linux-musl"),
            ("Darwin", "x86_64", "x86_64-apple-darwin"),
            ("Darwin", "arm64", "aarch64-apple-darwin"),
        )
        for platform, machine, target in cases:
            with self.subTest(platform=platform, machine=machine):
                sandbox = InstallerSandbox(platform, machine)
                try:
                    sandbox.release(target)
                    result = sandbox.install("--version", VERSION)
                    self.assertEqual(result.returncode, 0, result.stdout)
                    self.assertEqual(
                        subprocess.check_output(
                            [sandbox.binary, "--version"], text=True
                        ).strip(),
                        "baseballhour 0.1.0",
                    )
                    self.assertIn(f"Downloading Baseball Hour v0.1.0 for {target}", result.stdout)
                    sandbox.assert_no_debris(self)
                finally:
                    sandbox.close()

    def test_latest_release_and_prefix_with_spaces(self):
        sandbox = InstallerSandbox()
        try:
            sandbox.release("x86_64-unknown-linux-musl")
            result = sandbox.install()
            self.assertEqual(result.returncode, 0, result.stdout)
            self.assertIn(f"Installed {sandbox.binary}", result.stdout)
            self.assertTrue(os.access(sandbox.binary, os.X_OK))
            sandbox.assert_no_debris(self)
        finally:
            sandbox.close()

    def test_successful_upgrade_replaces_existing_binary_atomically(self):
        sandbox = InstallerSandbox()
        try:
            sandbox.release("x86_64-unknown-linux-musl")
            old = sandbox.seed_existing_binary()
            result = sandbox.install("--version", "v0.1.0")
            self.assertEqual(result.returncode, 0, result.stdout)
            self.assertNotEqual(sandbox.binary.read_bytes(), old)
            self.assertEqual(
                subprocess.check_output([sandbox.binary, "--version"], text=True).strip(),
                "baseballhour 0.1.0",
            )
            sandbox.assert_no_debris(self)
        finally:
            sandbox.close()

    def test_download_and_validation_failures_preserve_existing_binary(self):
        cases = ("missing archive", "bad checksum", "wrong binary version")
        for failure in cases:
            with self.subTest(failure=failure):
                sandbox = InstallerSandbox()
                try:
                    fixture = sandbox.release(
                        "x86_64-unknown-linux-musl",
                        "9.9.9" if failure == "wrong binary version" else VERSION,
                    )
                    if failure == "missing archive":
                        next(sandbox.releases.glob("*.tar.gz")).unlink()
                    elif failure == "bad checksum":
                        fixture.checksum.write_text(f"{'0' * 64}  archive.tar.gz\n")
                    old = sandbox.seed_existing_binary()
                    result = sandbox.install("--version", VERSION)
                    self.assertNotEqual(result.returncode, 0, result.stdout)
                    expected = {
                        "missing archive": "Could not download the release",
                        "bad checksum": "Checksum mismatch",
                        "wrong binary version": "binary version does not match the release",
                    }[failure]
                    self.assertIn(expected, result.stdout)
                    self.assertEqual(sandbox.binary.read_bytes(), old)
                    sandbox.assert_no_debris(self)
                finally:
                    sandbox.close()

    def test_unsupported_systems_fail_before_installing(self):
        cases = (
            ("FreeBSD", "x86_64", "Supported platforms are macOS and Linux"),
            ("Linux", "riscv64", "Supported processors are x86_64 and ARM64"),
        )
        for platform, machine, message in cases:
            with self.subTest(platform=platform, machine=machine):
                sandbox = InstallerSandbox(platform, machine)
                try:
                    result = sandbox.install("--version", VERSION)
                    self.assertNotEqual(result.returncode, 0, result.stdout)
                    self.assertIn(message, result.stdout)
                    self.assertFalse(sandbox.binary.exists())
                    sandbox.assert_no_debris(self)
                finally:
                    sandbox.close()

    def test_options_require_values(self):
        cases = (
            (("--version",), "--version requires a value"),
            (("--prefix",), "--prefix requires a value"),
            (("--version", "--prefix"), "--version requires a value"),
        )
        for arguments, message in cases:
            with self.subTest(arguments=arguments):
                sandbox = InstallerSandbox()
                try:
                    result = sandbox.run(*arguments)
                    self.assertNotEqual(result.returncode, 0, result.stdout)
                    self.assertIn(message, result.stdout)
                    self.assertFalse(sandbox.binary.exists())
                    sandbox.assert_no_debris(self)
                finally:
                    sandbox.close()


if __name__ == "__main__":
    unittest.main(verbosity=2)
