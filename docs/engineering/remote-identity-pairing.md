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
