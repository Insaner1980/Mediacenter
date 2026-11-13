# Task 21 Android Keystore device evidence

On 2026-11-13, USB ADB recognized the test Pixel as an authorized device with Android API 37 after installation of `android-sdk-platform-tools-common` and USB reconnection. The installed Remote reports application ID `com.emmah.colordock.remote`, version code `1`, and version name `1.0.0`. `apksigner verify --print-certs` successfully read both signers and their SHA-256 certificate digests differed. The debug candidate cannot update this installed app in place.

The isolated test artifacts were built from Android revision `caad0fda0cb682e739ecb521b944a33ed404aabb` with the existing Task 7 Gradle init script. `aapt2` verified that the app package is `com.emmah.colordock.remote.task7fixture` and the runner package is `com.emmah.colordock.remote.task7fixture.test` targeting the isolated app. Only those two packages were installed. They remain installed on the test phone.

| Instrumentation invocation | Verified result |
| --- | --- |
| `encryptedRoundTripAndForgetPreserveTelevisionBytes` | Passed on-device encryption, reload, forget, corrupt-record rejection and preservation of the fixture's television credential and metadata bytes. |
| `encryptedRecordSurvivesAnActualProcessRestart`, write phase | Passed. Stored the synthetic record through Android Keystore. |
| `encryptedRecordSurvivesAnActualProcessRestart`, read phase | Passed after `adb shell am force-stop com.emmah.colordock.remote.task7fixture`. Recovered the synthetic record and removed it. |

All three invocations returned raw runner status `1` (started), `0` (passed), and final instrumentation code `-1`, with `OK (1 test)` for each. No assumption-failure or skipped status was reported. Sanitized runner output is retained in the Task 21 checkpoint workspace. Phone-to-desktop pairing, HDMI/audio, LG production settings and upgrade preservation still require their own acceptance runs.
