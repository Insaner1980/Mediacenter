# HBO ordinary Chrome handoff — 0.1.2

Date: 5 November 2026. Base revision: `b6768b4` plus the changes committed with this record.

The user confirmed that the HBO login form works in their ordinary Chrome profile, including a fresh screenshot, while Mediacenter's dedicated profile still failed. After the manual display/audio tradeoff was explained, the user delegated the implementation choice. HBO now opens its fixed catalog URL in ordinary Chrome. The other seven services retain their managed windows.

The existing service operation still checks the configured VPN policy and managed-playback conflicts first. It then launches `/usr/bin/google-chrome` with only `https://www.hbomax.com/`. It does not pass an app mode, private profile, debugging flag, or security override; copy/reset profile data; or register ordinary Chrome as owned playback. Display/audio movement, focus/close operations and app-exit cleanup cannot act on this external browser. A one-second check catches immediate launcher failure; an ongoing browser is left running with a background waiter, without kill-on-drop or cancellation killing. Success means the launch request was accepted, not that HBO authenticated or played video.

The home notice says to continue in Chrome and select HBO's display/sound in desktop settings. Failed launch retains the error rather than the success notice. Stale exited managed-session entries are removed before opening another service, preventing an external handoff from leaving obsolete controls behind.

Validation:

- UI regression first failed because the ordinary-Chrome explanation was missing; after implementation, `npm test` passed all 51 tests.
- Production frontend build and ESLint passed.
- Four native tests passed for exact Chrome handoff arguments, immediate success/failure, cancellation before spawn, and an ongoing process surviving handoff plus later cancellation.
- Full native suite with `--test-threads=1`: 123 passed, 1 ignored hardware probe; no failures.
- Clippy with `--all-targets -- -D warnings` and Rust formatting passed.
- Debian build and archive/library checks: see `hbo-ordinary-chrome-package.json`.

No live user browser or television was moved, closed or opened by these automated tests. The user installs the update and verifies actual HBO sign-in/playback. This release selects the working browser route; it does not claim that the unresolved dedicated-profile authentication bug was fixed.
