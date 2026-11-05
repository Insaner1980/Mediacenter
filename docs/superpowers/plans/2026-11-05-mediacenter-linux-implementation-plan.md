# Mediacenter Linux Desktop Application Implementation Plan

> **For agentic workers:** Use `superpowers:executing-plans` to implement this plan task by task. Use `superpowers:subagent-driven-development` only when that execution method is selected. Steps use square checkbox syntax for tracking. Read the visual references and the constraints before writing product code.

**Goal:** Build a personal Linux desktop media center that launches streaming services through the computer's NordVPN connection, keeps its control interface on the computer, supports optional HDMI playback on a television, and organizes local videos, music, photos, and external storage.

**Architecture:** Use a Tauri desktop shell with a React and TypeScript interface and a Rust integration layer. Open streaming services in separately managed Google Chrome windows; use mpv for local video and music. Rust owns device discovery, process lifetimes, playback state, filesystem access, and SQLite persistence.

**Tech stack:** Tauri 2, React, TypeScript, Vite, CSS, Rust, Tokio, Serde, SQLite through rusqlite, mpv JSON IPC, ffprobe/ffmpeg, NordVPN CLI, UDisks2 over D-Bus, PulseAudio-compatible audio control, and an X11 window-placement adapter.

**Specification:** [Corrected product and visual brief](/home/emma/Dev/Mediacenter/docs/design/2026-11-05-claude-visual-design-brief.md), the user's subsequent request to write this implementation plan, and the binding references in Section 2.

**Document date:** 5 November 2026.

**Current authorization:** Produce this plan. Writing this document does not start application implementation, install dependencies, change VPN settings, or modify another application. The older brief's instruction not to write an implementation plan is superseded by the user's explicit request for this document. Its product and visual constraints remain applicable.

## Global constraints

1. Mediacenter is a Linux desktop application. Its main control window stays on the computer's display.
2. The primary purpose is streaming through the computer's VPN connection. HDMI is an optional picture and sound output.
3. **Color Dock is a separate application already installed on the television. Do not change, replace, integrate with, reuse the design of, or write files in Color Dock.**
4. Do not modify the existing Android Remote application. Remote integration is outside the first release.
5. **The finished application must look like the approved Colour Wall desktop images in Section 2. They are binding visual targets, not loose inspiration.**
6. Deliver both light and dark themes from the first usable UI milestone. Use the same information architecture and layout in both.
7. Include Netflix, YouTube, HBO Max, Viaplay, MTV Katsomo, Ruutu, Yle Areena, and Plex in the fixed reference order.
8. Use the checked-in official service-logo files. Each tile contains only its logo, without a second visible service-name caption.
9. **No dot separators anywhere.** Do not add middle dots, bullet separators, circular status lights, decorative balls, dot-based pagination or loading indicators, or three-dot menus to application-owned screens.
10. Preserve the geometry of official logos, including circles intrinsic to a trademark. Do not redraw a logo to remove part of its official artwork. The no-dot rule applies to added UI, and does not require altering brand assets or user-owned media.
11. Use mouse, trackpad, and keyboard as primary controls. Do not implement a television-first launcher, remote-only navigation, or the old enlarged selected tile.
12. Read media in place. Do not move, rename, delete, format, repair, or reorganize the user's files or drives.
13. Keep all user media, browser profiles, settings, and logs local. No accounts, cloud backend, telemetry, or remote-control server are required for this application.
14. Streaming compatibility, VPN acceptance, HDMI playback, and available resolution must be measured with the real services. Do not equate a successfully opened browser window with working playback.

## Review focus

These are the five highest-priority failure classes. Their acceptance tests belong to the tasks shown here.

| Input or condition | Required behavior | Owning tasks |
| --- | --- | --- |
| VPN disconnects, status is stale, CLI output changes, or the daemon is unavailable | Never show a fabricated connected state; prevent new VPN-required launches and explain recovery | 01, 05, 06, 15 |
| Television disappears during playback, or monitor coordinates are negative or scaled | Keep the control window reachable; recover the owned playback window and audio without moving unrelated windows | 01, 07, 08, 15 |
| A USB drive is removed during scanning or playback and later returns under a different mount path | Cancel affected work, retain metadata as offline, and reconnect by filesystem identity | 09, 10, 11, 14 |
| A filename includes Finnish characters, shell syntax, a leading dash, invalid UTF-8 bytes, or a symlink outside the selected folder | Preserve identity and handle the file safely without shell execution or escaping the authorized source | 04, 09, 10, 11 |
| An ordinary Chrome session already exists, a managed profile is locked, or an app-owned process crashes | Preserve the user's normal browser session; never kill or manipulate a process merely because its name contains Chrome or mpv | 04, 06, 07, 15 |

---

## 1. Product definition and scope

### 1.1 What the first complete release does

The user opens Mediacenter on their Linux computer. The home screen shows their own media on the left and the eight streaming services on the right. Connection information reports the actual NordVPN state and the actual selected playback output. The user can start a service, select content in that service's own browser interface, and move its playback window to the television when an HDMI display is available.

The same application browses selected local folders and attached drives, resumes local videos, plays music and local playlists, and presents photo slideshows. These local features continue to work without an internet connection or an attached TV.

The application is a desktop control center. It is not a new streaming provider, a universal streaming catalogue, an implementation of Netflix's player, or an application installed on the TV.

### 1.2 Explicit control boundary

| Activity | Controlled by Mediacenter | Controlled by the existing application or service |
| --- | --- | --- |
| Choose and open a streaming service | Yes | Service content is rendered by Chrome |
| Choose VPN country and connect or disconnect | Requests through the installed NordVPN client | NordVPN manages the tunnel and network enforcement |
| Choose a streaming title, sign in, handle subscriptions, choose service-specific subtitles | Open or bring back the correct browser window | The service's own website |
| Move an owned streaming window to or from the TV | Yes, on the supported X11 target after verification | The desktop compositor ultimately places the window |
| Seek or pause all eight streaming services from one custom control bar | Not a first-release promise | Use each service's browser controls |
| Local video/music transport, seek, volume, queue, subtitle and audio tracks | Yes | mpv decodes and plays the file |
| Photos and slideshows | Yes | A restricted Mediacenter presentation window displays images |

Do not display working-looking universal streaming transport controls without a verified integration. The first release centralizes launching, VPN state, output selection, local media control, and return-to-computer actions. Streaming content selection and service-specific playback controls remain in the service browser. This is an intentional technical boundary and must be visible in the implementation documentation.

### 1.3 First-release exclusions

1. No Color Dock or Remote changes, casting protocols, HDMI-CEC integration, television installation package, or phone pairing.
2. No scraping of service catalogues, credentials, watch history, or playback pages.
3. No downloading protected streams, extracting DRM keys, injecting browser extensions, or using a custom DRM player.
4. No shared streaming continue-watching database. Resume history is for local media only.
5. No NAS discovery, Plex server administration, transcoding server, metadata cloud, or automatic poster downloads. Plex initially opens the official web application.
6. No filesystem editing, disk formatting, automatic repair, or forced unmount.
7. No automatic changes to the desktop display topology, global audio defaults, VPN firewall configuration, or startup applications.
8. No complete Wayland window-management support in the first release. Detect Wayland and offer a clear reduced-capability mode rather than claiming X11 placement works there.

### 1.4 Implementation decisions made by this plan

The user selected the purpose and appearance. The stack, storage model, staged release order, strict VPN launch policy, and adapter boundaries below are engineering decisions made in this plan. They are not presented as earlier user-selected technical requirements.

The selected stack reuses the existing HTML/CSS design directly and delegates service playback to the already installed browser. A Qt-based interface could also work, but would require rebuilding the approved visual language in another UI system. Electron is not selected because bundling another Chromium runtime does not itself establish DRM support, and the planned application already uses installed Chrome for streaming.

## 2. Binding visual references

### 2.1 Authoritative files

| Reference | File | How it must be used |
| --- | --- | --- |
| Dark theme | [colour-wall-desktop-dark.jpg](/home/emma/Dev/Mediacenter/docs/design/reference/colour-wall-desktop-dark.jpg) | Mandatory appearance target for the dark home screen |
| Light theme | [colour-wall-desktop-light.jpg](/home/emma/Dev/Mediacenter/docs/design/reference/colour-wall-desktop-light.jpg) | Mandatory appearance target for the light home screen |
| Executable visual specification | [2026-11-05-mediacenter-colour-wall-desktop.html](/home/emma/Dev/Mediacenter/docs/design/2026-11-05-mediacenter-colour-wall-desktop.html) | Source of layout, colors, spacing, typography, logo treatment, hover, focus, and responsive behavior |
| Official assets and provenance | [provenance.json](/home/emma/Dev/Mediacenter/docs/design/assets/official-logos/provenance.json) | Source files, original URLs, and checksums of all eight logos |
| Corrected brief | [2026-11-05-claude-visual-design-brief.md](/home/emma/Dev/Mediacenter/docs/design/2026-11-05-claude-visual-design-brief.md) | Product intent and the corrections to the original TV-first interpretation |

**Mandatory implementation instruction: reproduce the application panel in these images faithfully. The app must look like this. Do not substitute a generic dashboard, a streaming-service clone, a dock, a wallpaper launcher, or a new visual direction.**

Dark reference:

![Mandatory dark-theme appearance](/home/emma/Dev/Mediacenter/docs/design/reference/colour-wall-desktop-dark.jpg)

Light reference:

![Mandatory light-theme appearance](/home/emma/Dev/Mediacenter/docs/design/reference/colour-wall-desktop-light.jpg)

Both screenshots are 1265 × 852 pixels. They include presentation material outside the application panel. The product surface is the `main.app` element in the HTML. Do not ship the headings “Colour Wall / Linux-työpöytä”, “Valittu kakkosvaihtoehto”, the mockup disclaimer, or the link to the design brief as application content.

The theme picker above the panel is a presentation control. In the application, expose Light and Dark through the native View menu and an Appearance section inside settings. This preserves the referenced home-panel layout. Default to Dark for a new profile and persist the explicit selection. Do not follow system theme changes unless a future System option is deliberately added.

### 2.2 Reference integrity

| File | SHA-256 at plan creation |
| --- | --- |
| Desktop HTML | `8e0765ee2984557ec562c0d772f9553500595009bacc44163b502e2868e1e163` |
| Dark screenshot | `9085891a95cdc94718ea56490e262d8d685639722f13bd9adf4e03f65352ee08` |
| Light screenshot | `b18beacb5a43be57643d617bd7c75ab4c0465b33580dd9518d2639ad92b22bc9` |

Keep these files as evidence. Never overwrite them just to make a visual regression test pass. The old four-direction HTML and `reference/mediacenter-option-2.png` are historical material, not the implementation target. The selected option is the revised Claude Colour Wall design, not the previously rejected image-generated “option 2”.

### 2.3 Visual tokens

Port the values from the reference HTML into `src/ui/theme.css`. Preserve the complete token set rather than approximating it with a framework palette.

| Token | Dark | Light |
| --- | --- | --- |
| Surface | `#202120` | `#f1eddf` |
| Primary text | `#f1eddf` | `#191912` |
| Secondary text | `#b0b0a6` | `#626256` |
| Section rules | `#56584f` | `#5e6052` |
| Row hover | `#2b2d29` | `#e5e3d5` |
| Icon background | `#efebdc` | `#191912` |
| Icon foreground | `#191b19` | `#f1eddf` |
| Soft track/background | `#32352e` | `#dbdccc` |
| Connected accent | `#c1e895` | `#285a28` |
| Borders | `#3c4037` | `#c5c6b5` |

The presentation page backgrounds `#111214` and `#e0dfd7` belong to the preview surround. The production client area uses the application surface, not a second nested mock desktop.

Use Bricolage Grotesque for the application typography and IBM Plex Mono for the uppercase section labels. Bundle the required WOFF2 fonts and their license files locally. Do not depend on Google Fonts requests at application startup. Preserve the reference weights, optical-size behavior, letter spacing, and line heights. Do not replace the distinctive headings with default system typography.

### 2.4 Home layout contract

1. The header retains the lowercase `mediacenter` wordmark and square play mark on the left.
2. The right side retains separate VPN and output blocks with a shield and a display icon. Show actual values; do not seed “United Kingdom” or “LG TV” as real device state.
3. The main area retains the personal-media column on the left and the service grid on the right.
4. The personal-media rows remain Videos, Music, Photos, Drives & USB, and VPN & Sound, in this order. Preserve their icon tiles, large bold labels, dividers, and secondary information.
5. The streaming grid remains four columns and two rows at normal desktop widths. Order: Netflix, YouTube, HBO Max, Viaplay, MTV Katsomo, Ruutu, Yle Areena, Plex.
6. Service tiles have no ordinal numbers, duplicate captions, permanent selection enlargement, or overlaid action pills.
7. Preserve the reference's approximately 1:2.1 column proportion, 36-pixel column gap, 16-pixel service gaps, 18-pixel tile radius, and 49-pixel personal-media icon blocks at the standard breakpoint. Use the exact HTML values for other dimensions.
8. Preserve the 3-pixel keyboard-focus outline with a 4-pixel tile offset and an inset hover treatment. No tile scaling or overlap.
9. Use container width for the responsive breakpoints corresponding to the reference's 1050- and 820-pixel layouts. At narrow desktop widths, a two-column service grid and vertical scrolling are allowed; logo clipping and horizontal scrolling are not.
10. Start with a 1280 × 800 logical-pixel window, clamped to the current desktop work area. Use a minimum window size of 800 × 600, also clamped for smaller work areas. Keep ordinary native Linux window decorations and resizing.
11. Restore window geometry only when it intersects a connected desktop work area. A disconnected-TV coordinate must not make the control window disappear.
12. Keep the footer's “Controls on this computer” meaning and a truthful output description. Do not add the old remote-control hint strip.

The native window frame is outside the image-matching area. Home layout must remain recognizably identical at matching content dimensions. Use container units or measured content-width breakpoints so a large desktop monitor does not unexpectedly enlarge a small application window.

### 2.5 Official logo mapping

| Service ID | Asset under `docs/design/assets/official-logos/` | Tile background |
| --- | --- | --- |
| `netflix` | `netflix.svg` | `#080808` |
| `youtube` | `youtube.png` | `#f90018` |
| `hbo_max` | `hbo-max.png` | `#46018a` |
| `viaplay` | `viaplay.svg` | `#600c41` |
| `mtv_katsomo` | `mtv-katsomo.svg` | `#4e0710` |
| `ruutu` | `ruutu.svg` | `#063f40` |
| `yle_areena` | `yle-areena.svg` | `#00796f` |
| `plex` | `plex.svg` | `#151515` |

Copy assets without editing their geometry, colors, aspect ratios, or embedded image content. Do not add CSS filters to recolor them. Keep the specific per-service sizing from the HTML. Store provenance alongside the packaged asset sources. Accessible button names contain the service name even though no second name is displayed.

### 2.6 Other screens must inherit the design

The reference defines the home screen. Videos, Music, Photos, Drives, and Settings must use the same typography, surfaces, dividers, square/rounded-rectangle icons, restrained borders, and spacing system. Keep the header and an explicit text-labelled Back action. Do not introduce an unrelated sidebar/dashboard template.

Use rectangular checkboxes, labelled buttons, conventional text menus, and progress bars. Do not use decorative circles, status balls, circular toggles, dot carousels, or an ellipsis menu icon. User filenames and metadata must be displayed faithfully even if they contain punctuation; the ban concerns app-authored decoration and copy.

Sample media and connection values are allowed only in deterministic development fixtures. A fresh production install shows truthful empty states such as “Add a video folder”, “No drives connected”, and “VPN disconnected”.

## 3. Verified starting environment

Read-only inspection on 5 November 2026 found:

| Item | Observed state |
| --- | --- |
| OS | Linux Mint 22.3, Ubuntu Noble base |
| Desktop/session | Cinnamon, X11 |
| Chrome | `/usr/bin/google-chrome`, version `157.0.8103.38` |
| NordVPN | `/usr/bin/nordvpn`, version `6.0.2` |
| mpv | `/usr/bin/mpv`, version `0.37.0` |
| FFmpeg | `6.1.1-3ubuntu5`; ffmpeg and ffprobe available |
| Audio utilities | pactl 16.1 and wpctl available |
| Display utilities | xrandr 1.5.2 and wmctrl 1.07 available |
| Application workspace | `/home/emma/Dev/Mediacenter`; design documents and assets, no implementation scaffold |
| Git | No `.git` directory in this workspace at inspection time |
| Development toolchain | node, npm, cargo, and rustc were not found on the inspected process PATH; this does not prove they are absent from the machine |

Do not infer account login, working VPN connectivity, installed codecs, HDMI connectivity, audio-server type, or working DRM from binary presence. These remain measured preflight results.

Before implementation, resolve any existing version-manager installations. Select a supported Node LTS release compatible with the chosen Vite release and a stable Rust toolchain compatible with Tauri 2. Record exact versions in `.node-version`, `rust-toolchain.toml`, `package-lock.json`, and `Cargo.lock`. Do not use floating dependency versions in release builds.

Tauri's Debian prerequisites include WebKitGTK and native development dependencies. Follow the current official prerequisites for the verified Ubuntu/Mint base rather than copying a Windows setup. The selected browser remains external to the Tauri webview. [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/)

## 4. Architecture and ownership

### 4.1 Runtime arrangement

```text
Linux computer display
  Mediacenter main window
    React views and locally bundled assets
          |
      typed commands / versioned events
          |
  Rust application services
    SQLite and source registry
    VPN coordinator ------------ installed NordVPN client
    Browser session manager ---- dedicated Chrome service profiles
    Local player --------------- owned mpv process and private IPC socket
    Photo presenter ------------ restricted second Tauri window
    Output coordinator --------- monitor adapter and audio adapter
    Storage adapter ------------ UDisks2

Optional HDMI television
  Only an owned Chrome, mpv, or photo presentation window
  Mediacenter main control window remains on the computer
```

### 4.2 Layer rules

1. React renders state and sends typed requests. It does not spawn commands, parse CLI output, traverse disks, read browser cookies, or hold VPN credentials.
2. Rust validates requests, applies policies, manages subprocesses, and owns the persistent state.
3. Integration adapters contain Linux-specific behavior. Their state machines must be testable with recorded, sanitized fixtures and fake command runners.
4. The home screen consumes a composed `HomeSnapshot`; it does not independently poll five different system utilities.
5. Services broadcast ordered events with monotonically increasing revisions. The frontend ignores old revisions and resynchronizes after reconnect or a detected revision gap.
6. A command's successful return means the defined operation completed, not that an unrelated external service is playing correctly. For asynchronous work, return an operation ID and emit progress/completion events.
7. All long work is cancellable and off the UI thread. Use bounded work queues, not one thread or child process per discovered file.

### 4.3 Process and data ownership

Use application identifier `io.emma.mediacenter` and development identifier `io.emma.mediacenter.dev`. Keep their data directories separate so fixture runs cannot overwrite the real library or browser profiles.

| Data | Location policy |
| --- | --- |
| SQLite database and durable app state | Application data directory resolved using the app identifier and XDG conventions |
| Chrome profiles | Private `browser-profiles/<service-id>/` directories under app data |
| Thumbnail cache | Application cache directory, bounded by an LRU budget |
| Logs | Application local state/log directory with rotation |
| mpv IPC sockets | Private per-session directory under `XDG_RUNTIME_DIR`; mode 0700 directory and a user-only socket |
| Temporary probing and image work | Private cache/temp subdirectories, cleaned after cancellation |

Do not put runtime data in the repository. Do not use Chrome's ordinary default profile. Chromium documents `--user-data-dir` as the supported profile-root override; separate profile roots are the basis for isolation here. [Chromium user-data directory documentation](https://chromium.googlesource.com/chromium/src/+/main/docs/user_data_dir.md)

Use one persistent profile per service. This makes session ownership and failure isolation easier to establish and preserves each service's login. The first release allows one managed streaming session at a time. Switching services asks the user to close the existing managed session or keep it and cancel the switch; do not silently stop an active film. Local music and a photo slideshow may coexist. Starting local video while another managed audiovisual session exists requires an explicit switch.

### 4.4 Proposed source tree

All paths below are relative to `/home/emma/Dev/Mediacenter`. This tree is a plan, not a claim that these files already exist.

```text
package.json
package-lock.json
.node-version
rust-toolchain.toml
vite.config.ts
tsconfig.json
README.md
public/
  logos/                         exact copies of official assets and provenance
  fonts/                         local fonts and license files
src/
  main.tsx
  app/App.tsx                    routing and top-level recovery
  app/controller.ts              command/event synchronization
  app/contracts.ts               generated IPC types
  app/backend.ts                 typed invoke/listen wrapper
  ui/theme.css                   reference tokens and typography
  ui/layout.css                  reference layout and responsive rules
  ui/Icons.tsx                   reference-derived generic icons
  ui/AppShell.tsx
  ui/StatusMessage.tsx
  features/home/HomeScreen.tsx
  features/home/ServiceTile.tsx
  features/home/PersonalMediaRow.tsx
  features/home/homeModel.ts
  features/vpn/VpnSettings.tsx
  features/streaming/StreamingSession.tsx
  features/outputs/OutputSettings.tsx
  features/library/SourcePicker.tsx
  features/library/MediaBrowser.tsx
  features/videos/VideosScreen.tsx
  features/music/MusicScreen.tsx
  features/photos/PhotosScreen.tsx
  features/photos/PresentationScreen.tsx
  features/drives/DrivesScreen.tsx
  features/settings/SettingsScreen.tsx
  features/player/LocalPlayerControls.tsx
  testing/fixtures.ts            development/test build only
src-tauri/
  Cargo.toml
  Cargo.lock
  build.rs
  tauri.conf.json
  capabilities/main.json
  capabilities/presentation.json
  permissions/                   explicit permissions for custom commands
  src/main.rs
  src/lib.rs
  src/contracts.rs
  src/commands.rs
  src/events.rs
  src/app_state.rs
  src/process/runner.rs
  src/process/registry.rs
  src/process/paths.rs
  src/storage/database.rs
  src/storage/migrations.rs
  src/storage/repositories.rs
  src/vpn/nordvpn.rs
  src/vpn/parser.rs
  src/vpn/policy.rs
  src/streaming/catalog.rs
  src/streaming/browser.rs
  src/streaming/sessions.rs
  src/outputs/monitors.rs
  src/outputs/x11.rs
  src/outputs/audio.rs
  src/outputs/coordinator.rs
  src/library/sources.rs
  src/library/scanner.rs
  src/library/probe.rs
  src/library/thumbnails.rs
  src/library/query.rs
  src/player/mpv.rs
  src/player/ipc.rs
  src/player/resume.rs
  src/music/queue.rs
  src/photos/presenter.rs
  src/devices/udisks.rs
  src/devices/registry.rs
  src/lifecycle.rs
  src/diagnostics.rs
  migrations/0001_initial.sql
  tests/                         integration and adapter tests
tests/
  unit/
  visual/
  e2e/
  fixtures/
scripts/
  verify-assets.mjs
  verify-ui-copy.mjs
docs/
  design/                        existing references remain intact
  engineering/preflight.md
  engineering/streaming-compatibility.md
  engineering/architecture-decisions.md
  engineering/verification.md
  engineering/packaging.md
  superpowers/plans/             this document
```

Keep a file focused on its listed responsibility. Do not create a generic plugin framework, provider abstraction for hypothetical future VPNs, or a second backend service.

## 5. Shared contracts and persistence

### 5.1 Core data types

Define the Rust DTOs in `src-tauri/src/contracts.rs` and generate matching TypeScript declarations into `src/app/contracts.ts` using a pinned type-generation tool compatible with the chosen Serde representations. Add a consistency check so hand-written frontend types cannot silently drift.

```typescript
type ServiceId = 'netflix' | 'youtube' | 'hbo_max' | 'viaplay'
  | 'mtv_katsomo' | 'ruutu' | 'yle_areena' | 'plex';
type Theme = 'dark' | 'light';
type MediaKind = 'video' | 'audio' | 'photo';
type SourceId = string;
type MediaId = string;
type SessionId = string;
type OperationId = string;
type MonitorId = string;
type AudioSinkId = string;

type VpnState = 'unavailable' | 'unauthenticated' | 'disconnected'
  | 'connecting' | 'connected' | 'disconnecting' | 'unknown' | 'error';
interface VpnSnapshot {
  revision: number;
  state: VpnState;
  country: string | null;
  killSwitch: 'enabled' | 'disabled' | 'unknown';
  observedAtMs: number;
  errorCode: string | null;
}
interface PlaybackTarget {
  monitorId: MonitorId | null; // null = this computer's current monitor
  audioSinkId: AudioSinkId | null; // null = current default, without changing it
}
interface OutputSnapshot {
  revision: number;
  desired: PlaybackTarget;
  effective: PlaybackTarget;
  state: 'ready' | 'partial' | 'missing' | 'unavailable';
  label: string;
}
interface MediaItem {
  id: MediaId;
  sourceId: SourceId;
  kind: MediaKind;
  displayName: string;
  availability: 'available' | 'offline' | 'missing' | 'unreadable';
  durationMs: number | null;
  thumbnailId: string | null;
}
interface ManagedSession {
  id: SessionId;
  kind: 'streaming' | 'local-video' | 'local-audio' | 'slideshow';
  serviceId: ServiceId | null;
  mediaId: MediaId | null;
  phase: 'starting' | 'ready' | 'playing' | 'paused' | 'ended' | 'error';
  target: PlaybackTarget;
}
interface OperationRef { operationId: OperationId }
interface AppError {
  code: string;
  message: string;
  retryable: boolean;
  operationId: OperationId | null;
}
```

For streaming, `ready` means the browser session is available. Do not emit `playing` or infer a title/time unless an actual supported integration supplies that evidence.

`HomeSnapshot` contains `revision`, `theme`, `vpn: VpnSnapshot`, `outputs: OutputSnapshot`, `sessions: ManagedSession[]`, local media summaries, drive summaries, and the eight service definitions. Empty summaries have explicit zero/empty values; never invent demonstration media in production.

Raw filesystem paths and process identifiers stay in Rust. Resolve user-facing IDs through repositories. Linux filenames that are not valid UTF-8 are retained as byte paths in the backend and shown with a safe display representation; they must not be round-tripped through lossy text.

Additional contract decisions: `SourceGrantId`, `DriveId`, `VolumeId`, and `PlaylistId` are opaque string IDs. A source grant is created only by the backend's native picker, expires after 10 minutes, and is consumed once. `MediaPage` contains `items: MediaItem[]` and `nextCursor: string | null`. A `CountryOption` contains provider-validated `id` and human-readable `name`. `ServiceDefinition` contains `id`, `name`, `logoAsset`, `tileColor`, and `vpnRequired`; the browser URL stays in the backend registry.

`MonitorDescriptor` contains `id`, `name`, `connector`, `primary`, `connected`, `scaleFactor`, and signed physical-pixel bounds `{x, y, width, height}`. `AudioSinkDescriptor` contains `id`, `name`, `available`, and the stable backend sink name. Persist stable identities and recompute transient coordinates and numeric sink-input IDs.

### 5.2 Commands and events

All commands return `Result<T, AppError>`. Async mutations return `OperationRef` unless their completion is immediate and local. Specify the corresponding Rust snake_case and generated frontend camelCase mapping once in the bridge.

| Command | Input | Output and behavior |
| --- | --- | --- |
| `get_home_snapshot` | none | `HomeSnapshot` |
| `set_theme` | `Theme` | persisted `Theme` |
| `refresh_vpn` | none | fresh `VpnSnapshot` |
| `list_vpn_countries` | none | validated country IDs and display names |
| `connect_vpn` | optional validated country ID | `OperationRef` |
| `disconnect_vpn` | none | `OperationRef`; explicit user action only |
| `set_vpn_requirement` | service ID, required boolean | saved service preference |
| `enable_kill_switch` | none | `OperationRef`; only from its explicit settings action |
| `open_service` | `ServiceId` | `OperationRef`; applies VPN policy before launch |
| `focus_session` | `SessionId` | completion status after requesting focus |
| `move_session` | session ID, `PlaybackTarget` | `OperationRef` |
| `close_session` | `SessionId` | `OperationRef`; only owned session |
| `list_outputs` | none | discovered monitors and audio sinks |
| `select_output` | `PlaybackTarget` | `OutputSnapshot`; changing preference alone does not move a running session |
| `choose_media_source` | optional desired media kind | opens native directory picker in backend and returns source grant |
| `add_media_source` | source grant, selected kinds | `SourceId`; does not accept an arbitrary frontend path |
| `remove_media_source` | `SourceId` | removes library association only, never disk files |
| `rescan_source` | `SourceId` | `OperationRef` |
| `query_media` | kind, optional source ID, query, cursor, limit | cursor page of `MediaItem`; limit at most 100 |
| `play_media` | media ID, target, resume choice | `OperationRef` |
| `control_player` | session ID, typed player action | accepted player state |
| `save_playlist` | optional playlist ID, name, ordered media IDs | playlist ID |
| `list_playlists` | none | playlist summaries with ID, name, and item count |
| `get_playlist` | playlist ID | name and ordered media IDs with availability |
| `remove_playlist` | playlist ID | removes playlist metadata only, never media files |
| `play_playlist` | playlist ID, target, start index | `OperationRef` |
| `get_music_queue` | none | current ordered queue, current index, shuffle flag, and repeat mode |
| `control_music` | session ID, typed queue action | updated queue state |
| `start_slideshow` | bounded media IDs, target, interval | `OperationRef` |
| `control_slideshow` | session ID, next/previous/pause/resume/stop | slideshow state |
| `list_drives` | none | drive and volume summaries |
| `mount_volume` | discovered volume ID | `OperationRef` |
| `eject_drive` | discovered drive ID | `OperationRef`; preflight for active use |
| `cancel_operation` | `OperationId` | cancellation acknowledgement |

Player actions are a tagged enum: pause, resume, stop, seekTo with nonnegative milliseconds, setVolume with an integer from 0 to 100, setMuted, selectAudioTrack, selectSubtitleTrack, and setFullscreen. Do not accept arbitrary mpv command arrays from JavaScript.

Music queue actions are next, previous, setShuffle with a boolean, setRepeat with `off`/`one`/`all`, enqueue with media IDs, removeAt with an index, and moveItem with source/destination indices. Validate indices against the current queue revision; reject a stale edit rather than changing the wrong item. Limit a saved playlist, queue, or slideshow request to 10,000 items and a playlist name to 200 Unicode characters. Persist the original queue order separately so turning shuffle off is predictable.

Cancelling an operation cancels its pending work and cleans up resources created by that incomplete operation. It does not close an unrelated completed session. Once a session exists independently, stop it with its typed session/player command.

Events: `home_changed`, `vpn_changed`, `outputs_changed`, `devices_changed`, `library_progress`, `session_changed`, and `operation_finished`. Each includes a revision or operation ID as appropriate. Progress events contain counts and status text, not filesystem paths or command-line dumps.

On application load, subscribe before requesting the initial snapshot; buffer events until the snapshot revision is known. Unsubscribe on teardown. A failed operation must always reach a terminal state and re-enable its controls.

### 5.3 SQLite schema

Use migrations, foreign keys, prepared statements, WAL mode, and a single bounded database worker. Keep slow probing and filesystem I/O outside database transactions.

| Table | Essential columns and constraints |
| --- | --- |
| `settings` | `key TEXT PRIMARY KEY`, validated `value_json TEXT`, `updated_at_ms INTEGER` |
| `service_preferences` | `service_id TEXT PRIMARY KEY`, `vpn_required INTEGER`, optional preferred country, optional validated last output identifiers |
| `media_sources` | UUID primary key, source type, raw local-root bytes or volume identity plus relative-root bytes, label, kinds, last seen and scan timestamps, availability |
| `media_items` | UUID primary key, source FK, relative path BLOB, display name, kind, size as 64-bit integer, mtime in nanoseconds, duration, dimensions, metadata JSON, availability; unique `(source_id, relative_path)` |
| `resume_positions` | media FK primary key, position in milliseconds, last-known duration, update timestamp, completed flag |
| `playlists` | UUID primary key, user name, creation/update timestamps |
| `playlist_items` | playlist FK, stable ordinal, media FK; primary key `(playlist_id, ordinal)` |
| `thumbnail_entries` | cache key primary key, media FK, source size/mtime fingerprint, relative cached filename, byte count, last access timestamp |

Do not store passwords, tokens, browser cookies, raw VPN diagnostic output, or whole browser history in SQLite. The browser owns its own authenticated profile.

Do not treat filename alone as identity. A changed file at the same path invalidates its old thumbnail and may invalidate resume data. An offline volume does not imply that its files were deleted. Mark missing files only after a complete successful scan of a source that remained available throughout that scan.

Create a consistent database backup before a schema migration. If migration fails, keep the previous database and show a recovery path. Do not silently replace it with an empty library.

## 6. Streaming and VPN behavior

### 6.1 Service registry

Keep the registry in Rust and expose read-only service definitions to the UI. Launch only these initial HTTPS destinations. Revalidate official regional entry points during the compatibility milestone; do not infer a playable region from a domain.

| Service | Initial URL |
| --- | --- |
| Netflix | `https://www.netflix.com/` |
| YouTube | `https://www.youtube.com/` |
| HBO Max | `https://www.hbomax.com/` |
| Viaplay | `https://viaplay.fi/` |
| MTV Katsomo | `https://www.mtv.fi/` |
| Ruutu | `https://www.ruutu.fi/` |
| Yle Areena | `https://areena.yle.fi/` |
| Plex | `https://app.plex.tv/` |

Redirects within Chrome are normal browser behavior. The Tauri shell must never load these sites into its privileged webview. User-entered arbitrary commands, Chrome flags, and service URLs are not a first-release settings feature.

### 6.2 Managed browser launch

1. Resolve the installed Chrome executable and verify it is an ordinary executable. Prefer the recorded system path; allow a native file-picker-based executable choice only if necessary.
2. Validate the service ID and current operation state.
3. Apply the VPN launch policy using a fresh provider observation.
4. Resolve the service's private profile directory and detect an existing managed session or profile lock.
5. Start Chrome using an argument vector equivalent to `--user-data-dir=<owned-profile>` and `--app=<registry-url>`. Do not invoke a shell. Probe app-window behavior on the installed version before relying on it.
6. Initially place the browser on the computer so the user can sign in and select content. Its login/cookie/subscription prompts belong to the browser.
7. Register the actual owned process identity and window identity. A short-lived launcher PID is not necessarily the lasting browser PID.
8. Publish a `ready` streaming session when the window exists. If ownership is ambiguous, offer manual browser use and do not issue window-control commands to guessed targets.
9. Repeated activation focuses the managed window instead of opening duplicates. Serialize operations per service and at the active-streaming-session level.

Never use `--no-sandbox`, disabled certificate checks, remote debugging ports, embedded login forms, or profile copying from the user's ordinary Chrome session. Do not use browser automation as the production streaming implementation.

On a busy profile, explain that its window may already be open. Do not delete Chrome lock files while any owner might still be alive. Closing Mediacenter does not delete profiles or log the user out.

### 6.3 VPN launch policy

The default for the eight services is `vpn_required=true`. A user can deliberately change a service to use the current connection in settings; this is useful if they later choose direct playback or a local Plex setup. Do not silently disable VPN requirements because a service fails to load.

For a VPN-required launch, require all of the following: a fresh successful NordVPN observation, `state=connected`, and a verified enabled provider Kill Switch. This is a conservative launch policy selected by this plan. If the Kill Switch is off, present an explicit “Enable NordVPN Kill Switch” setup action with a plain explanation that it affects the computer's network access outside Mediacenter. Do not enable it silently.

NordVPN's installed CLI exposes status, settings, country selection, connection commands, and Kill Switch configuration. Use the installed version's help as the syntax authority and keep parsing fixtures for that version. [NordVPN Linux command documentation](https://support.nordvpn.com/hc/en-us/articles/20196094470929-How-to-install-the-NordVPN-app-on-Linux-distributions)

The app's launch gate is not a firewall. NordVPN performs network enforcement. A connected label or polling loop alone cannot guarantee that packets remain inside the tunnel. On an observed disconnect, block new required launches and report the connection loss; do not claim the application instantly paused or secured every browser request.

### 6.4 VPN state machine and timing

1. Detect missing executable, missing daemon access, unauthenticated state, disconnected state, in-progress operations, connected state, unrecognized output, and timeout separately.
2. Set `LC_ALL=C` for controlled CLI invocations where supported. Parse ANSI-stripped output and tolerate extra fields. Unknown or contradictory output yields `unknown`, never `connected`.
3. Poll every 5 seconds when no managed streaming session is active, every 2 seconds while one is active, and every 1 second during a connection transition. Allow only one polling command at a time.
4. A status observation older than 10 seconds is stale for display purposes. Before launching a required service, perform a fresh check; do not rely solely on the last background poll.
5. Bound status/settings commands to 5 seconds. Bound a connect operation to 60 seconds. If the CLI times out, reconcile with a new status query because the daemon may still be connecting.
6. Validate country choices against the provider's discovered country list. Pass the selected country as a single validated argument, never a shell fragment.
7. Disable repeated Connect/Disconnect actions while their transition is unresolved. Do not queue contradictory requests indefinitely.
8. An app exit must not disconnect a VPN connection or disable Kill Switch. The user may rely on that connection elsewhere.
9. Do not silently change protocol, split tunneling, LAN discovery, DNS, allowlists, auto-connect, or authentication settings.

Country information reports NordVPN's observed state. It is not a claim about the catalogue or licensing region a service will offer.

### 6.5 Compatibility gate

Create `docs/engineering/streaming-compatibility.md` with one row for each service and columns for test date, browser version, account/subscription prerequisite, login, actual playback, VPN country, observed service response, computer playback, HDMI playback, audio, subtitles, and measured quality if the service exposes it.

Use `not tested`, `passed`, `failed`, and `requires account` honestly. Do not mark success from a homepage, screenshot, or a non-DRM trailer alone. User account interaction and any challenge remain manual.

Netflix's own documentation describes browser/OS requirements and restrictions associated with VPN use; these are a concrete reason to make real playback a pre-implementation gate, not to promise arbitrary catalogue access or 4K. Apply the same measurement discipline to all eight services. [Netflix browser requirements](https://help.netflix.com/en/node/30081), [Netflix and VPN use](https://help.netflix.com/en/node/114701)

If a required service fails because of its Linux/DRM/VPN support, record the limitation and assess it before spending time on the rest of the application. Do not hide it behind an “Open” button or attempt DRM workarounds.

## 7. HDMI, window placement, and audio

### 7.1 Desktop and television model

Support an extended desktop: the main control window stays on the computer, and an owned playback window can move to the TV. Mirroring is an operating-system setting and does not provide independent control and playback surfaces.

Track `controlMonitorId` separately from the playback target. Prefer the built-in computer display when identifiable; on a desktop with multiple external displays, use the monitor where the user establishes the main window and allow an explicit “Computer display” selection in output settings. Do not assume that the OS primary display is always the computer or that every external display is a TV. Automatic geometry restoration must keep the main window on the resolved control monitor; only the user may deliberately move it elsewhere. If that monitor is unavailable, choose a reachable remaining monitor and report the fallback.

Detect monitors by stable output/EDID-derived identity where available, with a connector-based fallback. Store a chosen output identity, not “monitor index 1”. Re-resolve geometry at use time. Handle negative coordinates, mixed scaling, and output reconnection.

Use the current X11 session as the first supported target. Tauri monitor APIs can enumerate displays and place the app's own windows; an external Chrome/mpv window requires a separate X11/EWMH adapter. Window fullscreen and window-manager placement are requests, not proof that a website's video element entered fullscreen. [EWMH application window properties](https://apol.pages.freedesktop.org/xdg-specs/wm-spec/latest/ar01s05.html)

### 7.2 Streaming-to-TV workflow

1. Open the selected service on the computer.
2. Let the user select a title and start playback in the service browser.
3. From Mediacenter's current-session view, “Move playback to TV” moves only that owned browser window to the selected output and requests window fullscreen.
4. Keep “Bring back to computer” available in the control application. Restore the previous computer geometry and focus the service window when requested.
5. Do not inject script to press a website's fullscreen or play button. Browser-level fullscreen and a site's video fullscreen are different capabilities; document measured behavior.
6. If moving or focusing is denied, report it and retain the session. Do not move the main application instead.

Moving a browser window to the TV also moves that service's controls. The user can interact with that window through the computer's mouse/keyboard or bring it back. The main Mediacenter interface remains on the computer throughout. Do not imply that two independent views of a single service browser are available.

### 7.3 Ownership-safe window handling

Resolve an X11 window by a verified process identity, start time, owned profile/session association, and observed window properties. Never identify a window solely by title text such as “Netflix”. Recheck identity before close/move operations so PID and window-ID reuse cannot target another application.

For the first implementation, wrap installed `wmctrl` with argument arrays and numeric verified window IDs. Parse read-only monitor/window information in the adapter. If the preflight cannot establish stable ownership, use an X11 library in that adapter rather than broadening matching rules.

Do not launch a global window manager, change desktop workspaces, alter screen resolutions, or reset the user's display configuration.

### 7.4 Audio routing

Detect the actual audio server. On this Mint target, first support PulseAudio-compatible routing through `pactl --format=json` where available, including PipeWire's PulseAudio compatibility service. Use `pactl subscribe` for changes and resynchronize snapshots after events. Keep parsing and routing in one adapter.

Enumerate stable sink names and their availability. Selecting HDMI picture does not prove an HDMI audio sink exists. Maintain desired and effective picture/audio outputs independently, and show a partial-result state when only one succeeds.

For mpv, select an explicitly enumerated `audio-device` when the backend supports the desired sink. For a managed Chrome session, associate sink inputs with the verified service process tree and move only those streams. New audio streams need to be routed when they appear. If reliable association cannot be demonstrated in preflight, show the limitation and offer the desktop sound settings; do not reroute all Chrome audio.

Changing a system default affects other applications and must be a separate, explicit action if added later. It is not the fallback for uncertain per-session routing. `wpctl set-default` also represents a default-device operation, not a substitute for moving an already identified stream. [WirePlumber wpctl documentation](https://pipewire.pages.freedesktop.org/wireplumber/man/wpctl.html)

On TV/audio removal, pause local video or music if its selected sink disappears, mark the target missing, and let the user resume on computer output. Recover the local playback window to the computer. For Chrome, attempt to recover the owned window and report the audio change; do not promise service playback was paused without an actual supported control channel.

## 8. Local media and external storage

### 8.1 Source selection and discovery

The user chooses folders through a native directory picker. Offer standard Videos, Music, and Pictures folders as choices, but do not scan the entire home directory automatically. Show the selected roots and accepted media kinds before scanning.

USB-A and USB-C describe connection types, not different media-library implementations. Use the filesystem and drive exposed by Linux. Do not invent a USB-C label if the operating system cannot identify the physical connector reliably. Use accurate drive labels and capacities instead.

Use UDisks2 for block devices, filesystems, mount points, mount/unmount, and power-off where supported. The mount point is supplied by the OS and can change; do not hard-code `/media/emma/<label>`. [UDisks2 filesystem interface](https://storaged.org/udisks/docs/gdbus-org.freedesktop.UDisks2.Filesystem.html)

Identify removable sources by filesystem UUID plus relative source path, with a documented additional device discriminator when needed. Detect duplicate UUIDs on simultaneously attached cloned volumes and ask the user to choose; never silently bind to the wrong disk. Sources without a stable UUID require a conservative device fingerprint and explicit reattachment when ambiguous.

### 8.2 Scanning and metadata

1. Scan only selected roots. Do not follow directory symlinks outside a root, recurse into symlink loops, or traverse unrelated mounts automatically.
2. Treat extension matching as candidate selection. Probe media before declaring it playable.
3. Video candidates include MP4, MKV, MOV, WebM, AVI, and M4V; audio candidates include MP3, FLAC, M4A, AAC, OGG, Opus, and WAV; photo candidates include JPEG, PNG, WebP, and common HEIF/AVIF variants when the installed decoder supports them.
4. Unsupported decoders produce an explanatory item state, not a crash or a promise that every extension is supported.
5. Use ffprobe with bounded JSON output, a maximum of two concurrent probes, and a 15-second default timeout per file. Stop work for an unavailable source. Probe duration, streams, dimensions, and a bounded set of useful tags.
6. Enumerate files in batches, write metadata in bounded transactions, and publish progress no faster than four times per second.
7. On an incremental scan, compare raw path identity, size, and modification time before probing again. Do not hash entire large videos by default.
8. Keep scans cancellable. Never mark unseen files missing when the scan was cancelled, partially denied, or interrupted by unplugging.
9. Generate thumbnails at at most 640 pixels on the long edge, with one thumbnail worker and a default 512 MiB LRU cache. Bound decoding time and dimensions; reject pathological files safely.
10. Cache keys include source identity, media identity, size, mtime, and thumbnail settings. Store cached images under app cache only.

Do not load SVG or HTML files as executable media documents inside the privileged UI. Photo presentation accepts supported raster formats and decoded safe image output. Serve library images through a restricted media-ID-based resource handler that checks source grants; do not enable unrestricted filesystem URLs. Render filenames, tags, and error messages as escaped text, never interpreted HTML.

Keep ffprobe/ffmpeg/mpv restricted to the resolved local input and authorized adjacent resources. Disable external-reference following and URL extraction where supported by the installed tool. Add a fixture that refers to an HTTP URL and verify that local indexing/playback does not fetch it. Validate the exact supported local-only protocol options during Task 01 rather than assuming an arbitrary flag is portable across installed versions.

### 8.3 Videos

The Videos screen offers local search, source selection, title/filename, duration when known, availability, and resume state. It uses the same design system and a virtualized list or restrained thumbnail grid for large libraries.

Selecting a video offers Resume when a valid unfinished position exists and Play from start. Save progress every 5 seconds, on pause, on a seek completion, and on normal stop. Do not block playback on a database write.

A position is resumable only when at least 10 seconds have been watched and the file is not completed. Treat it as completed at natural end-of-file or at 95 percent of duration. Unknown-duration media has no percentage completion rule. Invalidate or clamp saved positions when the underlying file changes or duration shrinks; do not seek past the new end.

Use mpv in a separate owned playback window, not embedded video inside the home screen. The app's main window holds the controls and remains on the computer.

### 8.4 mpv integration

Use a dedicated mpv process with application-controlled configuration, user scripts disabled, URL extraction disabled, and a private Unix-domain JSON IPC socket. Send structured commands and observe responses/events by request ID; never send a shell command string. The public mpv manual documents this IPC mechanism and properties such as playback time, pause, tracks, and audio-device selection. Validate features against installed mpv 0.37.0 rather than assuming every option in a newer manual exists. [mpv manual](https://mpv.io/manual/stable/)

Application-specific choices:

1. One local audio/video player session at a time. A new play request resolves the existing session explicitly.
2. Spawn with app-owned configuration and no user Lua script loading. Accept local resolved files only; do not accept remote URLs or arbitrary playlist references through the media API.
3. Connect IPC with a 5-second startup deadline. Limit incoming message size to 1 MiB and reject malformed responses without crashing the UI.
4. Use a monotonically increasing request ID and a 5-second command deadline. Process unsolicited events independently from command replies.
5. Wait for the file-loaded event before applying resume position or track selection. Subscribe to time, duration, pause, idle/end, track list, and volume state.
6. Emit position updates to the UI at no more than four times per second. Store resume position on the separate persistence schedule.
7. On an unsupported file or codec error, identify the affected media item, preserve the library, and offer another file.
8. On normal stop, request player shutdown and wait up to 3 seconds. If necessary, terminate only the verified owned process. Clean its socket directory after exit.
9. On app crash recovery, do not trust a stored PID alone. Verify identity before reconnecting or terminating; otherwise treat the old session as unavailable.

Pass the initial media filename as a native OS-string argument after an option terminator. JSON IPC cannot preserve arbitrary non-UTF-8 path bytes. For a file whose path cannot be represented losslessly in JSON, restart the owned player with that native path argument when changing files, then reattach IPC and restore the queue/output state. Do not replace bytes with the Unicode replacement character and play a different path. Gapless music playback is not required by this release.

Provide Play/Pause, Stop, seek bar with a rectangular thumb, elapsed/duration text, volume, mute, audio-track selection, subtitle selection, playback target, and Bring back to computer. Load adjacent local subtitles only within the selected source; a subtitle file outside it requires a native picker grant.

### 8.5 Music

Music uses the same library and mpv engine. Show available title/artist/album tags with filename fallback. Provide local search, an ordered queue, next/previous, shuffle, repeat off/one/all, and named local playlists. Represent these states with text or appropriate non-dot icons.

Persist playlist identity and order. Keep an offline item in a playlist as unavailable; skip it during playback with a clear message rather than silently deleting it. A queue mutation does not reorder the original playlist unless the user explicitly saves it. Music continues during navigation within Mediacenter.

### 8.6 Photos and slideshows

Show a thumbnail browser with source filter and filename/date information. Decode orientation correctly. Open an individual image in a viewer with Fit and Actual size choices; never stretch or crop by default.

Run a slideshow in a separate Tauri presentation window on the chosen output. The main window provides previous/next, pause/resume, stop, and interval. Default interval is 5 seconds; permit integers from 1 to 60 seconds. Default to source order, with an optional explicit shuffle action. Preload at most the next two decoded images and release previous full-size buffers.

The presentation window has fewer capabilities than the main window: it can receive its assigned images and slideshow state and report readiness, but cannot launch processes, browse arbitrary files, control VPN, or mount devices.

On source removal, stop the slideshow cleanly. On TV removal, pause and offer to continue on the computer. Do not silently advance through hundreds of missing images.

### 8.7 Drives and safe removal

The Drives & USB screen lists discovered drives/volumes, accurate capacity, mount status, library association, and active use. Offer Mount, Browse, Add folder to library, and Safely remove only when applicable.

Before ejecting a drive, check every associated volume for active playback, slideshow images, thumbnail work, and scans. Ask to stop that application's work first. Close the application's open handles, unmount all relevant mounted filesystems, then request drive power-off only if supported and appropriate. UDisks2 notes that power-off may affect other drives on a shared physical device, so represent the whole device and its volumes rather than pretending one partition is always independent. [UDisks2 drive interface](https://storaged.org/udisks/docs/gdbus-org.freedesktop.UDisks2.Drive.html)

Let desktop Polkit handle any required OS authentication. Do not store a sudo password, invoke a password prompt in a shell, force-unmount a busy volume, or report safe removal before the operation succeeds. If another application holds the drive busy, report that result and leave it mounted.

## 9. Reliability, privacy, and operational behavior

### 9.1 Narrow native permissions

Use explicit Tauri capabilities for the main and presentation windows. Register custom commands in the app manifest and permission configuration so the presentation window cannot invoke main-window operations merely because both share the process. Tauri documents that ordinary registered app commands need explicit restriction when finer command permissions are required. [Tauri capabilities](https://v2.tauri.app/security/capabilities/)

Use locally bundled scripts, styles, fonts, and logos. Adopt a release CSP that allows only the application's required local resources and restricted media handler; no arbitrary remote frames, remote scripts, or production `unsafe-eval`. Keep development-server allowances separate from release configuration.

### 9.2 Subprocess runner contract

`CommandRunner` accepts a typed executable identity, argument vector, bounded environment additions, timeout, cancellation token, and output limit. The caller cannot submit arbitrary shell source.

Use `std::process::Command`/Tokio equivalents, not `sh -c`. Reject invalid identifiers before resolving paths. Preserve filenames as native OS strings. Cap ordinary command output at 1 MiB, with a separately bounded 4 MiB allowance for large discovery snapshots. Drain stdout and stderr concurrently to avoid deadlocks.

On timeout, terminate and reap only the command process group that Mediacenter created. Long-lived Chrome and mpv sessions use the process registry rather than this short-command timeout path. Child ownership includes PID, start time, executable identity, and session association.

### 9.3 Application lifecycle

Use single-instance behavior: launching Mediacenter again raises the existing control window. Do not create a second scanner or duplicate player owner.

Closing while local playback, music, or a slideshow is active offers Keep open or Stop playback and quit. Closing while only a managed streaming window is open offers Leave service open and quit, Close service and quit, or Cancel. Default to the least disruptive choice and remember only an explicit user preference. Never disconnect VPN on app exit.

On startup, recover persisted library/settings state and discover actual current device/output states. Previous session records are evidence to reconcile, not instructions to auto-start a film or reopen all services.

On suspend/resume, discard stale connection observations, rescan output/device identities, and recheck active IPC connections. Do not show the last pre-suspend VPN state as current.

### 9.4 Logs and diagnostics

Use structured local logs with component name, operation ID, event, duration, and error code. Redact credentials, browser profile contents, full CLI diagnostics, public IP addresses, and full media paths by default. A display name is enough for normal user-facing errors.

Rotate at 5 MiB per file with three retained files. Include a local Diagnostics screen that reports dependency versions and capability availability. Any diagnostic export is an explicit action with a preview of what it contains; do not upload it automatically.

### 9.5 Performance targets

These are implementation acceptance targets, to be measured on the user's computer rather than claimed as existing performance.

| Operation | Target |
| --- | --- |
| Warm application launch to responsive home | At most 2 seconds, excluding asynchronous service discovery |
| Theme switch | No restart, no light/dark flash on subsequent launch |
| Library search over 10,000 indexed items | First page within 300 ms after query debounce under normal local-disk conditions |
| Main UI during scanning | Remains interactive; no synchronous disk traversal on the rendering thread |
| Idle background work | No directory-wide rescans or busy polling loops |
| Drive/output change reflected in UI | Within 2 seconds of a received OS event, with periodic reconciliation fallback |
| Large source scan | Bounded concurrency and memory; progress and cancellation remain usable |

Do not optimize by removing the reference typography, replacing logos with text, or deleting essential error states.

## 10. Implementation sequence

Each task has a concrete deliverable, file ownership, interfaces, and acceptance checks. Preserve this dependency order. Review and commit coherent changes if a Git repository has been initialized; do not assume the existing folder already has version control. The commands below are for future implementation, not commands executed while writing this plan.

### Task 01: Prove the core viewing path before building the application

**Files:** Create `docs/engineering/preflight.md`, `docs/engineering/streaming-compatibility.md`, and `docs/engineering/architecture-decisions.md`.

**Consumes:** Current machine, installed applications, user's available service accounts, and Section 6's compatibility matrix.

**Produces:** Recorded `CapabilityReport` decisions covering browser DRM playback, VPN parser inputs, X11 window ownership, HDMI picture/audio, and installed mpv options. No application scaffold is required for this task.

- [ ] Record OS/session, package versions, toolchain availability, audio-server type, display identities, and dependency paths without collecting account secrets.
- [ ] Verify one real protected streaming title in the installed Chrome, first on the computer and then with the intended VPN/HDMI setup. Record the exact service, observed result, and limitations.
- [ ] Exercise all eight services enough to fill their matrix cells honestly; leave account-dependent tests marked unavailable until the user can complete them.
- [ ] Prove that a dedicated Chrome profile can coexist with the ordinary browser, retain login, be identified without title-only matching, and be returned from TV to computer.
- [ ] Prove per-session audio routing and monitor recovery with the actual HDMI connection. Distinguish unavailable hardware from a passed test.
- [ ] Capture sanitized NordVPN 6.0.2 output fixtures for connected, disconnected, unauthenticated/daemon-error, settings, and country-list cases. Do not change the user's network merely to manufacture an error fixture; synthetic fixtures can cover unobserved failures and must be labelled.
- [ ] Play a small known local video and audio fixture with mpv and verify required IPC capabilities.
- [ ] Record go/no-go findings. If the primary viewing use case fails for reasons outside the application, surface that result before continuing. Do not replace it with a claim that the launcher works.

**Acceptance:** The evidence separates tested playback from assumptions and identifies whether automatic browser placement and per-session audio routing are feasible on this machine. No Color Dock or Remote changes.

### Task 02: Establish the application shell and reproducible toolchain

**Files:** Create root toolchain/build files, `src/main.tsx`, `src/app/App.tsx`, `src-tauri/src/main.rs`, `src-tauri/src/lib.rs`, `src-tauri/tauri.conf.json`, initial capability files, and `README.md`.

**Consumes:** Task 01 platform findings and the fixed stack decision.

**Produces:** A Tauri application window with bundled local React content and documented build/test commands.

- [ ] Resolve or install the required development toolchain through the normal authorized development setup; record exact versions and native packages. Do not reinstall existing runtime applications unnecessarily.
- [ ] Initialize a local Git repository if still absent, with generated output, media caches, logs, browser profiles, and secrets excluded. Preserve all existing design files.
- [ ] Scaffold the smallest Tauri 2 + React + TypeScript application with Vite and strict TypeScript checking. Configure the app identifier, native window frame, initial size, and minimum size from Section 2.
- [ ] Add npm scripts `dev`, `build`, `typecheck`, `lint`, `test`, `test:visual`, `test:e2e`, `verify:assets`, `verify:ui-copy`, and `tauri`. Document the exact corresponding commands.
- [ ] Add Rust formatting, linting, and test entry points. Separate development fixtures from release builds.
- [ ] Run `npm run build`, `npm run typecheck`, and `cargo check --manifest-path src-tauri/Cargo.toml`; then launch the native app and verify that it is an ordinary resizable Linux window.

**Acceptance:** A release build uses bundled resources and starts without a development server. No media or VPN mutation is performed by opening the shell.

### Task 03: Reproduce the approved home screen and both themes

**Files:** Create `src/ui/*`, `src/features/home/*`, `src/testing/fixtures.ts`, `public/logos/*`, `public/fonts/*`, `tests/visual/home.spec.ts`, and the asset/copy verification scripts.

**Consumes:** Binding HTML, screenshots, font files/licenses, and official logo assets.

**Produces:** `HomeScreen({snapshot, onAction})`, `ServiceTile({service, onOpen})`, `PersonalMediaRow({kind, summary, onOpen})`, and CSS tokens matching the references.

- [ ] Copy the official assets without modification and verify their checksums against provenance. Bundle the fonts with licenses.
- [ ] Extract the application panel into React components without redesigning its structure. Replace presentation-only buttons with actual component events; do not retain mockup disclaimers in product content.
- [ ] Implement Dark and Light with persisted user choice, native View menu actions, and an Appearance settings section. Apply the theme before first paint on subsequent launches.
- [ ] Implement ordinary hover and keyboard focus without tile enlargement. Keep service buttons logo-only and named accessibly.
- [ ] Add deterministic reference fixtures with the screenshot's sample values, available only in development/test mode.
- [ ] Add visual checks at 1280 × 900, 1440 × 900, and 960 × 800 viewport sizes, plus native-window inspection at 100% and 150% desktop scaling. Compare only the application panel at the same content size.
- [ ] Verify `home_light_matches_reference`, `home_dark_matches_reference`, `all_eight_logo_tiles_have_single_visible_brand`, `narrow_window_has_no_horizontal_overflow`, and `keyboard_focus_does_not_change_grid_geometry`.
- [ ] Run `npm run verify:assets`, `npm run verify:ui-copy`, and `npm run test:visual`. Inspect saved images manually before accepting a baseline.

**Acceptance:** Both themes visibly match the chosen design. An empty production snapshot does not show the reference's invented media or VPN/TV state. This milestone must be accepted visually before integration work changes the home screen.

### Task 04: Add typed IPC, persistence, and safe native process infrastructure

**Files:** Create `src/app/backend.ts`, generated contracts, `src/app/controller.ts`, `src-tauri/src/contracts.rs`, `commands.rs`, `events.rs`, `app_state.rs`, `process/*`, `storage/*`, migration SQL, and corresponding Rust/TypeScript tests.

**Consumes:** Section 5 DTOs and Task 02 shell.

**Produces:** `AppState`, `CommandRunner::run(spec, cancellation)`, `ProcessRegistry`, repository functions, ordered events, `get_home_snapshot`, and `set_theme`.

- [ ] Define DTOs and generate TypeScript. Add `contracts_match_generated_types` so drift fails verification.
- [ ] Write tests `command_timeout_reaps_owned_child`, `arguments_are_not_shell_interpreted`, `oversized_output_is_bounded`, and `pid_reuse_is_not_treated_as_owned`; verify meaningful failures before implementing those behaviors.
- [ ] Implement the runner, registry, native path handling, and explicit command permissions. Deny privileged commands from the presentation window.
- [ ] Add the initial migration and repositories with foreign keys and transaction boundaries. Test `migration_failure_preserves_previous_database` and `offline_source_is_not_deleted`.
- [ ] Implement snapshot/event synchronization and operation terminal states. Test `late_event_does_not_overwrite_new_snapshot` and `failed_operation_releases_busy_state`.
- [ ] Run the affected Rust tests and frontend controller tests, then verify `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` and TypeScript checking.

**Acceptance:** No arbitrary shell or filesystem API is exposed to React. Persistence survives restart, migrations fail safely, and stale events cannot fabricate current state.

### Task 05: Implement NordVPN status and explicit connection control

**Files:** Create `src-tauri/src/vpn/{nordvpn,parser,policy}.rs`, `src/features/vpn/VpnSettings.tsx`, sanitized fixtures, and `src-tauri/tests/vpn.rs`.

**Consumes:** `CommandRunner`, repositories, event publisher, Task 01 fixtures.

**Produces:** `refresh_vpn`, `list_vpn_countries`, `connect_vpn`, `disconnect_vpn`, `enable_kill_switch`, `set_vpn_requirement`, and `VpnSnapshot` updates.

- [ ] Write table-driven parser tests for all observed states, ANSI output, unknown fields, missing values, timeout, localized/unrecognized text, and daemon permission errors. `unrecognized_output_never_means_connected` must fail until implemented.
- [ ] Implement serialized provider calls and timing from Section 6. Return `unknown` when confidence is insufficient.
- [ ] Implement the launch predicate `evaluate_launch(servicePreference, freshVpnSnapshot)` with explicit refusal reasons. Test `required_service_rejects_disconnected_stale_or_unknown`, `required_service_rejects_unverified_kill_switch`, and `current_connection_service_does_not_change_vpn`.
- [ ] Build settings for country selection, Connect, Disconnect, and explicit Kill Switch setup. Explain system-wide effects at the action, not in a permanent technical dashboard.
- [ ] Connect the real state to the home header. Use text and the shield icon; no colored status ball.
- [ ] Run `cargo test --manifest-path src-tauri/Cargo.toml --test vpn`. Manually verify read-only status first; perform connection changes only as explicit test actions with the user's working session accounted for.

**Acceptance:** Connected state is evidence-backed, errors are recoverable, repeated clicks cannot race, and app shutdown leaves NordVPN settings and connectivity intact.

### Task 06: Launch and manage the eight service browsers

**Files:** Create `src-tauri/src/streaming/{catalog,browser,sessions}.rs`, `src/features/streaming/StreamingSession.tsx`, `src-tauri/tests/streaming.rs`, and frontend session tests.

**Consumes:** VPN policy, process registry, service catalogue, and HomeScreen open events.

**Produces:** `open_service`, `focus_session`, `close_session`, and owned `ManagedSession` records.

- [ ] Test `unknown_service_id_is_rejected`, `double_click_creates_one_session`, `normal_chrome_profile_is_untouched`, `locked_profile_is_not_deleted`, and `launcher_exit_does_not_mean_browser_closed`.
- [ ] Implement dedicated profile paths, argument-vector construction, registry-only URLs, and the single-active-streaming-session policy.
- [ ] Register actual process/window ownership using the proven preflight method. Keep a failed/ambiguous launch distinguishable from a successfully owned session.
- [ ] Add a current-session screen with Open service window, Move playback, Bring back to computer, and Close service. Show only controls supported by the actual adapter.
- [ ] Verify login persistence by restarting a managed service profile without inspecting its cookies. Confirm that the ordinary Chrome session remains untouched.
- [ ] Run the streaming tests and update the real-service compatibility matrix when native playback is exercised.

**Acceptance:** All eight tiles launch their official destinations through the correct policy; service switching and profile failures cannot damage an unrelated browser session. No claim of universal browser playback control.

### Task 07: Add display discovery and owned-window placement

**Files:** Create `src-tauri/src/outputs/{monitors,x11,coordinator}.rs`, `src/features/outputs/OutputSettings.tsx`, and `src-tauri/tests/outputs.rs`.

**Consumes:** Process/window registry, monitor capability findings, session records.

**Produces:** monitor portion of `list_outputs`, `select_output`, `move_session`, and picture-side `OutputSnapshot`.

- [ ] Test monitor matching with reordered outputs, negative coordinates, mixed scale, disconnected remembered outputs, and duplicate display names.
- [ ] Test `move_playback_never_moves_main_window`, `unverified_window_is_not_moved`, and `wayland_reports_manual_placement`.
- [ ] Implement discovery and stable target resolution. Add X11 placement only for verified owned windows and native placement for the photo window.
- [ ] Implement reversible Move to TV and Bring back actions, keeping desired target separate from observed effective target.
- [ ] Add hotplug handling and clamp recovered main/playback geometry into a reachable computer work area.
- [ ] Run the output tests. Manually test an actual extended desktop, TV left of the computer, TV reconnect, and a denied/failed fullscreen request.

**Acceptance:** The control interface stays on the computer; the selected owned playback window moves correctly or reports a truthful limitation. No desktop topology changes.

### Task 08: Add audio output discovery and session routing

**Files:** Create `src-tauri/src/outputs/audio.rs`, extend the output coordinator/UI, and create `src-tauri/tests/audio.rs`.

**Consumes:** Verified session process ownership and discovered audio-server capability.

**Produces:** audio portion of `list_outputs`, routing operations, and effective audio state.

- [ ] Test JSON parsing, absent HDMI sink, sink disappearance, stream creation after playback starts, and ambiguous Chrome process association.
- [ ] Implement sink discovery, subscribe/resnapshot handling, and routing only for an owned mpv or managed Chrome stream.
- [ ] Keep system default audio unchanged. Implement `ambiguous_stream_is_not_rerouted` and `unrelated_chrome_audio_is_untouched` as explicit regression tests.
- [ ] Combine picture/audio results without reporting complete success after a partial failure.
- [ ] Run audio adapter tests and verify routing with simultaneous sound from an unrelated application. Disconnect HDMI and confirm the documented recovery behavior.

**Acceptance:** The application can identify what it routed; it never silently moves all browser or desktop audio to the TV.

### Task 09: Add media sources and removable-device identity

**Files:** Create `src-tauri/src/library/sources.rs`, `src-tauri/src/devices/{udisks,registry}.rs`, `src/features/library/SourcePicker.tsx`, initial `DrivesScreen.tsx`, and `src-tauri/tests/sources.rs`.

**Consumes:** Database repositories, native directory picker, UDisks2 discovery.

**Produces:** source grants, `choose_media_source`, `add_media_source`, `remove_media_source`, `list_drives`, and source availability events.

- [ ] Test source-root validation, overlapping roots, external symlink rejection, symlink cycles, non-UTF-8 path identity, and removal of a library source without deleting files.
- [ ] Implement grants issued by the backend's native picker. Deduplicate identical roots; avoid repeated indexing of overlapping folders.
- [ ] Discover devices and volumes through UDisks2; distinguish drive identity, volume identity, and current mount point.
- [ ] Test `replug_at_new_mount_path_preserves_source_id`, `duplicate_uuid_requires_disambiguation`, and `usb_c_does_not_require_separate_media_logic`.
- [ ] Show mounted/unmounted/offline sources and accurate device labels without automatically scanning every attached disk.
- [ ] Run the source/device tests and manually attach a drive through each available connector type. Record unsupported hardware instead of inferring success.

**Acceptance:** Library identity survives ordinary reattachment, and selected folders remain the filesystem-access boundary.

### Task 10: Index media and build responsive library views

**Files:** Create `src-tauri/src/library/{scanner,probe,thumbnails,query}.rs`, `src/features/library/MediaBrowser.tsx`, `src/features/videos/VideosScreen.tsx`, and `src-tauri/tests/library.rs`.

**Consumes:** source registry, repositories, process runner, restricted image-resource handler.

**Produces:** `rescan_source`, `query_media`, thumbnail resources, cancellation, and library summary events.

- [ ] Prepare small local fixtures covering supported media kinds, corrupt files, missing permission, unusual filenames, a large synthetic directory index, and a removed volume. Do not copy the user's private media into test fixtures.
- [ ] Test `cancelled_scan_does_not_mark_files_missing`, `unplug_during_probe_marks_source_offline`, `changed_file_invalidates_thumbnail`, and `slow_probe_does_not_block_ui`.
- [ ] Implement bounded scan/probe/thumbnail queues and transactional batch writes using the budgets in Section 8.
- [ ] Implement indexed search, cursor pagination, stable sorting, and a bounded thumbnail cache.
- [ ] Render Videos and shared library states using the approved design language. Include empty, scanning, partial-error, and offline states.
- [ ] Run library tests and measure a 10,000-item fixture search. Confirm that cancellation stops queued work promptly and that no read escapes a granted source.

**Acceptance:** Real media appears incrementally; a slow, damaged, or unplugged source cannot freeze the control application or erase the library.

### Task 11: Implement local video playback and resume

**Files:** Create `src-tauri/src/player/{mpv,ipc,resume}.rs`, `src/features/player/LocalPlayerControls.tsx`, and `src-tauri/tests/player.rs`.

**Consumes:** media IDs, source path resolution, output coordinator, process registry, resume repository.

**Produces:** `play_media`, `control_player`, mpv-backed sessions, and resume updates.

- [ ] Use a fake IPC peer to test interleaved events/replies, malformed JSON, oversized lines, timeouts, unexpected process exit, and a second load request during startup.
- [ ] Test `resume_waits_for_file_loaded`, `completed_video_does_not_resume_near_end`, `changed_duration_clamps_resume`, and `removed_volume_stops_owned_playback`.
- [ ] Implement owned process startup, private socket lifecycle, property subscriptions, typed player actions, and bounded shutdown.
- [ ] Implement controls and track selectors using real mpv state. Do not optimistically display Playing after a failed load.
- [ ] Connect video playback to selected display/audio outputs and preserve the main window location.
- [ ] Run player tests and play real sample video on computer and HDMI. Verify pause, seek, stop, resume after restart, subtitles, audio tracks, and TV removal.

**Acceptance:** The app controls local playback, resumes correctly, and cleans up only its own player process and socket.

### Task 12: Add music queues and playlists

**Files:** Create `src-tauri/src/music/queue.rs`, `src/features/music/MusicScreen.tsx`, and `src-tauri/tests/music.rs`; extend playlist repositories.

**Consumes:** audio media queries, local player service, playlist storage.

**Produces:** `save_playlist`, `list_playlists`, `get_playlist`, `remove_playlist`, `play_playlist`, `get_music_queue`, `control_music`, and queue state.

- [ ] Test queue order, end-of-track advancement, repeat modes, deterministic seeded shuffle in tests, duplicate playlist entries, and offline items.
- [ ] Implement queue ownership independent from the currently rendered screen. Music continues during normal navigation.
- [ ] Implement playlist editing with explicit save behavior and stable ordered media IDs.
- [ ] Render title/artist/album fallback and queue status without dot separators. Keep counters and fields visually separated by spacing or labels.
- [ ] Run music tests and verify a mixed-format playlist with a missing item and an unplugged source.

**Acceptance:** A playlist is persistent, queue behavior is predictable, and unavailable files are retained as library records rather than erased.

### Task 13: Add photos and an independent slideshow output

**Files:** Create `src-tauri/src/photos/presenter.rs`, `src/features/photos/{PhotosScreen,PresentationScreen}.tsx`, restricted presentation capabilities, and `src-tauri/tests/photos.rs`.

**Consumes:** photo media queries, restricted decoded-image resources, output coordinator.

**Produces:** `start_slideshow`, `control_slideshow`, and a restricted presentation window.

- [ ] Test EXIF orientation, very large/corrupt images, unsupported formats, ordered navigation, interval bounds, pause/resume, and cancellation on source removal.
- [ ] Implement bounded preload and image-buffer release. Never load arbitrary HTML/SVG from a media folder into the app webview.
- [ ] Create the presentation window with only its required permissions and image assignments. Add a test that it cannot invoke VPN/process/device commands.
- [ ] Render the photo browser/viewer in the shared visual system and playback controls in the computer's main window.
- [ ] Run photo tests and verify a slideshow on the TV while the controls remain on the computer; disconnect the output and verify pause/recovery.

**Acceptance:** Images retain orientation and aspect ratio; the photo window cannot access the main window's privileged operations.

### Task 14: Finish drive mounting and safe removal

**Files:** Extend `src-tauri/src/devices/udisks.rs`, `registry.rs`, `DrivesScreen.tsx`, and add `src-tauri/tests/eject.rs`.

**Consumes:** device identities, active scan/player/slideshow registries, cancellation and shutdown APIs.

**Produces:** `mount_volume`, `eject_drive`, active-use summaries, and truthful final device state.

- [ ] Test mounted/unmounted devices, a multi-volume drive, active playback, active thumbnail generation, busy external applications, permission denial, and devices without PowerOff support.
- [ ] Implement active-use detection and an explicit Stop using and safely remove action. Cancel own work and wait for handles to close before unmounting.
- [ ] Unmount relevant volumes through UDisks2 and power off only when supported; never force removal or alter a filesystem.
- [ ] Implement `busy_unmount_does_not_report_safe_to_remove` and `one_volume_failure_prevents_false_drive_success`.
- [ ] Run eject tests and verify with a disposable test drive, including a deliberately open file in another application.

**Acceptance:** Safe-to-remove feedback follows the OS operation result. Media records persist as offline after removal.

### Task 15: Integrate lifecycle, failures, and end-to-end behavior

**Files:** Create `src-tauri/src/lifecycle.rs`, `diagnostics.rs`, final settings screen, `tests/e2e/workflows.spec.ts`, and `docs/engineering/verification.md`.

**Consumes:** all implemented service interfaces.

**Produces:** a coherent application with startup reconciliation, single-instance behavior, exit choices, recovery, and diagnostic visibility.

- [ ] Test startup with no TV, no network, no VPN client, no mpv, no selected folders, and unavailable external drives. Each missing optional capability must degrade independently.
- [ ] Test VPN loss while a browser is open, suspend/resume, HDMI hotplug during local playback, browser/player crashes, and simultaneous source scan plus theme change.
- [ ] Verify that home summaries use actual state and never keep the demo's connected-country, drive sizes, or media titles in production.
- [ ] Verify app exit choices, no VPN disconnect on exit, no unrelated process termination, and no leftover privileged listener or debug server.
- [ ] Run focused unit/integration suites, frontend end-to-end workflows with fakes, and native manual scenarios with real hardware. Clearly separate automated mocked evidence from real service playback evidence.
- [ ] Run both theme visual checks again because status/error integration can disturb spacing. Review empty/error/long-name states as well as the golden demo fixture.

**Acceptance:** The completed features behave as one application, recover from realistic interruptions, and preserve the visual contract.

### Task 16: Package, install-test, and prepare a usable handoff

**Files:** Finalize `src-tauri/tauri.conf.json`, package metadata, desktop entry/icon, `README.md`, `docs/engineering/packaging.md`, and release verification records.

**Consumes:** verified application build and compatibility findings.

**Produces:** a versioned Linux Mint/Ubuntu `.deb`, checksums, installation instructions, and a recorded smoke test.

- [ ] Declare the required native runtime dependencies for the supported Mint/Ubuntu target. Keep Chrome and NordVPN as externally managed prerequisites with clear detection; do not bundle or silently install them.
- [ ] Build a Debian package with Tauri's supported bundling flow. An AppImage is a later optional deliverable, not a prerequisite for completion. [Tauri Debian packaging](https://v2.tauri.app/distribute/debian/)
- [ ] Install the package in a representative clean test environment or controlled test account, then verify start-menu launch, ordinary window behavior, local assets/fonts, and removal of the package without deletion of user media.
- [ ] Verify the packaged application against the real machine for VPN status, one actual streaming session, local video/music/photo playback, HDMI, and drive reconnect. Do not rely only on the development server build.
- [ ] Confirm that updates preserve the app database, browser logins, theme selection, and source registry. Record the database migration/recovery procedure.
- [ ] Produce release checksums and document tested versions, unsupported conditions, and any service-specific limitation discovered in Task 01/06.

**Acceptance:** A packaged desktop application can be launched without a terminal or development server and its documented behavior matches the tested build.

## 11. Verification strategy

### 11.1 Appropriate automated coverage

Focus automated tests on state transitions, parser changes, process ownership, path handling, database migrations, cancellation, device identity, and output recovery. Do not create a test for every CSS declaration or assertions that merely duplicate static implementation constants.

Use Vitest and React Testing Library for frontend models and interactions, Playwright for deterministic rendered UI/visual checks, and Rust unit/integration tests for backend policies and adapters. Browser-only UI tests do not prove native WebKitGTK behavior; inspect and smoke-test the packaged Tauri application as well. Native automation can be added through supported Tauri/WebDriver tooling if it provides useful coverage without becoming a separate project.

External integrations use a fake `CommandRunner`, recorded sanitized outputs, a fake mpv IPC peer, and synthetic UDisks/audio/output events. Real streaming login and playback remain manual compatibility evidence; do not commit account cookies or attempt to make CI log into paid services.

### 11.2 Visual acceptance procedure

1. Render the reference HTML and implemented home with identical deterministic sample data, dimensions, fonts, and theme in the same test environment.
2. Capture only the application panel. The old JPEG files remain human-readable design evidence; generate separate lossless test baselines from the authoritative HTML instead of comparing different JPEG artifacts byte for byte.
3. Use the same content-width breakpoints in both renderings. At the canonical case, align the application panel dimensions before comparison; do not compare an entire browser page to a native window frame.
4. Compare layout landmarks: column boundary, section labels, row dividers, tile bounds, logo bounds, header information, and footer alignment. Target at most 2 CSS pixels of deviation for these landmarks at matching dimensions.
5. Check colors and fonts against the source tokens. Automated image tolerances may accommodate antialiasing, but cannot justify different typography, resized logos, rearranged tiles, or changed proportions.
6. Inspect both light and dark screenshots manually at each visual milestone. Keep reference fixtures and production empty states as separate tests.
7. Do not accept a regression by regenerating a baseline from the changed product. A deliberate design change needs an updated approved reference.

### 11.3 No-dot and logo checks

Scan app-authored UI strings and rendered fixture text for added Unicode middle-dot/bullet/circle separators. Also inspect CSS pseudo-elements, status widgets, loading indicators, menu icons, and component-library defaults. A text scan alone cannot detect a CSS-drawn status ball.

Exclude official logo files, user media, external service websites, technical source syntax, and unmodified user metadata from the decoration rule. Do not corrupt a filename or trademark to satisfy a superficial text check.

Verify all eight logo files against provenance and inspect `object-fit: contain`, nonzero image dimensions, sufficient contrast, and the absence of duplicate visible captions in both themes. An accessible name is required and is not a duplicate visual label.

### 11.4 Manual release matrix

| Scenario | Required result |
| --- | --- |
| First launch on computer without TV | Correct chosen design, real empty/state data, useful local features |
| Both themes, restart, native scaling | Same layout, saved theme, no first-paint flash or clipped logos |
| Service launch with VPN ready | Correct dedicated browser profile and honest session state |
| VPN disconnected, unknown, stale, or Kill Switch unverified | Required launch blocked with an actionable explanation |
| VPN drops during service playback | Provider enforcement is respected; app reports loss and blocks new required launches |
| Existing ordinary Chrome windows | They retain their profiles, position, playback, and audio |
| HDMI TV attach/detach | Main UI stays reachable on computer; owned playback target resolves or reports missing |
| Mixed/negative monitor geometry | Placement uses actual coordinates and revalidated identity |
| HDMI picture works but audio sink is absent | Partial state is visible; no false picture-and-sound success |
| Local video resume, subtitles, multiple audio tracks | Correct state and user-selected behavior after restart |
| Music playlist with an unavailable file | Queue continues predictably; offline item is retained |
| Photo slideshow on TV | Controls stay on computer, aspect ratios and orientation are correct |
| USB-A/USB-C drive disconnect and reconnect | Same library source reconnects without duplicating or losing records |
| Busy or permission-denied safe removal | No forced unmount and no false safe-to-remove message |
| Long/Finnish/non-UTF-8 filenames | No crash, command injection, source escape, or wrong-file playback |
| Packaged build offline | Local fonts/logos, library, local playback, and settings remain usable |
| App exit while other media applications run | Only explicitly selected Mediacenter-owned sessions are affected |

## 12. Definition of done

The application is complete only when all applicable items below are true. Unsupported provider behavior must be documented; it cannot be converted into a passing item by changing the wording of the feature.

- [ ] The dark and light home screens faithfully match the two referenced images and their HTML specification.
- [ ] All eight official service logos appear once per tile, without duplicate captions or approximation artwork.
- [ ] There are no added dot separators, decorative status balls, dot loading indicators, or ellipsis menus in application-owned UI.
- [ ] The main application is a normal usable Linux desktop window on the computer and stays useful without a TV.
- [ ] VPN state, launch requirements, and explicit connection actions behave correctly with the installed NordVPN client.
- [ ] Actual streaming compatibility has been recorded for every service, including failures and account-dependent gaps. The primary intended viewing setup has been demonstrated, or its blocker is clearly identified before calling the project successful.
- [ ] Owned streaming windows can be moved to and returned from the HDMI TV on the supported X11 target without disturbing ordinary browser windows.
- [ ] Audio output is truthful and restricted to identifiable owned streams; no silent system-wide rerouting.
- [ ] Local videos support playback, seek, tracks/subtitles, and resume.
- [ ] Music supports playback, queue, saved playlists, and defined repeat/shuffle behavior.
- [ ] Photos support browsing and an independently controlled slideshow output.
- [ ] Selected internal folders and removable drives can be indexed safely, survive unplug/replug, and remain readable in the library when offline.
- [ ] Mount and safe-removal actions use the OS interfaces and correctly handle busy or denied operations.
- [ ] Startup, suspend/resume, process crashes, missing dependencies, and output loss have useful recovery paths.
- [ ] The packaged `.deb` launches without a development server, uses local assets, and preserves user state across updates.
- [ ] Automated checks, native visual review, real-hardware verification, and service compatibility evidence are recorded separately and accurately.
- [ ] No Color Dock files, TV installation, Remote files, unrelated browser profiles, or original user media were modified.

**Final visual rule for the implementer: build the Linux application shown in the approved dark and light Colour Wall desktop images. Preserve that appearance while replacing demonstration values with real state and adding the required functionality. Do not redesign the home screen during implementation.**
