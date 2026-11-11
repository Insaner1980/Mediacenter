# Remote control implementation checkpoint

Resumed at the user’s explicit request on 2026-11-11. Tasks1–16 have completed source implementation and independent review with zero blocking findings. Native source is `fd25b91`, Android `23263dd`. Continue independent Task20 background/startup work while Task17 supported manual Chrome installation and Tasks18–19 remain gated. Physical acceptance remains pending.

Task 11's scoped re-review approved both focus fixes with zero open findings. Desktop source is `afc31cd`, metadata `9d92bdd`, Android `b95505d`. Do not redispatch Task 11. Its report and `task-11-fix1-review.md` contain the verified handoff.

## Resume

Continue `docs/superpowers/plans/2026-11-09-remote-mediacenter-sofa-control.md` at independent Task20 after the latest ledger and prepared task-20 brief/spec/context/dispatch. Tasks1–16 are reviewed; do not redispatch them. Task20 depends on reviewed Task16 and can proceed while Task17 manual installation is pending. Preserve Tasks17–19 and actual-hardware Task21 as explicit outstanding gates.

Task 10's scoped re-review approved both fixes with zero open findings. Desktop source is `9cc72f1`, paired metadata `b3c2861`, and Android `fbca8d2`. Read its final report and `task-10-fix1-review.md` for APIs and evidence; do not redispatch its implementation.

- Tasks 1–6 are implemented, tested and independently reviewed. Last reviewed desktop implementation commit: `bd75fc0`.
- Android Task 7 source commits: `1d2c63c` and `6a832db`, based on protocol commit `cb3dc1914adb0ba1efc454f28018710f51c62290`. Independent review approved the connection-ownership fixes; no Critical or Important findings remain.
- Task 7 supplies independent connection, discovery, pinned TLS, encrypted credentials and QR/manual pairing. Its exact handoff and hardware-test procedure are in `task-7-report.md`.
- Task 8 reconnect/resync/uncertain-command recovery is implemented at `3ddeaf8` with reviewed fixes at `44298c2`. No Critical or Important findings remain.
- Task 9 desktop source is reviewed through `10e2b64`; its Android contract correction is `45e3175`. No Critical or Important findings remain.
- Task 10 supplies optional audio-only routing and foregrounding of existing mpv, photo and managed-browser windows. Reviewed Task 11 adds five acknowledged renderer methods (52 implemented methods total) and shared confirmation resolution.
- Continue independent Task20; resume Task17 and Tasks18–19 after its manual installation gate, then finish Tasks21–22. These six tasks remain unfinished.
- Task 17 is incomplete: experimental loading did not survive restart; automated normal-UI drag/drop reported “No dragged path.” The disposable Chrome profile was opened for the user's explicit Load unpacked action. Once the user confirms installation and closure, run the reviewed harness with `--installed`. Never interpret the experimental failure as proof normal installation is impossible. Host/broker/input remain unimplemented. Consult `task-17-report.md`; its early source range is `e3e189b..3e1588e`.

Read the durable workspace `.superpowers/sdd/2026-11-09-remote-mediacenter-sofa-control/` before resuming: the latest entries of `progress.md`, then `task-20-brief.md`, `task-20-spec-extract.md`, `task-20-context.md` and `task-20-dispatch.md`. Task16 final report/review are the approved target/lifetime handoff; Task15 final report and scoped fix1 review supply settings/authority. Task 14 final report and its two fix reviews are the USB/recovery handoff. Task 13 final report and `task-13-fix1-review.md` are the reviewed source-management handoff. Task 12 final report and `task-12-fix1-review.md` supply the reviewed phone integration handoff; Tasks 9–11 reports supply native/repository/renderer interfaces. Task 17's pending browser handoff is in its brief, context and report. That gitignored workspace remains on disk; do not run `git clean -fdx`.

The user resumed the remaining work with “jatka” after the scope/time explanation. Task 17 remains partial while manual installation is pending. Keep the browser feasibility experiment bounded and continue without another scope confirmation. Do not describe the whole project as complete. The first actual Chrome 157 experiment admitted the extension through experimental CDP loading but did not retain it on restart; the supported installation route is being checked separately. Native host and tab-input acceptance are not yet established.

The user has not yet answered how the real Android phone will be available for device testing (USB debugging, wireless ADB, or later). An approved ADB inventory on 2026-11-10 found no attached devices. This does not block independent source work. No physical phone/HDMI/audio/streaming acceptance has been claimed.

## Verified baseline

Task 6 full native regression: 264 passed, zero failed, two existing ignored. Frontend: 159 tests passed; production build, lint, typecheck, generated contract parity, all-target Clippy and formatting passed. Main-window-only setup permissions verified; presentation capability unchanged. Four synthetic pairing/manual layout checks (800/1440 widths, light/dark) passed. Durable native/visual evidence lives in `checkpoint-evidence/`.

Task 6 review found no Critical or Important issues. Two nonblocking notes remain: mdns-sd’s Announce event proves an announcement attempt, not successful multicast delivery or phone reachability; Vitest prints an informational environment-performance suggestion. Physical phone/camera/LAN acceptance is still outstanding. Task 4 Android protocol tests: 18 passed, Detekt and ktlint passed. Do not rerun completed suites without a new reason.

Task 7 Android broad source gate: 255 unit tests passed; Android lint, debug APK build and instrumentation-test compilation passed. A later focused NSD callback fix passed 38 tests plus Detekt, ktlint and Android lint. The broad snapshot predates that localized change; logs and JUnit summaries distinguish them in `checkpoint-evidence/`. The final connection-ownership fixes passed 11 focused controller/pairing/UI tests plus Detekt and ktlint; scoped re-review approved both findings. Device instrumentation has not run; the guarded isolated-package procedure is in `task-7-report.md`.

## Preserve unrelated work

Do not stage or discard the pre-existing changes in `docs/engineering/streaming-compatibility.md`, `docs/design/2026-11-09-simplified-concepts/`, or `docs/engineering/evidence/viaplay-connection-comparison.md`. The user-supplied plan file was already untracked and is intentionally left that way.

Do not alter HDMI mirroring, introduce TV mode or persistent playback bars, or claim the unapproved mockups are the UI specification. Production profiles, media and settings were not used as test fixtures.

## Environment

From the Mediacenter repository, source `scripts/dev-env.sh` for the local Node/Rust tools. Android SDK/JDK/Gradle are under ignored `.tools/remote-android/`; SDK level remains 37. Copies of the existing helper scripts are saved under the workspace’s `resume-tools/`; restore them to their original `/tmp` paths if needed after reboot. Verification logs are also copied to `checkpoint-evidence/`. Route writes to `/home/emma/Dev/Remote`, dependency downloads, Git index/commits, and loopback/IPC test runs through the normal approval mechanism. Do not rerun completed task suites without a new reason.

Task 7 deferred minor: a failed manual challenge needs a clearer retry/start-over action. Verification logs retain toolchain warnings and intentional unused legal-resource notices; do not describe them as warning-free. These are tracked for final review, separately from the mandatory physical acceptance gate.

## Task 8 verified checkpoint

Remote implementation range: `6a832db..44298c2`. The broad pre-review run passed 289 unit tests, Android lint, debug APK assembly and instrumentation-test compilation. The review fixes then passed 34 focused tests and APK assembly; the final test-only shortening passed 12 request-recovery tests plus Detekt and ktlint. Scoped re-review approved both fixes without new findings. Evidence distinguishes these snapshots instead of claiming the broad run covered later fixes.

Recovery waits for authoritative snapshots before enabling mutations, uses bounded foreground retries, preserves explicit pairing/identity/version/permission stop states, and queries uncertain commands by ID instead of replaying them. Recovery metadata is scoped to the server and phone pairing and removed through the explicit forget flow. No APK was installed on the phone and no production pairing was used as a fixture.

One nonblocking source warning remains at `MediacenterConnection.kt:146` (unused `Unit`); baseline dependency/Gradle and legal-resource warnings also remain recorded. Task 7's manual-challenge retry UX note remains deferred. Real phone/HDMI/audio/streaming acceptance remains mandatory in Task 21.

## Task 9 verified checkpoint

Desktop range `3e1588e..10e2b64` implements native player, queue, photo and activity controls with session/revision guards and owner-bound confirmations. Explicit queue-entry adoption preserves an already-playing confirmed music session without restarting it. Photo decoding releases the presentation mutex, so Stop and snapshots remain responsive; obsolete transitions cannot replace a newer session.

The pre-review full native run passed 287 tests with three explicitly ignored native/hardware tests. Review fixes then passed targeted media/photo/music suites, 160 frontend tests and the frontend build. Real mpv null-output proofs separately passed EOF observation and confirmed duplicate-entry music adoption. The final photo-lock correction passed 12 presenter tests and four media tests, strict all-target Clippy, formatting and diff checks. Independent scoped re-review approved both fix rounds with zero open findings. These runs cover distinct source snapshots; the broad run was not repeated after localized fixes.

The unreleased v1 contract now includes the spec-required `selection_too_large` error: 88 methods, schema hash `07a8097ba6955b32f25251ff502ea2bd29334b31a5292b79873d508ff2fcd23f`. Android `45e3175` passed 19 protocol tests, Detekt and ktlint. No SDK/dependency upgrade or deployed-client compatibility claim was made. Task 10 owns existing-window foregrounding; Task 12 must perform explicit entry/revision/session-bound queue adoption after confirmation. Browser and physical-device acceptance remain pending.

## Task 10 verified checkpoint

Desktop source commits `3d0e827` and `181353c`, with paired metadata at `e056d22`; Android source commits `e2746ae` and `09533a8`. Audio-only routing, optional phone Sound settings and existing-window foregrounding are implemented, including return to the exact paused photo presentation. The updated unreleased contract uses `ForegroundAck` for the two transient foreground methods: 88 methods, current schema hash `d658222be2962dc3859c001c154d7f7626946bee14d47972c36bab2ed195266e`.

The desktop broad run passed 308 tests with four explicitly ignored native/hardware cases. Later photo completion passed 16 focused Task 10 tests and 15 presenter tests, Clippy and formatting. Android evidence separately records 20 protocol tests, eight controller/rendered-UI tests, and seven final status tests plus quality/APK checks; lint belongs to the earlier recorded snapshot. Device instrumentation has not executed.

The reviewed fixes are desktop `9cc72f1` and Android `fbca8d2`, with metadata at `b3c2861`. Expired/not-found confirmations now dismiss and allow a new choice. Native idle/preference states are distinct from a real routing loss; a transient fault retained in home publication preserves loss after the effective sink ID is cleared. It does not change the serialized native/TypeScript/wire shape.

Fix round 1 passed four native regressions plus state 9, audio-control 3, outputs 19 and presenter 15 tests, strict Clippy and formatting. Final Android verification passed 13 tests (10 controller and three rendered Compose), Detekt, ktlint, fresh Android lint, APK assembly and instrumentation-test compilation. The phone tests consume exact JSON emitted by the native projector. Scoped independent re-review approved both findings without new issues. These localized results are separate from the earlier broad run. Known build warnings remain nonblocking baseline notes; real phone/HDMI/audio/browser acceptance is still Task 21.

## Task 11 verified checkpoint

The AppState-owned renderer bridge waits for main-window acknowledgement, binds native dispatch IDs to the authenticated phone/connection, and rejects stale generations, contexts and unauthorized actions. Semantic media/navigation actions preserve native authority; remote entry suppresses automatic local folder setup. Shared server confirmations resolve across desktop and the existing phone Sound sheet. No wire/schema/SDK/dependency change was needed; schema hash remains `d658222be2962dc3859c001c154d7f7626946bee14d47972c36bab2ed195266e`.

Final native regression passed 324 tests, zero failures and four existing ignored cases; Android passed 16 focused tests, quality, lint, APK and instrumentation compilation. Frontend counts are reported as 154 affected tests followed by 30 localized tests plus build/lint/type-generation checks, but their original stdout was not retained as files. Fix round 1 must retain its actual new output.

The reviewed focus correction at `afc31cd` uses explicit list/row/grid sections, adjacent grid rows and prior stable-ID order for removal recovery. It passed 51 affected tests across ten files, build/typecheck, lint and generated DTO checks; actual RED/GREEN/check output is retained under `checkpoint-evidence/task11-fix1/`. Scoped re-review approved both findings without new issues. The earlier native 324 and Android 16 gates cover unchanged corresponding code. Real Tauri/phone/HDMI acceptance remains Task 21.

Task 12 supplies the composed phone Remote/Library/Settings pages, D-pad repeat/text entry, common phone confirmation sheet, playback/queue/photo flows and bounded thumbnail cache. Its native opaque-ID producer completes fresh phone-only thumbnail browsing with source/version checks and unchanged desktop previews. Source gates and independent review are complete; no Important or Critical findings remain.

## Task 12 verified checkpoint

Desktop source commits are `c00e5ca`, `f5070a6` and the final test fixture cleanup `30506a6`; paired metadata is `8562d36`. Android source is `a1e775b`, with reviewed fixes at `6906bae`. Final Android verification passed **339 tests**, zero failures/errors/skips, Detekt, ktlint, fresh Android lint, APK assembly and instrumentation-test compilation. Actual shared Compose tests cover portrait, landscape and large font layout; physical phone testing has not run.

The initial full native run passed 328 tests with four existing ignored cases. A later inode-identity correction passed its regression, eight real thumbnail tests, authenticated bounded/revocable HTTP fetch, Clippy and formatting; the broad run is not claimed for that later source. Protocol parity passes 88 methods with optional playlist query and schema hash `bfd145ef3eac74d3ca23e1ca26234ef4384a4f466d5759c2efef3f68eec7e2ae`. SDK/dependencies and desktop display/audio defaults are unchanged.

The initial review found stale-confirmation handoff loss and live snapshots overwriting an active seek gesture. Fix `6906bae` preserves the validated workflow through explicit replacement confirmation and gives seek gestures session ownership. Its four observed RED cases then passed within 13 focused tests, Detekt, ktlint, fresh lint, APK and instrumentation compilation. Scoped `task-12-fix1-review.md` approved both findings with zero new/open issues. The earlier 339-test broad result is a separate snapshot and was not repeated after these localized fixes.

Final native eight-file and Android 38-file hash manifests identify tested source. Continue Task 13 from its prepared brief/spec/context/dispatch; do not redo Task 12. The target page still needs Task 16 integration under the actual selector, and physical acceptance remains Task 21. Existing protobuf/Gradle warnings are nonblocking notes for the final branch review.

## Task 13 historical shutdown checkpoint — review fix subsequently closed

Desktop source `7a4b8f6c21b1609b4ec58f5f69773da319838095`, Android `d40b9cda2dda7705e38817ebef7997d48028c274`, paired metadata `45b919a7d6150be3da6942cacaeb450d9907fc39`. All task source changes are committed. The native full run passed **343 tests**, zero failures and four existing ignored cases. Android passed **358 tests** across 71 suites, zero failures/errors/skips, Detekt, ktlint, fresh lint, APK assembly and instrumentation compilation. No physical acceptance has run.

This adds opaque descriptor-bound folder selection, source addition/removal/UUID-less reattachment, scan progress/cancellation and cancellation recovery after phone process restart. Closed local scan-work metadata covers explicit scans and initial folder-add scans; it contains no action payload or path and retains the existing bounds/owner/epoch/no-replay rules. Native runtime capabilities are 62; canonical protocol stays at 88 methods and schema `bfd145ef3eac74d3ca23e1ca26234ef4384a4f466d5759c2efef3f68eec7e2ae`.

Review input is the immutable `review-task13.diff` (179,631 bytes, SHA-256 `5ce3d26dd39dc389955395b474c07ecdb43024a630c92a2020d5ac7b7f81c5c2`) in the durable workspace. Independent `task-13-review.md` is complete: zero Critical, one Important, one Minor; spec issues and quality Needs fixes. The Important finding is that a prepared folder Add can survive removal of its only authorized root: stale-preview refresh and final grant checks validate descriptor/expiry but omit current root authorization. No fix was started before shutdown.

The shutdown handoff required a round 1 fix and scoped re-review; both are now complete as recorded below. Review bases are desktop `45b919a` and Android `d40b9cd`; subsequent checkpoint commits change documentation only. The Minor finding is known baseline Android build/runtime warnings, tracked for final maintenance review. No Task 14 implementation has started. Do not mark Task 13 complete from its passing tests.

Exact source manifests (`task-13-native-source-manifest.json`, 12 files; `task-13-android-source-manifest.json`, 24 files), `task-13-report.md`, actual gate logs and JUnit summaries remain in the durable workspace. Current helper scripts are copied under `resume-tools/` for reboot recovery. Task 14 brief/spec/context are prepared; supplement its context with the final Task 13 API handoff before dispatch.

Historical shutdown status on 2026-11-10: all implementation/review agents were stopped. The user resumed on 2026-11-11; see the current fix-round status below. Tasks 1–12 remain complete.

## Task 13 fix round 1 — approved 2026-11-11

Source fix is committed at `27ae3e603818fcb837b3ac8275d31477e7afaa9d`; Android remains `d40b9cda2dda7705e38817ebef7997d48028c274`. Recovery implementer `/root/task13_fix1_recovery` reproduced two behavioral RED failures, implemented the two-file correction and froze the source. Native helper is released. Actual logs are retained in `task-13-fix1-evidence/`, with exact changed-file hashes in `task-13-fix1-native-source-manifest.json`.

The correction shares the existing authorized-root policy between browsing, confirmation refresh and final registration. A revoked root clears its pending Add action and cannot obtain another usable preview. Registration checks current authorization under the same database transaction/mutex as its insert and source removal. Existing default/removable authorization remains valid, and no inverse grant/database lock order was found. Android and wire schema are unchanged.

Affected native regressions passed 38 library tests, four folder tests and one confirmation test; final controller bytes passed nine tests after a Clippy-only style correction. Final strict Clippy, formatting and diff checks passed. These focused gates are distinct from the earlier 343-native/358-Android broad snapshots. Independent `task-13-fix1-review.md` approved I1 with zero new/open findings and verified source hashes and retained evidence.

Task 13 is complete. Continue Task 14 using its prepared brief/spec/context/dispatch, now supplemented with the final Task 13 API handoff. The daemon restart during fix1 did not restart implementation: the same agent resumed from surviving files/logs. Chrome manual-install and phone-availability questions were asked asynchronously; no answer has been inferred. Physical acceptance remains Task 21.

## Task 14 source checkpoint — one phone recovery fix remains

Desktop source `91f3d13253d902f508f2857a03ab16b608cfad6b`, Android `eff8ad1bc2030ee449d19549570fbd8f9daa6567`. The four existing drive methods implement opaque insertion-bound drive/volume handles, full sibling/owned-use previews, noninteractive normal mount/eject, stale/replug guards and owner-bound confirmation/operation lookup. Native timeout or unconfirmed effect remains `outcome_unknown`; cancellation is requested until native completion. The phone integrates drive Settings through the existing common confirmation owner. No force/deletion/path command, dependency/SDK/schema change or real device effect was introduced.

Frozen source manifests cover14 native and10 Android paths and were verified after parent broad gates. Native full gate passed361 tests with four existing ignored cases; Android passed365 tests across73 suites, Detekt, ktlint, fresh lint, APK and instrumentation compilation. Scoped controller/rendered layout cases cover portrait, landscape and2x font. Actual logs/summaries and task-14-report.md remain in the durable workspace. Protocol parity remains88 methods/schema bfd145ef3eac74d3ca23e1ca26234ef4384a4f466d5759c2efef3f68eec7e2ae; runtime methods66.

The observed old cancellation-test scheduling assumption was corrected to accept either an active cancelled token or its already-persisted terminal Cancelled outcome. Existing cancellation/replay assertions remain. First sandbox listener failures were rerun through normal escalation. Restored Android wrapper requires explicit `-Dorg.gradle.java.home=/home/emma/Dev/Mediacenter/.tools/remote-android/jdk17`; JDK25 Detekt failed before analysis. Existing build warnings remain separate.

Independent Task14 review is complete: zero Critical, one Important, one Minor (existing build warnings). I1 loses accepted drive-operation tracking when the phone disconnects before terminal completion, so reconnect cannot display the native success/busy/authorization/unknown result or offer running cancellation. Original `/root/task14_drives` is implementing fix round1/5 from the verbatim finding in `task-14-fix1-dispatch.md`; parent owns Android installs/gates/commits. Native source is unchanged. Require actual disconnect-before-terminal RED/GREEN and scoped re-review before Task15. Task15 draft context also records the plan ownership ruling for existing safe Connection methods devices.list/devices.revoke/pairing.begin (normative5.3/5.4/6.3). Do not mark Task14 complete from passing gates alone. Physical USB/phone/HDMI acceptance remains Task21.

## Task 14 fix round 1 — tested, scoped review pending

Android source `fbf9d22f59271e0df28b7eaba39ae65a6af7b854` fixes I1 with closed owner/server/epoch-bound drive-work metadata and authoritative error/status retention. Recovered running cancellation requires a fresh native observation, while native terminal outcomes survive controller reconstruction. The canonical Settings drive view presents the recovered result/cancel once; no mutation is replayed. Native source is unchanged at `91f3d13`.

Actual RED reproduced missing busy result after acceptance→disconnect→repository reconstruction→native lookup. Core/source-scan regression gate passed19 tests; extended recovery/UI gate passed41. After a UI-only extraction/test-format correction, final10 UI/controller tests, Detekt, ktlint, fresh lint, APK and instrumentation compilation passed2m23s. These scoped results are separate from the earlier361-native/365-Android broad snapshot. All13 final source hashes matched `task-14-fix1-android-source-manifest.json`; actual logs/JUnit summaries remain in durable checkpoint evidence.

Next is scoped `task-14-fix1-review.md` against immutable joint fix package, bases MC e6157b8 / Remote eff8ad1. Do not start Task15 or mark Task14 complete until I1 is approved. Chrome manual-install and physical phone/USB/HDMI acceptance remain pending.

## Task 14 fix round 2 — message correlation in progress

Scoped fix1 review approved I1 and all13 hashes/recorded gates. It found one new Important I2 in `MediacenterTargetPage`: retaining an older drive result suppresses a newer drive request’s rejection/timeout message without matching its identity. Native and durable recovery remain unchanged. Original `/root/task14_drives` is implementing round2/5 from `task-14-fix2-dispatch.md`, with canonical per-request rendered regression and exact message provenance/deduplication. Bases MC e8c5090 /Remote fbf9d22. Do not restart Task14 or proceed to Task15 until scoped re-review closes I2.

## Task 14 fix round 2 — tested, scoped I2 review pending

Android source `9a6988d7f85c085b9e1eaf73bf94287134cae21c` adds ephemeral native-message provenance and deduplicates only the represented request/operation in Settings. A later admission error, timeout, local send failure and Remote/Library feedback remain visible. Matching terminal recovery replaces stale running text/cancel for that same operation. Active operation tracking is preserved; no native, pending-store, wire, dependency or SDK change occurred.

Actual rendered RED reproduced hidden B busy feedback after completed A. First complete gate passed27 tests and quality/fresh lint/APK/instrumentation in2m4s. A newly reported redundant safe call was removed with one token; final8 rendered tests and the same quality/fresh lint/APK/instrumentation gates passed1m35s with no Kotlin source warnings. Existing Gradle warnings remain. Seven final hashes matched `task-14-fix2-android-source-manifest.json`; both snapshots and actual JUnit/logs remain in checkpoint evidence.

Next: scoped `task-14-fix2-review.md` for I2, from bases MC e8c5090 /Remote fbf9d22. I1 is already addressed; do not restart its implementation or rerun unchanged broad suites. Task15 remains prepared but undispatched until I2 closes.

## Task 14 approved checkpoint — continue Task 15

`task-14-fix2-review.md` approves I2; original I1 remains approved, with zero Critical/Important findings. Native source is `91f3d13253d902f508f2857a03ab16b608cfad6b`, Android `9a6988d7f85c085b9e1eaf73bf94287134cae21c`. The report and immutable review packages identify every tested snapshot. Task14 is source-complete; do not reimplement it or rerun its unchanged broad suites.

One nonblocking M2 remains: on the busy active-controller cancel-ack branch, generic cancellation-waiting text can temporarily duplicate the matching recovered row. There is one control and no false completion. Task16 owns the focused presentation regression/fix while integrating the canonical target shell; final whole-branch review must triage it. Existing M1 dependency/Gradle warnings remain separately tracked. Physical USB/phone/HDMI acceptance remainsTask21.

Task15 prepared context records native VPN/launch-policy authority, private recovery/message APIs, the ownership ruling for existing trusted-phone device/revoke/additional-pairing methods and the concrete bounded read-DTO gap. Its implementer must propose exact optional pre-release contract fields after native API inspection before modifying the shared schema. Task15 has now been dispatched to `/root/task15_settings`; its exact optional DTO/admin/LAN-lease/version/provider rulings are in `task-15-rulings.md`. An early Android confirmation-routing test reproduced the missing handler; native fake-backend RED and implementation are in progress. Native helper belongs solely to that implementer; parent owns actual Remote installs/Gradle/Git. No real VPN/pairing/settings effect is authorized as a test fixture.

## Task 15 active checkpoint

Bases desktop `94b2c8c4b1f0c024f3d3cd8fd644add3311cd11b`, Android `9a6988d7f85c085b9e1eaf73bf94287134cae21c`. Read current ledger and `task-15-rulings.md` before continuing. Optional bounded native service preferences/redacted connection diagnostics and narrow admin authority were approved before schema changes; generated Rust/Kotlin/fixtures must stay paired. No final Task15 source gate, commit or review is claimed yet.

## Task 15 tested source — independent review pending

Desktop source `c7655e487f887c1a73ab2840153dd3e9ff5a1c8a`, Android `3b92c4646fc555d8fa2d2e0ff1ef1354e6e5a633`. Existing VPN methods now use native policy/observation, exact country postconditions, bounded admission and socket-origin/listener-generation/lease authority rechecked after awaited reads and immediately before effect. Routine native theme/service preferences and trusted-phone list/revocation/additional pairing share the existing confirmation/journal/owner boundaries. New QR material is ephemeral, bounded and redacted; no secret replay or persistence. Safe connection guidance is based on observed native version and verified read-only help, with no automatic network/protection repair.

Canonical protocol remains88 methods/major1/SDK37. Additive optional servicePreferences, redacted connectionDiagnostics and ready.qrRows are approved in task-15-rulings.md; schema is `275823d7721bebd9a408dfe221a5a6d56bcffa4db6342eda703195120265fb7c`. Actual Rust/Kotlin parity passed. Runtime methods76; startup/end-session remain Task20 and browser methods remain Task17–19.

Final native25-path manifest SHA256 `2f5e054b6431256cc36ab992dd65801bfcb5311251e33cf048617f18cbb348e6` matches the368-pass/zero-failure/four-existing-ignored library run and final strict Clippy/format checks. The earlier19 integration tests are a separate unchanged-boundary snapshot. Android full gate passed396 tests/81 suites, quality/fresh lint/APK/instrumentation in2m52s. A later two-file guidance-only amendment passed three rendered tests and the same quality/fresh lint/APK/instrumentation gates in1m48s; all19 final hashes match `task-15-android-source-manifest.json` SHA256 `7fda619b86428c80c040732b2d69ed7fb8aa2c69c6e35ea06913ff71e4bf78da`. Earlier snapshots and actual RED/GREEN logs/JUnit summaries are retained separately in the durable workspace.

Evidence caveat: one initial legacy deferred-method test entered newly implemented read-only native VPN observation before its old assertion failed; it used temporary AppState and performed no VPN/settings/pairing mutation. The parent also checked installed NordVPN6.0.2 version/read-only help; sandbox blocked unrelated CLI log rotation. New effect regressions use isolated fake providers/stores. No real VPN change, credential rotation, APK installation or physical acceptance occurred.

Task15 source is committed and frozen for independent spec/quality review. Do not start Task16 or mark Task15 complete until that gate closes. Task16 owns target selection/mounting and deferred Task14 M2 cancellation-ack presentation. Task17 supported Chrome manual installation and Task21 phone/HDMI/USB/VPN acceptance remain pending; no answer or outcome has been inferred.

## Task 15 independent review — fix round1 active

`task-15-review.md` reports zero Critical, two Important and one baseline Minor. I1: cancelling/replacing/expiring a VPN preview during awaited confirmation reads can still journal/spawn work and then panic on a removed registry entry. I2: using home.revision for settings means ordinary VPN polling invalidates unchanged phone preferences. Original implementer `/root/task15_settings` resumes from exact findings in task-15-fix1-dispatch.md, bases MC56f0b61/Remote3b92c46. Native helper belongs exclusively to that worker; parent owns actual Android installs/gates/Git. Require deterministic RED/GREEN for live confirmation admission and actual publication versus preference revision, then scoped independent re-review. Task16 remains gated; baseline toolchain warnings stay in final maintenance triage.

## Task 15 fix round1 — tested, scoped review pending

Native fix source `fd25b91089ab4b1387c98fe3db58acab2a50b155`; Android remains `3b92c4646fc555d8fa2d2e0ff1ef1354e6e5a633`. I1 validates the live owner/action/request/epoch/expiry after awaited reads and keeps journal admission plus consumption under the same synchronous confirmation lock. I2 uses private persisted `routine_settings_revision` in the existing settings table; real preference changes and its counter commit atomically, while VPN publications do not advance it. Existing transport setup CAS is separate and unchanged. Task20 startup/keep-available must use RemoteStore.save_preferences, which shares this atomic boundary.

Actual RED reproduced all three cancellation/expiry/replacement cases and both publication/revision cases. Affected VPN/policy24, settings9, store19 and setup9 tests passed; the subsequently strengthened rollback test passed1/1. Final all-target Clippy-Dwarnings, formatting and diff checks passed. Six exact source hashes match manifest `task-15-fix1-native-source-manifest.json`, SHA256 `3a48e6cfa4c446c1f71810b7c8f8581cbbe7f4571d3485126b5cda2f0c22ba99`. Actual logs and prior test-only compile failures remain in checkpoint-evidence/task15-fix1; no unchanged broad native/Android suite was repeated. Source/helper released; no Android/wire/dependency/SDK change or production effect.

Next is scoped I1/I2 re-review from MC56f0b61 /Remote3b92c46. Do not mark Task15 complete or start Task16 before approval. Earlier broad368-native/396-Android results remain separate pre-fix snapshots.

## Task15 approved — continue Task16

Scoped task-15-fix1-review.md approves I1 and I2 with zero new/open findings. Native sourcefd25b91/Android3b92c46 are final Task15 sources. The independently reviewed six-file correction and retained actual RED/GREEN/strict evidence close the task at source level. Physical and browser acceptance remain separate.

Task16 now owns canonical LG/Mediacenter selection/mounting, held-input and confirmation transitions, Task14 M2 cancellation-ack deduplication and the deferred Task7 failed-manual-challenge retry minor. Its context contains exact reviewed API handoffs. No Task15 implementation or unchanged broad verification should be repeated.

## Task16 active — target/header integration

Bases MC7fb520ee07a92c233b59a5481521cf5cc3dfa871 /Remote3b92c4646fc555d8fa2d2e0ff1ef1354e6e5a633. Sole implementer `/root/task16_targets`; parent owns actualRemote/Gradle/Git. First17-path core is installed but uncommitted, using the existing LG header location for one target selector and retaining target-specific destinations. Main/unit/instrumentation compilation passed; ControlTargetViewModel2 and active cancellation-ack regression1 passed. First static feedback has five scoped issues; worker is correcting them with cohesive helper/parameter changes and adding actual target lifecycle/manual-retry tests. No final source freeze/full gate/review is claimed. Read latest ledger and task-16-report.md before resuming, not the initial Task15 dispatch.

The disposable Chrome profile was reopened on2026-11-11 for supported manual Load unpacked installation from browser-extension; user completion remains pending. No installed-extension/host/tab-input proof has been inferred. Fresh ADB inventory has no attached device; SDK37 is present but no emulator/image is installed and /dev/kvm is absent. Source-independent work continues; physical acceptance remains unrun.

## Task16 tested source — independent review pending

Android source `23263dd89dbb6c51f413e731f9afd7b15de45dd8`; native remains `fd25b91089ab4b1387c98fe3db58acab2a50b155`. A single selector occupies the existing LG header location; LG/Wallpapers/ColorDock and MC Remote/Library/Settings retain distinct readiness and commands. Target/context generations retire held input and volatile confirmation/QR state, including rapid same-frame return and blocking setup/appearance overlays. Accepted repository work remains queryable without replay. The deferred drive cancel-ACK duplicate and failed manual challenge retry are corrected.

Actual behavioral REDs reproduced cancel-ACK duplication, consumed-observation retention and rapid held-input non-disposal. Focused68 tests passed; final whole Android gate passed411 tests/86 suites with zero failures/errors/skips, Detekt, ktlint, fresh lint, APK and instrumentation compilation in2m49s. All27 source hashes and exact dirty/staged sets matched `task-16-android-source-manifest.json`, SHA256 `214c0de65a59e6c39c508746634929ec076b764c0c32bd5471c50a5d666e92f7`. Exact command artifacts and separate interim/RED evidence remain in the durable workspace. Baseline Gradle/Compose-test/protobuf warnings remain disclosed. No native/protocol/dependency/SDK change or physical installation occurred.

One explicit parameter-count annotation on the hoisted target-page API is a recorded controller ruling; all other checks remain strict. Thumbnail loading preserves the original server/epoch keys and fresh wrapper identity; an unsafe uninstalled method-reference shortcut was rejected. Review the complete Task16 range from MC7fb520e/Remote3b92c46. Do not start Task20 until independent spec/quality review closes. Browser/manual installation and physical acceptance remain pending; no user response has been inferred.

## Task16 approved — continue independent Task20

`task-16-review.md` approves spec and quality with zero Critical/Important findings. Its sole Minor is disclosed baseline toolchain warning debt. The assigned Task14 cancel-ACK duplicate and Task7 manual challenge retry are addressed. Native/TLS/storage/ownership evidence remains with earlier reviewed tasks; physical/browser/startup behavior is not inferred from this Android source gate.

Continue Task20 from prepared context, including actual read-only Cinnamon D-Bus signatures, private persisted settings-counter ownership and reviewed ControlTargetViewModel context-retirement APIs. Native startup/keep-available writes must use RemoteStore.save_preferences. Do not run the old future-method test against newly implemented startup effects; its example must first change to an actually deferred method. All effect fixtures use explicit isolated paths/fakes; real user autostart/lock/idle and phone/HDMI acceptance remain Task21.

## Task20 active — background/startup/owned-session lifecycle

Bases MC7582ae25ae7a4722e7d65e72630a28dcee141309 /Remote23263dd89dbb6c51f413e731f9afd7b15de45dd8. Sole implementer `/root/task20_lifecycle` owns native source/helper; parent owns actualRemote/Gradle/Git. Read latest ledger, task-20-rulings.md and current report before resuming; do not redispatch an initial Task16 implementation. Task20 native/desktop/Android source is uncommitted and not finally verified.

Approved boundaries: same existing SettingsEndSession phases/confirmation and owned effects, KeepAvailable versus explicit Quit, user-only path-injected autostart with owned rollback, narrow Cinnamon D-Bus idle/lock handling, optional bounded SettingsState.idleInhibition. Native setting writers coordinate existing network-config revision and RemotePreferences CAS under settings_gate→inner→DB, preserving routine counter and listener generation; no new method/SDK/dependency or generic IPC forwarding. Detailed exact semantics and rollback/unknown rules are in task-20-rulings.md. All effect fixtures are fake/isolated; real startup/lock/idle/VPN/pairing/display/audio effects are not test prerequisites.

Actual early RED: native EndSession returns Unsupported; desktop ExitDialog lacks phone-relaunch warning. Early Android8-path stage is installed; main/unit/instrumentation compilation passed, with one strict Compose root finding still being corrected. Its optional contract fields are an interim slice, not final paired-fixture/hash evidence. Native helper remains worker-owned; parent Android is idle until the next coherent stage.

## Task20 tested source — independent review pending

Desktop source `d7051a5227c90b95fb452b4f4368e14d06537f5c`, Android `899edad359f06262061e2345200ef49e48f04657`. KeepAvailable hides only the existing UI; explicit Quit remains separate and warns about relaunch. User-only owned XDG startup, single-instance restore, confirmed EndSession and scoped Cinnamon idle/lock adapters are implemented. EndSession retains LAN availability and targets immutable admitted resources, including exact scan cancellation tokens. Accepted work survives later preview removal; cancelled or replaced pending previews cannot admit effects. No OS shutdown, ordinary Chrome teardown, display/audio change or data/pairing reset is added.

Lifetime settings share the existing persisted counter/CAS and coordinated owned network-config revision. A bounded private write-ahead marker precedes file effects and survives a crash or failed rollback. Launch and phone settings refuse uncertain availability; desktop status stays readable, with explicit Disable→Enable recovery through the existing path. No startup-file repair occurs merely at launch. All effect fixtures use isolated paths/fakes; actual user autostart, inhibitor, lock, VPN and pairing effects were not test fixtures.

The final 27-path desktop manifest is `task-20-source-manifest.json`, SHA256 `2bdcf286c8707339d957dccd83cbb4552277f935ec03c41085be97343e420b70`; all actual/staged/post-gate hashes matched. Native full gate passed400 library tests and19 integration tests, with four existing explicitly ignored tests; check, all-target Clippy-Dwarnings and format passed. Android final14-path manifest `d69b7ddee5dca328073bdb6415dfd7a57ddac24a45569cf7d80fa5ca2c8a79d5` matches414tests/87suites plus quality/fresh lint/APK/instrumentation compilation. Canonical protocol remains88methods/major1/SDK37 with approved optional idleInhibition; strict Rust/Kotlin generation parity passed schema `f7231d1864fa77b52ba035dd3418bfae57ec2787924bee9a957e58765695a1a0`.

Frontend typecheck/lint/full189tests/build/assets passed before a two-literal baseline separator cleanup; final UI-copy guard, related9tests and build passed after it. Existing E2E first normal run passed12/14; the first two timed out during initial loading. Both passed a focused unchanged-code rerun without increased timeouts (2/2,23.1s). The original failure and diagnostic logs remain retained; do not represent this as one clean14/14 run. All103 existing visual tests passed. Generated images were archived in this plan workspace and prior tracked evidence restored; no persistent desktop playback bars or new display-mode UI was introduced.

Read final `task-20-report.md`, rulings and immutable joint package for independent spec/quality review from bases MC7582ae25/Remote23263dd. Do not mark Task20 complete or dispatch Task21 until review closes blocking findings. After approval, continue the prepared labelled local-media Task21 interoperability/fake end-to-end checkpoint; full sofa/browser acceptance still requires the actual phone/computer/HDMI/audio and supported Chrome installation. Task22 accepted release remains gated. No reply to the manual installation/device prerequisites has been inferred.

## Task20 independent review — fix round1 active

`task-20-review.md` reports zero Critical, two Important and one baseline Minor. I1: owned startup deletion and directory publication need durable ordering before the uncertainty marker is cleared. I2: failed explicit recovery can clear a pre-existing marker while restoring the old incoherent files. Original implementer `/root/task20_lifecycle` resumes from verbatim findings in task-20-fix1-dispatch.md, bases MC5e3a6e1/Remote899edad. Require isolated fault-injected adapter ordering and failed-recovery/reopen RED/GREEN before scoped review. Native helper belongs solely to worker; parent owns actual Android/Git. No Task21 dispatch until blocking findings close. Prior400+19/414/189/visual103 snapshots remain valid evidence for their exact bytes, not proof of these two missing failure cases.

## Task20 fix round1 — tested, scoped review pending

Native fix source `92f22d97de687e4d0156fe9e6ded6663fbf0b505`; Android remains `899edad359f06262061e2345200ef49e48f04657`. Owned startup unlink/rollback-to-absence and directory publication now sync their durability dependencies before marker removal can certify success. Failed recovery retains pre-existing uncertainty even after rolling back the new attempt; phone settings and reopen stay suspended until successful explicit saved-choice repair. Only two native files changed; no schema/client/frontend/dependency change.

Actual isolated REDs reproduced four ordering/recovery failures plus one coordinator rollback error. GREEN5 and the38 affected adapter/setup/lifecycle/settings tests passed, strict all-target Clippy-Dwarnings and format passed. Two final actual/staged hashes match `task-20-fix1-native-source-manifest.json`, SHA256 `61b7777e514cc6bb3bea128fb60a91e176b70cee993f3f86c456de3d85e3720d`. Exact commands/output remain in checkpoint-evidence/task20-fix1. Earlier400+19/414/189/visual103 runs are separate pre-fix broad snapshots; no unchanged broad suite was repeated. Native helper is released; no process or pending approval remains.

Next: scoped I1/I2 review from MC5e3a6e1/Remote899edad. Do not close Task20 or start Task21 before approval. Physical login/power/idle/lock/phone/HDMI tests and supported Chrome installation remain unrun; fault-injected fsync tests establish the adapter/error contract, not physical power-loss acceptance.
