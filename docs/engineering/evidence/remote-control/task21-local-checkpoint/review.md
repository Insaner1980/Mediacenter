### Spec Compliance

- ✅ **Spec compliant for the permitted, labelled local-media SOURCE CHECKPOINT.** The change supplies actual Rust serialization → actual Kotlin parsing/reduction, a real Dispatcher/journal conversation with a fake native effect, fixed-allowlist desktop UI tests, and sanitized source-regression evidence. No missing, extra, or misunderstood source-checkpoint requirement found (`src-tauri/src/remote/interop_tests.rs:105`, `scripts/task21-interop.mjs:41`, `Remote/app/src/test/java/com/emmah/colordock/remote/mediacenter/RustInteropTest.kt:59`, `tests/e2e/remote-control.spec.ts:50`).
- ✅ Production scope is preserved: the new Rust module is `#[cfg(test)]`, Android additions are test/resource files, and desktop IPC substitution exists only in Playwright initialization. No production behavior, dependency, SDK, schema, display/audio configuration, or runtime arbitrary-command surface is added (`src-tauri/src/remote/mod.rs:33`, `tests/e2e/remote-control.spec.ts:9`, `tests/e2e/remote-control.spec.ts:39`). Compatibility metadata continues to say `source_checkpoint`, explicitly excluding accepted-release status (`protocol/remote/v1/compatibility.json:10`).
- ⚠️ **Full Task21 remains PARTIAL; Task22 accepted release remains gated.** Browser runtime/provider work, connected Android instrumentation, actual phone/API37 permissions, sofa-only operation, HDMI mirroring and audible output, USB, LG/Color Dock, signing/upgrade preservation, real second-process delivery/login/lock/idle, home-LAN latency/reconnect measurements and the two-hour session remain unproved. Instrumentation compilation and fake/native/codec tests do not establish those results (`docs/engineering/evidence/remote-control/task21-local-checkpoint/README.md:76`, `README.md:80`, `README.md:96`, `final-gates.json:42`).
- ⚠️ Formal draft2020 schema/oneOf/x-replies validation remains the previously deferred Task1 item. This corpus establishes typed wire compatibility for the exported cases, including the ten browser method shapes; it does not establish browser runtime support or exhaustive validation of every possible protocol value (`README.md:57`, `README.md:82`).

### Strengths

- Actual typed server requests and method-correlated replies are serialized, including 88 method shapes, 101 replies, pairing/preparation/error variants, six operation outcomes, decimal values above 2^53/u64 max, and rejected malformed/semantic/oversize inputs. Kotlin checks complete method sets and structural roundtrips, preserving quoted counters rather than coercing them into JSON numbers (`src-tauri/src/remote/interop_tests.rs:15`, `interop_tests.rs:105`, `interop_tests.rs:178`; `Remote/app/src/test/java/com/emmah/colordock/remote/mediacenter/RustInteropTest.kt:59`, `RustInteropTest.kt:103`).
- The recovery trace passes through actual Dispatcher admission and SQLite journaling; repeated intent returns the same acceptance, changed intent conflicts, terminal lookup succeeds, and both effect count and journal rows are asserted equal to one. Kotlin checks the corresponding typed operation identity/status/error rather than merely searching JSON text (`src-tauri/src/remote/interop_tests.rs:30`, `interop_tests.rs:90`, `interop_tests.rs:96`; `Remote/app/src/test/java/com/emmah/colordock/remote/mediacenter/RustInteropTest.kt:141`).
- The actual reducer buffers a Rust event before the snapshot, advances a sequence beyond JS exact-integer range, requires resync on a gap, and returns to readiness after a snapshot (`src-tauri/src/remote/interop_tests.rs:151`; `Remote/app/src/test/java/com/emmah/colordock/remote/mediacenter/RustInteropTest.kt:125`).
- Prepare creates a fresh exporter destination, binds relevant source/consumer hashes, stages only the approved resource/test, and leaves protected installation to the parent. Verify compares that bound run with staged and installed bytes without regenerating it. A paired checkout can bootstrap its staging consumer from the installed source test; no stale corpus fallback exists. Isolated tests cover installed tampering, changed serializer source, and absent prepared output (`scripts/task21-interop.mjs:17`, `task21-interop.mjs:28`, `task21-interop.mjs:41`, `task21-interop.mjs:56`; `scripts/task21-interop.test.mjs:32`, `task21-interop.test.mjs:41`).
- Desktop tests exercise actual application dialogs with rejected unexpected IPC, including selected-network setup, occupied-port recovery, pairing cleanup, revoke/cancel, and close/quit choices. Existing production-path tests are mapped rather than reimplemented in a generic harness; the coverage matrix distinguishes each seam from physical acceptance (`tests/e2e/remote-control.spec.ts:39`, `remote-control.spec.ts:50`, `remote-control.spec.ts:68`, `remote-control.spec.ts:79`; `docs/engineering/evidence/remote-control/task21-local-checkpoint/README.md:53`).
- Final source evidence records native 405+19 with four explicit existing ignores, Android 418/88 with zero failures/errors/skips, frontend 189/31, extension 2, E2E 17 and visual 103, plus strict/build/format gates. Prior failures and real-device omissions remain visible (`docs/engineering/evidence/remote-control/task21-local-checkpoint/final-gates.json:7`, `README.md:90`).

### Issues

#### Critical (Must Fix)

- None.

#### Important (Should Fix)

- None.

#### Minor (Nice to Have)

- **M1 — Existing toolchain warning debt remains visible.** `docs/engineering/evidence/remote-control/task21-local-checkpoint/android-broad.txt:3` and `android-broad.txt:4` record deprecated Gradle APIs; `android-broad.txt:75` records protobuf's terminally deprecated `sun.misc.Unsafe` usage. The source gates pass, but their output is not warning-free and future toolchain removal needs maintenance. Track the originating build/plugin/dependency work separately; no general upgrade is required for this checkpoint. This is inherited, explicitly disclosed debt, not new Task21 behavior (`README.md:94`).

### Assessment

**Task quality: Approved for the local-media SOURCE CHECKPOINT.** Counts: **Critical 0 / Important 0 / Minor 1** (inherited warnings; no new source blocker).

**Reasoning:** The source checks exercise meaningful production DTO, reducer, dispatcher, journal and UI seams without introducing runtime test hooks. The evidence and compatibility labels accurately preserve the boundary between passing source regressions and unfinished physical/browser acceptance.

**Checks performed:** Read the binding brief, spec extract, context, rulings, implementer report and immutable joint package; verified package size 337434 bytes and SHA256 `f4ec12a6347f234d38ab0275ade15964fb655cbf984e4fab4db485ebd050de86` for MC `5dd0bee..3dc5d4d20fa53bf2addda42f2a8c8aacd8478bc4` and Remote `899edad..caad0fda0cb682e739ecb521b944a33ed404aabb`. Bounded reads included the oversized fixtures, inspected by parsed lengths/values where appropriate; truncated log/resource output was recovered with narrower reads. No Git rediff, suite rerun, source/index/HEAD mutation, or subagent dispatch occurred.

**Focused outside-diff check:** Named risk: the exporter might bypass the production operation path or use a different server envelope. Checked `src-tauri/src/remote/dispatch.rs:17`, `dispatch.rs:275`, `dispatch.rs:279`, `dispatch.rs:285` and `src-tauri/src/remote/server.rs:747`: the existing effect hook is test-only and sits inside `operations().start`, while the server builds the same `RemoteReply` success/failure envelope. No additional source exploration or runtime test was necessary.
