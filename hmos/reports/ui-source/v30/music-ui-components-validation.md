# Music panel, footer and complete lyrics content

The new `MusicPanelContent`, `MusicFooterContent` and `LyricsDialogContent` use the actual `MusicUiView` and Controller callback contract. They do not own the outer panel or footer material. The enclosing Index remains responsible for material overrides and the 22-radius music shell.

The design was read from `build/win-cloud-20261005/lib/music/music_panel.dart`, `lib/little_tips.dart`, and `lib/hold_reorder.dart`. Icon code points were checked in the installed Flutter `icons.dart`, matching the MorrowIcons font used by Index. The four reference files have byte/hash inventories in the report.

Implemented content includes the 17-size header icon with a 7 gap, 12-size semibold title, 18-size add icon, 48-size rounded cover fallback, 11-size title, 9-size track position/state, 40-size play/pause target, seek/timestamps and the footer-lyrics switch. The selected track surface has an inset edge for the neumorphism/clay styles, with negative-depth shading; flat style adds no edge. The current-track area is constrained to 64, and playlist rows to 40, so an inset Canvas cannot enlarge the enclosing content.

The playlist retains its child and the same Scroller after its first visit; collapse clips it to zero height rather than destroying it. It is limited to 240 height and keyed by stable track/import identity. Row menu actions move up/down or remove an actual track. The 36 × 40 drag handle uses the Flutter 380 ms hold and a private track/import/hash/revision/order ticket. Measured row geometry, target-half insertion, edge scrolling and lifecycle cancellation reject stale or missing identities. No external drag payload is accepted.

Ordinary music writes and seek controls follow the stable-library and foreground gates. A selected idle track can be started explicitly without inventing a prepared duration. While a library operation is Unknown, the main control still offers pause for the original reported owned player in its actual playing phase; it does not enable seek, selection or another mutation. The Controller remains responsible for the real owned-player queue and pause confirmation.

Recovery shows only the Controller's admitted original-operation flags. A retained file with an existing original request has a separate inspect button, with no generated track ID. A file without such a request presents only the actual Native pending choices or the explicitly admitted new-import action. There is no Unknown spool delete/cancel action. Closing failures use the actual `cleanup_failed` phase and retain the original retry path.

Full lyrics use a scrollable, copyable Text without maxLines, substring or ellipsis. A complete local candidate that has not been saved remains explicitly marked as unsaved and viewable after an Unknown lyrics write. It is not silently cropped to the Native 49,152-byte metadata limit. The footer uses the actual playing/lyrics-display flags and actual footer text; its default content has no fill, blur, border or shadow. Cover import, online lyrics and encrypted music have no pretend functional buttons.

## Bounded verification

Run: `hmos/tool/run-music-ui-components.cjs initial`.

- 24 / 24 actual-source component method/data tests passed, exit 0, 4,133.6841 ms.
- All 9 test and actual-source inputs and all 4 design-reference files were exact before/after the run.
- Test log SHA-256: `93058494901a3a8a08a807bf43ff8beb199860f7dc5d97dc12c46e86732f06b1`.
- Panel: 27,920 bytes, SHA-256 `7a8dcc1c02932c0ab75461e32d6b663da11730c33df931341b38986ced439a6e`.
- Footer/dialog: 4,487 bytes, SHA-256 `4acf34b63ffbd842f1d5aaf944e00d083a43a131f25fbf42655196cd1ea4278d`.

The harness extracts the actual component fields and methods and loads the real MusicUi, MusicLibrary, MusicPlayback and UiStrings modules. ArkUI rendering, measured Area/GestureEvent, Scroller, timers and callback delivery are controlled seams. Builder source checks bound the design constants and the uncropped lyrics contract; they do not render ArkUI.

Qualification: **PASS_SCOPED_ACTUAL_COMPONENT_METHODS**. SDK compilation, device rendering, real pointer/drag behavior, AVPlayer audio, portrait/tablet layouts and universal frame removal are **NOT_RUN** in this initial report. They require the root's fresh integrated build and device evidence. The initial freeze and its reports remain historical evidence; the concrete SDK repair below supersedes the component source hashes.

## SDK a1 repair and original unsaved lyrics identity

The root's `dev30-music-ui-a1-sdk-build.log` failed compilation and specifically identified the component method `position` conflicting with `CustomComponent.position`, and the Linear Progress options lacking their required `value`. The method and all call sites are now `positionMs`; the Progress options include `value: 0`. Callback-interface typing is a shared MusicUi contract change owned by the root, not edited by this component task. This repair run does not by itself prove successful SDK compilation.

The shared view added `unsavedLyricsTrackId` and `unsavedLyricsTitle`. The panel now names the original unsaved track and sends that actual ID to `lyrics_view`, even when another track is selected. The Controller must preserve those original candidate fields and emit the unsaved body only when the viewer target matches. When an unsaved body is admitted, the dialog uses its original track title and an explicit unsaved source label, so it cannot label the body with another viewer's saved title/source. A new actual-method test covers these independent title/body targets and preserves the full candidate.

Run: `hmos/tool/run-music-ui-components.cjs sdk-a1-repair`.

- 25 / 25 passed, exit 0, 4,980.8854 ms; 9 inputs and 4 references exact before/after.
- Log SHA-256: `78c499bf66dbcb6944c66a93fc2a2929db97d4cd71042dfa6806f7b76bb1598d`.
- Panel: 28,252 bytes, SHA-256 `6a627db1b98c978908445b603a4b380d467dce5b812eeb372c2e5ddbd386c7af`.
- Footer/dialog: 4,777 bytes, SHA-256 `7817b70bc7e5222110bdbe9ae0fb1c98c89b1b8cc123133c806da5e93a81b3a9`.

This repair is retained historical evidence. The final caption correction below replaces the footer/tool freeze. It has the same bounded method/provider scope as the initial run, with fresh SDK success and device rendering still unproven here. Earlier files were retained unchanged; future corrections require a new labelled run.

### Final original-candidate caption correction

An unsaved candidate has not received an accepted Native parse result. Its dialog therefore no longer reuses `lyricsUntimed` from an older saved/cache entry to label that candidate as plain text. The `sdk-a1-repair2` run retained 25 / 25 passing tests, exit 0, 4,501.8814 ms, with 9 inputs and 4 design references exact before/after. Log SHA-256: `93df777994d812476bf8dd21cfa8bf4ea9109c25cd285f2e562297c65510b784`.

Final component/tool freeze: Panel stays 28,252 bytes / `6a627db1b98c978908445b603a4b380d467dce5b812eeb372c2e5ddbd386c7af`; Footer/dialog is 4,805 bytes / `dacae9261a10ee6c1a7ff5a02de9b0e79a525f06f930d39d64bb9f22866be317`. The final actual-source test file is 17,017 bytes / `8c74578cab30e4875b4302daa8e1699c74d1186b31fdf4f7a546858da31f8b43`.
