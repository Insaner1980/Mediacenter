# Browser feasibility gate

Status: **Task 17 feasibility passed on the disposable Chrome profile. Chrome can take focus during input.** Local-media operation is independent of this extension.

The disposable admission harness uses the actual `/opt/google/chrome/google-chrome`. It stores a new profile under `.tools/browser-feasibility/profile` and redirects `XDG_CONFIG_HOME` to `.tools/browser-feasibility/config`. It never opens existing profiles or reads cookies. The extension ID is `njhbcllpkbihpkioegopblabopnkhndk` (public manifest key only).

`browser-extension/harness/admission.mjs` first attempts the official experimental `Extensions.loadUnpacked` CDP operation over inherited pipes with the required unsafe-extension-debugging switch. It then closes that process and relaunches the same disposable profile without extension-loading or unsafe-extension-debugging switches. Both phases must return the exact extension ID, and the second must retain the exact enrollment identifier. This is a test harness, not the proposed installation mechanism. Debugging has no TCP listener.

Supported installation remains an explicit per-profile unpacked installation through `chrome://extensions`, Developer mode, Load unpacked. This additional component is required only for streaming remote input. The user must approve Chrome's visible debugger permission/attachment indications. No warning-suppression switches are used.

Sources supplied with the task: [native messaging](https://developer.chrome.com/docs/extensions/develop/concepts/native-messaging), [tab-scoped debugger API](https://developer.chrome.com/docs/extensions/reference/api/debugger), [explicit unpacked installation](https://developer.chrome.com/docs/extensions/get-started/tutorial/hello-world), [experimental CDP extension loading](https://chromedevtools.github.io/devtools-protocol/tot/Extensions/).

Real Chrome native-host connection, input, pointer, unrelated-target isolation, focus changes, and service-worker reconnection remain unverified until recorded below. A passing admission probe alone does not establish them.

## Actual Chrome result — 2026-11-10

Parent launched the reviewed harness with `source scripts/dev-env.sh && node browser-extension/harness/admission.mjs`. The installed build identified itself as `Chrome/157.0.8103.50`. Experimental loading succeeded; the real extension page returned its exact ID and an explicitly created profile enrollment identifier. After process shutdown and restart without `--enable-unsafe-extension-debugging` or any extension-loading switch, the extension page was unavailable. The harness exited unsuccessfully with `extension page unavailable` and did **not** mark retention passed.

The disposable artifact `.tools/browser-feasibility/admission-result.json` contains the exact launch arguments and result. Its sanitized durable counterpart is `browser-admission-result.json` beside this document. No native host was registered or launched. No production profile was enrolled, modified, or copied.

This establishes only that the experimental harness loading path does not meet the required persistence gate on this build. It does not establish that supported Developer mode / Load unpacked installation fails. That supported explicit per-profile installation remains unverified. Do not proceed to Tasks 18–19 or claim streaming input readiness from the admission result or from unit tests.

## Revised implementation boundary and handoff

Keep local media usable independently. Hold browser broker integration, service launch enrollment, phone touchpad and streaming acceptance until the supported installation path is proved in a disposable profile. The next bounded gate must use explicit supported installation, restart Chrome without one-shot loading/unsafe flags, and then prove a narrowly registered native host, tab-scoped Unicode/key/click input, unrelated-tab and external-focus isolation, debugger detach/navigation invalidation, service-worker reconnection and visible warning behavior.

No global input injection, ordinary-Chrome process ownership, login/cookie copying, debug TCP port, or suppression of Chrome security UI is an acceptable fallback. Ordinary-profile/HBO login and HDMI audio remain outside this proof. Managed `--app` windows and service-specific authentication popups are unverified; one-time installation and browser/OS security dialogs require desktop access.

## Supported UI attempt — native chooser handoff

The parent also ran `node browser-extension/harness/admission.mjs --supported-ui` with no unsafe-extension-debugging/loading flags. Developer mode was enabled through Chrome's rendered control. The one documented CDP drag/drop attempt did not supply a usable native folder identity: Chrome visibly reported **Failed to load extension / No dragged path**. The extension page stayed unavailable. See `browser-supported-install-ui.png` and `browser-supported-install-result.json`.

This automation limitation does not establish that supported Load unpacked installation fails. The next action is desktop folder selection, with no private Chrome APIs or profile edits:

```sh
XDG_CONFIG_HOME=/home/emma/Dev/Mediacenter/.tools/browser-feasibility/config /opt/google/chrome/google-chrome --user-data-dir=/home/emma/Dev/Mediacenter/.tools/browser-feasibility/profile --no-first-run --no-default-browser-check chrome://extensions/
```

In that disposable profile, choose **Load unpacked** and select `/home/emma/Dev/Mediacenter/browser-extension`. Then close/relaunch and verify retention before native-host/broker investment. Do not repeat the experimental loader as a substitute. This manual step is pending.

After the manual installation and closing the disposable Chrome process, parent can review/run `source scripts/dev-env.sh && node browser-extension/harness/admission.mjs --installed`. This mode never loads an extension, modifies install preferences, or uses unsafe-extension-debugging flags. It verifies the already installed extension page, enrolls a synthetic profile identifier, closes Chrome, and checks that same extension and identifier after relaunch. Result: `.tools/browser-feasibility/installed-retention-result.json`.

## Supported retention and native messaging — 2026-11-13

The user installed the extension through Developer mode → Load unpacked in the disposable profile and closed Chrome. The `--installed` probe then passed on Chrome `157.0.8103.78`. Two launches used the same isolated profile and inherited debugging pipe. The expected extension and synthetic enrollment survived the restart. Neither launch used an extension-loading or unsafe-debugging flag. See [retention result](browser-installed-retention-result.json).

The native-input fixture starts a one-use broker in a private runtime directory. Chrome discovers its native host through the exact-origin manifest in the disposable user-data directory's `NativeMessagingHosts` subdirectory. The fixture removes that manifest after each run. The broker checks both Unix peer UID and a random secret and rejects expired, stale and reused grants. The native host and broker example build with `--features browser-proof`. The Mediacenter runtime does not start the fixture.

Run from the repository root:

```sh
source scripts/dev-env.sh
cargo build --locked --manifest-path src-tauri/Cargo.toml --features browser-proof --bin mediacenter-browser-host --example browser-broker-proof
MEDIACENTER_BROWSER_HOST="$PWD/src-tauri/target/debug/mediacenter-browser-host" node --test browser-extension/tests/*.test.mjs
node browser-extension/harness/native-input.mjs --active-catalog
node browser-extension/harness/native-input.mjs --background-tab
MEDIACENTER_BROWSER_APP_WINDOW=1 node browser-extension/harness/native-input.mjs --active-catalog
bash browser-extension/harness/external-focus.sh
```

- Active catalog tab: Unicode text, one Enter and one click reached the catalog page. The unrelated tab's input and event counters stayed unchanged. A stale generation was rejected, and navigation and worker reload both retired the previous target.
- Background catalog tab: the final fixture creates the catalog active, then activates the unrelated tab before sending input. Unicode, Enter and click reached the catalog. The unrelated tab stayed unchanged. An earlier fixture with a catalog created inactive observed no events after Chrome acknowledged the calls. A command acknowledgment alone cannot establish delivery.
- External focus: on a private Xvfb/Metacity display, an unrelated GTK text entry had focus before the probe. The catalog received all three inputs. The entry stayed empty, received no Enter, and remained open. Chrome took focus.
- Managed `--app` launch: the original app window remained open and unchanged. The catalog target opened in a separate browser window. This tests tab creation in the same profile with a pre-existing app window.
- Worker reload: the old generation remained invalid. A fresh one-use grant connected to the host and created a new target after reload.
- Chrome displayed its debugger warning with a Cancel button when the attached catalog tab was active. [Warning screenshot](browser-native-input-warning.png)

See [native-input results](browser-native-input-result.json) for the four scenarios. The private-display runner uses the local Xvfb build and Metacity. The Xvfb fixture needed a Chrome process without a session-bus portal connection for loopback page loads. This was a fixture adjustment only. Task 18 must account for focus changes and the difference between command submission and observed delivery. Task 19 still needs provider-specific app-window and login behavior. Production streaming readiness remains pending.

## Task 18 viewport check

`node browser-extension/harness/native-input.mjs --resize` passed on Chrome `157.0.8103.78`. The fixture measured the enrolled page, changed its CSS viewport using Chrome's test emulation command, then tried the old input. The extension returned `stale_viewport` and the page observed no text, key or click. A subsequent active-catalog run also passed. Production viewport generation updates and Android input flow remain to be integrated.
