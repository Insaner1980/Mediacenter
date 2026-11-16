# Scoped UI review

Independent reviewer: `/root/device_ui_review`, 16 November 2026.

Scope: the working-tree change to `RemoteTab.kt` and new `TargetAppShellTest.kt` against Android `137fdd1454e321bdcf2037a2bbc5ffc4e4c83083`.

Verdict: approved; no Critical, Important or Minor findings. The Mediacenter scaffold supplies themed content color and system-bar padding, and the real page applies the modifier to its outer column. LG navigation and the target/generation key are retained. Tests cover theme colors, top/bottom insets and absence of LG navigation; existing tests cover target switching.

Declined scope: broader media behavior, LAN pairing timeout and pending HDMI mirroring. These remain Task 21 acceptance work; the review is not device acceptance. The reviewer ran no builds and made no changes.

Reviewed final source SHA-256:

- `RemoteTab.kt`: `94f21022c6fc1181f5176e0a2dc4579be52599d95ba02ee81d99383a1be635a7`
- `TargetAppShellTest.kt`: `dc9fe88a8d0934ac7737871884c41154428e8554df1b36e66722d5d181bc1626`

## Opt-in physical probe follow-up

The same scoped reviewer inspected the new `PhysicalMediacenterProbeTest.kt` separately. Verdict: safe diagnostic probe, no Critical or Important finding. The test does not confirm identity, pair, store trust or send credentials. Its observed EPERM occurs before TLS, so it provides no TLS acceptance evidence.

Deferred Minor: the TCP preflight uses `endpoint.literal` rather than `network.resolve(endpoint)`, which omits the interface scope for IPv6 link-local addresses. This run is an IPv4 baseline; IPv6 preflight acceptance remains out of scope until the resolver is used or the probe is explicitly restricted. Network-policy changes, pairing, HDMI acceptance and the earlier UI fix were excluded from this review.

## Paired follow-up review

The same independent reviewer approved the final typed network epoch fix (`f842322`), with no remaining findings. An Important issue in the intermediate Display-based version omitted link-layer interface indices; a failing AF_PACKET regression was added and typed address hashing resolved it. Both original-padding and interface-recreation regressions passed in the final suite.

The opaque audio-output mapping (`42996f2`) was approved with no findings. Review confirmed consistent list/requested/effective IDs, current-available-sink resolution for prepare/test and retained native confirmation validation. No global protocol-ID relaxation was introduced.

The extended pin/challenge probe was approved; its resolver correction resolves the earlier IPv6 preflight Minor. The new isolated stored-pairing snapshot probe was approved with one diagnostic-hygiene Minor: an untrusted frame type could enter assertion output. Android `92229bd` maps it to fixed hello/snapshot/other tags, resolving that Minor. Final checks/build and the actual Pixel probe passed. The reviewer performed read-only source review, not device acceptance.

## Live adoption corrections and single-app migration

The same reviewer approved the Android playback-title fallback and the narrow shared VpnCountries bound increase to 250, with no remaining findings. Source gates and device tests are recorded separately from the still-partial full acceptance run.

The reviewer inspected the temporary local migration helpers read-only. Export encryption matched the production stores' aliases, IV/tag layout, atomic writes and authenticated roundtrips. The restore whitelists only the two original DataStore preference files, verifies exact hashes, excludes old encrypted credential files and sidecars, and imports portable records under fresh aliases. Old/new package IDs, label, signatures, preserved APK and installed APK hashes are gated; the companion remains until live verification. One blocker found the initial destructive uninstall outside the rollback guard. Moving it inside the guarded stage resolved the finding; scoped re-review approved the reinstall/restore stage. The installation and both credential imports subsequently passed. This approval is a code/data-preservation review, not proof of TV or Mediacenter UI connectivity after migration.
