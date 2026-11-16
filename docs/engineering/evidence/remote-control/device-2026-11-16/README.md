# Physical-device continuation — 16 November 2026

This run continues Task 21. Full sofa and release acceptance remain incomplete.

Current continuation: [paired follow-up and confirmed HDMI picture](#paired-follow-up-and-confirmed-hdmi-picture). Earlier sections below retain the failures and pending permissions as historical observations.

## Setup and preservation

- Desktop checkout: `f6ba2b0`; tested desktop artifact is the unchanged candidate built from `525184a80aca337b033f289367b1c66f9d18b0d6`. All three entries in `releases/remote-streaming-candidate/SHA256SUMS` passed a fresh check.
- Pixel 9 is authorized over USB, Android API 37. Android source starts at `137fdd1454e321bdcf2037a2bbc5ffc4e4c83083`.
- The latest source was built using the existing isolated application ID, `com.emmah.colordock.remote.task7fixture`. `aapt2` verified that ID, `apksigner` verified the signature and matched it to the previously installed isolated fixture before `adb install -r`. The first isolated APK SHA-256 was `9d44918b3708f0dbef2ebd1bdea5bb800d0188f262fcf81c9cdd3700ac64d303`.
- The production Remote package was not updated, uninstalled or cleared. The standard debug APK and candidate manifest were preserved. The production desktop package and application data were not updated. The extracted desktop candidate runs with private XDG directories and generated video, tone and image fixtures.
- HDMI now detects the LG TV. The actual layout is **extended**, with the laptop at 1920×1080 and TV at 3840×2160 to its right. No display layout was changed. Mirrored sofa acceptance awaits the requested mirror setup and physical picture confirmation.
- The audio card initially used analog duplex despite an available HDMI port. Setup selected HDMI stereo with the existing analog microphone input. The HDMI sink appeared, unmuted at 40%. This proves endpoint availability, not audible TV output.

## Observations

| Check | Observed result | Scope / remaining work |
| --- | --- | --- |
| Android LAN permission denied | The app returned to setup with the local-network permission requirement. | Denial path observed. Permanent denial and connected revocation remain pending. |
| LAN permission requested again | Android showed its permission dialog again; after Allow, setup exposed the phone Wi-Fi network and it could be selected. | Physical API-37 grant/retry observed. |
| Camera denied | Setup displayed “Camera access declined. You can pair manually.” and retained address, port and manual comparison controls. | Fallback UI observed; successful manual pairing remains pending. |
| Desktop remote listener | The actual packaged app reported listening on the selected Wi-Fi interface; `ss` confirmed a listener bound to that interface's address and configured port. | Real packaged listener, not a mocked backend. |
| Manual pairing over Wi-Fi | The phone returned the secure-pairing failure message. A separate phone-side TCP connect to the same listener timed out. | **Blocked** at network reachability. No USB forwarding was substituted for LAN acceptance. |
| Network policy | NordVPN reported LAN Discovery disabled and Kill Switch disabled. Neither setting nor the VPN connection was changed. | Permission requested before changing LAN Discovery. The timeout alone does not prove which host/phone policy blocks traffic. |

No pairing code, verification phrase, device serial, account details, private media or credential material is included here.

## Phone UI regression

The real Mediacenter target page placed its header beneath the Android status bar and rendered ordinary text in black on the dark background. The LG branch already used `RemoteAppShell` with a `Scaffold`; the Mediacenter branch called its page directly with an empty modifier. This omitted both the themed content color and system-bar padding.

The correction wraps only the Mediacenter branch in a `Scaffold` using the theme background and passes its content padding to the existing page. LG bottom navigation and target/generation ownership are unchanged.

- Before the correction, all three new tests failed for the expected reasons: default black text in each theme and content starting at y=0 despite supplied system insets. [RED log](ui-red.txt)
- After the correction, the three new tests and four existing control-target tests passed, with zero failures/errors/skips. [Focused GREEN log](ui-green-focused.txt)
- Independent scoped review approved the correction with no findings. Broader media behavior and the network/display acceptance blockers were explicitly outside that narrow review and remain open here.
- The first broad gate stopped in Detekt with the Java-version error `25.0.5`. This was a toolchain failure, not a test pass. [Initial failure](broad-jdk25-failure.txt) The rerun uses the previously established JDK 17 Gradle setting, while the configured unit-test launcher retains JDK 25.

## Verified correction and current handoff

- Android correction committed locally as `36302ec` (the reviewed source hashes are unchanged). All **431 unit/Compose tests** passed with zero failures/errors/skips. Detekt, ktlint, Android lint, isolated APK assembly and instrumentation APK assembly passed. The broad command finished successfully in 7m19s. [Full log](broad-jdk17.txt), [machine-readable summary](verification.json), [scoped review](review.md).
- The updated app and runner package IDs were checked before installing them. Seven tests ran on the actual Pixel: the three new shell regressions and four existing target/command-isolation cases. All seven returned started/pass statuses; the runner ended with `OK (7 tests)` and instrumentation code `-1`. [Device output](device-ui.txt)
- The corrected isolated APK SHA-256 is `d13cc64e0fc61dc97bdcf7a639b6888b0292c25871fb5e228636486305681a2a`. Both isolated APKs, source metadata and `SHA256SUMS` are archived locally in `releases/remote-device-20261116/`. This is an isolated test build; it does not replace the signed production Remote or alter the earlier release compatibility manifest.
- Normal launch followed by selecting Mediacenter was visually checked on the phone. Its header now begins at y=173 rather than y=0; ordinary text is light on the dark background, and the system status/navigation bars remain separate. Private screenshots were inspected but not committed.
- Existing toolchain/dependency warnings remain in the logs. The new regression class also uses the repository's older Compose test-rule API, which emits a deprecation warning. No dependency or SDK upgrade was made.

## Authorized network and mirror follow-up

The user then explicitly authorized enabling Linux LAN Discovery and applying the mirror. The earlier pending LAN/mirror requests above are resolved.

- `nordvpn set lan-discovery on` succeeded; a fresh settings read confirmed enabled. The Linux VPN connection and Kill Switch were not changed.
- XRandR now shows both outputs at 1920×1080+0+0. Cinnamon reports one primary logical monitor with both physical outputs. Applying this configuration through its persistent DisplayConfig API produced `~/.config/cinnamon-monitors.xml`; that file was checked for the shared origin and both 1080p modes. Logout/reboot retention was not exercised.
- The phone could ping the desktop, but TCP still timed out after LAN Discovery was enabled. UFW was active with a default drop policy. An administratively approved rule was added for only the current test phone, the selected Wi-Fi interface and the desktop's TCP port 47831. The phone-side TCP check then succeeded. Exact private rule/rollback details are retained in the ignored checkpoint workspace. No general firewall policy was changed.
- The real Android app still could not probe the certificate. A new explicit, read-only physical test uses the production Wi-Fi wrapper and certificate probe without pairing, storing trust or sending credentials. The device run failed at `Network.bindSocket` with **EPERM before TLS**. The phone reports active VPN package `com.nordvpn.android` and `bypassable=false`. Android documents that direct network selection depends on the VPN allowing bypass. [Android VPN API](https://developer.android.com/reference/android/net/VpnService.Builder#allowBypass())
- Permission for a **phone-side** test-app VPN exclusion, or a brief VPN pause as an alternative, was requested separately and is still pending. No phone VPN setting was changed. NordVPN documents per-app exclusion through split tunneling. [NordVPN instructions](https://support.nordvpn.com/hc/en-us/articles/19618692366865-What-is-Split-Tunneling-and-how-to-use-it-with-NordVPN)
- Two brief tones were sent successfully to the explicit HDMI audio endpoint. Physical confirmation of the same TV picture and audible TV sound was requested and is still pending. A successful audio command is not an audible-output pass.

The opt-in probe is Android commit `98b82ba`. Its assembly, Detekt and ktlint checks passed. Its actual device execution is a recorded **failure**, not a passing TLS test. [Device failure](phone-vpn-probe-failure.txt), [build](probe-build.txt), [quality checks](probe-quality.txt), [setup summary](network-display-followup.json).

The scoped diagnostic-test review found no Critical or Important issue. One deferred Minor limits its TCP preflight to the tested IPv4 scope: it uses the endpoint literal rather than the production resolver's interface-scoped IPv6 address. Do not use this preflight as IPv6 link-local acceptance. TLS, pairing and media acceptance are not inferred from this review.

Next: obtain the phone VPN choice, rerun the physical probe after the approved change, and perform fresh manual and QR pairing. The phone's camera permission is currently denied from the fallback test; request it normally before QR use. Confirm TV picture and sound before the sofa boundary. The production signed Android upgrade, service sessions, LG controls, USB/eject, lifecycle, latency and two-hour tests remain open.

## Paired follow-up and confirmed HDMI picture

The user authorized all necessary setup and asked to stop repeated permission questions. Both the isolated fixture and existing Remote were excluded from the phone's NordVPN tunnel using its normal split-tunneling UI, preserving the other exclusions. **Reconnect now** was selected to apply the changes. VPN remains active for other apps. The real selected-Wi-Fi certificate probe then passed; the earlier EPERM failure is retained above.

Two desktop defects were reproduced and corrected:

- `f842322`: the network epoch hashed `SockaddrStorage` debug text, including inactive storage bytes. The real listener repeatedly replaced its network and canceled fresh pairing windows. A deterministic regression constructed semantically identical IPv4 storage with different unused bytes and failed with the original hash. Typed address hashing now excludes those bytes while retaining interface indices, IPv6 scope, VPN changes and order independence. Review caught the intermediate Display-based version losing link-layer indices; its failing regression is retained, and typed hashing corrected it. [Original failure](network-epoch-red.txt), [review regression](network-epoch-review-red.txt), [passing suite](network-epoch-rust.txt), [Clippy](network-epoch-clippy.txt).
- `42996f2`: native PulseAudio names contain characters forbidden in the wire `Id`. Snapshot capture rejected the real HDMI sink, so an authenticated WebSocket closed before sending hello. The real-name snapshot regression failed before the correction. Remote list/requested/effective IDs now use a uniform SHA-256-derived opaque ID; prepare/test resolve it against current available native sinks. Confirmation retains its native descriptor/session checks. Native preferences, backend arguments and the protocol schema are unchanged. [RED](pulse-snapshot-red.txt), [GREEN](pulse-snapshot-green.txt), [434 passing Rust tests, 4 existing ignored](pulse-rust.txt), [Clippy](pulse-clippy.txt), [package build](pulse-build.txt), [88-method contract comparison](device-contract.txt).

Android `92229bd` adds an optional independently verified pin to the read-only challenge probe and an opt-in isolated stored-pairing subscription test. The latter uses the production transport and reducer without sending control commands or storing credentials. Its final Pixel execution passed in 0.364 seconds. TCP preflight now uses the production scoped resolver, and failure diagnostics map message types to fixed tags. [Final checks/build](probes-final-build.txt), [certificate after VPN correction](phone-certificate-after-vpn.txt), [pinned challenge](phone-pinned-challenge.txt), [socket before correction](phone-socket-before-pulse-fix.txt), [final socket pass](phone-socket-final.txt), [review](review.md).

Actual manual pairing compared all six independent native/phone words before submitting the one-use code and succeeded. The ordinary phone UI subsequently showed **Connected**, queried the library, started the synthetic video and paused it. The isolated desktop's default video directory was indexed by opening Videos at the computer before the phone query. This setup used computer/ADB actions and is **not sofa-only acceptance**.

The owned video window was visually inspected and showed the generated color pattern. Its audio stream was observed on the explicit HDMI sink, unmuted at 40% system volume. The user then selected **HDMI 1** on the television and explicitly confirmed the **same image on TV and computer**. Audible TV sound remains pending the user's answer after a fresh video sound test. The video was paused again. Configuration and audio command success are not substituted for physical sound confirmation.

The new desktop package and isolated test runner are archived with exact source metadata and SHA256SUMS in `releases/remote-device-20261116/paired-followup-42996f2/`. The earlier streaming candidate, production desktop installation, production Android APK and user library remain unchanged. Existing Remote's LAN permission and VPN exception were changed with authorization; its retained LG pairing connected successfully.

Before production adoption, the complete inactive application-data directory was copied privately with symlinks preserved rather than followed. The earlier local 0.1.6 Debian package's executable hash matched the installed executable; that matching rollback package was included in the backup. The installed package has not yet been updated.

QR/camera, signed production Android update, full local-media sofa use, provider sessions, USB, OS lifetime, latency and two-hour acceptance remain open. The phone also misleadingly labels active playback “Nothing playing” when the native optional activity title is absent; phase/time and controls work, but that caption fallback remains to fix. [Current machine-readable status](paired-followup.json).
