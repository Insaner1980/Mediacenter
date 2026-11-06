# Verification record and release checklist

Prepared **5 November 2026**. **Release acceptance is incomplete.** This record separates implemented behavior, automated evidence, earlier native probes and checks still requiring an actual package, account or device. Pending checks are not passes.

The user is currently watching TV. Do not open test windows on the TV, move existing playback, reroute sound, change VPN state, or mount/eject real storage during background development. The remaining hardware and account checks need a later explicit test session.

## Evidence already available

| Evidence | What it establishes | What it does not establish |
| --- | --- | --- |
| Final automated candidate: Rust 119 passed,1 deliberately ignored native probe; frontend 22 passed; visual 10 and E2E 1 passed | Integrated code at `3294443`; see [machine-readable summary](evidence/automated-candidate.json), [Rust log](evidence/automated-rust.txt), [frontend log](evidence/automated-frontend.txt), [visual log](evidence/automated-visual.txt) and [E2E log](evidence/automated-e2e.txt) | Actual provider playback, hardware mutation and a clean installed environment |
| Device module checkpoint: 15 passed, 1 read-only native probe ignored | Multi-volume failure, busy/denied operations, unsupported power-off, protected drives, active-reader failure, shared-device confirmation, identity models | A real mount, unmount or power-off was not performed by these tests |
| Earlier source/device read-only probe, recorded in the implementation ledger | UDisks2 discovered 1 drive and 2 mounted volumes at that time | Device removal, hardware reconnect or permission-dialog behavior |
| Earlier design checkpoint, recorded in the implementation ledger | 10 browser layout checks and 1 theme/navigation E2E check passed | Final native pixels, both-theme review after all new states, actual monitor scaling |
| [Final extracted-package native smoke](evidence/candidate-native-smoke.json) | Final package opened without Vite; private database and real native snapshot loaded; all eight logos and both themes visually checked: [dark](evidence/candidate-native-dark.png), [light](evidence/candidate-native-light.png) | Clean install/menu, real desktop decorations or accelerated GPU acceptance; dark Xvfb capture required test-only software rendering |
| [Native shell smoke](evidence/native-shell-smoke.json) | Standalone debug process with one computer window, 1280 × 800; no TV window created | Native pixels were not reviewed; no installed release was tested |
| [Private mpv IPC smoke](evidence/mpv-private-ipc-smoke.json) | Real mpv accepted private socketpair IPC, emitted `file-loaded` and exited cleanly with null video/audio outputs | Visible playback, audible output, HDMI, subtitles or source-removal behavior |
| [Earlier mpv preflight](evidence/mpv-preflight.json) | Generated test fixture, pause/seek/track observation and one matching PulseAudio stream; volume zero | Audible output and packaged application integration |
| [Chrome placement preflight](evidence/chrome-window-preflight.json) | Disposable blank app window ownership and movement, ordinary Chrome preserved | Exact scaled placement, service/DRM playback or HDMI sound |
| [Streaming compatibility matrix](streaming-compatibility.md) | Public YouTube title reached 1:07/10:34 in the existing Chrome profile | Managed-profile login/DRM, other providers, delivered 4K quality, TV picture or audible playback |

The first row records the final automated candidate; older checkpoints below it remain historical evidence. Neither replaces the native acceptance matrix. The local ignored ledger is `.superpowers/sdd/2026-11-05-mediacenter-linux-implementation-plan/progress.md`; maintained release evidence belongs in this document and `evidence/`.

The preflight's scaling and audio findings remain open: requested X11 positions were doubled on the observed Cinnamon session; an HDMI port was available but only an analog sink was exposed. Record the current state when retesting instead of treating those earlier observations as permanent hardware facts. See [preflight](preflight.md).

## Final candidate record

| Field | Value |
| --- | --- |
| Source revision and local changes | `3294443a774c69e55dbf85900038095a5c1dca5e`; no uncommitted application changes at build |
| Configured app version | 0.1.0, verified in Debian metadata |
| Production identifier | `io.emma.mediacenter` |
| Build date, host OS and architecture | 5 November 2026; Linux Mint 22.3 / Noble; amd64 |
| Node / Rust / Tauri CLI versions | Verified: Node 26.13.0, Rust 1.99.1 (58cf8f9c3), Tauri CLI 2.12.4 |
| Exact `.deb` filename and byte size | `releases/0.1.0/Mediacenter_0.1.0_amd64.deb`, 7,744,610 bytes |
| SHA-256 | `f990740db66ffc5b690056d98483375da3a770adc1c0da36ce7222c979139356` |
| Debian Package / Version / Architecture / Depends | `mediacenter` / `0.1.0` / `amd64`; [exact dependency metadata](evidence/debian-candidate.json) |
| Desktop entry and icon | Archive has `Terminal=false`, `Exec=mediacenter`, matching icon; desktop-file-validate passed. Installed-menu check pending |
| Linked release libraries | Host `ldd`: no missing libraries. Clean-target resolution pending |
| Installation environment | Pending representative clean Mint/Noble environment |
| Overall release decision | **Not accepted**; manual and package gates remain open |

Follow [packaging](packaging.md) for build, inspection, checksum and preservation procedures. Do not fill in a checksum before the exact release artifact exists.

## Automated candidate checks

For each row record the date, source revision/local changes, command, result and log path. A failure in an unrelated module still belongs in the record. A test that uses a fake backend must remain labelled as such.

| Command | Candidate result | Evidence |
| --- | --- | --- |
| `npm run typecheck` | Passed within `npm run build` | [Automated record](evidence/automated-candidate.json) |
| `npm run lint` | Passed | [Automated record](evidence/automated-candidate.json) |
| `npm test` | Passed | [Automated record](evidence/automated-candidate.json) |
| `npm run build` | Passed | [Automated record](evidence/automated-candidate.json) |
| `npm run verify:assets` | Passed | [Automated record](evidence/automated-candidate.json) |
| `npm run verify:ui-copy` | Passed | [Automated record](evidence/automated-candidate.json) |
| `npm run test:visual` | 10 passed; browser fixture | [Log](evidence/automated-visual.txt) |
| `npm run test:e2e` | 1 passed; browser fixture | [Log](evidence/automated-e2e.txt) |
| `npm run rust:check` | Passed | [Automated record](evidence/automated-candidate.json) |
| `npm run rust:test` | Passed | [Automated record](evidence/automated-candidate.json) |
| `npm run rust:lint` | Passed | [Automated record](evidence/automated-candidate.json) |
| `npm run rust:format` | Passed | [Automated record](evidence/automated-candidate.json) |
| `npm run tauri -- build --bundles deb -- --locked` | Passed with `CARGO_BUILD_JOBS=2` | [Artifact record](evidence/debian-candidate.json) |
| Archive, dependency, desktop/icon and checksum inspection | Passed on build host; no maintainer scripts or private runtime data in archive | [Artifact record](evidence/debian-candidate.json) |

The final native smoke used a private virtual display and private XDG directories. Its default accelerated captures intermittently missed images; software rendering in the test subprocess produced the verified dark capture. These test-only environment variables were not added to the package or the user session. The final package contains no temporary diagnostic instrumentation.

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

## Open implementation details

Exact previous browser geometry restoration and a dedicated local-player fullscreen toggle remain deferred minor details. Returning playback currently uses a reachable normal computer window. An outside-source subtitle picker is not provided; adjacent supported files within the selected source are supported. See [implementation decisions](implementation-decisions.md).

## Simplified desktop UI — 6 November 2026

Application revision `44e091902b22199456e68a3ab298aba2a76e6e50` implements the six-screen simplified design. [Acceptance record](evidence/simplified-ui/acceptance.md) records139 passing unit tests,83 visual checks,12 E2E workflows, type/lint/asset/copy/build checks and Rust integration. [Screenshot gallery](evidence/simplified-ui/index.html) includes immutable-reference comparisons, overlays, responsive/theme cases and disposable native WebKitGTK captures. [Visual rulings](evidence/simplified-ui/visual-review.md) explicitly record deviations including true16:9 video thumbnails.

A native release executable was built without bundles and exercised on a private authenticated virtual display with private XDG directories. Both themes, font/logo rendering, real dialog open/Escape focus return and resizing were observed. This is not physical-display, media-codec, HDMI/audio, mount/eject or provider acceptance. All unperformed hardware rows above remain pending. No update was installed over the user's application and no existing user configuration was reset.

## Automatic default media folders — 6 November 2026

Revision `3062630` adds automatic Pictures/Videos/Music selection and indexing, plus one-step Open folder for other local or mounted removable folders. [Acceptance](evidence/automatic-media-folders/acceptance.md) records146 UI unit tests,95 visual checks,13 E2E workflows and130 passing Rust tests with2 existing hardware tests ignored. A production native smoke automatically indexed/displayed synthetic PNG/MP4/WAV files from private XDG media folders. A new native command is enabled only for the main window; UUID-less attachment confirmations are preserved. [Gallery](evidence/automatic-media-folders/index.html) contains the new UI and native evidence. The installed application-menu binary and user runtime state were not updated.
