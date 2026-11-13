import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


SCRIPT = Path(__file__).with_name("remote-release.py")
HOST = "io.emma.mediacenter.browser"
ORIGIN = "chrome-extension://njhbcllpkbihpkioegopblabopnkhndk/"


class RemoteReleaseTest(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name)
        self.host = self.root / "package with spaces" / "mediacenter-browser-host"
        self.host.parent.mkdir()
        self.host.write_text("#!/bin/sh\n", encoding="utf-8")
        self.host.chmod(0o700)
        self.profile = self.root / "managed-profile"
        self.profile.mkdir()
        self.registration = self.profile / "NativeMessagingHosts" / f"{HOST}.json"

    def run_cli(self, *args, success=True, env=None):
        result = subprocess.run(
            [sys.executable, SCRIPT, *args], text=True, capture_output=True, env=env
        )
        if success:
            self.assertEqual(result.returncode, 0, result.stderr)
        else:
            self.assertNotEqual(result.returncode, 0, result.stdout)
        return result

    def registration_command(self, action, success=True):
        return self.run_cli(
            action, "--host", self.host, "--user-data-dir", self.profile, success=success
        )

    def test_register_verify_and_remove_exact_host(self):
        preferences = self.profile / "Default" / "Preferences"
        preferences.parent.mkdir()
        preferences.write_bytes(b'{"preserve":"this"}')
        self.registration_command("install")
        data = json.loads(self.registration.read_text(encoding="utf-8"))
        self.assertEqual(set(data), {"name", "description", "path", "type", "allowed_origins"})
        self.assertEqual(data["name"], HOST)
        self.assertEqual(data["path"], str(self.host))
        self.assertEqual(data["type"], "stdio")
        self.assertEqual(data["allowed_origins"], [ORIGIN])
        self.registration_command("verify")

        original_inode = self.registration.stat().st_ino
        self.registration_command("install")
        self.assertEqual(self.registration.stat().st_ino, original_inode)
        unrelated = self.registration.with_name("com.example.other.json")
        unrelated.write_bytes(b"another host")
        self.host.unlink()
        self.registration_command("uninstall")
        self.registration_command("uninstall")
        self.assertFalse(self.registration.exists())
        self.assertEqual(unrelated.read_bytes(), b"another host")
        self.assertEqual(preferences.read_bytes(), b'{"preserve":"this"}')

    def test_existing_same_name_configuration_is_preserved(self):
        self.registration.parent.mkdir()
        for registration in (b'{"name":"io.emma.mediacenter.browser","path":"/usr/bin/other-host"}', b"null"):
            with self.subTest(registration=registration):
                self.registration.write_bytes(registration)
                for action in ("install", "verify", "uninstall"):
                    result = self.registration_command(action, success=False)
                    self.assertIn("Existing registration differs", result.stderr)
                self.assertEqual(self.registration.read_bytes(), registration)

    def test_concurrent_installs_publish_one_complete_manifest(self):
        command = [sys.executable, SCRIPT, "install", "--host", self.host, "--user-data-dir", self.profile]
        with (
            subprocess.Popen(command, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE) as first,
            subprocess.Popen(command, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE) as second,
        ):
            for process in (first, second):
                _, error = process.communicate(timeout=5)
                self.assertEqual(process.returncode, 0, error)
        self.assertEqual(list(self.registration.parent.iterdir()), [self.registration])
        self.registration_command("verify")

    def test_symlink_to_registration_elsewhere_is_preserved(self):
        self.registration.parent.mkdir()
        other = self.root / "other.json"
        other.write_bytes(b"some other host")
        self.registration.symlink_to(other)
        self.registration_command("install", success=False)
        self.registration_command("uninstall", success=False)
        self.assertTrue(self.registration.is_symlink())
        self.assertEqual(other.read_bytes(), b"some other host")

    def test_symlink_to_native_messaging_directory_is_rejected(self):
        other = self.root / "other"
        other.mkdir()
        self.registration.parent.symlink_to(other, target_is_directory=True)
        result = self.registration_command("install", success=False)
        self.assertIn("Not a directory", result.stderr)
        self.assertEqual(list(other.iterdir()), [])

    def test_missing_user_data_dir_and_nonexecutable_host_leave_no_manifest(self):
        self.profile.rmdir()
        result = self.registration_command("install", success=False)
        self.assertIn("Chrome user-data directory or registration is missing", result.stderr)
        self.assertFalse(self.profile.exists())
        self.profile.mkdir()
        self.host.chmod(0o600)
        result = self.registration_command("install", success=False)
        self.assertIn("Native host executable is unavailable", result.stderr)
        self.assertFalse(self.registration.parent.exists())

    def test_default_targets_chrome_user_data_directory(self):
        for override in ("HOME", "XDG_CONFIG_HOME", "CHROME_CONFIG_HOME", "CHROME_USER_DATA_DIR"):
            with self.subTest(override=override):
                env = {**os.environ, "HOME": str(self.root / "home")}
                for name in ("XDG_CONFIG_HOME", "CHROME_CONFIG_HOME", "CHROME_USER_DATA_DIR"):
                    env.pop(name, None)
                override_path = self.root / override
                if override != "HOME":
                    env[override] = str(override_path)
                data_dir = {
                    "HOME": self.root / "home" / ".config" / "google-chrome",
                    "CHROME_USER_DATA_DIR": override_path,
                }.get(override, override_path / "google-chrome")
                data_dir.mkdir(parents=True)
                self.run_cli("install", "--host", self.host, env=env)
                self.assertTrue((data_dir / "NativeMessagingHosts" / f"{HOST}.json").is_file())

    def source_fixture(self, files):
        source = self.root / f"source-{len(list(self.root.glob('source-*')))}"
        source.mkdir()
        subprocess.run(["git", "init", "-q", source], check=True)
        for name, content in files.items():
            file = source / name
            file.parent.mkdir(parents=True, exist_ok=True)
            file.write_text(content, encoding="utf-8")
        subprocess.run(["git", "-C", source, "add", "."], check=True)
        subprocess.run([
            "git", "-C", source, "-c", "user.email=test@example.test", "-c", "user.name=Test",
            "-c", "commit.gpgSign=false", "commit", "-qm", "candidate",
        ], check=True)
        revision = subprocess.check_output(["git", "-C", source, "rev-parse", "HEAD"], text=True)
        return source, revision.strip()

    def test_candidate_manifest_reads_exact_sources_and_hashes_artifacts(self):
        schema = "c" * 64
        extension = json.loads((SCRIPT.parent.parent / "browser-extension/manifest.json").read_text())
        extension["version"] = "2.5.0"
        desktop, desktop_sha = self.source_fixture({
            "src-tauri/tauri.conf.json": '{"version":"0.8.2"}',
            "src-tauri/Cargo.toml": '[package]\nversion = "0.8.3"\n',
            "browser-extension/manifest.json": json.dumps(extension),
            "protocol/remote/v1/manifest.json": json.dumps({"protocolMajor": 1, "schemaSha256": schema}),
        })
        android, android_sha = self.source_fixture({
            "app/build.gradle.kts": 'versionName = "3.1.5"\nversionCode = 7\n',
            "app/src/test/resources/mediacenter-v1.json": json.dumps({"manifest": {"protocolMajor": 1, "schemaSha256": schema}}),
        })
        (desktop / "src-tauri/tauri.conf.json").write_text('{"version":"1.0.0"}')
        deb = self.root / "candidate.deb"
        apk = self.root / "candidate.apk"
        apk.write_bytes(b"android artifact")
        args = (
            "candidate-manifest", "--desktop-root", desktop, "--desktop-revision", desktop_sha,
            "--android-root", android, "--android-revision", android_sha,
            "--deb", deb, "--apk", apk,
        )
        package = self.root / "deb-package"
        control = package / "DEBIAN" / "control"
        control.parent.mkdir(parents=True)
        control.write_text(
            "Package: mediacenter\nVersion: 0.8.2\nArchitecture: all\n"
            "Maintainer: Test <test@example.test>\nDescription: Fixture\n"
        )
        packaged_extension = package / "usr/share/mediacenter/browser-extension/manifest.json"
        packaged_extension.parent.mkdir(parents=True)
        packaged_extension.write_text(json.dumps(extension))
        (packaged_extension.parent / extension["action"]["default_popup"]).touch()
        (packaged_extension.parent / extension["background"]["service_worker"]).touch()
        packaged_helper = packaged_extension.parent.parent / "remote-release.py"
        packaged_helper.touch()
        subprocess.run(["dpkg-deb", "--build", package, deb], check=True, capture_output=True)
        result = self.run_cli(*args, success=False)
        self.assertIn("Debian package is missing its executable native host", result.stderr)
        self.assertEqual(result.stdout, "")

        packaged_host = package / "usr/bin/mediacenter-browser-host"
        packaged_host.parent.mkdir(parents=True)
        packaged_host.write_bytes(self.host.read_bytes())
        packaged_host.chmod(0o755)
        subprocess.run(["dpkg-deb", "--build", package, deb], check=True, capture_output=True)
        output = self.run_cli(*args)
        data = json.loads(output.stdout)
        self.assertEqual(data["status"], "candidate")
        self.assertEqual(data["desktop"]["revision"], desktop_sha)
        self.assertEqual(data["desktop"]["version"], "0.8.2")
        self.assertEqual(data["desktop"]["sha256"], hashlib.sha256(deb.read_bytes()).hexdigest())
        self.assertEqual(data["android"]["revision"], android_sha)
        self.assertEqual(data["android"]["version"], "3.1.5")
        self.assertEqual(data["android"]["versionCode"], 7)
        self.assertEqual(data["android"]["sha256"], hashlib.sha256(b"android artifact").hexdigest())
        self.assertEqual(data["schemaSha256"], schema)
        self.assertEqual(data["extension"], {"version": "2.5.0", "id": ORIGIN.split("/")[2]})
        self.assertEqual(data["nativeHost"]["version"], "0.8.3")

        incompatible, incompatible_sha = self.source_fixture({
            "app/build.gradle.kts": 'versionName = "3.1.5"\nversionCode = 7\n',
            "app/src/test/resources/mediacenter-v1.json": json.dumps({"manifest": {"protocolMajor": 2, "schemaSha256": schema}}),
        })
        result = self.run_cli(
            *args, "--android-root", incompatible, "--android-revision", incompatible_sha, success=False
        )
        self.assertIn("Desktop and Android protocol manifests must agree", result.stderr)
        self.assertEqual(result.stdout, "")


if __name__ == "__main__":
    unittest.main()
