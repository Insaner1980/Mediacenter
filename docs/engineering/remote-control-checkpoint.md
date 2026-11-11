# Remote control implementation checkpoint

Resumed at the user’s explicit request on 2026-11-11. Tasks 1–13 have completed source implementation and independent review. Task 13 root-authorization fix `27ae3e6` passed scoped re-review with zero open findings. Task 14 USB source gates passed, but review found one phone recovery issue; its first fix round restored outcomes, and the second round passed covering gates and awaits scoped I2 review. Task 17 has a partial Chrome experiment at `3e1588e` with manual test-profile installation pending. Physical acceptance remains outstanding.

Task 11's scoped re-review approved both focus fixes with zero open findings. Desktop source is `afc31cd`, metadata `9d92bdd`, Android `b95505d`. Do not redispatch Task 11. Its report and `task-11-fix1-review.md` contain the verified handoff.

## Resume

Continue `docs/superpowers/plans/2026-11-09-remote-mediacenter-sofa-control.md` at Task 15 after reading the latest ledger, prepared Task 15 brief/spec/context/dispatch and final Task 14 report/reviews. Tasks 1–14 are complete; do not restart them. Retain the pending **Task 17** browser installation step.

Task 10's scoped re-review approved both fixes with zero open findings. Desktop source is `9cc72f1`, paired metadata `b3c2861`, and Android `fbca8d2`. Read its final report and `task-10-fix1-review.md` for APIs and evidence; do not redispatch its implementation.

- Tasks 1–6 are implemented, tested and independently reviewed. Last reviewed desktop implementation commit: `bd75fc0`.
- Android Task 7 source commits: `1d2c63c` and `6a832db`, based on protocol commit `cb3dc1914adb0ba1efc454f28018710f51c62290`. Independent review approved the connection-ownership fixes; no Critical or Important findings remain.
- Task 7 supplies independent connection, discovery, pinned TLS, encrypted credentials and QR/manual pairing. Its exact handoff and hardware-test procedure are in `task-7-report.md`.
- Task 8 reconnect/resync/uncertain-command recovery is implemented at `3ddeaf8` with reviewed fixes at `44298c2`. No Critical or Important findings remain.
- Task 9 desktop source is reviewed through `10e2b64`; its Android contract correction is `45e3175`. No Critical or Important findings remain.
- Task 10 supplies optional audio-only routing and foregrounding of existing mpv, photo and managed-browser windows. Reviewed Task 11 adds five acknowledged renderer methods (52 implemented methods total) and shared confirmation resolution.
- Continue Tasks 15–16, resume Task 17 after its manual installation gate, then Tasks 18–22. These eight tasks remain unfinished.
- Task 17 is incomplete: experimental loading did not survive restart; automated normal-UI drag/drop reported “No dragged path.” The disposable Chrome profile was opened for the user's explicit Load unpacked action. Once the user confirms installation and closure, run the reviewed harness with `--installed`. Never interpret the experimental failure as proof normal installation is impossible. Host/broker/input remain unimplemented. Consult `task-17-report.md`; its early source range is `e3e189b..3e1588e`.

Read the durable workspace `.superpowers/sdd/2026-11-09-remote-mediacenter-sofa-control/` before resuming: the latest entries of `progress.md`, then `task-15-brief.md`, `task-15-spec-extract.md`, `task-15-context.md` and `task-15-dispatch.md`. Task 14 final report and its two fix reviews are the USB/recovery handoff. Task 13 final report and `task-13-fix1-review.md` are the reviewed source-management handoff. Task 12 final report and `task-12-fix1-review.md` supply the reviewed phone integration handoff; Tasks 9–11 reports supply native/repository/renderer interfaces. Task 17's pending browser handoff is in its brief, context and report. That gitignored workspace remains on disk; do not run `git clean -fdx`.

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

Task15 prepared context records native VPN/launch-policy authority, private recovery/message APIs, the ownership ruling for existing trusted-phone device/revoke/additional-pairing methods and the concrete bounded read-DTO gap. Its implementer must propose exact optional pre-release contract fields after native API inspection before modifying the shared schema. No Task15 source implementation has started yet.
