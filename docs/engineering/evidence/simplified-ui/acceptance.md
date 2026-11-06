# Simplified UI implementation acceptance

Date: **6 November 2026**. Application source: **44e091902b22199456e68a3ab298aba2a76e6e50** on `codex/mediacenter-linux`. The following evidence commit changes documentation and a screenshot wait condition only. [Gallery](index.html), [visual review and exceptions](visual-review.md), [execution decisions](execution-record.md), [machine-readable verification](verification.json).

Implemented all six views, persistent playback/slideshow, library/source management, playlist editing with guarded drafts, output/VPN settings, truthful drive removal and shared accessible sheets/dialogs. Production uses existing native services and persistent data. Fixture dependencies are DEV-only. No Rust, generated contract, dependency/lockfile, capability or persistence-schema changes were made.

## Actual final checks

Commands used the existing tools via `source scripts/dev-env.sh`; all listed outcomes are observed, not planned.

| Command | Result | Evidence |
| --- | --- | --- |
| `npm run typecheck` | Pass | [log](logs/typecheck.txt) |
| `npm run lint` | Pass | [log](logs/lint.txt) |
| `npm test` | **139 passed**, 24 files | [log](logs/unit.txt) |
| `npm run verify:assets` | 8 official logos, 18 local fonts verified | [log](logs/assets.txt) |
| `npm run verify:ui-copy` | Pass; no forbidden dot decoration | [log](logs/copy.txt) |
| `npm run build` | Pass | [log](logs/build.txt) |
| `npm run test:visual` | **83 passed**: six reference sizes, 48 responsive/theme cases, 18 exceptional-state cases, six text-zoom cases and existing asset/focus/release checks | [log](logs/visual.txt) |
| `npm run test:e2e` | **12 passed** | [log](logs/e2e.txt) |
| `npm run test:visual -- --grep '200 percent text'` | **6 passed** after refining capture waits for visible lazy-loaded images | [log](logs/text-zoom.txt) |
| `npm run rust:check` | Pass | [log](logs/rust.txt) |
| `npm run tauri -- build --no-bundle -- --locked` | Native release executable built | [log](logs/native-build.txt) |
| Private native smoke | Both themes, Home/Videos, real native dialog, Escape focus return, 1280×800 and800×600 resize observed | [record](native-smoke.json), [log](logs/native-run.txt), gallery |
| Approved design hashes | All six PNGs unchanged | [hashes](approved-sources.sha256) |
| Final scope review | No Rust/contracts/dependencies/capability changes; fixture markers absent from built JS; unrelated streaming notes excluded | Git diff and release-exclusion visual tests |

The last screenshot-test refinement initially expected all six offscreen images to decode at800×600. It correctly failed against lazy loading (only two/four nearby thumbnails rendered). The test now asserts all six cards and waits for the visible first image; all six follow-up tests pass. Production code was unchanged by this refinement.

## Review findings resolved

One independent whole-branch reviewer found four important defects. Each was reproduced with a failing regression, fixed, and followed by the green139-test suite:

- Closing a parent editor and its confirmation together could leave the application inert. Dialog inert ownership now uses reference counts and restores the root opener; E2E confirms navigation after Save.
- Sources did not observe native `sources_changed`; disconnects could leave stale availability. A typed production observer invalidates queries without losing the selected source. Tests cover an event during a pending query and a delayed initial source snapshot.
- A failed new filter could leave the previous results actionable. Stale items and paging stay disabled until the current query succeeds.
- Playlist save errors appeared behind the modal. Errors are now inside the editor while the draft remains intact.

Additional regressions cover slideshow interval bounds, the10000-item/cursor limit, explicit missing-display recovery and failed physical-device removal. No deferred minor findings were reported.

## Native observation and boundaries

The exact release executable ran on a private authenticated Xvfb display and private session bus with disposable XDG configuration/data/cache/runtime directories. No package was installed. The user's desktop, application database, browser profiles, VPN, audio defaults, outputs and drives were not changed. Read-only native VPN status remained real. Private audio routing was deliberately unavailable, so the truthful “Check playback output” state appears.

Local fonts and official image assets rendered in both themes. WebKitGTK glyph metrics and native menu chrome differ from Chromium. Real dialog initial focus and Escape return were visually observed. The800×600 capture shows the returned Manage folders focus and independently scrolling navigation. Software compositing was enabled only for the smoke subprocess; no production setting was changed. Native media-photo decoding is **not** proved by SVG logo rendering.

## Remaining controlled checks

Real HDMI picture/audio, audible music, actual video playback/tracks/subtitles, native media-image decoding and photo presentation, native picker grants, disposable-drive mount/eject/busy failure, provider DRM/login playback, physical desktop scaling/multi-monitor behavior and exit policies with real owned sessions remain **pending**. Existing [hardware matrix](../../verification.md) retains these rows. Fixture success is not hardware evidence.

The visual direction is implemented and compared screen by screen. Explicit exceptions, particularly true16:9 video cards versus inconsistent generated crops, are recorded in [visual review](visual-review.md); this is not a claim of pixel-identical fidelity. No distributable was requested or produced in this task. Changes remain on the existing feature branch for the next controlled desktop review.
