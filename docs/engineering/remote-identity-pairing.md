# Native remote identity and pairing

Task 4 supplies identity, pairing and authentication primitives. It does not
start a listener, modify an application profile, or create a production identity.

## Dependencies

The reviewed direct dependencies are exact-pinned in `src-tauri/Cargo.toml` and
`Cargo.lock`: rustls 0.23.46 (`ring`, `std`, `tls12`, default features disabled),
rcgen 0.14.10 (`ring`, `crypto`, default features disabled), getrandom 0.4.3, and
subtle 2.6.1. SHA-256 continues to use existing sha2 0.10.9. The pinned Rust
1.99.1 toolchain is used. Feature names were verified against the downloaded
crates' own Cargo manifests. No general dependency upgrade is part of this task.

Primary API references:

- https://docs.rs/rustls/0.23.46/rustls/ — explicit ring provider, server
  certificate/key validation and TLS handshake signature verification.
- https://docs.rs/rcgen/0.14.10/rcgen/ — `KeyPair::generate`,
  `CertificateParams::new` / `self_signed`, `Certificate::der`,
  `KeyPair::serialize_der`.
- https://docs.rs/getrandom/0.4.3/getrandom/fn.fill.html — OS randomness.
- https://docs.rs/subtle/2.6.1/subtle/trait.ConstantTimeEq.html — fixed-size
  hash and certificate-pin comparison.

## Native startup and integration

Call `ServerIdentity::load_or_create` once per process, with an app-private root
and the native `IdentityNamespace`, before constructing `ControlServices`,
`OperationRegistry`, or the event hub. The development and production namespaces
have separate 0700 directories, UUIDs, certificate/private-key bundles and
credentials. Their listener ports remain the listener task's responsibility.

The UUID persists independently in `server-id`. The certificate and PKCS#8 key
share an atomically replaced `identity.json`; both files are 0600. Reads reject
symlinks, oversized files and malformed bundles. Private key material has no
Debug implementation. Missing key material first revokes every active credential
for that UUID, preserving rows, then generates a new certificate; the caller
must show `requires_repairing()`. Corrupt existing material fails closed.

Build TLS from `server_config()`. DNS names and addresses never establish trust:
`pinned_client_config()` verifies the complete certificate SHA-256 pin and
rustls verifies the handshake signature. A different certificate fails without
sending any buffered application secret. There is no trust-all verifier.

`PairingManager` belongs to that identity and is initially closed. Only explicit
native setup or an already-paired phone's validated confirmation may call
`begin`. The caller must record `finish_pairing_issuance` only when it actually
issues the returned ephemeral candidate; never persist or journal that candidate.
The manager serializes each attempt via mutable access. QR and manual pairing
share the same 120-second monotonic deadline and successful-use consumption.
Five failed secrets close an attempt. Five failures per peer and 20 globally in
60 seconds apply across newly opened attempts. Failure tracking stays bounded by
that global limit. The SQLite store enforces eight active phones atomically.

`parse_pair_request` is the bounded, duplicate-key-rejecting POST body parser.
The manual challenge is `{ "v": 1, "mode": "manual_challenge" }`; it returns
only `{ "v": 1, "serverDeviceId": "...", "attemptId": "..." }` during an active
window and does not extend or consume the attempt.

Manual clients first use `CertificateProbe`, whose verifier records the
presented certificate and deliberately rejects the TLS handshake before
application data. They call `probe.observation()` once to capture an immutable
`CertificateObservation`, display that object's locally derived `phrase()`, and
retain the exact object in the confirmation UI. Explicit human confirmation
consumes that displayed object via `observation.confirm_phrase(true)` to obtain
a fresh strictly pinned configuration. A retry or parallel probe can produce a
new observation, but cannot alter the phrase or fingerprint of the displayed
one. Never fetch a new observation in the confirmation callback: any replacement
observation requires displaying its own phrase and a new explicit confirmation.
Only this new connection may request a challenge or
transmit the 12-character base32 code. Persist the pin only after pairing success.
The fixed `bip39-en-v1` list selects six consecutive 11-bit indices from the
first 66 bits of the complete certificate's SHA-256 hash, most-significant bit
first. This is a certificate comparison phrase, not a BIP-39 seed mnemonic.

Use `AuthManager::authenticate_subscription` for socket admission, pass its
already-admitted subscription to `state::snapshot_from_admitted_subscription`, and
use
`AuthManager::revoke_device` after confirmation validation for revocation.
The event hub owns the shared synchronous admission gate, so duplicate manager
wrappers cannot race admission against revocation. Revocation commits the DB
change, clears that owner's queued private events and wakes blocked receivers
with `unauthorized`. Never create a new raw subscription after authentication: it
would lose a revocation that occurred between those steps. The listener must close the socket when its subscription
returns this terminal error. This also supports self-revocation. No gate or
SQLite guard should be held across network awaits. Server disable and listener
shutdown remain the listener lifecycle task's responsibility.

## Task 5 listener integration

`RemoteServer` is managed by native setup in a disabled state. Construct one
`ServerRuntime` from the already loaded `Arc<ServerIdentity>` and identity-bound
`ControlServices`, then retain that runtime across enable/disable. Its first
construction recovers unfinished journal entries once for the native hub/process.
Re-enabling a listener does not recover live work. A second live runtime is
rejected; pending native dispatch work retains the same registration lease and
per-device admission pool. A failed constructor releases its registration.

Task 6 supplies selected interface addresses and the local interface inventory
to `BindAddresses::selected`. This native-only type rejects wildcards, loopback,
public addresses, missing interface membership, mixed ports, duplicate addresses,
and unscoped IPv6 link-local addresses. Tests use an explicit private loopback
fixture. `default_port` returns 47831 for production and 47832 for development.
Binding all requested addresses must succeed before any accept loop starts;
occupied ports fail clearly and never select an implicit alternative. Task 6 owns
interface discovery, network UI, alternative-port persistence and DNS-SD.

The HTTPS endpoints are POST `/remote/v1/pair`, GET `/remote/v1/ws`, and GET
`/remote/v1/thumbnails/{opaqueId}`. There is no HTTP static file server, Tauri
forwarder or media-download route. Bearers are accepted only through a single
Authorization header. Query strings and browser Origin headers are rejected.
HTTP parsing has bounded headers/buffers and an overall five-second
TLS/header/authentication deadline. Every non-upgrade response closes its HTTP
connection. Successful WebSocket upgrade retains its correct Upgrade header.

The initial authenticated socket sends Hello, the actual native snapshot and
buffered events, using the exact admitted subscription. A `state.get` refresh
keeps the last successfully delivered sequence on that same subscription.
Queued events still advance sequence even when snapshot topic revisions already
cover their payloads. Never reset the stream to the current hub sequence while
older events remain queued. Overflow closes with WebSocket Policy and the static
reason `resync_required`; the client must reconnect for a fresh admitted snapshot
before applying further deltas. No additional unversioned JSON error envelope is
introduced. The independent subscription revocation signal interrupts snapshots,
reads and blocked writes, including thumbnail responses.

Current advertised methods are `state.get`, `capabilities.get`, `operations.get`,
`operations.cancel`, and `player.pause`. Pause uses the same managed player/session
validation and IPC implementation as local controls. Accepted durable work is
journaled before effects and is not cancelled merely because its reply is lost.
Cancellation acknowledges a request, not native completion. Future catalogue
methods remain unsupported until their owning task provides the service adapter;
that includes browser pointer input and its eventual coalesced channel, renderer
input, and phone-initiated pairing/revocation confirmation workflows. Native
`begin_pairing` remains the explicit local setup boundary, not a remotely callable
journal handler. Transient request hashes/results remain bounded to 256 IDs in
that connection's memory and never enter SQLite or logs.

Remote thumbnails resolve opaque IDs through native cache metadata, reopen the
source with existing descriptor/lease checks, and use a separate `png512-v1`
profile capped at 512 pixels and 512 KiB. Desktop `png640-v1` behavior and published
thumbnail IDs remain unchanged. Four per-phone thumbnail permits are held through
the HTTP response lifetime. No original path is returned.

Additional exact-pinned dependencies are tokio-rustls 0.26.6 (default features
disabled; ring and tls12), tokio-tungstenite 0.30.0 (default features disabled;
handshake), and direct uses of already locked futures-util 0.3.34 (sink), hyper
1.12.0 (server/http1), hyper-util 0.1.21 (tokio), http-body-util 0.1.5 and bytes
1.12.1. Existing locked versions are preserved. Primary API references:

- https://docs.rs/crate/tokio-rustls/0.26.6 — maintained async Rustls transport.
- https://docs.rs/crate/tokio-tungstenite/0.30.0/features — minimal handshake feature.
- https://docs.rs/tokio-tungstenite/0.30.0/tokio_tungstenite/struct.WebSocketStream.html — raw upgraded streams, read cancellation safety and send boundaries.
- https://docs.rs/hyper/1.12.0/hyper/server/conn/http1/struct.Builder.html — bounded HTTP/1 headers/buffers and timed reads.
