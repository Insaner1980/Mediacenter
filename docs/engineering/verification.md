# Verification record and release checklist

Prepared **5 November 2026**. **Release acceptance is incomplete.** This record separates implemented behavior, automated evidence, earlier native probes and checks still requiring an actual package, account or device. Pending checks are not passes.

The user is currently watching TV. Do not open test windows on the TV, move existing playback, reroute sound, change VPN state, or mount/eject real storage during background development. The remaining hardware and account checks need a later explicit test session.

## Evidence already available

| Evidence | What it establishes | What it does not establish |
| --- | --- | --- |
| Full Rust checkpoint: 88 unit tests passed, 1 native test ignored; photo decode 4 passed; photo model 3 passed | 95 passing tests at the checkpoint before subsequent lifecycle, command and fingerprint integration; fake device safety, queue/model behavior, source boundaries, process policy and fixture decoding | Final integrated build acceptance, actual USB eject, provider playback, visible native image quality |
| Frontend checkpoint: 17 tests across 7 files passed; TypeScript passed | Interaction/state behavior at that development checkpoint, including 4 drive UI tests | Native WebKitGTK behavior or tests of later changes |
| Device module checkpoint: 15 passed, 1 read-only native probe ignored | Multi-volume failure, busy/denied operations, unsupported power-off, protected drives, active-reader failure, shared-device confirmation, identity models | A real mount, unmount or power-off was not performed by these tests |
| Earlier source/device read-only probe, recorded in the implementation ledger | UDisks2 discovered 1 drive and 2 mounted volumes at that time | Device removal, hardware reconnect or permission-dialog behavior |
| Earlier design checkpoint, recorded in the implementation ledger | 10 browser layout checks and 1 theme/navigation E2E check passed | Final native pixels, both-theme review after all new states, actual monitor scaling |
| [Native shell smoke](evidence/native-shell-smoke.json) | Standalone debug process with one computer window, 1280 × 800; no TV window created | Native pixels were not reviewed; no installed release was tested |
| [Private mpv IPC smoke](evidence/mpv-private-ipc-smoke.json) | Real mpv accepted private socketpair IPC, emitted `file-loaded` and exited cleanly with null video/audio outputs | Visible playback, audible output, HDMI, subtitles or source-removal behavior |
| [Earlier mpv preflight](evidence/mpv-preflight.json) | Generated test fixture, pause/seek/track observation and one matching PulseAudio stream; volume zero | Audible output and packaged application integration |
| [Chrome placement preflight](evidence/chrome-window-preflight.json) | Disposable blank app window ownership and movement, ordinary Chrome preserved | Exact scaled placement, service/DRM playback or HDMI sound |
| [Streaming compatibility matrix](streaming-compatibility.md) | Public YouTube title reached 1:07/10:34 in the existing Chrome profile | Managed-profile login/DRM, other providers, delivered 4K quality, TV picture or audible playback |

These are development checkpoints, not final candidate results. Rerun the required checks after integration and attach the resulting code revision, artifact hash and logs below. The local ignored ledger is `.superpowers/sdd/2026-11-05-mediacenter-linux-implementation-plan/progress.md`; maintained release evidence belongs in this document and `evidence/`.

The preflight's scaling and audio findings remain open: requested X11 positions were doubled on the observed Cinnamon session; an HDMI port was available but only an analog sink was exposed. Record the current state when retesting instead of treating those earlier observations as permanent hardware facts. See [preflight](preflight.md).

## Final candidate record

| Field | Value |
| --- | --- |
| Source revision and local changes | Pending final integration/build |
| Configured app version | 0.1.0; verify against the built package |
| Production identifier | `io.emma.mediacenter` |
| Build date, host OS and architecture | Pending artifact build record |
| Node / Rust / Tauri CLI versions | Repository pins Node 26.13.0, Rust 1.99.1 and Tauri CLI 2.12.4; record actual build versions |
| Exact `.deb` filename and byte size | Not recorded at document preparation |
| SHA-256 | Not recorded; generate from the actual artifact |
| Debian Package / Version / Architecture / Depends | Pending `dpkg-deb --field` inspection |
| Desktop entry and icon | Pending archive inspection and clean installed-menu verification |
| Linked release libraries | Pending; require no missing library and verify clean-target dependency resolution |
| Installation environment | Pending representative clean Mint/Noble environment |
| Overall release decision | **Not accepted**; manual and package gates remain open |

Follow [packaging](packaging.md) for build, inspection, checksum and preservation procedures. Do not fill in a checksum before the exact release artifact exists.

## Automated candidate checks

For each row record the date, source revision/local changes, command, result and log path. A failure in an unrelated module still belongs in the record. A test that uses a fake backend must remain labelled as such.

| Command | Candidate result | Evidence |
| --- | --- | --- |
| `npm run typecheck` | Pending final rerun | |
| `npm run lint` | Pending final rerun | |
| `npm test` | Pending final rerun | |
| `npm run build` | Pending final rerun | |
| `npm run verify:assets` | Pending final rerun | |
| `npm run verify:ui-copy` | Pending final rerun | |
| `npm run test:visual` | Pending final rerun; browser fixture | |
| `npm run test:e2e` | Pending final rerun; browser fixture | |
| `npm run rust:check` | Pending final rerun | |
| `npm run rust:test` | Pending final rerun | |
| `npm run rust:lint` | Pending final rerun | |
| `npm run rust:format` | Pending final rerun | |
| `npm run tauri -- build --bundles deb -- --locked` | Pending final build | |
| Archive, dependency, desktop/icon and checksum inspection | Pending artifact | |

An ignored native test is not a passing hardware test. Do not remove its ignored marker or replace mocked success with a hardware-success claim. Update the counts above only with a corresponding recorded execution.

## Native and package acceptance matrix

All rows below are **pending** unless later evidence is entered. Use generated fixtures or a disposable test drive. Record version, date, exact scenario, observed result and evidence path; avoid screenshots containing private titles or account information.

| Scenario | Required observation | Status |
| --- | --- | --- |
| Clean package install and application-menu launch | Dependencies resolve; installed executable opens a normal computer window without Vite or a developer terminal | Pending |
| Both themes and native scaling | Final controls, official logos/fonts, long names and error states match the approved layout in WebKitGTK; no first-paint flash or clipped controls | Pending |
| Restart, update and uninstall preservation | Theme, sources, playlists, resume data and managed browser login persist; uninstall preserves application data and original media | Pending |
| Offline/missing prerequisites | Local media remains useful without internet/TV; missing Chrome/NordVPN/mpv errors are truthful and independent | Pending |
| VPN-required launch and recovery | Fresh connected/Kill Switch observation gates launch; stale/unknown/disconnected states block appropriately; no fabricated connected status | Pending authorized network scenario |
| Managed service playback | Real protected title where applicable, provider response, login retention, subtitles and measured quality recorded per service | Pending; use [service matrix](streaming-compatibility.md) |
| Managed Chrome alongside ordinary Chrome | Only the verified managed window/profile is controlled; existing browser playback and geometry remain unchanged | Pending packaged integration |
| HDMI picture and sound | Real picture and audible HDMI output, identifiable owned stream, effective target verified; missing sink or routing failure remains partial | Pending; prior missing-sink finding open |
| X11 geometry/scaling | Negative coordinates, mixed scaling and fullscreen moves verified by actual geometry; main window stays reachable on computer | Pending; prior coordinate finding open |
| Output disconnect and suspend/resume | Owned playback/photo presentation recovers predictably; no unrelated window or global audio-default changes | Pending |
| Local video | Visible playback, pause, seek, stop, resume, subtitle/audio tracks and process/socket cleanup | Pending packaged real playback |
| Music | Audible mixed-format queue, repeat/shuffle/order/duplicates, playlist restart persistence, missing item and unplugged-source behavior | Pending packaged real playback |
| Photos | Orientation/aspect ratio, large/corrupt-image errors, interval/pause/navigation, independent TV presentation and output recovery | Pending packaged native review |
| Folder selection and source boundary | Native picker grants only selected roots; unusual/Finnish/non-UTF-8 names remain correctly associated; no read escapes the granted source | Pending native picker/integration |
| Drive reconnect | UUID source survives a changed mount path; duplicate identity blocks ambiguity; UUID-less candidate requires explicit reattachment | Pending disposable drive |
| Drive safe removal | Active playback, scans, thumbnail/decode work and slideshows release handles; every volume handled; sibling warning is explicit | Pending disposable multi-volume device |
| Busy or denied removal | Another application's open file/permission refusal prevents success; partial unmount leaves accurate source state; no forced unmount | Pending disposable drive and controlled external process |
| Unsupported power-off | Successful unmount is distinguished from hardware power-off; a failure never reports safe removal | Pending suitable device |
| App exit with owned sessions | Chosen exit policy closes only owned work; leaving a service open is explicit; NordVPN stays unchanged; no privileged listener remains | Pending controlled desktop scenario |

Service access, subscriptions and regional availability can prevent completion of a row. Record the blocker and tested response. Opening the correct URL is not a substitute for playback acceptance.

## Manual result template

Copy this block for each performed scenario:

```text
Scenario:
Date / tester:
Source revision / local changes:
Package filename / SHA-256:
OS / desktop session / display scaling:
Relevant Chrome / NordVPN / mpv / FFmpeg versions:
Hardware or generated fixture:
Authorized changes for this test:
Steps:
Expected behavior:
Observed behavior:
Result: pass | fail | partial | blocked
Evidence file(s):
Cleanup and preservation check:
Remaining limitation:
```

A release handoff should name the exact artifact and checksum, list measured limitations and retain unresolved rows. Do not mark Tasks 01, 15 or 16 complete merely because implementation, mocked checks or the Debian build succeeded.
