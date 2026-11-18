# Remote–Mediacenter Sofa Control Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox syntax for tracking. Do not execute implementation merely because this document exists; this delivery is a requested plan.

**Goal:** Let Emma operate Mediacenter from the existing Android Remote application while sitting on the sofa, with the Linux computer powered on beside the television and connected to it by HDMI.

**Architecture:** Add an authenticated local-network control server inside Mediacenter and a separate Mediacenter client inside Remote, alongside the existing LG TV client. Reuse Mediacenter's native media services and the existing mirrored desktop, and expose browsing, playback, audio controls and routine administration on the phone. Handle Chrome streaming through an explicitly enrolled browser bridge, with capability reporting and service-specific acceptance rather than assuming local-player controls also control websites.

**Tech Stack:** Existing Tauri 2 / Rust / Tokio / SQLite / React / TypeScript desktop application; existing Android Kotlin / Jetpack Compose / OkHttp / DataStore application; TLS WebSocket and authenticated HTTPS; DNS-SD discovery; existing mpv, PulseAudio-compatible routing, UDisks2 and X11 integrations; a small Manifest V3 Chrome extension and Rust native messaging host for the streaming milestone.

**Spec:** The normative product specification is embedded in sections 1–12 of this file. It incorporates the user's approved direct-LAN direction and the subsequently clarified sofa/HDMI requirement. The user's explicit correction is binding: HDMI already mirrors the computer screen to the television. Do not build a new TV mode or require display routing. Detailed protocol and task decisions below are engineering proposals for implementation, not claims of existing capabilities or hardware validation.

**Prepared:** 2026-11-09.

**Repositories:**

- MC = /home/emma/Dev/Mediacenter
- R = /home/emma/Dev/Remote
- RK = R/app/src/main/java/com/emmah/colordock/remote
- RT = R/app/src/test/java/com/emmah/colordock/remote
- RAT = R/app/src/androidTest/java/com/emmah/colordock/remote

All paths below use these aliases. Deliver source changes in the correct repository; Remote was copied from another computer and may need its Android build environment restored. The present planning environment permits writing MC, not R; arrange normal workspace access before implementing R changes.

## Global Constraints

- Preserve the current Mediacenter visual identity and the existing Remote application.
- The user's computer and TV already mirror the same desktop through HDMI. Preserve that configuration; do not add a television mode, monitor picker, display-role configuration, window migration workflow or resolution changes.
- Do not restore persistent playback bars to Mediacenter's library, queue or other browsing screens. The user explicitly rejected that change.
- Do not implement the generated mockups in docs/design/2026-11-09-simplified-concepts as an approved UI specification.
- Use one canonical location per function within each application. Phone controls are an intentional remote interface, not permission to duplicate desktop controls across screens.
- Target the existing Linux Mint 22.3 / Ubuntu 24.04, x86-64, Cinnamon X11 environment first.
- Preserve Remote's current minSdk, targetSdk and compileSdk of 37 unless a separate compatibility change is approved.
- Preserve current dependency versions and lockfiles except where a task needs a reviewed new dependency. Do not combine this work with general upgrades.
- Keep video, music and streaming playback on the computer/HDMI output. This is remote control, not media casting or video transmission to the phone.
- Keep all control traffic on the selected trusted local network. No accounts, cloud relay, router port forwarding or Internet-exposed control endpoint.
- Retain LG TV and Color Dock functionality. A missing LG TV connection must not prevent Mediacenter control.
- Preserve media files, source associations, playlists, resume positions, browser profiles and existing pairing records through upgrades.
- Never send raw shell commands, arbitrary Tauri command names, filesystem paths or arbitrary Chrome DevTools commands over the remote protocol.
- Retain verified ownership checks for managed mpv and Chrome processes. Ordinary Chrome remains ordinary Chrome; remote enrollment does not grant ownership of all its windows.
- Never disable NordVPN or its Kill Switch as an automatic connectivity repair. Do not change global audio defaults as a routing fallback.
- One-time setup can require access to the computer. After setup, supported daily flows and their normal recoveries must be usable from the sofa.
- Do not claim readiness from mock tests alone. Final acceptance requires the actual phone, computer, HDMI television, audio output and representative streaming services.

## Review Focus

1. Existing HDMI mirroring must remain unchanged; audio hotplug and display changes must not cause automatic monitor reconfiguration or unexpected laptop-speaker playback; Task 10 owns these tests.
2. A command whose reply is lost must not skip twice, eject twice or launch twice after reconnection; Tasks 3, 5 and 8 own these tests.
3. LG TV connection loss, Android backgrounding and target switching must not route a command to the wrong device; Tasks 7 and 16 own these tests.
4. A folder, device, session, dialog or browser tab changing between preview and confirmation must invalidate the old action; Tasks 9, 11, 13, 14 and 18 own these tests.
5. VPN reconnection, Android permission revocation and server identity changes must have distinct recoveries without insecure trust resets; Tasks 4, 6, 8 and 15 own these tests.

---

## 1. Product requirements and acceptance scope

### 1.1 Confirmed user scenario

The computer is on a stool next to the television because the HDMI cable is short. Emma is on the sofa with her phone and cannot conveniently reach the computer's keyboard, touchpad or screen. The phone must do substantially more than play/pause: it must allow choosing content, entering text, controlling sound and recovering from ordinary failures.

The TV already mirrors everything shown on the computer. HDMI display configuration is working and is not a problem this feature is being asked to solve.

The computer is initially powered on and signed into its graphical desktop. Do not interpret "powered on" as permission to bypass the OS login screen or unlock it remotely.

### 1.2 Required daily capabilities

Assign requirement IDs so implementation and acceptance can be traced.

| ID | Requirement | Completion owner |
| --- | --- | --- |
| R01 | Pair once; reconnect by device identity after address changes | Tasks 4–8 |
| R02 | Keep LG TV, Color Dock and Mediacenter control independent | Tasks 7, 16 |
| R03 | Browse/search local videos, music, playlists and photos on the phone | Task 12 |
| R04 | Play on HDMI; pause, seek, stop, volume, mute, subtitles and audio tracks from phone | Tasks 9, 10, 12 |
| R05 | Manage queue, reorder/remove tracks, shuffle/repeat and save/edit playlists | Tasks 9, 12 |
| R06 | Show photos and run/control slideshows on TV | Tasks 9, 12 |
| R07 | Navigate the existing mirrored Mediacenter UI with directional pad, OK and Back | Tasks 10, 11 |
| R08 | Enter search text on phone without using the computer keyboard | Tasks 11, 12, 18 |
| R09 | Control playback volume and, when needed, choose audio output on phone without altering display mirroring | Task 10 |
| R10 | Return to Mediacenter menus and back to current playback from phone | Tasks 10, 11 |
| R11 | Manage normal media folders, scan, mounted USB media and safe removal | Tasks 13, 14 |
| R12 | View/change supported VPN preferences without silently losing LAN control | Task 15 |
| R13 | Open/switch streaming services; operate enrolled pages with phone pointer/text input | Tasks 17–19 |
| R14 | Keep server reachable while UI is hidden; opt-in start at desktop login | Task 20 |
| R15 | Display errors, confirmations and progress on phone with an actionable recovery | All feature tasks; Task 21 integration |
| R16 | Revoke a phone, disable remote access and preserve secrets during updates | Tasks 4, 8, 22 |
| R17 | Keep the desktop uncluttered and avoid duplicate phone actions | Tasks 11, 12, 16, 21 |

### 1.3 Planned releases

This is one coordinated feature split into testable milestones, not unrelated projects.

- **A — Secure connection:** discovery, pairing, authenticated state and a single native playback command. Developer checkpoint only.
- **B — Local-media sofa use:** phone library, complete native playback, existing-window navigation, phone confirmations and reconnect behavior. Usable for local media after physical acceptance.
- **C — Daily administration:** folders, USB, audio recovery, VPN, startup and LG target independence.
- **D — Streaming sofa use:** browser bridge, phone touchpad/keyboard, supported service navigation and real-service validation.
- **E — Packaged acceptance:** compatible Android/Desktop artifacts, migration/rollback documentation and final sofa-only test.

Do not describe B as "full Mediacenter remote control" while R13 is unfinished. A streaming service is accepted individually; success with one service does not certify the others.

### 1.4 Explicit limits

- Initial pairing, OS permission prompts, application installation and browser-extension installation may require one-time computer access.
- OS login/unlock, BIOS, installing software, entering privileged administrator credentials, changing the router, DRM bypass and accessing another user's desktop are outside this feature.
- Wake-on-LAN, PC power-on, HDMI-CEC, remote desktop video and phone lock-screen controls are later optional projects.
- Remote's existing TV power command continues to mean TV power. It must never become PC shutdown by context accident.
- A closed laptop lid can trigger hardware/desktop sleep policy. Keep it open for baseline acceptance; do not silently modify lid behavior.
- If a website displays an OS-level prompt, protected browser UI, CAPTCHA or unsupported authentication step, report that exact limitation. Do not advertise fully unattended service login unless physically demonstrated.
- Paired phones are trusted to operate media and supported settings. Pairing does not confer arbitrary computer administration.

## 2. Evidence from the current repositories

These are source observations, not test-run results. Read the listed files again at implementation time because other work may change them.

| Existing area | Observed behavior | Implication |
| --- | --- | --- |
| RK/AppContainer.kt | Constructs LgTvClient and CompanionClient bound to TV transport | Add an independent client, not a replacement LG URL |
| RK/tv/TvModels.kt | LG-specific states, commands and port-3001 WSS endpoint policy | Define Mediacenter models and endpoint policy separately |
| RK/ui/RemoteViewModel.kt | Concrete LgTvClient dependency; repeat commands; foreground recovery | Reuse behavioral patterns, not LG state as a common readiness flag |
| RK/ui/RemoteTab.kt; RK/MainActivity.kt | Remote, Wallpapers and Color Dock tabs; several routes depend on TV Ready | Make shell routing target-aware and remove LG gating from MC routes |
| RK/tv/LgDockChannel.kt | Color Dock communication uses LG webapp/connectToApp | This is not a generic LAN transport for Linux |
| R/app/src/main/AndroidManifest.xml | LAN permission and multicast permission already present | Retain existing TV path and add permission-aware NSD/MC connection |
| MC/src-tauri/src/commands.rs | Desktop commands are guarded by window label and delegate to services | Share native service logic; do not expose the Tauri invoke surface on LAN |
| MC/src-tauri/src/player/mpv.rs | Native IPC actions and player/session ownership checks already exist | Reuse for local playback |
| MC/src-tauri/src/music/service.rs, queue.rs, playlists.rs | Queue revisions, playlist persistence and playback functions exist | Adapt with stable remote IDs and concurrency checks |
| MC/src-tauri/src/photos/presenter.rs | Separate presentation window, slideshow state and control | Reuse presentation; add phone-friendly still-photo selection |
| MC/src-tauri/src/outputs/control_window.rs | Restores main window to selected/computer display | Preserve current window placement; mirrored TV already shows the same content |
| MC/src-tauri/src/streaming/sessions.rs | Launch resolves computer display; VPN/preflight precedes switching | Preserve current display behavior and preflight order; no new screen routing is required |
| MC/src-tauri/src/streaming/external.rs | HBO hands URL to ordinary Chrome without managed ownership | Needs explicit browser-session enrollment; never kill that browser |
| MC/src-tauri/src/events.rs, operations.rs | Events primarily target main Tauri window; operations are bounded | Add native subscription/status infrastructure for phone consumers |
| MC/src-tauri/src/storage/database.rs | Current migration handling recognizes schema version 1 | Introduce sequential, tested migrations; do not just append a table at runtime |
| MC/src-tauri/src/lib.rs, lifecycle.rs | Closing main window leads to exit handling; no LAN listener | Add controlled background lifetime while remote access is enabled |
| MC/src/app/App.tsx | UI navigation lives in React state | Add a renderer bridge for navigation only; playback remains native |

RK expands to R/app/src/main/java/com/emmah/colordock/remote as defined in the header.

Mediacenter package/Cargo configuration currently reports 0.1.6; the README has older release narrative. Prefer inspected source and package metadata over assuming README version prose is current.

## 3. User experience

### 3.1 One-time setup

1. Enable "Remote control" in one Mediacenter Settings section.
2. Choose the local network interface. Show its friendly name/address; exclude VPN tunnel, loopback and virtual/container interfaces by default.
3. Display a readable pairing QR code in the existing Mediacenter window. The mirrored TV already shows it; no window movement or display selection is required.
4. In Remote, use the device selector → Add Mediacenter → Scan code. Provide a manual path if camera permission is declined.
5. Show "Connected to Mediacenter" on the phone. Device names are editable labels; cryptographic identity is separate.
6. Keep the existing HDMI picture and current audio setup. If sound needs changing, provide an optional audio-output selector in phone settings; do not require it during pairing.
7. Offer, unchecked initially, "Start Mediacenter when I sign in" and explain that the OS session must be signed in.
8. Offer browser-bridge setup separately. Local media must work without it.

Remote pairing and browser extension setup are explicit setup steps; avoid exposing technical transport fields on the everyday remote screen.

### 3.2 Phone information architecture

Retain the visual style of the current Remote. Use its existing target header/device selector.

- **LG TV selected:** preserve the current LG remote, TV apps, Wallpapers and Color Dock behavior.
- **Mediacenter selected:** show three destinations within that target: Remote, Library and Settings. Do not add every media category to the global app navigation.
- **Remote:** one D-pad or browser touchpad surface, one transport control group, current title/status, and contextual overflow for subtitles/audio tracks.
- **Library:** Videos, Music and Photos; Playlists is within Music. Search operates on the selected category.
- **Settings:** Sound, Media folders & drives, VPN, Connection. Keep uncommon options collapsed.
- **Queue:** a sheet opened from one Queue button on the Remote playback view. Library item menus may add items to it, but must not provide duplicate queue editors.
- **Playlists:** saved collections in Music; queue sheet owns "Save queue as playlist".
- **Now playing access:** one navigation shortcut from Library, with no extra transport strip on Library or Settings.

Selecting Mediacenter must not disconnect the TV credential/session unnecessarily. Commands still have one explicit target. A later convenience mapping of MC volume to TV volume is not part of the initial design; first make each label unambiguous.

### 3.3 Existing mirrored interface and focus

Use the existing main Mediacenter window where it already opens. The TV mirrors it automatically. Do not create a second main window, a TV mode, a display configuration wizard or a separate fullscreen/scaling preset.

- Preserve desktop typography/colors/components; add visible focus states for remote navigation.
- Keep native playback controls in the phone and existing player surfaces. Do not introduce the rejected desktop bottom bars.
- Direction keys navigate visible actionable elements, with deterministic grid/list behavior.
- OK activates the focused action. Back dismisses the topmost dialog, then returns through navigation history.
- "Mediacenter" on the phone foregrounds the existing menu window; "Return to playback" restores the actual active media window.
- Going to the menu does not silently stop playback. Stopping has a separate explicit action.
- Returning from a modal or player restores the last valid item by stable ID, not its old pixel coordinate.
- When an item disappears, focus the next surviving item, previous item or section heading in that order.
- Renderer navigation commands return success only after the renderer acknowledges the applied change.

### 3.4 Daily phone workflows

**Video:** choose item → choose Resume or Start over when applicable → play using the existing working display/audio configuration → use phone pause/seek/subtitles → return to library → choose another item with a single replacement confirmation if needed.

**Music:** tap track to play; item menu adds to queue/playlist; edit queue by stable entry IDs; previous/next, shuffle and repeat remain in the canonical playback/queue location.

**Photos:** tap thumbnail to show that still image on TV; swipe/buttons select adjacent image; start slideshow with the current filtered selection; pause/interval/shuffle/fit and stop remain reachable on phone.

**Folders:** choose known media root or mounted drive → browse directories on phone → add folder for selected media kinds → show scan progress and cancellation.

**Switching:** phone requests a new activity, receives an accurate preview of what will stop, confirms once, then watches the operation. No required confirmation is left only on the laptop.

**Streaming:** choose service → open in the existing desktop, mirrored on TV → phone automatically shows touchpad and keyboard access for the enrolled browser target. Use website-native playback/subtitle menus when no verified semantic API is available.

## 4. Architecture and authority boundaries

~~~mermaid
flowchart LR
    Phone[Remote Android app] -->|TLS commands and state| Server[Mediacenter LAN server]
    Server --> Native[Existing native services]
    Native --> Player[mpv]
    Native --> Photos[Photo presentation]
    Native --> Outputs[Existing audio routing]
    Server -->|Navigation only| UI[Existing UI mirrored by HDMI]
    Server --> Bridge[Local browser bridge]
    Bridge --> Chrome[Explicitly enrolled Chrome tab]
    Phone -->|Existing LG connection| TV[LG television]
~~~

### 4.1 Native server

Proposed module: MC/src-tauri/src/remote/.

Responsibilities: listener lifecycle, TLS, pairing, client authentication, version negotiation, typed dispatch, snapshots, event subscriptions, bounded request handling and phone-specific confirmation state.

It must not duplicate player, queue, source, output or VPN business rules. Extract minimal reusable application services where logic currently sits inside a guarded Tauri command. Keep the existing window guards and capability restrictions.

An authenticated remote request receives a RemotePrincipal with device ID and granted scopes. The dispatcher uses an explicit method allowlist, never a string-to-Tauri-command forwarding mechanism.

### 4.2 Android client

Proposed package: RK/mediacenter/.

MediacenterClient owns one active MC connection, independent of LgTvClient. Its repository publishes immutable state via StateFlow. ViewModels use injected interfaces, including test fakes.

Retain TvTarget's LG restrictions. Introduce a separate MediacenterEndpoint instead of broadening a security-sensitive TV URL parser.

### 4.3 Shared service layer and events

Introduce MC/src-tauri/src/control/ for new cross-entry-point workflows:

- PlaybackService: typed local playback actions.
- ActivityService: preflight and confirmed replacement between player, photo and managed-browser activities.
- AudioService: requested versus effective audio routing; display configuration is unchanged.
- LibraryService: bounded metadata queries and safe folder proposals.
- SettingsService: supported settings with revision checks.
- OperationRegistry: status lookup and terminal outcomes.

Existing native modules remain authoritative for actual behavior. These services coordinate them; they are not a rewrite.

Add a native event hub that can feed Tauri and remote consumers. No socket waits while holding SQLite or player locks. Events are emitted after committed state changes; a broken phone connection must not turn a successful media action into a rollback.

### 4.4 Renderer bridge

Expose only typed UI events: Navigate, MoveFocus, Activate, Back, SetSearchText and PresentConfirmation.

Each carries a renderer generation and request ID. React checks that the target/context is still current, applies the change, then acknowledges through a narrowly scoped Tauri command. A renderer reload invalidates pending UI requests. Native media requests continue working without UI acknowledgements.

Do not claim that an event emitted to a window proves that the window applied it.

## 5. Connection, pairing and credentials

### 5.1 Listener and discovery

- Default port: TCP 47831 on the selected LAN addresses. This is a project default, not a claim of an assigned standard port.
- If occupied, fail clearly and offer an explicit alternative port; persist it and update QR/discovery. Do not silently listen on an unrelated interface or public address.
- Production and development must have different identities, credentials and listener ports; development default 47832.
- Validate local interface address membership, IPv4/IPv6 handling and firewall behavior. Bind concrete LAN addresses rather than all interfaces.
- Use service type _emma-mc._tcp.local. as the project discovery identifier. Discovery TXT contains only protocol major, public device ID and display name.
- DNS-SD is a location hint, not proof of identity. Android NSD supports this discovery model [S1].
- QR connection remains usable if multicast is blocked but direct LAN TCP is permitted.
- Rebind/republish after network address changes; do not destroy credentials.
- No unauthenticated endpoint returns media titles, folders, thumbnails or output/device inventory.
- Web UI remains packaged local content; do not expose Vite, Tauri IPC or a general web file server.

### 5.2 TLS and device identity

Generate a per-installation server key and self-signed certificate using maintained TLS/crypto libraries. Persist a random server UUID independently of its name and IP.

QR contains: protocol major, device UUID, candidate local addresses, port, SHA-256 certificate fingerprint, a pairing-attempt ID and a 256-bit one-use random secret.

Pin the complete certificate fingerprint obtained through pairing. Validate the pin before sending any pairing secret or existing credential. A changed certificate requires explicit new pairing; never fall back to accepting every certificate.

Keep server key material in the app's private data directory with directory mode 0700 and file mode 0600. Do not put it in diagnostics or the repository. Regenerating a missing key must invalidate old pairings and visibly require new pairing.

### 5.3 Pairing protocol

- Pairing is disabled except during an explicit setup window.
- Window lifetime: 120 seconds, measured with a monotonic clock.
- QR secret: cryptographically random 32 bytes, stored hashed in memory, constant-time comparison, single successful use.
- POST /remote/v1/pair requires the pairing-attempt ID, secret and a device label of at most 64 Unicode characters.
- Success issues device ID and random 32-byte bearer credential over pinned TLS. Persist only its hash on desktop.
- Credentials are reusable for reconnect, scoped to this server and revocable individually.
- Maximum 8 paired phones; setup can remove an existing phone before adding a ninth.
- Rate limits: 5 pairing failures per peer per 60 seconds, 20 failures globally per 60 seconds; close the current pairing attempt after its fifth failed secret.
- Additional pairing from an already paired phone requires a phone confirmation and creates a new short-lived attempt; it does not expose the existing credential.

Manual fallback: enter address/port; compare the server certificate's six-word verification phrase displayed on TV and phone; explicitly confirm the match; then enter the TV's random 12-character base32 one-use code. The phrase derives from 66 bits of SHA-256 using a fixed 2048-word versioned list. Do not treat a short PIN alone as certificate authentication. Apply the same expiry/failure limits. QR is the primary flow.

### 5.4 Credential storage and revocation

- Android: separate MediacenterCredentialStore, Android Keystore-wrapped encrypted bytes, no plaintext DataStore token and no token in backup.
- TV credentials stay in their existing store and are not migrated into MC credentials.
- Desktop paired_devices rows: device ID, token hash, label, scopes, created timestamp, last seen timestamp and revoked timestamp.
- Scopes: browse, playback, navigation, outputs, library_manage, settings and browser_control. Initial explicit personal-device pairing grants these listed scopes.
- Revocation invalidates active sockets immediately. The revoking phone may revoke itself; show that it must pair again.
- "Forget on this phone" removes local credentials only; label it separately from server revocation.
- Server-disabled mode closes sockets/listeners and stops advertising; desktop disabling remains available without a paired phone.

### 5.5 Endpoints and resource boundaries

| Endpoint | Authentication | Purpose |
| --- | --- | --- |
| POST /remote/v1/pair | Active one-time pairing attempt | Initial credential issuance |
| GET /remote/v1/ws | Authorization bearer header | Typed commands, results and events |
| GET /remote/v1/thumbnails/{opaqueId} | Authorization bearer header | Bounded cached previews only |

Do not put bearer credentials in URLs, query parameters or discovery records. Reject browser Origin headers on the control endpoint; only native clients are supported. Disable broad CORS. No arbitrary URL fetch or media-file download endpoint.

## 6. Versioned wire contract

### 6.1 Envelope

Define canonical schema/fixtures under MC/protocol/remote/v1/. Copy the same fixtures into R tests with a SHA-256 manifest.

~~~json
{
  "v": 1,
  "id": "request-uuid",
  "method": "player.pause",
  "params": {
    "sessionId": "session-uuid",
    "paused": true
  },
  "expected": {
    "serverEpoch": "boot-uuid",
    "sessionId": "session-uuid"
  }
}
~~~

~~~json
{
  "v": 1,
  "id": "request-uuid",
  "ok": true,
  "result": {
    "status": "accepted",
    "operationId": "operation-uuid"
  }
}
~~~

~~~json
{
  "v": 1,
  "id": "request-uuid",
  "ok": false,
  "error": {
    "code": "stale_state",
    "message": "The queue changed. Review it and try again.",
    "retryable": false,
    "refresh": ["queue"]
  }
}
~~~

- Use dot-separated wire method names, camelCase field names, explicit typed method-specific parameters and snake_case error codes.
- Major version mismatch rejects before commands. Additive optional fields are allowed within v1; unknown method or enum values return unsupported, never silently select a default.
- All protocol counters/revisions are JSON decimal strings to avoid JavaScript integer precision loss. Existing bounded u32 domain counters may be converted to/from decimal strings at the adapter.
- Times use integer milliseconds or explicit seconds fields as named; do not mix durationMs and positionSeconds.
- Reject non-finite numbers, negative positions and volume outside 0–100.
- Session commands require current serverEpoch and sessionId. Queue edits additionally require queue revision.
- Never reuse request IDs for different parameters.

### 6.2 Hello and snapshots

Immediately after authenticated upgrade, server sends hello: major/minor, serverEpoch, deviceId, version, capabilities and snapshot sequence. Then send bounded state snapshots before enabling mutation controls.

Snapshot topics: connection, activity, player, queueSummary, slideshow, outputs, sourcesSummary, vpn, ui and operations. Large libraries, full queues and playlists are paged queries, not unbounded hello payloads.

Each topic has a revision; events have monotonically increasing sequence strings within a serverEpoch. Subscribe before capturing snapshots and buffer bounded events so the client cannot miss an update between fetch and subscription. On overflow/gap send resync_required; stop applying deltas and fetch snapshots.

Player position updates: at most 2 Hz while playing, immediate updates on pause/seek/stop. Phone can interpolate only while connected and playing; it must stop extrapolating when state becomes stale.

Initial state must come from actual native services. It must not use the last phone state as truth.

### 6.3 Command catalogue

Every method below has a schema, scope check, success shape and fixture. Group names are not permission to add unspecified methods.

| Group | Methods | Preconditions/results |
| --- | --- | --- |
| connection | state.get, capabilities.get | Safe authenticated state |
| library | library.query, library.item | Kind/source/query/cursor/limit; opaque IDs |
| player | player.play, player.pause, player.seek, player.volume, player.mute, player.audioTrack, player.subtitleTrack, player.stop | Existing native controls; current session except play |
| queue | queue.get, queue.enqueue, queue.remove, queue.move, queue.playEntry, queue.next, queue.previous, queue.shuffle, queue.repeat | Stable entry IDs and expected revision; never untrusted positional edits |
| playlists | playlists.list, playlists.get, playlists.save, playlists.delete, playlists.play | Expected playlist revision for edit/delete |
| photos | photos.show, photos.slideshow.start, photos.next, photos.previous, photos.pause, photos.interval, photos.shuffle, photos.fit, photos.stop | Active photo session and selection context |
| activity | activity.prepare, activity.confirm, activity.showMenu, activity.showPlayback | Preview replacement, then perform once |
| ui | ui.navigate, ui.moveFocus, ui.activate, ui.back, ui.search | Renderer/context generation and request acknowledgement |
| audio | audio.outputs, audio.prepare, audio.confirm, audio.test | Existing audio configuration, explicit output change and observed result; no display mutation |
| sources | sources.list, folders.roots, folders.children, folders.prepareAdd, folders.confirmAdd, sources.scan, sources.cancelScan, sources.prepareRemove, sources.confirmRemove, sources.confirmReattachment | Bounded capabilities, revision/identity checks |
| drives | drives.list, drives.mount, drives.prepareEject, drives.confirmEject | Revalidate physical device and affected volumes |
| vpn | vpn.state, vpn.countries, vpn.prepareChange, vpn.confirmChange, vpn.requirement.set | Existing policy retained; no automatic protection weakening |
| browser | browser.open, browser.state, browser.pointer, browser.click, browser.scroll, browser.key, browser.text, browser.back, browser.reload, browser.close | Enrolled target + target generation + capabilities |
| operation | operations.get, operations.cancel | Lookup by requestId or operationId (exactly one); own permitted operation; cancellation best effort |
| confirmation | confirmations.get, confirmations.cancel | Owner-bound pending confirmation |
| settings | settings.get, settings.theme.set, settings.startAtLogin.set, settings.endSession | Revision checks; no arbitrary settings key |
| devices | devices.list, devices.revoke, pairing.begin | Trusted phone scope; confirmation for new pairing/revocation |

Navigation keys are a fixed enum. Browser text is distinct from Mediacenter search text; it cannot select an arbitrary OS window.

### 6.4 Limits

- Incoming WebSocket message: maximum 64 KiB including the complete fragmented message.
- Outgoing control/snapshot message: maximum 256 KiB; page data before this limit.
- Maximum 4 authenticated simultaneous client sockets and 16 unauthenticated in-flight handshakes.
- Read/header/authentication timeout: 5 seconds. Idle unauthenticated clients are closed.
- Per connection: 20 normal commands/second with burst 40; pointer updates use a separate coalesced 30 Hz channel; at most 8 expensive work items queued per device.
- Per-client event buffer: 256 messages; resync on overflow.
- Heartbeat ping every 10 seconds; mark disconnected after 30 seconds without response.
- Request acknowledgement timeout: 5 seconds. Accepted operations have operation IDs and individual native timeouts, not a false five-second failure.
- Library query: maximum 200 Unicode characters, page size 50 default/100 maximum, cursor 2048 bytes maximum.
- Phone text payload: 4096 Unicode characters maximum; no logging of text.
- Add-to-queue batch: 100 items maximum. Queue/full playlist reads are paged at 100 items.
- Thumbnail response: at most 512 KiB, 512-pixel longest edge, cached and bounded to 4 simultaneous requests per phone; no original path disclosure.
- Thumbnail cache on phone: 100 MiB LRU, removed on forgetting the device.
- Server validates these limits independently of Android UI restrictions.

### 6.5 Retries, deduplication and operations

Create a persistent command journal for durable mutations, keyed by device ID + request ID, with normalized parameter hash, serverEpoch, status, operation ID and bounded result metadata.

Persist accepted intent before executing a mutation. A repeated ID with the same hash returns the original acceptance/result; another hash returns request_id_conflict. Input-command deduplication is the separate transient exception below. Journal retention is 24 hours, maximum 10,000 records; do not evict running work. Reject new mutations with busy if capacity cannot be safely freed.

Transient navigation, pointer, key and text input use only in-memory, connection-scoped request deduplication (256 most recent IDs). They are never journaled or replayed after disconnect; this includes their parameter hashes, which could leak low-entropy typed secrets. An uncertain text insertion is reported for the user to inspect, never retried automatically.

Exactly-once physical effects across a process crash cannot be promised. On restart, unfinished journal entries become outcome_unknown; inspect native/domain state before deciding success. Do not execute them again automatically.

After disconnect:
- Safe reads can retry.
- Submitted mutations are queried by ID, not blindly replayed.
- New serverEpoch discards pending UI input, drag deltas and key repeats.
- "Next", "Previous", toggle-like actions, service launches and eject never replay automatically.
- Prefer explicit paused/muted/shuffle states over toggle commands.
- Operations expose accepted/running/succeeded/failed/cancelled/outcome_unknown.
- Cancellation means cancellation requested until native completion is observed.
- Keep completed operation metadata for 24 hours within the same journal limit; no sensitive text/credentials in it.

### 6.6 Confirmation model

A ConfirmationPreview contains confirmationId, owner device ID, action kind, affected item/device labels, domain revisions, expiry and a single confirm action. TTL is 60 seconds.

Store the complete action on the server; confirmation requests send only the ID. Bind it to the originating phone, action parameters, session and resource identities. Revalidate immediately before effect. Changed state returns confirmation_stale with a fresh preview, not automatic acceptance.

At most one confirmation sheet is visible per phone. Display local and remote representations of the same confirmation ID if both are needed, but resolving either resolves both. A second click is harmless.

Use confirmations for activity replacement, source removal, device ejection, interruptive VPN changes, phone revocation, browser enrollment and ending a session with active playback. Ordinary pause/seek/search needs none.

## 7. Preserve HDMI mirroring; control existing windows and sound

### 7.1 No new display feature

The user has explicitly confirmed that everything on the computer already appears on the TV through HDMI mirroring. Treat that as the working baseline.

Do not add TelevisionProfile, menuMonitorId, playbackMonitorId, couchScale, television-mode state, a monitor selection wizard or automatic display configuration. Do not move windows between monitors, call xrandr to change layout, alter resolution or require fullscreen for remote use.

Leave existing desktop display preferences and window placement behavior intact. Phone-originated playback uses the same launch path and settings as an equivalent local click.

### 7.2 Optional audio selection

Existing audio should continue working without setup changes. Expose volume/mute first. Audio output selection is an optional item under phone Settings → Sound, using actual discovered sinks.

audio.prepare returns the selected sink and affected owned sessions. audio.confirm revalidates and routes only their audio while preserving the current display target. Extract an audio-only service operation; do not call a combined display/audio function that might reposition a window.

Phone shows requested and confirmed audio outputs separately while a change is pending. If no owned stream exists yet, store the desired output for the next applicable launch and label it as a preference, not a successful move.

For active streams, bounded observation verifies routing. An optional test tone runs through an application-owned stream to the chosen sink. Do not change the global system default.

### 7.3 Foregrounding from the sofa

activity.showMenu foregrounds the existing main window without moving it or stopping playback. activity.showPlayback foregrounds only the verified current player or explicitly enrolled browser target.

The mirrored TV automatically reflects these changes. Return to the current playback position; do not relaunch the item to bring its window forward. Foregrounding must not restore the rejected desktop playback bars.

### 7.4 Connection changes

- HDMI unplug/replug and TV power cycling are hardware/OS display events. Do not try to repair them by reconfiguring mirroring.
- If the current audio output disappears, pause local media and show "Audio output unavailable" with Retry or Choose sound output on phone; do not silently switch to laptop speakers.
- If only display geometry changes, preserve playback and let the existing desktop placement/reachability behavior handle it. Maintain the phone connection.
- Browser audio/pausing capabilities remain explicit; do not falsely claim a pause succeeded.
- Test the actual mirrored setup, resolution changes imposed by the OS and audio hotplug. Extended-desktop support is outside this feature's acceptance scope.

## 8. Remote navigation, library and administration

### 8.1 Navigation bridge implementation rules

Use a FocusRegistry with stable semantic IDs for sidebar entries, cards, rows, tabs and dialog controls. Scope focus to the topmost modal. Horizontal/vertical movement should respect layout; at list boundaries move only along defined section transitions.

A remote activation must reference the current focusContext revision. If a modal opens or content changes after the command was sent, reject rather than click whatever is now under focus.

Hold-to-repeat is generated on Android (450 ms initial delay, 180 ms interval, matching existing Remote behavior). Each command is a bounded move; no persistent OS key-down state. Stop on release, background, disconnect, target change and modal change.

ui.search updates a known search field for a named route. Reject hidden/unrelated field targets. Use phone keyboard composition locally; send committed text with monotonically increasing edit sequence and debounce 150 ms.

### 8.2 Phone library and playback

Use existing library IDs and availability. Never play an item just by accepting a phone-supplied path.

Display offline files as offline without deleting them. Cancel obsolete search work, and ensure a late result for an old query does not replace the latest result.

For queue mutation, resolve entryId to current index under the queue service's serialization gate, verify revision and then execute the existing operation. Do not derive indexes on Android.

Add playlist revision persistence, because current playlist save operations alone are insufficient to resolve concurrent phone/desktop edits. Return a conflict that allows reload or deliberate replacement.

Show still photos using a paused photo presentation with a server-owned selection ID. Snapshot the selected filtered photo order with a maximum 10,000 IDs. Return selection_too_large when necessary instead of silently choosing a partial set.

### 8.3 Folder and source management

Existing native folder-picker grants cannot be called directly from a phone. Build a directory-selection adapter with opaque root and node IDs.

Initial remotely browsable roots:
- Existing authorized library roots.
- The signed-in user's Music, Videos and Pictures directories, resolved through existing default-folder logic.
- Mounted removable media explicitly offered as a candidate root.

Additional arbitrary locations require a one-time local grant. Do not expose the user's entire home directory or system filesystem as a side effect.

folders.children returns directory names and opaque handles only, not hidden credential directories or raw paths. Handles expire after 5 minutes and are bound to device/root identity. Use descriptor-relative, no-escape traversal and revalidate at final addition; simple string prefix checking is insufficient against symlink/race changes.

Remote source addition uses the existing validation/scanning path after producing a valid grant. Show chosen folder, media kinds and scan progress on phone. Source removal removes the library association, never underlying files.

### 8.4 USB and privileges

Use existing UDisks identity and physical-drive impact previews. Device ejection must show all affected volumes and active playback, then stop only owned use and attempt normal safe removal.

If another process keeps the drive busy, report busy. Do not force-unmount.

If mounting requires an OS authorization dialog, return local_authorization_required. Do not leave a remote spinner waiting for a dialog on the laptop or ask for the OS password in Remote. Baseline acceptance uses a volume mountable by the signed-in session.

### 8.5 VPN

Provide current country/state, country selection, connect/disconnect and existing per-service requirements through native services. Refresh state before service launch exactly as today.

Before interruptive VPN change, show the consequence on phone and verify the LAN route is currently working. When a requested VPN change breaks connectivity, reconnect and query the original operation; do not repeatedly issue the change.

LAN setup checks selected interface, peer reachability and documented NordVPN LAN discovery/allowlist behavior [S3]. Offer explicit setup guidance for the actual installed version. Never automatically alter arbitrary subnets, firewall policy, Kill Switch or internet routing.

## 9. Streaming control as a separate integration

### 9.1 Selected design and reason

Use a small explicitly enabled Chrome extension with native messaging to Mediacenter, targeting one enrolled tab at a time. Chrome documents native messaging for extension/native communication [S4] and a tab-targeted debugger API [S5].

Do not use global X11 keyboard/mouse injection as the primary browser control. A focus race could type into an unrelated window. Do not expose a Chrome debugging port on the network.

The extension is an additional component proposed by this plan. It is required for the full streaming milestone, not for local media. Test feasibility early in Task 17 before building the whole streaming UI.

### 9.2 Components and enrollment

- MC/browser-extension/: Manifest V3 extension, service worker, enrollment page, target registry and typed input adapter.
- MC/src-tauri/src/bin/mediacenter-browser-host.rs: native messaging executable with length-framed stdin/stdout.
- MC/src-tauri/src/remote/browser_bridge.rs: local Unix-socket broker, operation routing and state.
- Socket under the signed-in user's runtime directory, parent mode 0700/socket mode 0600; verify peer UID. A random per-installation local broker secret is additionally required.
- Native host registration permits only the stable bundled extension ID; never a wildcard origin.
- Fixed public extension key stabilizes the development extension ID; never commit a signing private key.
- One-time installation is explicit in every Chrome profile used for remote streaming. Validate distribution using actual Google Chrome, not only Chromium or Chrome for Testing. Chrome documents local unpacked installation for development [S6].
- Extension identity/configuration is stored per profile; do not copy cookies/profile databases between profiles.

At setup the user enrolls the extension instance/profile with Mediacenter. On each service launch the broker issues a short-lived random launch nonce bound to service ID and selected enrolled profile. The extension creates a dedicated browser window/tab for that catalog service and reports its target identity; unrelated matching tabs must not be auto-enrolled by URL search.

For managed profiles, use a bundled extension launch page carrying only that local nonce, redeemed through the native broker. For ordinary Chrome/HBO, use the already enrolled ordinary-profile extension to create a dedicated window. This retains ordinary-profile login state without claiming ownership of the Chrome process.

The extension may close only its dedicated enrolled tab/window and must not terminate ordinary Chrome. When an enrolled window is no longer dedicated, close only the enrolled tab after confirmation.

### 9.3 Input and capabilities

The phone sends browser.pointer normalized coordinates, click, scroll, fixed key enum and committed text. Extension maps these to a deliberately restricted subset of tab-targeted input operations. No arbitrary script/CDP method is accepted from the phone.

Use debugger permission only for this feature, with its visible Chrome permission/attachment indications intact [S5]. Do not suppress Chrome security UI.

Input references both browserSessionId and targetGeneration. Navigation, tab replacement, loss of enrollment, debugger detach and browser restart invalidate the generation.

Pointer coordinates are relative to the browser content viewport, clamped to 0–1 and converted using the current viewport metrics. Display a small visible pointer marker in the enrolled tab while touchpad mode is active. Do not send screenshots/video to the phone.

Generic key set: Enter, Escape, Tab, ShiftTab, ArrowUp, ArrowDown, ArrowLeft, ArrowRight, Backspace and Space. Search/password text goes through committed text insertion, is never persisted/logged and is not copied to the system clipboard.

Semantic play/pause/seek/volume and metadata are optional capabilities, advertised only for verified adapters. For unknown sites, show touchpad and website-native controls. Never display a successful native-player pause action for a Chrome tab.

### 9.4 Authentication, redirects and protected UI

Allow only service catalog origins and explicitly reviewed authentication origins. Maintain that list per service. Do not accept arbitrary URLs supplied by the phone.

An unexpected top-level origin stops text input and presents an approval/rejection on the paired phone with the hostname. Approval grants only that session's specific origin; no permanent universal origin permission.

Handle normal page redirects, same-tab authentication and related popup flows with explicit target transitions. A popup is not automatically an enrolled control target merely because it appeared.

Browser chrome, extension install dialogs, password-manager popups and OS dialogs are outside tab-content input. Document one-time setup requirements and provider-specific exceptions.

Do not read stored passwords, cookies or authentication tokens. Do not record typed passwords or full URL query strings in operation journals, logs or fixtures.

### 9.5 Mirrored display and ordinary Chrome boundaries

Create/foreground the enrolled browser window normally, without choosing a monitor or modifying the display layout. The existing HDMI mirror shows the browser on TV. Pointer coordinates are browser-content coordinates; changes in viewport size invalidate stale pointer input until fresh metrics arrive.

Managed Chrome can retain existing process-scoped audio routing. Ordinary Chrome/HBO is not safe to route by "all Chrome PIDs": use verified per-tab media-element volume when supported and require one-time correct desktop/HDMI audio configuration otherwise. Report audio-routing capability truthfully.

This is a specific limitation of ordinary-profile streaming. The full acceptance report must state whether HBO's HDMI audio and pointer/keyboard flows work from the sofa after setup. If not, leave that service partial instead of weakening ownership boundaries.

## 10. Background lifetime, startup and sleep

- Remote server runs inside the Mediacenter process, not a root daemon.
- With remote access enabled, ordinary window Close offers "Keep available for Remote"; remember that choice. Hiding the UI must not stop the listener.
- Phone "End session" stops owned playback after confirmation, releases any session inhibitor and leaves the server available.
- "Quit Mediacenter" is a separate desktop/advanced action and warns that phone reconnection requires relaunch. Do not silently expose it as the ordinary phone Stop button.
- Opt-in XDG desktop-session autostart launches the existing binary with --remote-background. This starts an ordinary user process after graphical login, creates native services and initially hides the menu window.
- Existing single-instance behavior must show the existing UI when the desktop launcher is clicked, without starting another listener.
- No automatic login, OS lock bypass or new privileged service. Crash recovery from a fully terminated process is not promised by an in-process server.
- While an explicit remote viewing session has active playback, request supported session idle inhibition if needed to prevent the viewing session blanking. Release on End session or when playback ends; do not add a TV mode or alter global sleep/lock settings.
- If the desktop is explicitly locked, disable renderer/browser input and report desktop_locked. Do not try to unlock. Native pause/stop may remain available.
- Android backgrounding stops discovery and input repeats; close the MC socket after 5 seconds unless foreground returns. Playback continues. Foreground reconnect performs authentication and a complete resync.

## 11. Storage, privacy and release compatibility

Add a sequential SQLite migration (next version after inspected baseline) for paired devices, command journal, playlist revisions and remote preferences. Use current transactional migration/backup behavior.

Do not put secrets into generic exported settings or diagnostic reports. Redact tokens, pairing QR payloads, text input, file paths, query strings and media names from transport diagnostic logs. Diagnostic events may include method name, anonymous device ID, duration and error code.

Phone retains connection settings and encrypted credentials, not an offline clone of the full library. Clear per-device thumbnails and transient metadata on forget.

Remote and MC have independent version numbers; negotiate protocol major/minor and capabilities. An old phone must show an actionable version mismatch before offering unsupported actions.

Ship a compatibility manifest containing MC revision/version, R revision/version, protocol schema hash, extension version/ID and native host version. A browser bridge failure must not break local playback.

Existing working-tree modifications are outside this plan. At execution start inspect both repositories, preserve unrelated changes and create isolated branches/worktrees if necessary. Do not commit another task's streaming evidence or UI experiments accidentally.

## 12. Reference sources and evidence limits

External documentation was checked while preparing the plan. Most design details, limits and architecture choices are project proposals, not claims made by these sources.

- **S1:** [Android network service discovery](https://developer.android.com/develop/connectivity/wifi/use-nsd) — DNS-SD service discovery and resolution.
- **S2:** [Android local network permission](https://developer.android.com/privacy-and-security/local-network-permission) — permission-aware LAN access; relevant to this repository's API-37 target.
- **S3:** [NordVPN Linux setup and commands](https://support.nordvpn.com/hc/en-us/articles/20196094470929-How-to-install-the-NordVPN-app-on-Linux-distributions) — LAN discovery/allowlist interaction.
- **S4:** [Chrome native messaging](https://developer.chrome.com/docs/extensions/develop/concepts/native-messaging) — extension-to-native channel and explicit allowed extension origins.
- **S5:** [Chrome debugger API](https://developer.chrome.com/docs/extensions/reference/api/debugger) — tab-targeted protocol transport, required permission and restricted domains.
- **S6:** [Chrome extension local setup](https://developer.chrome.com/docs/extensions/get-started/tutorial/hello-world) — explicit developer installation.
- **S7:** [Chrome tabs API](https://developer.chrome.com/docs/extensions/reference/api/tabs) — tab management and permission distinctions.

No physical-device test, build, dependency installation, VPN modification, pairing, browser enrollment or application-code change was performed to prepare this plan.

## 13. File and interface map

Paths marked new do not exist yet; they are implementation targets, not evidence of current functionality.

| Area | Create | Modify/integrate |
| --- | --- | --- |
| Wire contract | MC/protocol/remote/v1/{schema.json,fixtures.json,manifest.json}; MC/scripts/verify-remote-contract.mjs; MC/src-tauri/src/remote/protocol.rs; RK/mediacenter/protocol/{Messages.kt,RemoteJson.kt} | MC/package.json; R/app/src/test/resources/mediacenter-v1.json |
| Shared control services | MC/src-tauri/src/control/{mod.rs,activity.rs,playback.rs,audio.rs,library.rs,settings.rs,confirmations.rs} | MC/src-tauri/src/commands.rs and existing native services |
| Native state | MC/src-tauri/src/remote/{state.rs,event_hub.rs,operations.rs} | MC/src-tauri/src/{app_state.rs,events.rs,operations.rs}; player/music/photo publication points |
| Storage | MC/src-tauri/migrations/0002_remote_control.sql; MC/src-tauri/src/remote/store.rs | MC/src-tauri/src/storage/database.rs |
| Server | MC/src-tauri/src/remote/{mod.rs,server.rs,dispatch.rs,limits.rs,network.rs,discovery.rs,tls.rs,pairing.rs,auth.rs} | MC/src-tauri/{Cargo.toml,Cargo.lock}; MC/src-tauri/src/lib.rs |
| Setup UI | MC/src/features/settings/RemoteSettings.tsx; MC/src/features/remote/{remoteApi.ts,PairingView.tsx} | MC/src/features/settings/SettingsScreen.tsx; MC/src/app/contracts.ts; MC/src-tauri/build.rs; MC/src-tauri/capabilities/main.json |
| Android connection | RK/mediacenter/{MediacenterClient.kt,MediacenterEndpoint.kt,MediacenterDiscovery.kt,MediacenterCredentialStore.kt,MediacenterTrust.kt,MediacenterConnection.kt,MediacenterRepository.kt} | RK/AppContainer.kt; R/app/src/main/AndroidManifest.xml; R/gradle/libs.versions.toml |
| Android setup | RK/ui/mediacenter/{PairMediacenterScreen.kt,PairMediacenterViewModel.kt,ConnectionStatus.kt} | RK/MainActivity.kt; RK/ui/ConnectionScreen.kt |
| Shared target shell | RK/ui/{ControlTarget.kt,ControlTargetViewModel.kt,TargetSelector.kt} | RK/ui/{RemoteTab.kt,MainScreenRoutes.kt}; RK/MainActivity.kt; existing LG routes |
| Phone library/control | RK/ui/mediacenter/{MediacenterRemoteScreen.kt,MediacenterViewModel.kt,LibraryScreen.kt,LibraryViewModel.kt,NowPlayingControls.kt,QueueSheet.kt,PlaylistScreen.kt,PhotoControls.kt,ConfirmationSheet.kt,TextInputSheet.kt} | Existing shared Remote button/theme components only where reusable |
| Renderer navigation | MC/src/features/remote/{RemoteNavigationBridge.tsx,focusRegistry.ts,remoteNavigation.ts} | MC/src/app/App.tsx; MC/src/ui/{Sidebar.tsx,Dialog.tsx}; library row/card components |
| Sources/drives/settings | MC/src-tauri/src/remote/folders.rs; RK/ui/mediacenter/{SettingsScreen.kt,AudioSettings.kt,FolderBrowser.kt,SourcesScreen.kt,DrivesScreen.kt,VpnSettings.kt} | Existing native source/device/VPN services and their tests |
| Browser integration | MC/browser-extension/{manifest.json,service-worker.js,target-registry.js,input.js,enroll.html,enroll.js}; MC/src-tauri/src/bin/mediacenter-browser-host.rs; MC/src-tauri/src/remote/{browser_bridge.rs,browser_protocol.rs}; RK/ui/mediacenter/BrowserControls.kt | MC/src-tauri/src/streaming/{browser.rs,sessions.rs,external.rs}; packaging |
| Lifecycle | MC/src-tauri/src/remote/{lifecycle.rs,autostart.rs,idle.rs}; MC/packaging/mediacenter-remote-autostart.desktop.in | MC/src-tauri/src/{main.rs,lib.rs,lifecycle.rs}; RK/MainActivity.kt |
| Evidence | MC/docs/engineering/remote-control.md; MC/docs/engineering/evidence/remote-control/; R/docs/validation/mediacenter-control.md | Both repository onboarding/build documentation |

Do not create empty placeholder modules for all rows at once. Create each file when its owning task supplies behavior and a meaningful test.

### 13.1 Core signatures

These signatures name boundaries; domain structs are typed and validated in Task 1. Do not substitute unvalidated JSON maps in production services.

Rust:
- RemotePrincipal { device_id: String, scopes: ScopeSet }
- CommandContext { principal: RemotePrincipal, request_id: String, server_epoch: String }
- async dispatch(ctx: &CommandContext, request: RemoteRequest, services: &ControlServices) -> Result<RemoteReply, RemoteError>
- async snapshot(principal: &RemotePrincipal, services: &ControlServices) -> Result<RemoteSnapshot, RemoteError>
- async subscribe_with_snapshot(principal: &RemotePrincipal, services: &ControlServices) -> Result<RemoteSubscription, RemoteError>
- async prepare_activity(ctx: &CommandContext, intent: ActivityIntent, services: &ControlServices) -> Result<ActivityPreparation, RemoteError>
- async confirm_action(ctx: &CommandContext, confirmation_id: &str, services: &ControlServices) -> Result<OperationRef, RemoteError>
- async apply_player_action(ctx: &CommandContext, session_id: &str, action: PlayerAction, services: &ControlServices) -> Result<(), RemoteError>
- async apply_audio_output(ctx: &CommandContext, request: AudioOutputRequest, services: &ControlServices) -> Result<OperationRef, RemoteError>
- async directory_children(ctx: &CommandContext, node_id: &str, cursor: Option<&str>, services: &ControlServices) -> Result<DirectoryPage, RemoteError>
- async send_ui_request(ctx: &CommandContext, request: UiRequest, bridge: &RendererBridge) -> Result<UiAck, RemoteError>
- async send_browser_input(ctx: &CommandContext, input: BrowserInput, bridge: &BrowserBridge) -> Result<BrowserAck, RemoteError>

Kotlin:
- interface MediacenterControl { val state: StateFlow<MediacenterState>; suspend fun connect(deviceId: String): McResult<Unit>; suspend fun request(request: McRequest): McResult<McReply>; fun disconnect() }
- interface MediacenterPairing { suspend fun pair(candidate: PairingCandidate): McResult<PairedMediacenter> }
- MediacenterRepository owns topic revisions, epoch, cached pages and pending-operation lookup; UI never calls OkHttp directly.
- ControlTarget = LgTv | Mediacenter(deviceId: String); it is a UI target, not an overloaded TvTarget.
- McResult = Success<T> | Failure(McError); McError carries code, safe message, retryability and required refresh topics.

TypeScript:
- createFocusRegistry(): FocusRegistry
- handleRemoteNavigation(request: UiRequest, registry: FocusRegistry, navigation: NavigationController): Promise<UiAck>
- NavigationController has navigate(route), back(), setSearch(route, text), getContext() and activate(focusId).
- Navigation context contains rendererGeneration, route, modalId, focusId and focusRevision.

### 13.2 Test commands and task convention

Commands below are intended for future implementation. They were not run during planning.

- MC Rust: source scripts/dev-env.sh, then cargo test --manifest-path src-tauri/Cargo.toml TEST_FILTER.
- MC UI: npm test -- tests/unit/FILE.test.tsx.
- MC browser workflows: npm run test:e2e -- tests/e2e/remote-control.spec.ts.
- Protocol: node scripts/verify-remote-contract.mjs --remote-root ../Remote.
- R JVM: ./gradlew :app:testDebugUnitTest --tests 'com.emmah.colordock.remote.PACKAGE.TestClass'.
- R device: ./gradlew :app:connectedDebugAndroidTest with the named test class selected through the instrumentation runner argument.
- Browser bridge: node --test browser-extension/tests/*.test.mjs.

Every task follows a red/green cycle: add the named behavioral test, run it and observe the expected missing behavior, implement the stated boundary, rerun that test and its affected regression suite, then commit only task-owned files in the relevant repository. A failing environment prerequisite is not the expected red test and must be resolved first.

For cross-repository tasks, make separate commits and record their paired SHAs in the compatibility manifest. Do not attempt an atomic Git commit across repositories.

## 14. Ordered implementation tasks

### Task 1: Establish v1 contracts and cross-language fixtures

**Depends on:** None.
**Files:** Create protocol/schema/fixtures, protocol.rs, Messages.kt, RemoteJson.kt and verification script from section 13. Modify MC/package.json to add verify:remote-contract.
**Tests:** MC/src-tauri/src/remote/protocol_tests.rs; RT/mediacenter/protocol/RemoteProtocolTest.kt.
**Consumes:** Sections 5–6 and 13.1.
**Produces:** RemoteRequest/RemoteReply/RemoteError, all DTOs named in 13.1, schema hash and deterministic JSON fixtures.

- [ ] Define fixtures for hello, empty/populated snapshots, each method, typed errors, paging, confirmation, journal outcomes and browser capabilities.
- [ ] Add tests rejecting unknown major version, unknown action enums, excess input, missing session/epoch, non-finite numeric values, out-of-range volume and invalid decimal revisions. Confirm an unknown optional field is tolerated.
- [ ] Run Rust protocol tests and RemoteProtocolTest; observe rejection/round-trip failures before implementing the parser.
- [ ] Implement strict typed validation and explicit snake_case/camelCase translations from existing native DTOs. Make all public counters string-valued on wire.
- [ ] Run both suites and verify:remote-contract against copied fixtures; require identical canonical schema hash and stable round trips.
- [ ] Commit contract and fixture changes in both repositories. No server exposure yet.

### Task 2: Add transactional persistence without losing existing data

**Depends on:** Task 1.
**Files:** New 0002_remote_control.sql and remote/store.rs; modify storage/database.rs and music/playlists.rs.
**Tests:** Inline remote::store::tests and storage migration tests.
**Consumes:** PairedDevice, JournalEntry, PlaylistRevision and RemotePreferences DTOs.
**Produces:** RemoteStore::{add_device,revoke_device,lookup_credential,accept_command,finish_command,get_command,prune}; revision-aware playlist persistence.

- [ ] Add a schema-1 fixture containing sources, playlist content, resume positions, queue and unrelated settings.
- [ ] Add migration tests proving all fixture data survives, migration is idempotent, version-2 opens correctly and an interrupted migration rolls back with usable backup.
- [ ] Add tests for revocation lookup, token-hash-only storage, request-ID hash conflict, journal retention bounds, protected running entries and restart marking unfinished outcomes unknown.
- [ ] Implement sequential version checks; current code's "version > 1" rejection must be updated deliberately.
- [ ] Run cargo test with storage and remote::store filters, including failed migration rollback.
- [ ] Commit migration/store changes. Keep server disabled by default for upgraded installations.

### Task 3: Expose native snapshots, operations and event subscriptions

**Depends on:** Tasks 1–2.
**Files:** New control/mod.rs and remote/{state,event_hub,operations}.rs; modify events.rs, operations.rs, app_state.rs and state publishers in player/music/photos.
**Tests:** remote::state::tests; remote::operations::tests; remote::event_hub::tests.
**Consumes:** Current native state and RemoteStore.
**Produces:** A read-only ControlServices facade over AppHandle/AppState, snapshot, subscribe_with_snapshot, OperationRegistry and revisioned topic events. Later tasks add typed mutation coordinators to the same facade.

- [ ] Add a test where player state changes between subscription and snapshot capture; the consumer must receive it exactly once or resync, never miss it.
- [ ] Add tests for out-of-order events, 257th queued event, independent slow clients and operation lookup after the UI listener disconnects.
- [ ] Add lost-reply test: the same request ID produces one native effect and returns the original operation ID.
- [ ] Implement bounded publication outside native locks. Coalesce position changes to 2 Hz while retaining immediate control-state transitions.
- [ ] Extend operation handling to retain terminal status and publish it to both existing UI and remote consumers; accepted is not completed.
- [ ] Run focused tests plus existing playback/music tests; commit.

### Task 4: Implement server identity, pairing and revocation

**Depends on:** Tasks 1–2.
**Files:** New remote/{tls,pairing,auth}.rs and corresponding test modules; update Cargo.toml/Cargo.lock.
**Tests:** remote::pairing::tests; remote::auth::tests; remote::tls::tests.
**Consumes:** RemoteStore and pairing contract.
**Produces:** ServerIdentity, PairingManager, authenticate_bearer and revoke_device behavior.

- [ ] Select maintained TLS, certificate-generation and CSPRNG libraries compatible with the pinned toolchain; record exact versions in Cargo.lock and an implementation note. Use constant-time secret comparison.
- [ ] Add tests for certificate identity persistence, key-file permissions, expired/consumed secret, five failed attempts, device limit, wrong pin, revoked device and absent pairing window.
- [ ] Add manual-pairing tests requiring certificate-phrase confirmation before accepting a code; verify QR pin validation precedes secret transmission.
- [ ] Implement the 120-second monotonic TTL, single-use issuance and hashed desktop credentials. Use a controllable clock/RNG in tests only.
- [ ] Test revocation closing an existing authenticated subscription, not just denying new connections.
- [ ] Run focused tests and commit. No trust-all TLS fallback or token logging.

### Task 5: Add the bounded authenticated LAN server

**Depends on:** Tasks 1–4.
**Files:** New remote/{mod,server,dispatch,limits}.rs; modify lib.rs and Cargo dependencies.
**Tests:** remote::server::tests and remote::dispatch::tests using loopback TLS.
**Consumes:** PairingManager, authentication, subscription and service interfaces.
**Produces:** Start/stop listener and the three endpoints in section 5.5.

- [ ] Add tests proving unauthenticated peers cannot obtain metadata, oversized fragmented frames are rejected, wrong Origin is rejected and limits are enforced before dispatch.
- [ ] Add tests that unknown methods cannot invoke existing privileged Tauri commands and scope failures have no native side effect.
- [ ] Implement TLS WebSocket with bounded buffers, ping deadlines, explicit method dispatch and cancellation on server stop.
- [ ] Integrate durable request journaling; keep transient UI/browser/text inputs connection-local and out of persistent hashes/logs.
- [ ] Test lost replies and duplicate commands; test one slow phone does not block desktop playback or another phone.
- [ ] Run focused server tests and existing command-guard tests; commit.

### Task 6: LAN binding, discovery and desktop pairing UI

**Depends on:** Tasks 4–5.
**Files:** New remote/{network,discovery}.rs; RemoteSettings.tsx, PairingView.tsx, remoteApi.ts; modify SettingsScreen.tsx, build.rs, capabilities/main.json and generated types.
**Tests:** remote::network::tests; tests/unit/remote-settings.test.tsx; tests/unit/pairing-view.test.tsx.
**Consumes:** Listener lifecycle, identity and PairingManager.
**Produces:** Explicit network enablement, DNS-SD advertisement and visual pairing ceremony.

- [ ] Test interface selection excludes tunnel/loopback/container defaults, port conflict is actionable and interface disappearance closes/rebinds without losing identity.
- [ ] Test enabling remote control is explicit, QR expires visibly, disabling stops advertising/listening and setup has no monitor-selection or TV-mode UI.
- [ ] Implement read-only network diagnosis and minimal local Tauri setup commands: get_remote_settings, set_remote_enabled, begin_remote_pairing, cancel_remote_pairing, list_remote_devices and revoke_remote_device.
- [ ] Register those commands in build.rs and lib.rs; grant only main-window permissions. Presentation window must retain its limited capabilities.
- [ ] Render QR locally from pairing payload; do not use a remote QR website. Add manual pairing display and accessibility text that does not expose secrets to logs.
- [ ] Run tests and capability checks; commit.

### Task 7: Add an independent Android client and pairing flow

**Depends on:** Tasks 1, 4–6.
**Files:** New RK/mediacenter connection/trust/discovery/credential files and ui/mediacenter pairing files; modify AppContainer.kt and AndroidManifest.xml only as needed.
**Tests:** RT/mediacenter/{MediacenterEndpointTest,MediacenterTrustTest,MediacenterClientTest,MediacenterDiscoveryTest}.kt; RAT/mediacenter/MediacenterCredentialStoreTest.kt.
**Consumes:** v1 fixtures, pairing QR/manual formats and discovery records.
**Produces:** MediacenterControl, MediacenterPairing and persisted PairedMediacenter.

- [ ] Test private LAN endpoint validation, selected Network socket binding, IPv6 bracket/scope handling and refusal of arbitrary public endpoint redirects.
- [ ] Test denial/revocation of LAN permission separately from camera denial. Camera denial must retain manual pairing.
- [ ] Implement a dedicated OkHttp client with pinned server identity, no redirects/proxy fallback and no reuse of the LG-specific endpoint policy.
- [ ] Implement Android NSD [S1] and current LAN permission handling [S2]; use discovery results only as address candidates for pinned devices.
- [ ] Implement camera scanning with a maintained on-device QR library; add explicit camera permission only if required by that chosen scanner. No image upload.
- [ ] Test Keystore-wrapped storage on device, process restart and unchanged LG credentials; commit.

### Task 8: Reconnect, resync and uncertain command recovery

**Depends on:** Tasks 3, 5, 7.
**Files:** New/extend MediacenterConnection.kt, MediacenterRepository.kt, ConnectionStatus.kt and remote operations/state tests.
**Tests:** RT/mediacenter/{ReconnectTest,RemoteStateReducerTest,RequestRecoveryTest}.kt.
**Consumes:** MediacenterControl and event/operation protocol.
**Produces:** Stable connected/connecting/reconnecting/permissionRequired/pairingRequired/identityChanged/failed states.

- [ ] Test same-device address change, Wi-Fi switch, app background beyond five seconds, process recreation and new serverEpoch.
- [ ] Test an accepted Next whose reply is lost: reconnect queries its request/operation and does not skip again.
- [ ] Test revoked credentials do not trigger endless retries; changed pin never replaces the stored pin automatically.
- [ ] Implement foreground retry delays 1, 2, 5, 10 and 10 seconds with jitter capped at 20%; after that show a manual Retry action.
- [ ] Disable mutations until initial snapshots are applied; reject sequence regressions and resync on gaps.
- [ ] Run reducer/reconnect tests with a fake clock and MockWebServer; commit.

### Task 9: Native playback, queue, playlist and photo adapters

**Depends on:** Tasks 1–3, 5.
**Files:** New control/{playback,activity,confirmations}.rs; extend control/mod.rs; modify commands.rs, player/mpv.rs, music/{service,queue,playlists}.rs and photos/presenter.rs only where sharing requires it.
**Tests:** control::playback::tests; control::activity::tests; remote::media::tests.
**Consumes:** Existing PlayerAction, MusicAction, slideshow services and command context.
**Produces:** Native media command catalogue, ActivityPreparation and shared confirmation execution.

- [ ] Test pause/seek/volume/track validation, unavailable media, stale session, stale queue revision and conflict on playlist edits.
- [ ] Test entry-ID resolution under serialization with concurrent desktop reorder; the requested entry, not its old index, must be changed.
- [ ] Test activity preflight failure preserves current playback and that confirmed switching stops only the previously previewed activity.
- [ ] Extract only necessary coordination from Tauri wrappers; local and remote paths must retain identical validation and process ownership.
- [ ] Add still-photo selection/presentation and paged full queue/playlist reads; do not invent metadata absent from source files.
- [ ] Run native media regression tests plus adapter tests; commit.

### Task 10: Audio-only changes and mirrored-window foregrounding

**Depends on:** Tasks 8–9.
**Files:** New control/audio.rs and RK/ui/mediacenter/AudioSettings.kt; modify outputs/audio.rs and activity service; integrate existing window focus methods without altering display configuration.
**Tests:** control::audio::tests; control::activity::tests; RAT/ui/mediacenter/AudioSettingsTest.kt.
**Consumes:** Existing audio discovery/routing, active native session and current mirrored desktop.
**Produces:** apply_audio_output, audio.outputs/prepare/confirm/test, activity.showMenu/showPlayback.

- [ ] Write a regression test that remote Play and Show menu do not change monitor selection, resolution, mirroring or stored main geometry.
- [ ] Test audio-only routing does not invoke picture/window-placement functions. Test stale sink, missing initial stream and audio hotplug.
- [ ] Implement optional audio preference update and verified routing for owned sessions. Keep requested/effective states distinct.
- [ ] Implement showMenu by showing/focusing the existing main window; implement showPlayback through existing verified session handles without relaunch.
- [ ] Test disappearance of the active HDMI sink pauses local media and does not route to laptop speakers automatically.
- [ ] Run targeted tests and record a later physical mirrored-HDMI check; commit. Do not create a TV-mode module.

### Task 11: D-pad navigation, text input and phone confirmations

**Depends on:** Tasks 3, 5, 9–10.
**Files:** New renderer bridge/focus registry files; modify App.tsx, navigation.ts, Sidebar.tsx, Dialog.tsx and actionable library components; add a narrow renderer acknowledgement Tauri command.
**Tests:** tests/unit/{remote-navigation,remote-focus,remote-confirmations}.test.tsx.
**Consumes:** UiRequest/UiAck, current navigation state and ConfirmationPreview.
**Produces:** Semantic remote navigation without OS-wide key injection.

- [ ] Test movement through sidebar/list/grid, scrolling focused elements into view, modal trapping, Back behavior and focus restoration after item removal.
- [ ] Test Activate with a stale focusRevision cannot activate a newly opened dialog button.
- [ ] Test search composition and out-of-order edit sequences; a late old query must not overwrite a newer query.
- [ ] Implement renderer generation/acknowledgement timeout and a no-renderer error. Do not report success merely after emitting a Tauri event.
- [ ] Render the same server confirmation ID on phone and desktop when appropriate; a single accepted response resolves both.
- [ ] Run existing dialog/navigation tests and new tests; commit. Confirm no new persistent player strip exists.

### Task 12: Phone library, now playing, queue and photos

**Depends on:** Tasks 8–11.
**Files:** New ui/mediacenter library/control components from section 13.
**Tests:** RT/ui/mediacenter/{LibraryViewModelTest,MediacenterViewModelTest}.kt; RAT/ui/mediacenter/{LibraryScreenTest,NowPlayingTest,QueueTest,PhotoControlsTest}.kt.
**Consumes:** MediacenterRepository, media commands, selection contexts and confirmation DTOs.
**Produces:** Complete R03–R06 phone workflows.

- [ ] Test search cancellation, paging, empty/offline/error states, resume/start-over choice and return-to-playback.
- [ ] Test exactly one transport-control group on the Remote page and no duplicated playback controls on Library or Settings.
- [ ] Implement queue sheet, stable-ID reorder/remove, shuffle/repeat and save/edit playlist with conflict refresh.
- [ ] Implement bounded thumbnail loading/caching and clear cache on device forget.
- [ ] Implement still-photo selection, previous/next and slideshow configuration. Keep interval within 1–60 seconds; reject excessive selections explicitly.
- [ ] Run ViewModel and Compose tests at phone portrait/landscape and large font size; commit.

### Task 13: Phone folder selection and source management

**Depends on:** Tasks 2, 5, 9, 12.
**Files:** New remote/folders.rs; extend control/library.rs; add FolderBrowser.kt and SourcesScreen.kt; integrate library/source_service.rs.
**Tests:** remote::folders::tests; control::library::tests; RAT/ui/mediacenter/FolderBrowserTest.kt.
**Consumes:** Existing source grants, default folders, mounted volume identity and confirmation service.
**Produces:** Opaque directory browsing and phone-authorized source operations.

- [ ] Test no arbitrary path input, absolute/traversal rejection, expired handles, cross-device handle use and symlink replacement between browse and confirm.
- [ ] Test only authorized roots appear; hidden/system credential locations are not exposed through a parent traversal.
- [ ] Implement 5-minute node handles, 100-directory pages and descriptor-relative revalidation before issuing the native source grant.
- [ ] Add scan/cancel/progress, association-only source removal and explicit UUID-less reattachment confirmation.
- [ ] Test source disconnect during scan preserves metadata and reports an accurate recovery.
- [ ] Run source/default-folder regression tests and phone folder UI tests; commit.

### Task 14: Phone USB mounting and safe removal

**Depends on:** Tasks 9, 12–13.
**Files:** Extend control/library.rs/confirmations.rs and remote dispatch; add DrivesScreen.kt; reuse devices/actions.rs and lifecycle.rs.
**Tests:** remote::drives::tests; RAT/ui/mediacenter/DrivesScreenTest.kt.
**Consumes:** Existing physical drive preview, volume identity and SourceLifecycle behavior.
**Produces:** drives.list/mount/prepareEject/confirmEject with phone confirmation.

- [ ] Test preview contains all affected sibling volumes and owned active use.
- [ ] Test drive replacement/replug between preview and confirm invalidates the old confirmation.
- [ ] Implement normal mount/eject via existing services; return local_authorization_required for privileged OS prompts.
- [ ] Test unrelated-process busy failure, cancellation, phone disconnect after accepted eject and terminal outcome lookup.
- [ ] Verify no force flag, arbitrary device path or filesystem deletion enters the remote API.
- [ ] Run fake-backend tests; defer real eject to an explicitly selected disposable test drive in acceptance; commit.

### Task 15: VPN and routine settings from phone

**Depends on:** Tasks 6, 8–9, 12.
**Files:** New control/settings.rs; add VpnSettings.kt and SettingsScreen.kt; integrate vpn/nordvpn.rs and policy.rs.
**Tests:** control::settings::tests; remote::vpn::tests; RAT/ui/mediacenter/VpnSettingsTest.kt.
**Consumes:** Native VPN observation/policy, confirmations and operation lookup.
**Produces:** VPN state/country actions, per-service requirements, theme and safe connection settings.

- [ ] Test country changes retain existing launch-policy checks and stale VPN observations remain unknown.
- [ ] Test a VPN change that drops LAN returns an uncertain operation on reconnect rather than issuing a second change.
- [ ] Implement phone previews for interruptive changes and preserve Kill Switch policy; never add "disable protection to connect" automation.
- [ ] Add redacted diagnostics for LAN permission, selected interface, listener and discovery state without exposing tokens or private media.
- [ ] Test denial of unsupported setting keys and phone scope failures.
- [ ] Run VPN policy/settings regressions; commit. Real VPN change remains an acceptance action on the user's chosen test session.

### Task 16: Integrate target switching without breaking LG/Color Dock

**Depends on:** Tasks 7–8, 12, 15.
**Files:** New ControlTarget.kt, ControlTargetViewModel.kt and TargetSelector.kt; modify MainActivity.kt, RemoteTab.kt, MainScreenRoutes.kt and shared button components.
**Tests:** RT/ui/ControlTargetViewModelTest.kt; RAT/ui/{ControlTargetTest,ExistingTvRegressionTest}.kt.
**Consumes:** Independent LG and MC connection states.
**Produces:** One device selector and target-specific destinations.

- [ ] Test MC connected/LG disconnected and the reverse; one must not gate the other's controls.
- [ ] Test target changes during held button, pending operation and open confirmation. Cancel repeats; never reinterpret a queued TV command as MC input.
- [ ] Preserve existing LG Remote/Wallpapers/Color Dock routes and TV app shortcuts.
- [ ] Implement MC Remote/Library/Settings navigation without app-wide extra feature tabs.
- [ ] Verify volume and power labels identify their target; TV power cannot shut down the PC.
- [ ] Run existing RemoteScreen/CompanionScreens tests and new target tests; commit.

### Task 17: Prove the browser bridge on the real Chrome build

**Depends on:** Tasks 1, 4–5; schedule this feasibility gate before investing in Tasks 18–19.
**Files:** Initial browser-extension files, native host binary, browser_protocol.rs and browser_bridge.rs; add docs/engineering/evidence/remote-control/browser-feasibility.md.
**Tests:** browser-extension/tests/{enrollment,native-framing,target-lifetime}.test.mjs; remote::browser_bridge::tests.
**Consumes:** Local broker authentication, typed BrowserInput and explicit enrolled profile.
**Produces:** A demonstrated tab-scoped input path and documented distribution/setup procedure.

- [ ] Test native framing, invalid extension origin, wrong broker secret/peer UID, oversized messages and stale launch nonce.
- [ ] Implement a minimal extension/native host with enroll, create dedicated catalog tab, key/text insertion and detach handling.
- [ ] On a disposable Chrome profile, prove ordinary text/Unicode input and pointer click work in a controlled local test page.
- [ ] Verify the actual installed Google Chrome can retain the extension across restart/profile relaunch and connect its registered native host. Do not rely on unsupported command-line extension-loading flags.
- [ ] Prove input stays in the enrolled tab when another application/window gets focus. Prove unrelated tabs are unchanged.
- [ ] Record extension warning behavior, service-worker reconnection and app-window/profile limitations. Commit the bounded proof and tests.
- [ ] If this gate fails, keep local-media work usable and revise the streaming design with evidence. Do not silently replace it with global desktop input or mark streaming complete.

### Task 18: Implement browser session control and phone touchpad/keyboard

**Depends on:** Tasks 8, 16–17.
**Files:** Complete extension target-registry/input modules and broker; add BrowserControls.kt and TextInputSheet.kt; integrate streaming session commands.
**Tests:** browser-extension/tests/{input,origin-policy,popup-transition}.test.mjs; RAT/ui/mediacenter/BrowserControlsTest.kt; remote::browser_input::tests.
**Consumes:** Browser session ID, targetGeneration, viewport metrics and supported capabilities.
**Produces:** Browser command catalogue and contextual phone control surface.

- [ ] Test pointer coordinate conversion, resize generation changes, 30 Hz coalescing, release/background behavior and out-of-order input.
- [ ] Test browser text is never journaled, logged, retried after uncertain delivery or sent to the system clipboard.
- [ ] Implement fixed input methods only; reject arbitrary CDP names, script strings, URLs and target IDs.
- [ ] Implement page/back/reload/close with explicit target lifetime; stop input on unexpected origins and require phone approval for session-scoped origin changes.
- [ ] Add contextual touchpad and keyboard in place of the D-pad when browser control is active; keep native playback control state separate.
- [ ] Run fixtures/mocked bridge tests and controlled-browser tests; commit.

### Task 19: Integrate streaming launch, switching and honest capabilities

**Depends on:** Tasks 9–10, 15, 18.
**Files:** Modify streaming/{sessions,browser,external}.rs and control/activity.rs; extend service capability DTOs and phone launch UI.
**Tests:** remote::streaming::tests; browser-extension/tests/service-session.test.mjs; RAT/ui/mediacenter/StreamingStateTest.kt.
**Consumes:** Existing service catalog/VPN preflight, enrolled extension profiles and activity confirmation.
**Produces:** Sofa-usable service launches and explicit per-service supported/partial states.

- [ ] Test preflight completes before interrupting existing playback and service switching uses the original profile without copying/resetting it.
- [ ] Route extension-enabled service launch through the enrolled-profile launch nonce. Normal non-remote launches without a bridge retain existing behavior.
- [ ] Preserve ordinary Chrome/HBO process ownership boundaries: close the enrolled tab only, do not kill Chrome or reroute every Chrome process.
- [ ] Test bridge loss or unsupported semantic controls removes unavailable controls and offers website touchpad operation.
- [ ] Verify no display layout/monitor change occurs; foreground the dedicated browser window on the existing mirrored desktop.
- [ ] Create a service-by-service acceptance matrix for every current catalog entry; record launch, login, browse, play, pause, subtitles, audio and return-to-menu separately.
- [ ] Commit. Real authenticated playback is a required later acceptance result, never inferred from browser launch.

### Task 20: Background availability, startup and idle behavior

**Depends on:** Tasks 5–6, 10, 16.
**Files:** New remote/{lifecycle,autostart,idle}.rs and autostart template; modify main.rs, lib.rs, lifecycle.rs and phone Settings.
**Tests:** remote::lifecycle::tests; remote::autostart::tests; tests/unit/remote-lifetime.test.tsx.
**Consumes:** Remote enabled preference, active native operations and desktop session.
**Produces:** Keep-available behavior, --remote-background, opt-in autostart and End session.

- [ ] Test Close with keep-available hides only UI; listener and player remain functional. Test explicit Quit closes listener and warns about relaunch.
- [ ] Test single-instance launcher restores the original UI without duplicate listener or state.
- [ ] Implement user-only XDG autostart as a reversible option; resolve installed executable path, quote desktop-entry arguments correctly and never add root/system startup.
- [ ] Probe supported session idle inhibition on target Cinnamon; use a narrow adapter with fake tests. If unavailable, report it without changing global power settings.
- [ ] Test End session cancels/stops owned work, releases inhibitor and retains LAN reachability; test explicit OS lock blocks browser/renderer input.
- [ ] Run lifecycle tests and a clean-account login-startup check later; commit.

### Task 21: End-to-end, regression and sofa-only acceptance

**Depends on:** Tasks 1–20 for full acceptance; local-media checkpoint may precede browser completion but must be labeled.
**Files:** New MC/tests/e2e/remote-control.spec.ts; native remote integration harness; R instrumentation suites; evidence under MC/docs/engineering/evidence/remote-control/.
**Consumes:** Packaged candidates from both repositories and the actual target hardware.
**Produces:** The acceptance evidence in section 16.

- [ ] Run protocol interoperability between real Android client parsing and Rust server serialization, not only one implementation's own fixtures.
- [ ] Exercise all required flows on deterministic fake native backends, including lost replies and confirmation races.
- [ ] Run existing desktop/Remote regression suites once on the final change set; investigate failures instead of disabling checks.
- [ ] Perform the sofa-only checklist with phone and mirrored HDMI TV. Record any need to approach the computer as a failure or an explicitly documented one-time prerequisite.
- [ ] Record measurable latency, long-session and reconnect results with device/build/network details.
- [ ] Review that TV mirroring and desktop visual style are unchanged and no bottom playback bars were restored.
- [ ] Commit only sanitized evidence and test changes; never credentials, private media, account details or browser profile state.

### Task 22: Package, upgrade and document the compatible release

**Depends on:** Accepted relevant scope from Task 21.
**Files:** Update MC packaging config/docs and R build/release docs; package native host/extension; add compatibility manifest and user instructions.
**Tests:** Clean-install/update smoke scripts and migration tests.
**Consumes:** Exact tested source SHAs, protocol hash and extension ID/version.
**Produces:** Versioned desktop package, Android APK and browser bridge bundle with explicit compatibility status.

- [ ] Build release candidates using pinned toolchains; record checksums, version numbers and matching revisions.
- [ ] Verify native host absolute path and explicit extension allowed_origins in the installed package; installation must not rewrite arbitrary Chrome preferences.
- [ ] Test upgrade preserves MC data, server identity, paired phones and LG credentials. Test revocation still works after upgrade.
- [ ] Test uninstall removes app/autostart/native-host registration as documented while preserving original media and deliberately retained user data.
- [ ] Document rollback: older MC versions reject newer schema; use a compatible backed-up database, never overwrite a live newer DB or silently rotate identity.
- [ ] Write concise setup, normal use, "Cannot connect", "No sound", browser capability and disable/revoke instructions. State that HDMI is already mirrored and needs no new configuration.
- [ ] Mark release accepted only for physically verified capabilities. Do not publish/upload/install on the user's active machine without the applicable execution authorization.

## 15. Dependency order and implementation checkpoints

Recommended order:

1. Tasks 1–6: shared contracts, safe server and setup.
2. Task 7, then Task 8: actual phone pairing and state recovery.
3. Task 17 early: browser feasibility result before committing to the complete streaming UI.
4. Tasks 9–12: local media, existing-window control and phone browsing.
5. Tasks 13–16: source/device/settings operations and LG-preserving shell integration.
6. Tasks 18–19: streaming input and service integration.
7. Task 20: keep-available and login startup.
8. Tasks 21–22: physical acceptance and packaging.

No concurrency is required. If implementation is delegated later, partition by these explicit interfaces and avoid simultaneous edits to AppContainer/MainActivity, commands.rs or shared wire fixtures.

**Checkpoint A:** Can pair and revoke a real phone; an unauthenticated peer cannot obtain library state; correct current player state arrives.

**Checkpoint B:** Can choose and control a local video entirely from phone with the existing HDMI mirror unchanged; controls remain available after the phone reconnects.

**Checkpoint C:** Can manage routine library/audio/USB/VPN operations without laptop-only confirmations; LG and Color Dock regression tests pass.

**Checkpoint D:** Can navigate an enrolled streaming page using phone input without affecting unrelated tabs; each accepted provider has recorded evidence.

**Checkpoint E:** Compatible installed builds pass the sofa-only test after desktop restart/sign-in and preserve data on upgrade.

## 16. Verification and acceptance checklist

### 16.1 Automated gates

MC:
- [ ] npm run typecheck
- [ ] npm run lint
- [ ] npm test
- [ ] npm run build
- [ ] npm run verify:assets
- [ ] npm run verify:ui-copy
- [ ] npm run verify:remote-contract
- [ ] npm run test:e2e
- [ ] npm run test:visual
- [ ] npm run rust:check
- [ ] npm run rust:test
- [ ] npm run rust:lint
- [ ] npm run rust:format
- [ ] node --test browser-extension/tests/*.test.mjs

R:
- [ ] ./gradlew :app:testDebugUnitTest
- [ ] ./gradlew :app:lintDebug
- [ ] ./gradlew :app:detekt
- [ ] ./gradlew :app:ktlintCheck
- [ ] ./gradlew :app:assembleDebug
- [ ] ./gradlew :app:connectedDebugAndroidTest on a compatible device/emulator, including current LG regression suites where fixtures permit.

Confirm actual Gradle task names using the repository's task listing before execution; record any required equivalent rather than claiming an unavailable task ran. External-service/dependency scans follow the repository's configured workflow, without conflating network/tool failure with test success.

### 16.2 Interoperability and failure cases

- [ ] Rust and Kotlin agree on every fixture and protocol hash.
- [ ] API-37 LAN permission denied, permanently denied and revoked while connected.
- [ ] Camera denied but manual pairing succeeds.
- [ ] Server certificate change stops reconnect without overwriting trust.
- [ ] QR expires, is reused or is presented after server restart.
- [ ] Wrong/expired/revoked token; ninth paired device; fifth simultaneous connection.
- [ ] Wi-Fi changed; mDNS blocked; direct QR address works; guest network isolation.
- [ ] Large malformed JSON, fragmented oversize messages and excessive commands remain bounded.
- [ ] Phone receives a state change while it is fetching the initial snapshot.
- [ ] Phone sleeps while holding a direction key; no continued scrolling.
- [ ] App process killed/recreated; no stale mutation replay.
- [ ] Desktop and phone edit the same queue/playlist simultaneously.
- [ ] USB device replaced between preview and confirmation.
- [ ] Native operation succeeds but its result event is lost.
- [ ] Chrome enrollment/tab/viewport changes during text input.
- [ ] Another Chrome tab or another application has focus; remote text stays scoped.
- [ ] VPN reconnect breaks LAN temporarily without credential reset or repeated mutations.

### 16.3 Actual sofa-only test

Preparation may be performed once at the computer. After the marked boundary, the tester stays on the sofa.

**Preparation:**
- [ ] Record phone model/Android version, desktop version/session type, Chrome/mpv versions and installed app SHAs.
- [ ] Confirm existing HDMI mirroring works before installing the feature; save a description of that baseline.
- [ ] Select non-private test videos/music/photos and a disposable USB drive.
- [ ] Complete app/browser installation, permissions, OS sign-in and initial pairing.
- [ ] Confirm existing sound output works; configure it once if necessary.
- [ ] Record which service accounts are available for testing without recording credentials.

**Sofa-only boundary — no computer keyboard/touchpad from this point:**
- [ ] Open Remote and reconnect to Mediacenter.
- [ ] Browse Videos, enter a title using phone keyboard, select and play it.
- [ ] Pause/resume; seek forward/back; change volume/mute; select an available subtitle and audio track.
- [ ] Foreground Mediacenter's existing menu, then return to the same playback position.
- [ ] Start another item through one phone confirmation; cancel once to prove original playback is preserved.
- [ ] Search Music, build a queue, reorder and remove an entry, save a playlist, play it.
- [ ] Browse Photos, show one still image, move next/previous, start/pause/change/stop slideshow.
- [ ] Add a folder from an authorized root; observe and cancel/restart scan; remove its library association without deleting files.
- [ ] Open USB media and safely eject with the full impact preview. Verify busy-device failure using a separate controlled process.
- [ ] Change audio output only when requested; verify display mirroring and window placement are unchanged.
- [ ] Select LG TV, operate TV volume, switch back to Mediacenter and verify MC commands target only MC.
- [ ] Open each service listed for this acceptance run, search/select content using touchpad/keyboard, play it, then return to Mediacenter.
- [ ] Turn phone screen off for two minutes; unlock and reconnect. Playback continues; state is correct; no duplicate Next action.
- [ ] Temporarily disconnect phone Wi-Fi; restore it and recover without approaching computer.
- [ ] Perform the specifically authorized VPN-change test; confirm protection remains as configured and Remote recovers.
- [ ] End session on phone; later reconnect and start another item while the application remains available.
- [ ] Check that no persistent desktop bottom player bars or new TV-mode UI appeared.

**Separate physical/setup tests:**
- [ ] HDMI unplug/replug and TV power cycle do not rewrite display layout.
- [ ] Loss of HDMI audio produces phone recovery without automatic laptop-speaker output.
- [ ] Desktop lock reports the limitation; Remote does not bypass it.
- [ ] Opt-in autostart works after desktop login; no duplicate process/listener.
- [ ] Upgrades retain pairing, source media, playlists, profile logins and unrelated LG settings.

### 16.4 Performance targets

Measure on the actual home LAN; these are acceptance targets, not present results.

| Measurement | Target |
| --- | --- |
| Connected D-pad press to visible UI response | p95 at most 200 ms over 100 presses |
| Connected local pause to observed paused state | p95 at most 300 ms over 30 actions |
| Search first page from warm local index | p95 at most 1 second for representative library |
| Phone foreground to usable state on stable LAN | at most 5 seconds in 9 of 10 attempts |
| Reconnect after network becomes available | at most 10 seconds in 9 of 10 attempts |
| Continuous mixed media session | 2 hours without stalled UI, runaway queue or required computer access |
| Native idle server | no sustained polling spike; measure baseline and feature delta |
| Android background | no continuing discovery loop or held-key traffic after socket shutdown grace |

If a target fails, retain the measurement and investigate; do not silently weaken it. Report network conditions separately from service-provider loading time.

### 16.5 Evidence format and final definition of done

For each test record test ID, date, candidate revisions, setup, action, observed result, pass/fail/blocked and a sanitized screenshot/log reference when useful.

A blocked provider test is not a passing provider test. An accepted LAN command is not evidence that HDMI sound was heard. A paired phone is not evidence that the sofa workflow is usable.

The intended full delivery is done only when:
- R01–R17 have an implementation and corresponding acceptance evidence.
- Daily supported flows do not require reaching the computer.
- Existing HDMI mirroring is unchanged.
- LG TV/Color Dock functionality remains available.
- Desktop bottom playback bars have not been reintroduced.
- Unsafe/unsupported actions are correctly rejected with phone-visible recovery.
- Packaged versions and protocol/browser compatibility are documented.
- Any remaining service-specific limitations are explicit and do not masquerade as completed support.

## 17. Planning self-review

This section records review of the plan, not execution results.

- [x] User correction incorporated: HDMI already mirrors the desktop; no new TV mode, monitor selection or routing project.
- [x] Requirements mapped to tasks and acceptance checks.
- [x] Existing implementation distinguished from proposed new modules.
- [x] Native media, UI navigation, LG control and browser input have separate authority boundaries.
- [x] Reconnect covers lost replies, state gaps, device address changes and credential changes.
- [x] Phone handles routine confirmations; initial OS/setup exceptions are explicit.
- [x] Queue/playlist revisions and stable IDs cover concurrent desktop/phone edits.
- [x] No implementation or test success is claimed by this document.
- [x] Work is split into independently testable tasks, including an early browser feasibility gate.
- [x] Source/capability limitations are represented in release acceptance instead of hidden fallback behavior.

**Next action after review:** Implement the approved scope task-by-task, starting with contracts and secure pairing. This document itself does not authorize changing the running system, VPN, installed packages or browser profiles.
