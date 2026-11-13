# Set up Mediacenter Remote

Use the Mediacenter Debian package and Remote Android APK named in the same compatibility manifest. The candidate manifest records both revisions and checksums. See the [acceptance record](evidence/remote-control/task21-22-acceptance.md) for the scope verified on hardware.

## Connect your phone

1. Open Mediacenter on the computer. In **Settings → Remote control**, choose your trusted local network and a port, then select **Enable remote control**.
2. Connect the phone to the same local network. In Remote, select **Add Mediacenter** and allow local network access when Android asks.
3. On the computer select **Pair a phone**. Scan the QR code, or choose **Pair without a camera** and enter the displayed address and port. Compare all six words before confirming the match and entering the one-use code. Cancel if any word differs.

Select Mediacenter as the target in Remote, then choose Videos, Music, Photos, or a service. Choose **Keep available for Remote** when closing the computer window to retain the connection. In Remote settings, **Start after computer login** starts Mediacenter after the next desktop sign-in. Use **End session** to stop owned playback after confirmation and keep Remote available.

Use the existing HDMI mirror. It needs no new display configuration.

## Set up Chrome for streaming controls

Local media can be controlled as soon as the phone is paired. For streaming input, set up the extension in each Google Chrome profile you plan to use. Use ordinary Chrome for HBO Max so it can use that profile's login. For a managed service, use the service's Chrome user-data directory under `$XDG_DATA_HOME/io.emma.mediacenter/browser-profiles/` or `~/.local/share/io.emma.mediacenter/browser-profiles/` with default Linux settings. Open the service once from the computer to create its directory.

1. As your desktop user, register the native host for ordinary Google Chrome. For a managed service or custom Chrome launch, supply the absolute `--user-data-dir` used by that browser. To identify the root in a running Chrome instance, open `chrome://version` and use the parent of **Profile Path**. Each user-data root has one registration. Extension enrollment is done separately in each profile. [Chrome user-data directories](https://chromium.googlesource.com/chromium/src/+/HEAD/docs/user_data_dir.md)

   ```sh
   python3 /usr/share/mediacenter/remote-release.py install
   python3 /usr/share/mediacenter/remote-release.py verify
   ```

   For example, to register the managed Netflix profile:

   ```sh
   profile_dir="${XDG_DATA_HOME:-$HOME/.local/share}/io.emma.mediacenter/browser-profiles/netflix"
   python3 /usr/share/mediacenter/remote-release.py install --user-data-dir "$profile_dir"
   python3 /usr/share/mediacenter/remote-release.py verify --user-data-dir "$profile_dir"
   ```

   The registration points to `/usr/bin/mediacenter-browser-host` and allows only extension ID `njhbcllpkbihpkioegopblabopnkhndk`. [Chrome native messaging](https://developer.chrome.com/docs/extensions/develop/concepts/native-messaging)

2. In that Chrome profile, open `chrome://extensions`, enable **Developer mode**, and choose **Load unpacked**. Select `/usr/share/mediacenter/browser-extension` and check the ID above. A managed profile can be opened as a regular Chrome window for setup. Close the service's existing window first, then run `google-chrome --user-data-dir="$profile_dir" chrome://extensions/` with the service directory set as above. [Load an unpacked extension](https://developer.chrome.com/docs/extensions/get-started/tutorial/hello-world#load-unpacked)
3. With Mediacenter running and remote control enabled, open the **Mediacenter Remote** extension. Select **Ordinary Chrome** for HBO Max or **Mediacenter-managed Chrome** and the corresponding service, then choose **Enroll this profile**. Mediacenter can start an enrolled service profile on demand from its standard service directory. Keep the enrolled ordinary Chrome profile open for HBO Max. If a service shows that it needs setup, open the intended profile and check the extension's connection.

On the phone, open the service and use the touchpad and keyboard on its enrolled page when those controls are available. Chrome displays its debugger attachment warning and may bring the service window forward. Browser menus, extension dialogs, password-manager prompts and operating-system dialogs need the computer. Some service sign-in flows may also need the computer. Use the website's controls for play, pause, seek or volume when Remote does not show a service control. [Chrome debugger permissions](https://developer.chrome.com/docs/extensions/reference/api/debugger)

## Cannot connect

- Confirm Mediacenter is open or was left available for Remote, and check its **Remote control** settings for the listener status.
- Check local network permission on the phone and connect both devices to the same trusted network. If discovery is unavailable, use the displayed pairing address and port. Guest Wi-Fi can prevent devices from reaching each other. If NordVPN is in use, check its LAN access settings for this network.
- If the app reports a version mismatch, compare the installed builds with the same compatibility manifest. If the computer reports that its certificate was replaced, verify that you're pairing with the expected computer and pair again from its screen.
- If the service is open but browser controls are unavailable, check enrollment in the intended Chrome profile and run `verify` for that same user-data directory. Reopen the service after Chrome closes, the debugger detaches, or the page requires a new connection.

## No sound

Turn on the TV, check its HDMI input, and use **Audio output** to select and test the intended HDMI endpoint. If the HDMI endpoint is missing, check the connection and retry after the endpoint reappears. For ordinary Chrome, including HBO Max, set the Chrome audio output to HDMI from the computer's sound settings during setup. Use the service's own volume and mute controls where available.

## Disable or remove

- **Disable remote control** in Mediacenter stops phone access. To remove one phone's authorization, select it under **Paired phones**, choose **Revoke**, and confirm. To use that phone again, pair it from Mediacenter.
- For uninstall, first turn off **Start after computer login** in Remote while it is connected, then quit Mediacenter. Check that `${XDG_CONFIG_HOME:-$HOME/.config}/autostart/io.emma.mediacenter-remote.desktop` is gone.
- Remove **Mediacenter Remote** from `chrome://extensions` in each enrolled profile. Run `python3 /usr/share/mediacenter/remote-release.py uninstall` for ordinary Chrome and repeat with each managed or custom `--user-data-dir` before removing the Debian package. The helper only removes the matching Mediacenter host manifest. Supply the same `--host` path you used to register a nonstandard build.
- Remove the Mediacenter package through the system package manager. This leaves the user data directory, Chrome profiles and original media on disk.

## Update and rollback

Quit Mediacenter before backing up its entire application data directory (`$XDG_DATA_HOME/io.emma.mediacenter/` or `~/.local/share/io.emma.mediacenter/`). Keep the backup private. It contains `library.db` and remote identity material. On Android, verify the APK application ID and signer against the installed Remote before updating in place. An APK with a different signer needs a build signed with the installed key. Keep the installed app and its data while obtaining the matching build.

Older Mediacenter builds refuse newer database schemas. To roll back, close Mediacenter, move the newer data directory aside, and restore a complete, compatible backup to the original location before starting the older package. Keep the database and remote identity from the same backup. Changes since that backup may need to be reapplied.

## Candidate manifest

Build the desktop from its locked dependencies with `npm ci` followed by `npm run tauri -- build --bundles deb -- --locked`. In the Remote checkout, build the Android test candidate with `./gradlew :app:assembleDebug` using SDK 37 and the repository's Gradle wrapper and Java toolchain. For an installed-device upgrade, build and verify a candidate signed with the installed key.

After building the tested Debian package and APK, run the generator from the source checkout with Python 3.11 or newer and `dpkg-deb`. Pass the full commits used to build both artifacts. `--android-root` defaults to `../Remote` relative to the Mediacenter checkout. The generator checks the shared protocol manifests and extension key at the named commits. It checks that the Debian archive contains the executable host, registration helper and matching extension manifest, then hashes both artifacts. Inspect the APK application ID, version, and signer with the Android SDK tools and record the physical results in the [acceptance checklist](evidence/remote-control/task21-22-acceptance.md).

```sh
python3 scripts/remote-release.py candidate-manifest \
  --desktop-revision "$desktop_revision" --android-revision "$android_revision" \
  --deb "$desktop_package" --apk "$android_package" > compatibility-manifest.json
```
