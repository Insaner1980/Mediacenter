# Packaging and installation

Checkpoint: **5 November 2026**. The 0.1.0 amd64 Debian development candidate was built from `3294443`. Its checksum, archive contents, desktop entry and host library resolution are recorded in [verification](verification.md) and [artifact evidence](evidence/debian-candidate.json). Clean installation, upgrade and uninstall acceptance remain pending.

The 0.1.1 update fixes direct opening from all eight streaming tiles, including reopening a closed service. Playback controls are available through a separate button on the home screen. The package and checksum are in `releases/0.1.1/`. Close Mediacenter, verify `SHA256SUMS` in that directory, then run `sudo apt install /home/emma/Dev/Mediacenter/releases/0.1.1/Mediacenter_0.1.1_amd64.deb` in your own terminal and reopen Mediacenter. The update has not been installed during background work. See [the update verification record](evidence/direct-service-opening.md).

The intended first package is version 0.1.0 for Linux Mint 22.3 / Ubuntu 24.04 Noble, amd64, Cinnamon X11. Do not claim support for older Ubuntu/Debian bases or other architectures from a build on this machine. Tauri recommends building on the oldest supported base because newer system libraries can raise the minimum runtime requirement. The bundler generates the Debian desktop integration and accepts additional dependencies in `bundle.linux.deb`. [Tauri Debian packaging](https://v2.tauri.app/distribute/debian/)

## Prerequisites and dependency policy

Build with the repository's Node/Rust versions and lockfiles. The native development packages are listed in the [README](../../README.md#development). End users need runtime packages, not the development toolchain or a running Vite server.

| Runtime requirement | Purpose and packaging treatment |
| --- | --- |
| `libwebkit2gtk-4.1-0`, GTK 3 | Tauri desktop rendering; verify the generated package dependency fields and linked release binary |
| `mpv` | Owned local video/music player |
| `ffmpeg` | `ffprobe` metadata plus thumbnail/photo decoding |
| `pulseaudio-utils` | `pactl` against PulseAudio or PipeWire's PulseAudio compatibility service |
| `wmctrl`, `x11-xserver-utils` | X11 placement and `xrandr` discovery |
| `udisks2` | Drive discovery and OS mount/removal requests over the system bus |
| Desktop session services | A running graphical user session, D-Bus and desktop Polkit authentication; a library package alone does not provide a usable session |
| Google Chrome, NordVPN | Separately installed and managed prerequisites; never bundled, silently installed or configured by package scripts |

On the inspected Noble system, `libgtk-3-0t64` provides `libgtk-3-0`, so the existing generic GTK dependency can be satisfied. Verify resolution in the clean target account/VM. The current app uses fixed `/usr/bin` executable paths; a shell alias or a custom PATH installation is not equivalent. Installed versions observed during preflight are recorded in [preflight](preflight.md); they are observations, not a tested minimum-version matrix.

Review `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml` and `package.json` together before release. Keep the version and identifier consistent. Ensure the icon configuration points to an existing PNG and that the generated `.desktop` file refers to the packaged executable and icon. No package maintainer script should remove user state or start playback, change outputs, change VPN settings, or mount a drive.

## Build and inspect without installing

From the repository root, after the final tests pass:

```sh
source scripts/dev-env.sh
npm ci
npm run tauri -- build --bundles deb -- --locked
```

Skip the first line when using the normal installed toolchain. The command builds release mode and runs the configured frontend build. Do not pass the development identifier override. The default output directory is `src-tauri/target/release/bundle/deb/`; a custom target directory or target triple changes this location. Record the exact produced filename rather than predicting it.

Set `release_deb` to that actual filename, then inspect it without running the application:

```sh
release_deb='/absolute/path/to/the-produced-package.deb'
dpkg-deb --field "$release_deb" Package Version Architecture Depends Maintainer
dpkg-deb --contents "$release_deb"
```

Extract to a new temporary directory for content inspection:

```sh
release_inspect_dir=$(mktemp -d)
dpkg-deb --raw-extract "$release_deb" "$release_inspect_dir"
```

Inspect the extracted `DEBIAN` metadata and any maintainer scripts, `/usr/share/applications/` entry, icon files and executable. For this project's locally built executable, inspect its linked libraries with `ldd` and require no `not found` entries. Confirm the package contains no browser profiles, source media, development database, credentials or `.tools` files. Review archive sizes and retain these observations with the release record. Host `ldd` success does not prove dependency installation on a clean target.

Create a checksum next to the specific artifact:

```sh
cd "$(dirname "$release_deb")"
sha256sum "$(basename "$release_deb")" > SHA256SUMS
sha256sum --check SHA256SUMS
```

Keep the checksum and package together. A checksum detects a changed artifact; it is not a publisher signature. Record the source revision and any uncommitted changes used to produce the artifact. A test result from before the final changes must not be attached to a later build as though the code were identical.

## Install, update and remove

These commands are instructions for the later controlled installation check; they have not been run as part of document preparation. Use a clean representative Mint/Noble VM or test account first, with no unrelated playback in progress.

Verify the checksum, then install the named local file:

```sh
sudo apt install /absolute/path/to/the-produced-package.deb
```

Open **Mediacenter** from the application menu. Confirm the normal computer window, local fonts/logos in both themes, source selection, restart persistence and useful missing-dependency errors. Do not start HDMI, VPN-change or device-removal tests until that specific test session is authorized. Installation success and process creation are separate from a usable, visually reviewed application.

For an update, close Mediacenter and its managed service windows, make the backup below, and install the new local `.deb` with `apt install`. Preserve the production application identifier. Test the updated settings, sources, playlists, resume records and service-profile login state; this preservation is a release requirement, not an upgrade result already demonstrated.

To remove the package, first inspect the `Package` field above, then run `sudo apt remove` with that exact package name. Verify the program/desktop entry was removed and user data and original media remain. Do not add a data-directory deletion command to normal uninstall instructions. Avoid `autoremove` during this preservation test because its scope includes other packages.

## Data preservation and recovery

For the release identifier, Tauri's usual Linux data directory is `${XDG_DATA_HOME:-$HOME/.local/share}/io.emma.mediacenter`. The normal cache directory is `${XDG_CACHE_HOME:-$HOME/.cache}/io.emma.mediacenter`. Verify the actual location if XDG paths have been customized. The development override uses the `.dev` identifier and separate directories.

| Location | Contents and treatment |
| --- | --- |
| Data directory `library.db` | Settings, media/source metadata, local resume information and playlists; preserve across updates |
| `library.db-wal`, `library.db-shm`, if present | SQLite WAL companions; do not copy a live database without a consistent backup or combine these with another database version |
| `library.pre-migration.db` | Current implementation's consistent backup created before a schema migration; not a continuous backup history |
| Data directory `browser-profiles/<service>` | Managed Chrome profile, including account/session data; preserve locally and never publish |
| Cache directory `thumbnails` | Generated previews; not the original media or the authoritative library database |
| User-selected media folders | Outside application storage; never moved or deleted by package maintenance |

For a full backup, first close Mediacenter and all managed Chrome windows, including a service window deliberately left open on app exit. Copy the entire data directory into a new private backup directory. Closing only the control window is insufficient if Chrome is still writing its profile. Keep the backup local and access-restricted; it contains browser account state. A backup made while the application is live requires SQLite's consistent backup mechanism and separate handling of live Chrome profiles.

The current database code enables foreign keys and WAL mode, backs up before migration using SQLite's backup API, and applies migration SQL in a transaction. It rejects a schema newer than it understands. The existing regression test proves rollback and backup preservation for an intentionally failed migration; it does not prove every future migration or package downgrade.

If startup reports a migration/database error, retain the original directory and error details. Do not delete the database to make the error disappear. With all application/profile writers closed, make a separate copy of the failed state. Restore a complete known-good data-directory backup compatible with the chosen application version, keeping the failed directory aside for diagnosis. If restoring only `library.pre-migration.db`, move the current database and its WAL/SHM companions aside together before placing the selected backup at `library.db`; never reuse stale WAL files with the restored database. Do not downgrade against a newer schema without a compatible backup.

## Release gate

A build/checksum is a candidate artifact. Release acceptance still needs a clean installation, packaged native UI review, update/uninstall preservation, managed protected-service playback, real local playback, HDMI picture/audio/scaling and recovery, and disposable-drive mount/eject/reconnect checks. See the [verification matrix](verification.md). The user's current TV session must remain undisturbed; real-output tests are deferred.
