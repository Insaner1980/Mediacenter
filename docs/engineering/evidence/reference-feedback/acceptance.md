# Annotated screenshot feedback — 0.1.6

User-approved scope is recorded in [the implementation plan](../../../superpowers/plans/2026-11-06-reference-feedback.md). Source base: `65ecc0d`; implementation and evidence are committed together. Private user screenshots are not copied into this repository.

## Behavior

- Music has one external mpv window, including files without cover art. Its controls stay visible, and the original filename replaces the descriptor number in the music title. No persistent music bar remains in Mediacenter. Queue keeps Previous, Next, Shuffle and Repeat; a saved repeat setting can still be disabled.
- Photos have one Close button and Escape. Fit/Actual size remain.
- Videos/Music/Photos retain Open folder and the current source selector. Comprehensive add/scan/cancel/remove operations live in Settings → Media folders. Open folder also selects mounted USB/external folders.
- Picture & sound opens the same compact modal on Home, every library, Drives & USB and repeated Settings opens. Save selection saves defaults; Apply to playback also moves the active managed session. Failed movement displays an error. Escape restores opener focus.
- Visible music windows participate in ownership-verified display placement. Audio reconciliation cannot claim picture success when music placement failed.

## Automated checks

150 UI unit tests; 132 Rust tests passed, 2 existing hardware tests ignored. 14 browser workflows passed. 103 visual checks passed (themes, viewport sizes, empty/error/offline states, 200% text and new popup screenshots). TypeScript, ESLint, Clippy, Rust formatting, asset/copy checks and production build passed.

The initial UI regression failed before implementation. A private native reproduction confirmed that the old audio-only launch produced sound without a player window. New Queue repeat/navigation and verified music-window placement regressions also failed before their fixes. The first popup E2E selector matched nested label text including option content; switching to its actual accessible combobox name fixed the test. No application label was weakened.

A fresh review found the removed queue controls and audio picture-placement exemptions; both were fixed, including the periodic audio reconciliation guard.

## Packaged native acceptance

Both native scripts passed against the extracted 0.1.6 package executable, SHA-256 `ff755c101e5ab4fb1cf78c99952d0e410cf9dd32d17311528efaf58cc6cde92a`. See [artifact inspection](../debian-0.1.6.json), [music/UI record](native/native-smoke.json) and [direct playback record](native/direct-playback/native-smoke.json).

The music run verifies one real mpv window for plain WAV and MP3 artwork, pause/resume, replacement, window close/replay and automatic queue advance. The direct run verifies resume, Play from start reset, q/window-close replay, video/video and video/music replacement, clicking a music duration cell, continued music across navigation, and queue EOF advancement.

A black secondary-video capture led to a stricter pixel test. The harness's forced `xrefresh` invalidated the video surface under Xvfb. Removing that display mutation during playback produced five distinct, colorful frames on the same package; [comparison](capture-diagnosis.json). Final full playback and music/UI runs passed with the corrected capture method. Temporary diagnostic logging was removed; no GPU-cache workaround or production change was needed.

## Visual review

[Browser output popup, Home/light](browser/output-home-light-1280.png), [Settings/dark at 800px](browser/output-settings-dark-800.png), [Music](browser/audio.png), [Photos](browser/photo.png), [Settings](browser/settings.png). All popup controls fit at tested sizes; details scroll within the modal. Photos retain a single Close, folder gears are absent and Music has no bottom bar. Native captures: [plain music](native/plain-music.png), [cover artwork](native/cover-music.png), [Home popup](native/output-home.png), [Settings popup](native/output-settings.png), [single photo Close](native/photo-viewer.png), [second video](native/direct-playback/second-video.png). Reviewed visible controls, artwork and the changing video frames.

## Limits

Native playback uses synthetic media, a private Xvfb display, private XDG data, and a private PipeWire/WirePlumber null sink with hardware discovery disabled. A routed audio stream and advancing timeline prove playback progress, not audible speaker quality. This does not test physical HDMI, native window-manager placement on multiple monitors, a physical USB drive, or installation/upgrade on the host. The user's media, output defaults, VPN and installed application remain untouched.
