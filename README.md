# Mediacenter

A personal Linux desktop media center for opening streaming services through the computer's VPN and browsing selected local videos, music and photos. The control window stays on the computer; a television is an optional playback output.

**Release status: 0.1.3 development candidate built; hardware and clean-install acceptance are incomplete.** A successful build or automated test run does not establish working protected streaming, HDMI sound, native scaling, USB removal or clean installation. See [verification](docs/engineering/verification.md), [preflight findings](docs/engineering/preflight.md) and the [service compatibility matrix](docs/engineering/streaming-compatibility.md).

The target is Linux Mint 22.3 / Ubuntu 24.04 Noble, x86-64, with Cinnamon X11. Wayland has reduced placement capabilities. Chrome and NordVPN are separately installed applications; Mediacenter does not install them or provide service subscriptions.

## Using the application

HBO Max opens in your ordinary Chrome profile; choose its display and sound through the desktop settings. Other streaming services use Mediacenter’s managed windows. Clicking a different service closes the previous managed window after preflight and opens the selected service; browser profiles are preserved.

Once a package has passed installation checks, open **Mediacenter** from the desktop application menu. Package preparation, installation, update and recovery instructions are in [packaging](docs/engineering/packaging.md). The locally built candidate and checksum are under `releases/0.1.3/`; this is not a fully accepted release.

- Open a streaming tile to use that service in its dedicated Chrome profile. Sign in and choose content in the service's own interface. VPN-required launches need a fresh connected observation and verified Kill Switch state. Service availability can still depend on the account, programme, region and provider's VPN policy.
- In **Videos**, **Music**, **Photos** or **Drives & USB**, choose a folder with the native picker, add it to the library and select **Scan folder**. Only selected folders are indexed. Media stays in place.
- Local video/music controls operate the owned mpv player. Adjacent SRT, ASS/SSA and WebVTT subtitles are loaded through source-bounded descriptors; external links are rejected. Music queues and playlists retain unavailable entries. Photos have a separate presentation window and controls on the computer.
- In **VPN & Sound**, select the desired output. Saving a target does not itself move existing playback. Use the explicit move/return controls and check the reported result. A missing HDMI audio sink is not working TV sound.
- In **Drives & USB**, mount a supported removable volume if needed. **Safely remove** first shows the affected physical device, sibling drives and active use; **Stop using and safely remove** requests shutdown of owned work before OS removal. Disconnect only after a confirmed success. A busy, denied or timed-out operation is not safe-removal confirmation.

The first preflight found an available HDMI port but no HDMI audio sink, and a coordinate-scaling mismatch. Those findings require native retesting after implementation; they are not resolved by unit tests. Only ordinary public YouTube playback in the existing browser profile has been observed. Managed-profile DRM, login persistence and the other services remain unverified.

## Development

Use the versions in `.node-version` and `rust-toolchain.toml`. Install the [official Tauri Linux prerequisites](https://v2.tauri.app/start/prerequisites/) through your OS package manager. On this target, the initial prerequisite command was:

```sh
sudo apt update
sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev patchelf
```

The workspace may contain ignored local tools under `.tools/`. From the repository root:

```sh
source scripts/dev-env.sh
npm ci
npm run tauri -- dev --config src-tauri/tauri.dev.conf.json
```

Skip `source scripts/dev-env.sh` when using normally installed Node and Rust. The development identifier `io.emma.mediacenter.dev` keeps its data separate from the release identifier `io.emma.mediacenter`. `npm run dev` starts the browser UI server only. Tauri starts the desktop application and native integrations; do not launch it as an unattended check during someone else's playback.

## Checks

```sh
npm run typecheck
npm run lint
npm test
npm run build
npm run verify:assets
npm run verify:ui-copy
npm run test:visual
npm run test:e2e
npm run rust:check
npm run rust:test
npm run rust:lint
npm run rust:format
```

The visual/E2E suites use browser fixtures; device mutations use fake backends. A separately ignored UDisks test performs read-only host discovery. Record exact versions, commands and evidence in [verification](docs/engineering/verification.md); do not convert a development checkpoint into packaged-release acceptance.

## Data and boundaries

Settings, source metadata, playlists and local resume information live in the application's SQLite database. Each streaming service has a separate browser profile. [Packaging](docs/engineering/packaging.md#data-preservation-and-recovery) documents the data locations and backup procedure. Do not put runtime databases, account cookies, browser profiles or private media in the repository.

Removing a source removes its library association without deleting original files. Unplugged sources retain metadata. UUID-less fingerprints are conservative reattachment hints and require explicit confirmation; they are not proof that a filesystem has remained unchanged.

The application must not disconnect NordVPN on exit, force-unmount a device, change the global audio default as a fallback, or control unrelated Chrome/mpv processes. It has no cloud account, telemetry, remote server, universal streaming transport, DRM workaround, Color Dock integration or Android Remote modification.

The [implementation plan](docs/superpowers/plans/2026-11-05-mediacenter-linux-implementation-plan.md) and [approved design brief](docs/design/2026-11-05-claude-visual-design-brief.md) define the remaining acceptance requirements.

Implementation choices and remaining minor details are recorded in [implementation decisions](docs/engineering/implementation-decisions.md).
