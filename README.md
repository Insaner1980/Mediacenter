# Mediacenter

Personal Linux desktop media center, being implemented from the [approved plan](docs/superpowers/plans/2026-11-05-mediacenter-linux-implementation-plan.md). This repository is work in progress, not a completed release. See [preflight](docs/engineering/preflight.md) for measured capabilities and unverified requirements.

## Development

Target: Linux Mint 22.3 / Ubuntu Noble, Cinnamon X11. Chrome and NordVPN remain externally installed applications. Their presence is not proof of streaming compatibility.

Install native build prerequisites using your OS package manager:

```sh
sudo apt update
sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev patchelf
```

Use Node from `.node-version` and Rust from `rust-toolchain.toml`. The initial workspace has local tools under ignored `.tools/`. When using those tools, run commands from the repository root:

```sh
source scripts/dev-env.sh
npm ci
npm run tauri -- dev --config src-tauri/tauri.dev.conf.json
```

On another machine with normally installed Node/Rust, skip `source scripts/dev-env.sh`. Do not copy browser profiles, runtime databases or user media into the repository.

## Checks

```sh
npm run build
npm run typecheck
npm run lint
npm test
npm run verify:assets
npm run verify:ui-copy
npm run test:visual
npm run test:e2e
npm run rust:check
npm run rust:test
npm run rust:lint
npm run rust:format
```

`npm run dev` runs only the UI development server. The real desktop app is started with Tauri. Browser fixture checks do not prove native WebKitGTK behavior, DRM, HDMI sound, or removable-drive recovery.

## Data and safety boundaries

Read media in place. Only explicitly selected folders may be indexed. Streaming uses dedicated Chrome profiles and the service's own playback controls. No universal streaming transport controls, DRM workarounds, cloud sync, telemetry, remote server, Color Dock integration or Android Remote modifications are planned for this release.

The application must never disconnect NordVPN on exit, change the global audio default as a fallback, force-unmount a drive, or stop an unrelated Chrome/mpv process. Missing or ambiguous integration capabilities must remain visible as unavailable or partial states.
