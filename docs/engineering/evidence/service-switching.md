# Direct service switching — 0.1.3

Date: 5 November 2026. Base revision: `36da678` plus the changes committed with this record.

Reproduction: open/sign in to YouTube, return to home, click Viaplay. Both the frontend and backend enforced an explicit-close rule. The frontend navigated to the active YouTube controls, with its explanatory message below a long settings screen. Previous tests intentionally retained that rule; they did not satisfy the user's expected direct switching behavior. The correction removes the interstitial and changes the shared native transition for every service.

The new transition focuses a matching current service. For a different one it preflights the requested destination (including configured VPN policy and profile availability), checks cancellation, closes only the verified owned current window, removes the retired session/output claims, and launches the requested destination. Failed preflight keeps the previous browser; failed ownership verification/close prevents replacement. Profiles are preserved. Ordinary Chrome remains outside ownership, including HBO's handoff.

Verification:

- Red UI regression: 57 failures, covering all 56 ordered service pairs plus a failed-switch case. Each reproduced the old-screen diversion or suppressed requested launch.
- Final `npm test`: 107 passed, including all switching pairs, same-service activation, service errors and explicit controls navigation.
- Frontend production build and ESLint passed.
- Full Rust suite (`cargo test --locked --manifest-path src-tauri/Cargo.toml -- --test-threads=1`): 125 passed, 2 ignored. Shared transition tests cover all ordered pairs, same-service focus, failed preflight, cancellation and uncertain ownership.
- The ignored native switching test was then explicitly run on a separate authenticated Xvfb display with its own window manager, D-Bus, temporary profile directories and null/unavailable audio configuration. It passed in 14.12 seconds. It opened actual Chrome windows in the sequence YouTube → Viaplay → Netflix → MTV Katsomo → Ruutu → Yle Areena → Plex → YouTube, checked the old owned process had exited, checked each new window's ownership and service ID, preserved a profile marker when returning to YouTube, and confirmed a separate Chrome process survived throughout. Same-service focus retained the same PID. See [native test output](service-switch-native.txt).
- Clippy (`--all-targets -- -D warnings`), Rust formatting and diff whitespace checks passed.
- Package build and metadata/archive/library/checksum inspection are recorded in `service-switching-package.json`.

The test used fresh isolated profiles; it did not enter credentials or prove login-cookie retention or authenticated provider playback. The user separately confirmed HBO and YouTube login on 0.1.2. Viaplay login/playback and other remaining service acceptance checks are still unverified. No user browser or TV/audio settings were changed by the native test. The 0.1.3 package requires installation and application restart.
