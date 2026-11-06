# Automatic media folders — 6 November 2026

Application commit: **3062630**. [Gallery](index.html), [verification record](verification.json), [design](../../../superpowers/specs/2026-11-06-automatic-media-folders-design.md).

## Behavior

- Photos selects the configured Pictures directory, Videos the Videos directory, and Music the Music directory. Files in subfolders appear in the existing gallery/list. Native Tauri directory APIs honor configured XDG locations. Indexing starts automatically and Refresh rescans the selected source.
- Open folder uses one native picker choice, adds the current media kind, selects the source and starts its scan. Mounted external drives/USB work through the same picker. Previously added sources remain in the dropdown. Advanced management stays behind the adjacent icon.
- Scans survive navigation, expose status/cancel, and queue a coalesced follow-up if another view adds a media kind during a running scan. Music folder actions appear only in Library, where results/errors are visible.
- Missing/disabled defaults do not grant the entire home directory or root. Defaults cannot create a fresh confirmation for a UUID-less USB attachment. Such volumes still require explicit selection through Open folder.
- Existing ancestor source identity is reused. Conflicting pre-existing descendant registrations retain the existing overlap error instead of creating duplicate indexes or resetting data. This uncommon migration case requires resolving the overlap in folder management.

The user delegated presentation choices. The chosen approach preserves the gallery and track list rather than adding a filesystem-tree navigation mode. External access uses Open folder; Drives & USB keeps Mount/Safely remove without automatic device mutation.

## Verification

| Check | Observed outcome | Evidence |
| --- | --- | --- |
| TypeScript / ESLint | Pass | [types](logs/typecheck.txt), [lint](logs/lint.txt) |
| Unit tests | 146 passed in25 files | [log](logs/unit.txt) |
| E2E workflows | 13 passed, including direct folder selection and canceling its scan | [log](logs/e2e.txt) |
| Visual checks | 95 passed, including12 new folder-action captures at800/1717px in both themes | [log](logs/visual.txt) |
| Assets / copy | Pass | [assets](logs/assets.txt), [copy](logs/copy.txt) |
| Frontend build | Pass | [log](logs/build.txt) |
| Rust tests | 130 passed;2 existing hardware tests ignored, not counted as passed | [log](logs/rust-tests.txt) |
| Clippy / format | Pass | [clippy](logs/rust-lint.txt), [format](logs/rust-format.txt) |
| Contract export | Regenerated; `src/app/contracts.ts` unchanged | [log](logs/contracts.txt) |
| Native release build | Pass, no bundle or installation | [log](logs/native-build.txt) |
| Native default-folder smoke | **Pass:** one generated PNG, MP4 and WAV indexed and displayed automatically | [record](native-smoke.json), [log](logs/native-run.txt), native gallery captures |

Full Rust tests required execution outside the restricted command sandbox because local socket and player-process tests were blocked there. The passing run used the normal test environment. The two explicitly ignored hardware scenarios remain pending.

The full visual suite writes its historical simplified-UI capture paths; those old evidence files were restored to their prior committed contents. New captures live here so the previous acceptance record remains tied to its original application version.

## Defects caught before completion

Independent read-only review found three defects: automatic default registration could renew UUID-less attachment confirmation; a second media-kind registration could lose its scan while an older scan was pending; and Music folder actions on Queue/Playlists hid their outcome. Regression tests were added, each failure observed, fixes applied, and the complete suite passed. No findings were deferred.

The first native smoke failed because the new command lacked its Tauri application-manifest/capability entry. A failing permission-boundary test was added. The command is now generated into the app manifest and allowed **only** for the main window; presentation permissions remain unchanged. The successful native run exercises that actual production IPC path.

## Native scope and remaining limits

The smoke runs on a private authenticated Xvfb display, private D-Bus session and private XDG folders. It generates one image, one one-second video and one silent WAV. It only navigates the library and reads its disposable database to verify automatic indexing. Actual PNG/video thumbnails rendered in WebKitGTK. It does not access user media or change the user's VPN, outputs, disks, audio defaults, application database or installed binary. Software compositing is confined to the test process.

Actual external-drive mounting/removal, audible playback, HDMI, protected streaming and physical-display behavior remain controlled hardware checks. The release executable is updated at `src-tauri/target/release/mediacenter`; the system application-menu installation is still the previous version.
