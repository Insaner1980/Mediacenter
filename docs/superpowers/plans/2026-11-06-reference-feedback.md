# Screenshot feedback implementation

Approved by the user: implement the four recommendations from the annotated Pictures/references screenshots. Continue on the existing feature branch; preserve unrelated streaming-compatibility edits.

1. One external music player: remove the persistent music bar, ensure an mpv window with usable controls both with embedded artwork and without it. Preserve queue ownership, close/replay, and next-track behavior.
2. Photo viewer: remove duplicate Close photo; retain the dialog Close and Escape.
3. Libraries: retain direct Open folder and current-folder selector, remove management gears/panels. Empty-library action opens the picker directly. Add Media folders management under Settings, preserving grant, scan, cancellation, and removal safeguards.
4. Header output control: open the same compact Picture & sound dialog on every route, including Settings and repeated opens. Reuse verified output operations. Show picture/sound choices; explicit Apply saves and moves active playback. Keep advanced controls in Settings. Closing restores focus and never discards unrelated playlist drafts.

Verification: failing UI regressions before implementation; native plain/audio-with-artwork window regression, output dialog and photo interaction; full UI/Rust/browser suites and responsive layout checks; fresh final code review. Build and inspect 0.1.6, test the extracted package executable with private generated media/audio/display, record limitations and provide install command. No host installation or real output/VPN changes.

Completed: all four approved changes, review fixes for queue controls and music display routing, final extracted 0.1.6 package playback/UI runs, and artifact inspection. Results and remaining hardware limits: [acceptance](../../engineering/evidence/reference-feedback/acceptance.md).
