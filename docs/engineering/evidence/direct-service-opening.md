# Direct service opening — 0.1.1

Date: 5 November 2026. Base revision: `1b48c08` plus the 0.1.1 changes recorded with this file.

The home action previously navigated to the service controls after launch. If a previous streaming session existed, it called `focusSession` even when the owned browser had exited. The backend already supports reopening an exited browser through `openService`, but the tile bypassed that path.

All eight tiles now call `openService` for a new or same-service session and keep the home screen behind the browser. The backend retains its launch/focus decision, VPN checks, process ownership checks and private-profile lock checks. A different live service still requires explicit closing. The controls screen is available via a separate home button, and its Open service window button also uses the reopening operation. Launch errors appear beside the tiles inside the app shell.

## Verification

- Regression tests first failed in five expected cases: automatic interstitial navigation, existing ready and unavailable sessions bypassing reopen, missing explicit controls button, and controls recovery.
- `npm test`: 50 passed, including each of eight service icons with no previous session, a ready session and an unavailable session. These tests exercise the real React navigation with the native backend boundary mocked; they do not prove provider login or video playback.
- `npm run typecheck`, `npm run lint`, `npm run build`, `npm run verify:ui-copy`: passed. An unsupported test query option was caught by TypeScript and removed before the final successful checks and build.
- `npm run test:visual`: 10 passed. `npm run test:e2e`: 1 passed.
- Initial parallel `cargo test --locked --manifest-path src-tauri/Cargo.toml` had one failure: `process::runner::tests::command_timeout_reaps_owned_child`. The test could not read the Python child's PID file after its 200 ms deadline while other checks were running. No native source was changed. The complete suite rerun with `-- --test-threads=1` passed: 119 passed, 1 ignored hardware probe. The timing-sensitive test remains a test-suite limitation.

Package details and checksum are in `direct-service-opening-package.json`. The package update must be installed and Mediacenter restarted before the user's installed application gains this behavior. No visible service window, TV output change, VPN mutation, or browser-profile reset was performed for this update. Provider login and protected playback remain separate manual acceptance checks.
