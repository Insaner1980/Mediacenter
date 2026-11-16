# Original Remote phone UI corrections

User-approved scope: screenshot findings 1,2,3,4,6,7,8,9. Mediacenter may keep its own visual style. Android source f7ed5a2, original application ID and launcher label Remote. The new AGENTS.md records the no-bullet-separator and icon-only-shortcut requirements.

Six rendered usability regressions were watched RED before the change. Independent read-only review found one P2 regression: hiding Queue with inactive player controls made retained queues inaccessible after Stop. The seventh RED test reproduced that case; Queue is now independently accessible whenever connected. The earlier library-entry test was strengthened to require a fresh request after clicking Browse library. Review found no other confirmed blocker. A 240dp button test does not claim a 240dp dialog window.

Final source gates passed 440 tests with zero failures/errors/skips, Detekt, ktlint, Android lint, ordinary debug APK and matching runner builds. The final Pixel run passed 15 rendered usability, playback and target-ownership tests. Actual LG/chooser/Mediacenter screenshots were inspected privately at the user's existing font setting. A final spacing adjustment fixed the actual 189px-wide icon tile being clipped to 183px; it now measures 189 by 189px. Both the menu and streaming buttons have their complete 126px height, and the MC D-pad is centered.

The APK was installed over the original package with the same signer. Both encrypted credential files and original TV/appearance preferences matched byte-for-byte before/after installation. The real Browse library action returned media, Show menus foregrounded the installed desktop, and both targets appeared Connected together. The runner was removed; only the original Remote remains. Temporary USB stay-awake was restored and font scale preserved. Screenshots, media names, private app data and credentials are excluded here and from artifacts. Full Task21/22 acceptance remains partial.

Local final artifact pair: releases/remote-device-20261116/integrated-ui-f7ed5a2/ (checksums verified).
