#!/usr/bin/env python3

import base64
import copy
import hashlib
import io
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch
import urllib.error

import package_npm as package
import publish_npm as publisher


class ReleaseTests(unittest.TestCase):
    def test_invalid_tags_fail_before_network(self):
        with patch.object(package, "github_json") as api:
            for tag in ("latest", "1.2.3", "v01.2.3", "v1.2.3-rc.1", "v1.2.3\n", "v١.2.3"):
                with self.subTest(tag=tag), self.assertRaises(ValueError):
                    package.resolve_release(tag, None, Path("unused"))
            api.assert_not_called()

    def test_strict_sidecars(self):
        name = "archive.tar.gz"
        digest = "a" * 64
        self.assertEqual(package.checksum(f"{digest}  {name}\n", name), digest)
        for value in (f"{digest} {name}\n", f"{digest}  {name}\r\n", f"{digest}  ../{name}\n",
                      f"{digest}  {name}\nextra\n", f"{'A' * 64}  {name}\n"):
            with self.subTest(value=value), self.assertRaises(ValueError):
                package.checksum(value, name)

    def test_platform_manifest_rejects_wrong_identity(self):
        fixture = package.load_release()
        for field, value in (("version", "01.0.0"), ("tag", "v9.9.9")):
            invalid = copy.deepcopy(fixture)
            invalid[field] = value
            with self.assertRaises(ValueError):
                package.validate_release(invalid)
        for field, value in (("target", "wrong-target"), ("sha256", "xyz")):
            invalid = copy.deepcopy(fixture)
            invalid["platforms"][0][field] = value
            with self.assertRaises(ValueError):
                package.validate_release(invalid)

    def test_resolution_binds_assets_and_cargo_to_attested_native_commit(self):
        fixture = package.load_release()
        commit = "1" * 40
        metadata = {"tag_name": fixture["tag"], "draft": False, "prerelease": False, "assets": []}
        with tempfile.TemporaryDirectory() as directory:
            cache = Path(directory) / "cache"
            output = Path(directory) / "output"
            cache.mkdir()
            output.mkdir()
            for asset in fixture["platforms"]:
                name = package.archive_name(fixture, asset)
                data = ("native " + asset["target"]).encode()
                digest = hashlib.sha256(data).hexdigest()
                for filename, content in ((name, data), (name + ".sha256", f"{digest}  {name}\n".encode())):
                    (cache / filename).write_bytes(content)
                    metadata["assets"].append({"name": filename, "size": len(content), "state": "uploaded"})
            cargo = base64.b64encode(f'[package]\nversion = "{fixture["version"]}"\n'.encode()).decode()

            def api(endpoint):
                return {f'releases/tags/{fixture["tag"]}': metadata,
                        f'commits/{fixture["tag"]}': {"sha": commit},
                        f"contents/Cargo.toml?ref={commit}": {"content": cargo}}[endpoint]

            with patch.object(package, "github_json", side_effect=api), patch.object(package.subprocess, "run") as run:
                release, source = package.resolve_release(fixture["tag"], cache, output)
                self.assertEqual(source, commit)
                self.assertEqual(len(release["platforms"]), 4)
                self.assertEqual(run.call_count, 4)
                for call in run.call_args_list:
                    command = call.args[0]
                    self.assertEqual(command[command.index("--source-digest") + 1], commit)
                    self.assertEqual(command[command.index("--source-ref") + 1], f'refs/tags/{fixture["tag"]}')
                for field in ("draft", "prerelease"):
                    metadata[field] = True
                    with self.assertRaises(ValueError):
                        package.resolve_release(fixture["tag"], cache, output)
                    metadata[field] = False
                metadata["assets"][0]["state"] = "new"
                with self.assertRaisesRegex(ValueError, "incomplete"):
                    package.resolve_release(fixture["tag"], cache, output)
                metadata["assets"][0]["state"] = "uploaded"
                archive = cache / metadata["assets"][0]["name"]
                archive.write_bytes(b"x" * archive.stat().st_size)
                with self.assertRaisesRegex(ValueError, "SHA-256 mismatch"):
                    package.resolve_release(fixture["tag"], cache, output)

    def test_integrity_and_evidence_reject_modified_tarball(self):
        release = package.load_release()
        with tempfile.TemporaryDirectory() as directory:
            archive = Path(directory) / "package.tgz"
            archive.write_bytes(b"original")
            evidence = Path(directory) / "evidence.json"
            commit = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=package.ROOT, text=True).strip()
            evidence.write_text(json.dumps({"tag": release["tag"], "native_commit": "1" * 40,
                                           "package_source_commit": commit, "filename": archive.name,
                                           "integrity": package.integrity(archive)}))
            package.verify_evidence(archive, release, evidence)
            archive.write_bytes(b"changed")
            with self.assertRaisesRegex(ValueError, "does not match"):
                package.verify_evidence(archive, release, evidence)


class PublicationTests(unittest.TestCase):
    def test_only_http_404_means_absent(self):
        for status in (401, 403, 404, 429, 500):
            error = urllib.error.HTTPError("registry", status, "failure", {}, None)
            with self.subTest(status=status), patch.object(publisher.urllib.request, "urlopen", side_effect=error):
                if status == 404:
                    self.assertIsNone(publisher.registry_integrity("0.1.0"))
                else:
                    with self.assertRaises(urllib.error.HTTPError):
                        publisher.registry_integrity("0.1.0")
            error.close()
        with patch.object(publisher.urllib.request, "urlopen", side_effect=urllib.error.URLError("offline")):
            with self.assertRaises(urllib.error.URLError):
                publisher.registry_integrity("0.1.0")

    def test_registry_metadata_requires_identity_and_integrity(self):
        for metadata in ({"error": "no"}, {"name": "baseballhour", "version": "9.0.0"},
                         {"name": "baseballhour", "version": "0.1.0", "dist": {}}):
            with patch.object(publisher.urllib.request, "urlopen", return_value=io.BytesIO(json.dumps(metadata).encode())):
                with self.assertRaises(ValueError):
                    publisher.registry_integrity("0.1.0")

    def test_publication_reconciles_exact_bytes_and_preserves_failures(self):
        evidence = {"native_commit": "1" * 40, "integrity": "sha512-expected"}
        with patch.object(publisher, "load_release", return_value={"version": "0.1.0"}), \
                patch.object(publisher, "verify_evidence", return_value=evidence), \
                patch.object(publisher, "inspect_tarball"), \
                patch.object(publisher, "registry_integrity") as registry, \
                patch.object(publisher.subprocess, "run") as run, \
                patch.object(publisher.time, "sleep"):
            args = (Path("exact.tgz"), Path("release.json"), Path("evidence.json"))
            registry.return_value = "sha512-expected"
            publisher.publish(*args)
            run.assert_not_called()
            registry.return_value = "sha512-different"
            with self.assertRaisesRegex(ValueError, "different bytes"):
                publisher.publish(*args)
            run.assert_not_called()
            registry.side_effect = [None, None, "sha512-expected"]
            publisher.publish(*args)
            self.assertIn("exact.tgz", run.call_args.args[0])
            self.assertIn("--provenance", run.call_args.args[0])
            run.reset_mock()
            # A version that appears after a few minutes still succeeds.
            late = publisher.VISIBILITY_TIMEOUT_SECONDS // publisher.VISIBILITY_POLL_SECONDS
            registry.side_effect = [None] * late + ["sha512-expected"]
            publisher.publish(*args)
            run.reset_mock()
            registry.reset_mock(side_effect=True)
            registry.return_value = None
            with self.assertRaisesRegex(ValueError, "still absent after 300 seconds"):
                publisher.publish(*args)
            # One check before publishing, then a poll every interval through the timeout.
            self.assertEqual(registry.call_count, 1 + late + 1)
            run.reset_mock()
            registry.reset_mock()
            registry.side_effect = [None]
            run.side_effect = subprocess.CalledProcessError(1, "npm publish")
            with self.assertRaises(subprocess.CalledProcessError):
                publisher.publish(*args)
            run.assert_called_once()


if __name__ == "__main__":
    unittest.main()
