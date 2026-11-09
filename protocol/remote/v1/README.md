# Remote protocol v1

`schema.json` is the canonical, versioned allowlist. It defines all 88 method-specific request and success shapes, bounded domain DTOs, snapshot topics, pairing payloads and typed error codes. No listener, discovery, pairing, command execution or browser enrollment is enabled by this contract package.

`fixtures.json` contains synthetic examples only: every request/success reply, hello, empty/populated snapshots, paging, all journal outcomes, browser capabilities and pending-origin approval, QR/manual pairing, confirmation phases, idle queues, saved-playlist edits, and shared invalid/malformed requests. Repeated `A`/`B` secret values and `synthetic-v1` verification words are test data, never credentials or an authentication wordlist.

`manifest.json` hashes recursively key-sorted compact JSON with array order preserved. Rust and Kotlin independently verify the schema SHA-256. `npm run verify:remote-contract` also checks the copied Android bundle and generated Rust/Kotlin code; Kotlin formatting and trailing commas may differ without changing generated tokens.

Regenerate from the Mediacenter checkout after sourcing `scripts/dev-env.sh`:

```sh
python3 scripts/generate-remote-contract.py --kotlin-output /tmp/remote-contract-stage
cargo fmt --manifest-path src-tauri/Cargo.toml
node scripts/verify-remote-contract.mjs --update-manifest --bundle /tmp/mediacenter-v1.json
```

Copy the two generated Kotlin files into Remote's `mediacenter/protocol` package and the bundle into `app/src/test/resources/mediacenter-v1.json`. Preserve handwritten `StrictJson.kt`, `ControlContract.kt`, and tests. Run each repository's tests, Kotlin formatter/lints, and `npm run verify:remote-contract` before committing both sides. Current dependency versions and SDK 37 are unchanged.

Wire fields are camelCase; methods are dot-separated; errors are snake_case. `Decimal` is a canonical unsigned 64-bit decimal string, including bounded native u32 revisions. Timestamps and durations are explicitly named numeric time units, not counters. Library limits default to 50 at the service boundary when omitted; pages/batches are capped at 100. Native adapters explicitly map audio→music, milliseconds→seconds, typed player actions, fit/actual photo behavior and stable queue handles. They reject unrepresentable values rather than truncating or selecting defaults.

JSON Schema plus supplemental `x-methods` preconditions, `x-maximum`, `x-maxBytes`, and generated semantic validators form the contract: epoch/session equality, canonical u64 bounds, byte limits, operation lookup XOR and revision requirements need semantic validation. Unknown optional fields are accepted. Unknown methods/action enums are unsupported; malformed JSON, non-finite numbers, invalid Unicode scalars, duplicate object keys and invalid bounds are rejected. Android performs an RFC 8259 syntax check before `JSONTokener`, which otherwise accepts additional JavaScript syntax. Replies are decoded against the originating method, never guessed from overlapping result shapes.

Confirmation workflows use the existing method catalogue. `devices.revoke` takes exactly one device ID (prepare) or confirmation ID (confirm). `pairing.begin` and `settings.endSession` take no ID to prepare, or only a confirmation ID to confirm. `browser.open` takes service/profile IDs to prepare, or only a confirmation ID to confirm browser launch/enrollment/origin approval. Browser state exposes a pending preview with hostname, browser session and target generation. Server adapters must scope these IDs to the owning phone, allowed action kind, native session/generation and expiry; no action executes in Task 1.

Idle queue edits require epoch and queue revision; next/previous additionally require the active session. `playlists.save` has explicit `save_queue`, `append`, `remove`, and `rename` actions. Saving the current queue requires its revision; any existing playlist edit also requires playlist revision. Appends/removals carry at most 100 item/stable-entry IDs per request, and playlist pages preserve distinct entry IDs for repeated media. Persistent playlist revisions and these effects belong to later tasks.

`pairing.begin` has a separate `PairingPreparation` success: `confirmation_required { preview }`, first-success `ready { operationId, candidate }`, or duplicate-completed `already_issued { operationId }`. The candidate and its one-use secret are ephemeral. Durable journal metadata stores only operation ID/status; it never stores the candidate/secret or a hash of those secret-bearing parameters. If the first ready reply is lost, the phone must explicitly start a new pairing attempt; it must not automatically replay to recover the secret.

The generated Android codec records narrowly scoped Detekt exceptions for mechanical `LongMethod`, `LargeClass`, `CyclomaticComplexMethod`, `TooManyFunctions`, and `MagicNumber` counts. These are inherent in emitting the complete reviewed schema allowlist. Handwritten parsing, all formatting, unused code checks, the schema and generator remain checked. The tradeoff is that generated structural-count metrics are not enforced; generator review, copied-contract hashes and behavioral tests carry those correctness checks. Unreachable private codecs are omitted rather than suppressed.

A `confirmation_stale` error requires a typed `replacementPreview`; both decoders and encoders reject a stale-confirmation error without it. The replacement confirmation ID and full preview survive failure round trips. `operations.cancel`, like `operations.get`, takes exactly one `requestId` or `operationId`, allowing cancellation when the original acceptance reply was lost.
