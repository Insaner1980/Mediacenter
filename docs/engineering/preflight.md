# Mediacenter preflight — 5 November 2026

Status: **partial; no release acceptance**. Implementation was requested after the plan was written. Its historical “produce this plan only” authorization is superseded by that request.

## Observed machine

| Capability | Direct observation |
| --- | --- |
| OS | Linux Mint 22.3, Ubuntu Noble base |
| Session | Cinnamon X11 |
| Chrome | `/usr/bin/google-chrome`, 157.0.8103.38 |
| NordVPN | `/usr/bin/nordvpn`, 6.0.2 |
| mpv | `/usr/bin/mpv`, 0.37.0 |
| FFmpeg | `/usr/bin/ffmpeg` and `/usr/bin/ffprobe`, 6.1.1-3ubuntu5 |
| Audio | PulseAudio compatibility server on PipeWire 1.0.5; pactl 16.1 |
| X11 tools | xrandr 1.5.2, wmctrl and xprop available |
| Initial displays | eDP-1 only, 1920 × 1080 at 0,0 |
| After user connected TV | HDMI-1, 3840 × 2160 at 1920,0, 30 Hz; eDP-1 remains primary |
| Audio after TV connection | HDMI port available, but only analog-stereo sink exposed; card profile remains analog-stereo + analog input |
| Initial toolchain | node, npm, cargo and rustc absent from PATH and common version-manager locations |
| Native development libraries | GTK/WebKitGTK pkg-config checks fail; build-essential installed |
| Package installation | `sudo -n true` reports password required; user asked to install official Tauri prerequisites in their own terminal |

## Completed technical probes

- Generated a six-second MPEG-4/PCM test fixture from FFmpeg testsrc2 and sine sources. No private media was accessed. Real mpv configured video output, advanced to 0.75 seconds, accepted pause and seek to 2 seconds, exposed video/audio tracks and exited cleanly. One PulseAudio stream matched the spawned mpv PID. Volume was zero: this is not audible confirmation. Temporary fixture and IPC socket were removed. See [mpv evidence](evidence/mpv-preflight.json).
- Opened a disposable Chrome `--app=about:blank` profile alongside ordinary Chrome. Verified spawned process, start time, executable, profile SingletonLock PID and X11 window PID before movement/close. Moved to the HDMI coordinate range and returned to the computer; pre-existing window geometry remained unchanged. See [Chrome evidence](evidence/chrome-window-preflight.json).
- The first ownership probe failed because Chrome flattens `/proc/PID/cmdline` into one NUL-terminated string. A NUL-delimited original-argument assumption is invalid. The successful probe used the owned profile lock in addition to process identity.
- **Scaling finding:** requesting 80,80 produced 160,160, and requesting 2000,100 produced 4000,200. The window reached the intended display, but exact physical placement is not yet proven. The X11 adapter must reconcile logical/physical coordinates and verify actual geometry instead of assuming a 1:1 mapping.
- User selected YouTube. In installed Chrome, Blender's “Big Buck Bunny 60fps 4K - Official Blender Foundation Short Film” (`aqz-KE-bpKQ`) reached 1:07 of 10:34. This used a test tab in the existing browser profile, not the disposable managed profile. It proves ordinary YouTube playback only; no DRM, HDMI picture, HDMI audio or managed-profile login persistence claim follows. The test tab was closed.
- NordVPN initially reported Connected, United States, Kill Switch disabled. After explicit user approval, `nordvpn set killswitch on` succeeded and a fresh settings query reported enabled. Other VPN settings and connection country were not changed. Initial sanitized status/settings/country fixtures are in `tests/fixtures/nordvpn/`.
- All eight official logo hashes and the three binding reference hashes match the plan.

## Outstanding acceptance evidence

Protected streaming playback, managed-profile login retention, Chrome stream ownership/routing, audible HDMI playback, unplug recovery, negative monitor geometry, mixed scaling, separate audio-only mpv playback, and the remaining service matrix are not tested. No network errors were deliberately induced. No account secrets or browser cookies were read or copied.

## Decision

Conditional go for toolchain and local shell preparation. Do not represent Task 01 as complete and do not accept streaming/output integrations or release packaging until their real-machine checks pass. The HDMI sink and scaling findings are concrete integration constraints, not evidence that opening a website completes the primary use case. Account-dependent DRM verification remains unavailable until an appropriate service is selected.

Native prerequisite list follows [Tauri's official Linux setup](https://v2.tauri.app/start/prerequisites/). Profile isolation follows [Chromium's user-data-directory documentation](https://chromium.googlesource.com/chromium/src/+/main/docs/user_data_dir.md). Installed `mpv --list-options` confirmed the required no-config, load-scripts, ytdl, input-ipc-server, audio-device and audio-client-name options.
