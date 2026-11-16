# Selected Settings redesign, 16 November 2026

Original Remote updated in place from `f7ed5a2` to `b9c4d37`. The user selected the quiet six-row Settings concept: Sound, VPN, Media, Devices, Appearance and Advanced. Dedicated native pages retain existing settings functions, with technical diagnostics under Advanced. Mediacenter now uses a compact device selector and bottom navigation.

447 unit/Compose tests pass, zero failures/errors/skips. Detekt, ktlint, Android lint and app/test APK builds pass. The source review found two material issues (scrolling recovery details and pending-action visibility); both received RED/GREEN regressions and fixes.

Installed original `com.emmah.colordock.remote`, label Remote, with the same signer and UID 10450. Both encrypted pairings and both original preference files matched byte-for-byte before first launch. Only original Remote is installed. Font scale remains 1.15 and temporary USB stay-awake was restored to 0. Desktop, HDMI, volume and VPN settings were not changed.

**Acceptance is incomplete:** the phone is locked. No physical tests, application screenshot or source-versus-installed visual comparison have been completed for this revision. The earlier 15 physical tests apply to the previous layout only. A pending asynchronous request asks the user to unlock the phone.

Resume: private helper `.superpowers/sdd/2026-11-16-simple-settings/device-tools.py` supports `instrument` (29 focused safe rendered tests) and `capture <label>`. Do not run its install again unless the APK changes. Inspect LG and Mediacenter, capture Settings at font scale 1.15, compare with `selected-reference.png`, fix material differences, update Android `design-qa.md`, and remove only the test runner after instrumentation. Preserve app data and both pairings.

Private selected reference, installer proof, installed-before APK, review/ledger and resume helpers remain outside Git in that task directory. Archive: `releases/remote-device-20261116/simple-settings-b9c4d37/`; SHA256SUMS verified.
