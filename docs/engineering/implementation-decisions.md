# Implementation decisions and remaining details

Recorded 5 November 2026. The implementation is a development candidate, not a fully accepted release. The chronological decisions below preserve the implementation ledger, including decisions later superseded. See [verification](verification.md) for acceptance evidence.

## Rulings

1. Ruling: Work in the newly initialized repository on a dedicated branch rather than create a second checkout — the initial folder contains only approved design documents and is already this task's isolated workspace — cost if wrong: no additional checkout-level separation.

2. Ruling: Current implementation request supersedes historical design-only/plan-only authorization — retain product constraints — cost if wrong: implementation would be premature, but current request is explicit.

3. Ruling: Continue Task 02 shell/toolchain preparation while Task 01's account-dependent and HDMI audio checks are pending — independent work is authorized and useful; retain explicit integration and release gates — cost if wrong: shell effort could need revision after compatibility findings.

4. Ruling: Enable Kill Switch only after explicit user approval (received); command succeeded and settings rechecked — system-wide effect was explained — cost if wrong: networking is blocked when VPN disconnects, as intended by the selected policy.

5. Ruling: Prepare pure UI and backend infrastructure while prior manual acceptance gates remain open — native compilation and manual checks may proceed independently — cost if wrong: additional work may need revision; no milestone is marked complete prematurely.

6. Ruling: Temporarily fail closed when a removable filesystem has no UUID instead of inventing a weak identity — fallback fingerprint/explicit reattachment is not implemented yet — cost if left unresolved: such sources cannot be saved, so task 09 remains incomplete.

7. Ruling: Use a private Unix socketpair inherited by mpv as fd 0 rather than a pathname listener — the installed mpv 0.37 manual explicitly supports input-ipc-client=fd://N, this captures file-loaded from native-argument startup without a connection race, and EOF closes the owned player — cost if wrong: compatibility outside this tested Linux/mpv target requires revalidation; there is no pathname socket to reconnect after a crash.

8. Ruling: Switch remaining independent music, photo, and device modules to scoped parallel agents using superpowers:dispatching-parallel-agents — shared shell/player interfaces are now concrete and file ownership is disjoint, allowing progress without TV tests — cost if wrong: integration differences need additional root review and regression tests. Root retains shared-file integration, lifecycle, packaging and final whole-branch reviewer. This supersedes the earlier inline-only workflow choice, not product constraints.

9. Ruling: Use local private Xvfb and extracted-package smoke for background package verification — the user is watching TV and visible hardware tests are deferred — cost if wrong: this does not replace clean system installation, start-menu, real-device or real-service release acceptance.

10. Final: Ruling: Account-dependent provider login/DRM/VPN acceptance and quality remain manual release gates — public YouTube and fake fixtures cannot establish those properties — cost if wrong: a provider may reject VPN/playback after implementation.

11. Final: Ruling: HDMI sound/scaling/disconnect checks remain deferred while the user watches TV — preserve the user's normal viewing session — cost if wrong: real output adaptation can still require fixes.

12. Final: Ruling: Actual drive mutation/reconnect and clean package installation remain unaccepted — fake backends and extracted-package smoke are limited evidence — cost if wrong: Polkit, package dependencies or hardware behavior can differ.

13. Final: Ruling: Same-user hostile mutation of private application directories is outside this implementation's ordinary source/process boundary — private permissions and conservative symlink checks are retained, without claiming resistance to a compromised user account — cost if wrong: an attacker with that user's permissions can subvert private state.

## Deferred minor details

- Returning a browser uses reachable normal computer geometry rather than its exact prior manually resized geometry; preserve this as an open plan detail, not a completed acceptance check.

- A dedicated local-player fullscreen toggle is absent; external player/window controls and TV placement still supply fullscreen. Explicit typed UI toggle remains an open plan detail.

## Final code review

A fresh, read-only whole-branch review covered `90626a6..4f7d00c`. It found no Critical issues and four Important issues: stale connected VPN display, initial browser placement, current music output controls, and adjacent subtitle loading. All four were fixed in `3294443` using failing regressions followed by the full passing suite. A further real mpv fixture verified restricted sidecar loading with null picture/audio output. No second reviewer was used.


## HBO Max ordinary Chrome handoff (0.1.2)

The managed HBO authentication error remained reproducible while the user's ordinary Chrome profile displayed the login form. The user delegated the choice after being told that ordinary Chrome would use manual display/audio control. We selected ordinary Chrome for HBO only. The existing open-service command still verifies configured VPN requirements and rejects conflicting managed playback before handing the fixed HBO catalog URL to `/usr/bin/google-chrome` without a private profile or app-mode flags.

Ordinary Chrome is not registered as an owned session: Mediacenter never moves its window/audio, closes it or kills it on timeout/cancellation/app exit. A bounded startup check reports immediate launcher failures, while a long-lived browser is left running and eventually reaped. The home screen explains the handoff. Other services retain managed playback. This is a compatibility fallback, not a claimed fix of HBO's underlying managed-profile login error; authenticated login and video playback remain user acceptance checks.


## Direct service switching (0.1.3)

The user expected selecting Viaplay after YouTube to open Viaplay. The earlier explicit-close rule instead navigated to YouTube's controls and buried the explanation below the visible settings. The user's request supersedes that rule: a service-tile selection now initiates the switch directly. The shared native transition first checks cancellation and focuses an already-open matching service. Otherwise it verifies VPN policy, Chrome availability, target display and target private-profile availability before closing the verified owned current browser. A preflight failure/cancellation preserves the previous service; uncertain ownership or closing failure prevents replacement. A successful close clears the old session and effective-output claims before the requested launch. Unexpected later launch failure is shown on home and leaves no stale service registered.

Profiles are not reset or deleted. HBO remains an external handoff; ordinary Chrome is never owned, closed or moved by this transition. Local playback/slideshow conflicts still require stopping the respective player. The idle page layout is unchanged.
