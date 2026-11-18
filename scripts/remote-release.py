#!/usr/bin/env python3
"""Register the packaged Chrome host or generate a Remote candidate manifest."""

import argparse
import base64
from contextlib import ExitStack
import fcntl
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import stat
import subprocess
import tarfile
from uuid import uuid4


ROOT = Path(__file__).resolve().parent.parent
HOST_NAME = "io.emma.mediacenter.browser"
HOST_DESCRIPTION = "Mediacenter browser control"
PACKAGED_HOST = Path("/usr/bin/mediacenter-browser-host")
PACKAGED_SUPPORT = PurePosixPath("usr/share/mediacenter")
EXTENSION_ID = "njhbcllpkbihpkioegopblabopnkhndk"
REGISTRATION_NAME = f"{HOST_NAME}.json"
HOST_DIRECTORY = "NativeMessagingHosts"
MISSING = object()


def default_user_data_directory():
    override = os.getenv("CHROME_USER_DATA_DIR")
    if override:
        return Path(override)
    config = os.getenv("CHROME_CONFIG_HOME") or os.getenv("XDG_CONFIG_HOME")
    config_directory = Path(config) if config else Path.home() / ".config"
    return config_directory / "google-chrome"


def native_manifest(host):
    return {
        "name": HOST_NAME,
        "description": HOST_DESCRIPTION,
        "path": str(host),
        "type": "stdio",
        "allowed_origins": [f"chrome-extension://{EXTENSION_ID}/"],
    }


def read_registration(directory):
    try:
        fd = os.open(
            REGISTRATION_NAME, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=directory
        )
    except FileNotFoundError:
        return MISSING
    with os.fdopen(fd, "rb") as manifest:
        metadata = os.fstat(manifest.fileno())
        if not stat.S_ISREG(metadata.st_mode) or metadata.st_size > 8192:
            raise ValueError("Existing native host registration must be a small regular JSON file.")
        return json.loads(manifest.read(8193))


def create_registration(directory, manifest):
    temporary = f".{HOST_NAME}.{uuid4().hex}.tmp"
    fd = os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600, dir_fd=directory)
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as file:
            file.write(json.dumps(manifest, indent=2) + "\n")
            file.flush()
            os.fsync(file.fileno())
        os.link(temporary, REGISTRATION_NAME, src_dir_fd=directory, dst_dir_fd=directory)
    finally:
        os.unlink(temporary, dir_fd=directory)


def register(args):
    user_data = args.user_data_dir or default_user_data_directory()
    if not user_data.is_absolute() or not args.host.is_absolute():
        raise ValueError("The Chrome user-data directory and host path must be absolute.")
    if args.action != "uninstall" and (not args.host.is_file() or not os.access(args.host, os.X_OK)):
        raise ValueError(f"Native host executable is unavailable: {args.host}")

    manifest = native_manifest(args.host)
    destination = user_data / HOST_DIRECTORY / REGISTRATION_NAME
    flags = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW
    with ExitStack() as stack:
        try:
            profile = os.open(user_data, flags)
            stack.callback(os.close, profile)
            if args.action == "install":
                try:
                    os.mkdir(HOST_DIRECTORY, mode=0o700, dir_fd=profile)
                except FileExistsError:
                    pass
            directory = os.open(HOST_DIRECTORY, flags, dir_fd=profile)
            stack.callback(os.close, directory)
        except FileNotFoundError:
            if args.action != "uninstall":
                raise ValueError(f"Chrome user-data directory or registration is missing: {destination}")
            print(f"Registration absent: {destination}")
            return

        lock = fcntl.LOCK_SH if args.action == "verify" else fcntl.LOCK_EX
        fcntl.flock(directory, lock)
        existing = read_registration(directory)
        if existing is not MISSING and existing != manifest:
            raise ValueError(f"Existing registration differs: {destination}")

        if args.action == "install" and existing is MISSING:
            create_registration(directory, manifest)
            os.fsync(directory)
            os.fsync(profile)
        elif args.action == "verify" and existing is MISSING:
            raise ValueError(f"Registration absent: {destination}")
        elif args.action == "uninstall" and existing is not MISSING:
            os.unlink(REGISTRATION_NAME, dir_fd=directory)
            os.fsync(directory)

    state = "Registration absent" if args.action == "uninstall" else "Registration verified"
    print(f"{state}: {destination}")


def source_file(root, revision, name):
    return subprocess.check_output(["git", "-C", root, "show", f"{revision}:{name}"], text=True)


def check_revision(root, revision):
    if not re.fullmatch(r"[a-f0-9]{40}|[a-f0-9]{64}", revision):
        raise ValueError("Provide the complete Git commit SHA for each tested repository.")
    resolved = subprocess.check_output(
        ["git", "-C", root, "rev-parse", "--verify", f"{revision}^{{commit}}"], text=True
    ).strip()
    if resolved != revision:
        raise ValueError(f"Revision is not a Git commit: {revision}")


def extension_id(public_key):
    digest = hashlib.sha256(base64.b64decode(public_key, validate=True)).hexdigest()[:32]
    return digest.translate(str.maketrans("0123456789abcdef", "abcdefghijklmnop"))


def artifact_data(path):
    with path.open("rb") as artifact:
        return {"filename": path.name, "sha256": hashlib.file_digest(artifact, "sha256").hexdigest()}


def verify_debian_package(path, version, extension):
    path = path.resolve()
    packaged_version = subprocess.check_output(["dpkg-deb", "-f", path, "Version"], text=True)
    if packaged_version.strip() != version:
        raise ValueError("Debian package version differs from the desktop source version.")

    extension_directory = PACKAGED_SUPPORT / "browser-extension"
    manifest = extension_directory / "manifest.json"
    required = {
        PACKAGED_SUPPORT / "remote-release.py",
        extension_directory / extension["action"]["default_popup"],
        extension_directory / extension["background"]["service_worker"],
        extension_directory / "launch.html",
        extension_directory / "launch.mjs",
    }
    packaged_host = PurePosixPath(str(PACKAGED_HOST).lstrip("/"))
    with subprocess.Popen(["dpkg-deb", "--fsys-tarfile", path], stdout=subprocess.PIPE) as package:
        files = set()
        has_host = False
        packaged_extension = None
        with tarfile.open(fileobj=package.stdout, mode="r|") as archive:
            for entry in archive:
                name = PurePosixPath(entry.name)
                if not entry.isfile():
                    continue
                files.add(name)
                if name == packaged_host:
                    has_host = bool(entry.mode & 0o111)
                elif name == manifest and entry.size <= 65536:
                    with archive.extractfile(entry) as data:
                        packaged_extension = data.read()
        # tarfile stops at the end-of-archive marker. Let dpkg-deb finish writing padding.
        while package.stdout.read(65536):
            pass
        package.stdout.close()
        if package.wait():
            raise ValueError("Could not inspect the Debian package contents.")

    if not has_host:
        raise ValueError("Debian package is missing its executable native host.")
    if not required.issubset(files):
        raise ValueError("Debian package is missing its browser extension files or registration helper.")
    if packaged_extension is None or json.loads(packaged_extension) != extension:
        raise ValueError("Debian package extension manifest differs from the desktop source manifest.")


def candidate_manifest(args):
    import tomllib

    desktop_root, desktop_revision = args.desktop_root, args.desktop_revision
    android_root, android_revision = args.android_root, args.android_revision
    check_revision(desktop_root, desktop_revision)
    check_revision(android_root, android_revision)

    desktop = json.loads(source_file(desktop_root, desktop_revision, "src-tauri/tauri.conf.json"))
    host = tomllib.loads(source_file(desktop_root, desktop_revision, "src-tauri/Cargo.toml"))
    extension = json.loads(source_file(desktop_root, desktop_revision, "browser-extension/manifest.json"))
    contract = json.loads(source_file(desktop_root, desktop_revision, "protocol/remote/v1/manifest.json"))
    gradle = source_file(android_root, android_revision, "app/build.gradle.kts")
    android_contract = json.loads(
        source_file(android_root, android_revision, "app/src/test/resources/mediacenter-v1.json")
    )["manifest"]

    id_from_key = extension_id(extension["key"])
    if id_from_key != EXTENSION_ID:
        raise ValueError("The source extension key does not match the registered extension ID.")
    if contract != android_contract or not re.fullmatch(r"[a-f0-9]{64}", contract["schemaSha256"]):
        raise ValueError("Desktop and Android protocol manifests must agree.")
    version_name = re.search(r'^\s*versionName\s*=\s*"([^"]+)"\s*$', gradle, re.MULTILINE)
    version_code = re.search(r"^\s*versionCode\s*=\s*(\d+)\s*$", gradle, re.MULTILINE)
    if version_name is None or version_code is None:
        raise ValueError("The Android source must declare its version name and code.")
    verify_debian_package(args.deb, desktop["version"], extension)

    print(json.dumps({
        "status": "candidate",
        "desktop": {
            "revision": desktop_revision,
            "version": desktop["version"],
            **artifact_data(args.deb),
        },
        "android": {
            "revision": android_revision,
            "version": version_name[1],
            "versionCode": int(version_code[1]),
            **artifact_data(args.apk),
        },
        "protocolMajor": contract["protocolMajor"],
        "schemaSha256": contract["schemaSha256"],
        "extension": {"version": extension["version"], "id": id_from_key},
        "nativeHost": {"name": HOST_NAME, "version": host["package"]["version"], "path": str(PACKAGED_HOST)},
    }, indent=2))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="action", required=True)
    for action in ("install", "verify", "uninstall"):
        command = commands.add_parser(action, help=f"{action.title()} one Chrome native host registration")
        command.add_argument("--user-data-dir", type=Path, help="Chrome user-data root (defaults to ordinary Google Chrome)")
        command.add_argument("--host", type=Path, default=PACKAGED_HOST, help=f"native host path (default: {PACKAGED_HOST})")
        command.set_defaults(run=register)

    command = commands.add_parser("candidate-manifest", help="Print compatibility and checksums for two release artifacts")
    command.add_argument("--desktop-root", type=Path, default=ROOT)
    command.add_argument("--desktop-revision", required=True)
    command.add_argument("--android-root", type=Path, default=ROOT.parent / "Remote")
    command.add_argument("--android-revision", required=True)
    command.add_argument("--deb", type=Path, required=True)
    command.add_argument("--apk", type=Path, required=True)
    command.set_defaults(run=candidate_manifest)

    args = parser.parse_args()
    try:
        args.run(args)
    except (KeyError, OSError, ValueError, subprocess.CalledProcessError, tarfile.TarError) as error:
        parser.exit(1, f"{parser.prog}: {error}\n")


if __name__ == "__main__":
    main()
