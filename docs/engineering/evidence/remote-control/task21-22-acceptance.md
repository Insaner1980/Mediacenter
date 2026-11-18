# Remote control acceptance

## Device continuation, 2026-11-18

The installed Pixel update restored both saved connections after a cold launch and reconnected to the desktop after a background interval. The desktop played a disposable HDMI video while Remote was stopped. Remote opened the LG app catalog while the desktop process and listener were stopped. With both apps running, Pixel controlled local video and opened test music and a photo. Two initial video starts failed with an internal error, followed by four successful starts. The system confirmed HDMI stream routing. Speaker audibility for this run awaits observation.

Pixel started Yle Areena from a closed managed Chrome profile, used the touchpad to select essential cookies, and closed the target tab while ordinary Chrome stayed open. YouTube is enrolled and its current VPN kill-switch requirement blocked launch. Provider playback and the remaining extended device checks stay open. The current package pair and automated results are in the [18 November checkpoint](../../remote-control-checkpoint.md#tilanne-18112026).

## Device continuation, 2026-11-16

The authorized Pixel 9/API 37 is connected and the isolated fixture is paired without replacing production Remote. The 1080p mirror is configured and saved. After selecting **HDMI 1**, the user confirmed the same TV/computer picture. Audible TV output remains awaiting confirmation.

Physical testing found and corrected the phone Mediacenter page's missing system-bar padding and theme content color. Android `36302ec` passed 431 unit/Compose tests, quality/build checks and seven tests on the Pixel. The actual corrected page was visually checked.

Linux LAN Discovery and the exact-phone-only UFW rule are applied. With the user's authorization, both Remote apps were excluded from phone NordVPN and the VPN reconnected; the real Wi-Fi certificate probe passed. Desktop `f842322` fixes unstable network epochs canceling pairing, and `42996f2` fixes native HDMI sink names invalidating snapshots. Secure manual pairing, a real pinned WebSocket/native reducer probe, and ordinary phone library/play/pause use now pass. Final Rust verification passed 434 tests with 4 existing ignored tests; quality, contract and package checks passed. QR, the full sofa-only boundary, production signed Android upgrade and the remaining physical/long-run cases are still open. [Detailed run, packages and evidence](device-2026-11-16/README.md#paired-follow-up-and-confirmed-hdmi-picture)

## Read-only inventory, 2026-11-13

| Prerequisite | Observation | Status |
| --- | --- | --- |
| Phone transport | USB ADB initially showed an authorized Pixel 9, Android 17, API 37. Installed Remote `com.emmah.colordock.remote`, version `1.0.0` (1). The phone was absent from ADB during final package verification. | Target phone run blocked pending reconnection and a compatible test installation. |
| Android update | The tested debug APK and installed Remote have different signing certificates. The isolated Keystore instrumentation passed write, force-stop and read on the phone. | In-place update blocked for that APK. [Keystore and signing evidence](task21-device-keystore.md) |
| Desktop | Linux Mint 22.3 amd64, Cinnamon on X11. Installed Debian Mediacenter `0.1.6`, Google Chrome `157.0.8103.78`, mpv `0.37.0`. | Inventory only. Installed version alone does not identify the running candidate revision. |
| HDMI video and audio | XRandR reported only the built-in panel connected, with `HDMI-1` disconnected. The sound server exposed one analog sink and no HDMI endpoint. | Blocked for the mirrored TV and audible HDMI tests. |
| Chrome enrollment | The supported installation and native-input checks ran on a disposable Chrome user-data directory. | Feasibility evidence recorded. [Task 17 results](browser-feasibility.md) |

Commands included `adb devices`, selected `adb shell getprop` fields, installed-package version lookup, `dpkg-query`, `google-chrome --version`, `mpv --version`, `xrandr --query`, and `pactl list short sinks`. This inventory observed installed versions, device availability, and output endpoints. It included no profile or media inspection.

## Browser production path

On actual Chrome 157, the production extension enrolled a disposable profile, connected to the real Native Messaging host and launched a synthetic service page through the real Rust broker. Chrome showed the submitted Unicode text in the dedicated tab, and the broker closed that tab. A second run started the closed managed profile and redeemed its nonce from the extension launch page. Provider sign-in, subscriptions, DRM playback and HDMI audio require the physical run below. [Runtime smoke result](browser-production-runtime-result.json)

| Service | Launch | Sign-in | Browse | Play | Pause | Subtitles | HDMI audio | Return to menu |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Netflix | Broker profile-selection fixture | Pending | Pending | Pending | Pending | Pending | Blocked | Pending |
| YouTube | Chrome with synthetic page | Pending | Pending | Pending | Pending | Pending | Blocked | Chrome fixture tab closed |
| HBO Max | Ordinary Chrome fixture | Pending | Pending | Pending | Pending | Pending | Blocked | Pending |
| Viaplay | Pending | Pending | Pending | Pending | Pending | Pending | Blocked | Pending |
| MTV Katsomo | Pending | Pending | Pending | Pending | Pending | Pending | Blocked | Pending |
| Ruutu | Pending | Pending | Pending | Pending | Pending | Pending | Blocked | Pending |
| Yle Areena | Pending | Pending | Pending | Pending | Pending | Pending | Blocked | Pending |
| Plex | Pending | Pending | Pending | Pending | Pending | Pending | Blocked | Pending |

## Candidate verification

- Desktop `525184a80aca337b033f289367b1c66f9d18b0d6`, Android `137fdd1454e321bdcf2037a2bbc5ffc4e4c83083`. The artifact manifest and SHA-256 checks are in `releases/remote-streaming-candidate/`. The APK has application ID `com.emmah.colordock.remote`, version `1.0.0` (1), API 37 and a verified debug signature.
- Rust passed 429 tests with four existing ignored tests. The frontend passed 189 unit tests and 17 browser E2E tests. Browser extension tests passed 18 and release tooling tests passed eight. Clippy, formatting, lint, typecheck, production build and the 88-method protocol comparison passed.
- Android passed 428 unit and Compose tests, Detekt, ktlint, Android lint, APK assembly and instrumentation compilation. Physical instrumentation evidence is limited to the earlier isolated Keystore test.
- The final Debian package passed clean install, upgrade from `0.1.5`, and removal in an isolated `dpkg` database and root. Running the extracted packages against disposable data verified the migration from schema 1 to schema 2, retained test playlists, queue and settings, rejected schema 2 with the old app and opened a compatible backup with the old app. The packaged helper registered, verified and removed its exact Chrome manifest. The final package also restored the original background process's main window on a second launch in a private X11/D-Bus session.

## Acceptance run

Record a result for each row using **pass**, **fail**, **blocked**, or **pending**. Keep test IDs, date, both candidate revisions, artifact checksums, phone model and Android API, desktop and browser versions, network conditions, setup, action, observed result and a sanitized evidence reference. Record each provider separately. Use disposable media and a spare USB drive. Keep credentials, pairing codes, device serial numbers, profile state, file paths and media names out of evidence.

| Test | What to observe | Current status |
| --- | --- | --- |
| T21-PAIR | From the target phone, pair by QR and manually with camera permission denied. Check Android LAN permission denial/revocation, certificate change, Wi-Fi loss, and a blocked-discovery network. | Partial: actual Pixel manual pairing, selected-Wi-Fi certificate/challenge and stored-pairing snapshot subscription pass. QR and remaining connected/error cases pending. |
| T21-LOCAL | At the sofa-only boundary, operate videos, music, playlists and photos. Verify seek, volume, mute, track controls, menu return, canceled replacement, and recovery after phone sleep or network loss. | Partial: ordinary phone library/query/video play/pause observed after computer setup; mirrored TV image confirmed by user. Audible TV sound and full sofa-only sequence pending. [Local-media source checkpoint](task21-local-checkpoint/README.md) |
| T21-DEVICES | Exercise library folder scan/cancel/remove, disposable USB busy/eject, audio output selection, and LG/Mediacenter switching. Verify state and side effects at the intended device. | Pending physical run. |
| T21-STREAM | In each available service, verify phone search, selection, playback, return to Mediacenter and control of only the enrolled tab. Record required computer access, login redirects/popups, visible debugger warning and audible HDMI output. Verify ordinary Chrome tabs and windows survive closing HBO Max. | Blocked for HDMI acceptance. Provider sessions pending. |
| T21-LIFETIME | Test End session, resume and network recovery, HDMI unplug/replug and TV power cycle, no automatic laptop-speaker playback, explicit lock, desktop login autostart and a single listener. Check the mirrored desktop for UI regressions. | Pending physical run. |
| T21-VPN | During a separately authorized VPN change, measure LAN recovery and check protection, credentials and absence of duplicate mutations. | Pending physical run. |
| T21-PERF | On the actual LAN, record raw timings and compare them with the table below. Observe a two-hour mixed-media run and background traffic. | Pending physical run. |
| T22-CLEAN | On a disposable target, install the Debian package and matching APK. Check package versions, extension ID, exact native manifest and executable host path. Restart Chrome and desktop and verify retained enrollment and connectivity. | Desktop isolated installation and Chrome enrollment passed. Paired clean install pending. |
| T22-UPGRADE | On a disposable copy of compatible prior state, pair a phone and record identity, playlists and a harmless LG/Chrome configuration. Update both packages. Verify retained state and that revoking the test phone prevents reconnection. | Desktop upgrade and fixture data retention passed. Paired upgrade and revocation blocked on the signed phone build. |
| T22-REMOVE | Turn off login startup, remove the extension and matching registrations, uninstall the package, then verify app, autostart and registrations are absent. Confirm original test media and retained application data still exist. | Isolated package and registration removal passed. Desktop login startup and extension removal together are pending. |
| T22-ROLLBACK | On a disposable copy, confirm the older build rejects a newer schema. Restore a compatible complete data backup and verify that the restored identity and library are usable. | Desktop schema rejection and fixture library recovery passed. Paired identity recovery pending. |

Complete setup once at the computer. Record the existing mirror and audible HDMI baseline, signed candidate installs, permissions, pairing, and account availability. After that boundary, remain on the sofa during the sofa-only sequence. Record any computer interaction and the step that required it.

| Performance measure | Acceptance target |
| --- | --- |
| Connected D-pad to visible response | p95 ≤ 200 ms across 100 presses |
| Connected local pause to observed pause | p95 ≤ 300 ms across 30 actions |
| Warm-index search first page | p95 ≤ 1 second for a representative library |
| Phone foreground to usable state | ≤ 5 seconds in at least 9 of 10 stable-LAN attempts |
| Network restored to usable connection | ≤ 10 seconds in at least 9 of 10 attempts |
| Continuous mixed-media session | 2 hours without stalled UI, runaway queue or computer access |
| Idle/background | Compare desktop baseline CPU use, and observe Android discovery and held-key traffic ending after socket shutdown grace |

See the [full acceptance plan](../../../superpowers/plans/2026-11-09-remote-mediacenter-sofa-control.md#16-verification-and-acceptance-checklist) and the [setup and rollback instructions](../../remote-control-setup.md).
