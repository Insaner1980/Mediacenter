# Architecture decisions

## 2026-11-05 — authorization and scope

The current user request authorizes implementing the named plan. Historical “design only” and “plan only” text remains background, not a veto on the new request. Preserve Colour Wall references, both themes, official logos and the no-added-dots rule. Do not modify Color Dock, Remote or private media.

## 2026-11-05 — preflight boundaries

Use the planned Tauri/React/Rust stack. Task 01 has partial real-machine evidence, not full acceptance. Continue independent local shell preparation while native package installation and account-dependent testing are pending. Do not substitute a mocked success for any missing hardware/service result. Cost of this ordering decision: some shell work could need revision if subsequent provider compatibility invalidates the intended viewing setup.

## 2026-11-05 — browser identity

Installed Chrome flattened its proc command line; exact original argv token checks failed in a real probe. The successful probe checked the directly spawned process, start time, executable, private profile lock PID and window PID. Production must also handle launcher exit, stale locks and PID reuse; the simple probe does not prove those cases. Never match on title alone.

## 2026-11-05 — output coordinates and audio

Cinnamon applied a factor of two to requested window positions in this session. Physical XRandR bounds and window-placement coordinates cannot be assumed equivalent. Reconcile scaling and observe the resulting position before reporting success. HDMI video detection is independent of audio availability: the active analog card profile does not expose an HDMI sink. Do not change the global audio profile or default automatically.

## 2026-11-05 — VPN policy

User explicitly approved enabling NordVPN Kill Switch. The command succeeded and a fresh read confirmed enabled. Keep this system setting on app exit. Do not automatically change country, protocol or any other network setting. Initial fixtures preserve the originally observed disabled state; subsequent enabled state must be recorded separately.
