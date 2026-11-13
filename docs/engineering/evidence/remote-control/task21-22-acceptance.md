# Remote control acceptance

## Read-only inventory, 2026-11-13

| Prerequisite | Observation | Status |
| --- | --- | --- |
| Phone transport | One authorized USB ADB device. Pixel 9, Android 17, API 37. Installed Remote `com.emmah.colordock.remote`, version `1.0.0` (1). | Available for an authorized test with a compatible build. |
| Android update | The tested debug APK and installed Remote have different signing certificates. The isolated Keystore instrumentation passed write, force-stop and read on the phone. | In-place update blocked for that APK. [Keystore and signing evidence](task21-device-keystore.md) |
| Desktop | Linux Mint 22.3 amd64, Cinnamon on X11. Installed Debian Mediacenter `0.1.6`, Google Chrome `157.0.8103.78`, mpv `0.37.0`. | Inventory only. Installed version alone does not identify the running candidate revision. |
| HDMI video and audio | XRandR reported only the built-in panel connected, with `HDMI-1` disconnected. The sound server exposed one analog sink and no HDMI endpoint. | Blocked for the mirrored TV and audible HDMI tests. |
| Chrome enrollment | The supported installation and native-input checks ran on a disposable Chrome user-data directory. | Feasibility evidence recorded. [Task 17 results](browser-feasibility.md) |

Commands included `adb devices`, selected `adb shell getprop` fields, installed-package version lookup, `dpkg-query`, `google-chrome --version`, `mpv --version`, `xrandr --query`, and `pactl list short sinks`. This inventory observed installed versions, device availability, and output endpoints. It included no profile or media inspection.

## Browser production path

On actual Chrome 157, the production extension enrolled a disposable profile, connected to the real Native Messaging host and launched a synthetic service page through the real Rust broker. Chrome showed the submitted Unicode text in the dedicated tab, and the broker closed that tab. The fixture fulfilled the page inside Chrome. Provider sign-in, subscriptions, DRM playback and HDMI audio require the physical run below. [Runtime smoke result](browser-production-runtime-result.json)

## Acceptance run

Record a result for each row using **pass**, **fail**, **blocked**, or **pending**. Keep test IDs, date, both candidate revisions, artifact checksums, phone model and Android API, desktop and browser versions, network conditions, setup, action, observed result and a sanitized evidence reference. Record each provider separately. Use disposable media and a spare USB drive. Keep credentials, pairing codes, device serial numbers, profile state, file paths and media names out of evidence.

| Test | What to observe | Current status |
| --- | --- | --- |
| T21-PAIR | From the target phone, pair by QR and manually with camera permission denied. Check Android LAN permission denial/revocation, certificate change, Wi-Fi loss, and a blocked-discovery network. | Pending on the paired candidates. |
| T21-LOCAL | At the sofa-only boundary, operate videos, music, playlists and photos. Verify seek, volume, mute, track controls, menu return, canceled replacement, and recovery after phone sleep or network loss. | Blocked on HDMI hardware and paired candidates. [Local-media source checkpoint](task21-local-checkpoint/README.md) |
| T21-DEVICES | Exercise library folder scan/cancel/remove, disposable USB busy/eject, audio output selection, and LG/Mediacenter switching. Verify state and side effects at the intended device. | Pending physical run. |
| T21-STREAM | In each available service, verify phone search, selection, playback, return to Mediacenter and control of only the enrolled tab. Record required computer access, login redirects/popups, visible debugger warning and audible HDMI output. Verify ordinary Chrome tabs and windows survive closing HBO Max. | Blocked for HDMI acceptance. Provider sessions pending. |
| T21-LIFETIME | Test End session, resume and network recovery, HDMI unplug/replug and TV power cycle, no automatic laptop-speaker playback, explicit lock, desktop login autostart and a single listener. Check the mirrored desktop for UI regressions. | Pending physical run. |
| T21-VPN | During a separately authorized VPN change, measure LAN recovery and check protection, credentials and absence of duplicate mutations. | Pending physical run. |
| T21-PERF | On the actual LAN, record raw timings and compare them with the table below. Observe a two-hour mixed-media run and background traffic. | Pending physical run. |
| T22-CLEAN | On a disposable target, install the Debian package and matching APK. Check package versions, extension ID, exact native manifest and executable host path. Restart Chrome and desktop and verify retained enrollment and connectivity. | Pending installed-candidate run. |
| T22-UPGRADE | On a disposable copy of compatible prior state, pair a phone and record identity, playlists and a harmless LG/Chrome configuration. Update both packages. Verify retained state and that revoking the test phone prevents reconnection. | Blocked on a compatible signed update for the installed phone. |
| T22-REMOVE | Turn off login startup, remove the extension and matching registrations, uninstall the package, then verify app, autostart and registrations are absent. Confirm original test media and retained application data still exist. | Pending installed-candidate run. |
| T22-ROLLBACK | On a disposable copy, confirm the older build rejects a newer schema. Restore a compatible complete data backup and verify that the restored identity and library are usable. | Pending installed-candidate run. |

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
