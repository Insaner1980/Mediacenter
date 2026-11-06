# Direct local playback — 6 November 2026

Source base: `a26bdb0`; the accompanying source changes and manifests build version **0.1.5**. [Package inspection](../debian-0.1.5.json) identifies the Debian archive and extracted executable by SHA-256. No user media or existing configuration was changed. The user-provided screenshots are not copied into this evidence.

## Behavior and root causes

- Clicking the video thumbnail, title or duration opens mpv directly and keeps the Videos library visible. The preview dialog and persistent Local video strip are removed. A secondary Play from start action remains for resumed videos.
- Clicking the music title or another row cell plays that song. Enqueue remains separate and does not also play the row. Native buttons retain keyboard activation.
- An exited mpv formerly remained in the player registry and home sessions, retaining its source lease. IPC EOF also incorrectly turned normal closure into a persistent connection error. Explicit starts then rejected the stale player. Terminal observation now stops/reaps and removes the session and clears its queue ownership.
- Explicit local starts replace the previous owned player under the shared music transition gate. Automatic next-track handling retains exact-session checks. Queue persistence reconciles candidate ownership against the live player, so an older cloned edit cannot restore a closed session.
- Startup failures and cancellation after routing retire the new session. Exact-session stop tolerates observer cleanup winning the race. Process identity checks remain in place.
- mpv allows a software graphics renderer when hardware rendering is unavailable. The virtual-display test originally exhausted hardware/legacy backends; this permits actual native video rendering on that display without forcing software on hardware-capable systems.

## Verification

| Check | Result |
| --- | --- |
| UI unit suite | 148 passed |
| Rust suite | 131 passed; 2 existing hardware tests ignored |
| Browser E2E | 13 passed |
| Visual/responsive checks | 95 passed, including both themes, narrow windows and 200% text |
| TypeScript, ESLint, Rust clippy/fmt | passed |
| Production build, fixture exclusion, local assets, UI copy | passed |
| Package archive, desktop entry, runtime library resolution, SHA-256 | passed; not installed on the host |

The new UI tests failed first against the old preview/row/strip behavior. The new Rust queue-edit regression first reproduced restoration of `closed-player`, then passed with reconciliation. Before the lifecycle fix, the real native test played a video, closed mpv with `q`, and reproduced the next-click failure: “Stop the current local player before starting another item.”

The [native regression script](../../../../tests/native/direct_playback.py) uses generated H.264 video, WAV tones and a PNG, private XDG media folders, authenticated Xvfb, a private session bus, and a private PipeWire null sink. Its WirePlumber configuration disables ALSA, camera and Bluetooth discovery. It checks advancing persisted positions, replacement process IDs, exactly one private mpv process, the isolated audio stream, and screenshots. The final run uses the executable extracted from the actual `.deb`; [native result](native-smoke.json) records its hash and measured positions.

Native cases: resume, restart an active video from zero, close with `q`, close with `WM_DELETE_WINDOW`, replay, video-to-video, video-to-music by title, music-to-music by duration cell, music continuing across Photos navigation, music-to-video, music after closing video, and automatic advancement to the next queued track at EOF.

## Scope and visual review

Screenshots show the video library without a video strip, a separate mpv window, and the audio strip only for the active music queue. The photo-navigation capture retains the current music title; the next-track capture identifies the queued second track. Web captures cover the updated video/music layouts in the existing design system. Historical 0.1.4 evidence was restored after the visual suite generated its new captures.

This verifies native decoding, advancing playback and routing to a private null sink, not audible speaker quality, physical HDMI routing, every codec, or the user's own media files. Native PipeWire streams omit the process-ID field expected by the existing pactl output-verification code, so the app may show output unconfirmed even while the test stream plays; the test verifies its single mpv stream on its isolated server. The general output-verification/physical-hardware acceptance remains open. Native Xvfb/portal diagnostics include unavailable hardware acceleration/realtime portal warnings; no host output or VPN change was made.

The package preserves the production application identifier. Installing it through the normal Debian package mechanism updates the existing application-menu entry; no Vite server or development command is needed afterwards.
