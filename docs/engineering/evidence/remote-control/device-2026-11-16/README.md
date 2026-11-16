# Physical-device continuation — 16 November 2026

This run continues Task 21. Full sofa and release acceptance remain incomplete.

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

Next: obtain the pending permission for the LAN Discovery setup, retry direct Wi-Fi reachability and fresh manual pairing, then QR pairing and media acceptance. The phone's camera permission is currently denied from the fallback test; request it normally before QR use. Restore the requested HDMI mirror and confirm physical TV picture and audible sound before the sofa boundary. The production signed Android upgrade, service sessions, LG controls, USB/eject, lifecycle, latency and two-hour tests remain open.
