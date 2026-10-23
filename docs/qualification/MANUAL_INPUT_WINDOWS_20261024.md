# Windows manual keyboard and IME finding

The user's additional account was that the title was pasted, the body could only be entered through the IME, Ctrl+Home did not respond, and Shift+Right worked. These are separate observations; a correct final selection does not prove that Ctrl+Home worked.

## Fresh clipboard regression and correction

A separate fresh original-host probe reproduced a missing-file failure when an in-memory clipboard `XFile` was passed to the path-based import API. Clipboard HTML/RTF/image sources can have a display name in `XFile.path`, not a disk file. That failed import leaves the original draft's capture incomplete and prevents further autosaving. This explains a reproducible failure class consistent with the manual observation; it is not a retrospective inspection or resolution of the retained manual owner.

The formal editor now passes the original `XFile` to a source-based import method. After the original owner admits the import, this method streams the source into a uniquely created file in its own host cache, enforces the existing byte bound and exact length, and uses the existing Cap'n Proto import path. It preserves that input through unresolved imports or failed shutdown, and removes only its own input files after the original host has closed successfully. It never interprets the clipboard display name as a disk path.

`clipboard-formal-native-002` passed one formal title/body clipboard test and all nine existing original-host draft-owner tests, with unchanged source during the run. The formal test verifies both memory attachment byte streams, natural autosave, Windows framework Ctrl+Home followed by Shift+Right, exact text/selection/request after a distinct original-host restart, no business submissions, and removal of both owned input files after successful closure. The test injects synthetic clipboard content and framework keyboard events; it does not read the system clipboard or establish actual native-keyboard acceptance. Its evidence is under `D:\build_cache\_audit\windows-restriction-lineage-20261022-019\clipboard-formal-native-002` and is reproducible with `tool/verify_editor_clipboard_windows.py` and a fresh output directory.

The earlier `owner-autosave-probe-003` failed on the memory source's relative display path and remains retained without replay. The corrected fresh probe 004 completed 35 original-host saves, two memory-source imports, byte readback, and normal closure. Neither probe replaces the still-open manual input gate.

The fresh `p3-manual-input-windows-003` process used the normal `WidgetsFlutterBinding`, formal `NewIdeaDialog`, and original protected Rust Workbench. The user manually supplied the title/body and corrected whitespace and selection. The final observed values matched the predefined text, including the emoji, newline, leading/trailing spaces, and the body selection from UTF-16 offset 0 to 1. The user reported no business submission.

An actual IME composing range was observed before confirmation. Its exact encoded request and generation were independently read from the original host while composing remained active. This proves that bounded intermediate composing snapshot persisted; it does not establish that the final text persisted.

Final automatic saving stopped at generation 10. The UI displayed the capture-incomplete message and offered original import review. One review action returned a generic unresolved message. After the user's completion signal, the original application failed its 45-second autosave-settling check with `Bad state: Original autosave did not settle`. No close/restart phase ran. The concrete capture failure is not yet established by this observation alone.

The original application/host and failed test driver remain retained. Exec session 55553 and application PID 14972 are sealed `SEALED_UNKNOWN_NOT_ACCEPTED_NO_REPLAY`; they must not be queried, closed, adopted or retried to obtain a new result. The prior native-input run 002 (exec 3329) remains independently sealed.

Evidence: `D:\build_cache\_audit\windows-restriction-lineage-20261022-019\manual-native-input-observation-003.json`, with unchanged source inventory at seal. The frozen source digest is `44aa16ab4a4dcbd45ea6051bc932bd17f782414196d9bd2ae3628d95ee846409`. Final native autosave/restart, full P3, source acceptance and release eligibility remain false. Further diagnosis uses fresh synthetic owners.


## Fresh manual case 004 passed

The user completed title and body paste, a real IME composing interval, exact whitespace and the first-character selection in fresh normal-binding process 23600. The original protected host persisted the composing snapshot and the final generation 21 request. The original app/host closed normally; a distinct process 21528 recovered exactly the same encoded request, generation, text and selection. Both phases verified no business submissions and closed successfully. The source inventory remained unchanged at digest `f3d23c4b20777d6d7697cbce5ee6f5c8182e4a440c29d7eecd02b7bbd249037a`.

Evidence: `D:\build_cache\_audit\windows-restriction-lineage-20261022-019\p3-manual-input-windows-004\result.json`. This is actual manually supplied Windows/IME input and durable original-host evidence. It does not identify the physical keyboard device, certify rendered pixels or close all P3 gates. Retained manual case 003 is unchanged and was not revisited.

The user reported that Ctrl+Home still failed using the numeric keypad with Num Lock off. A separate fresh normal-binding diagnostic, with no business owner, observed actual Control+Numpad7 events, Num Lock off, and no selection movement. The editor now maps unlocked Windows Numpad7/Numpad1 to the existing focused EditableText Home/End actions, including Ctrl and Shift combinations. Num Lock on does not activate these aliases. Twelve scoped regression methods passed, including the formal original-host clipboard/autosave/restart test; native verification of the corrected keypad path is separate and pending. Evidence: `keyboard-home-probe-001` and `clipboard-formal-native-003` under the same audit directory.


## Corrected native keypad case 005 passed

The user confirmed that, with Num Lock off, Ctrl+numeric-keypad Home returned the caret from the second line to the beginning and Shift+Right selected the first space. Fresh original process 17432 observed native Control+Numpad7 and a selection change from 22 to 0. Natural autosave persisted generation 3 and the final selection 0 to 1. After normal closure, distinct process 23236 restored the exact original encoded request, generation, text and selection. Both phases completed with zero business submissions and normal original-owner closure. The source inventory stayed unchanged at digest `b8cc7d0c65d567599c80a1dd2a513d29849e7ce0ba48038654108b502b8f5f7f`.

Evidence: `D:\build_cache\_audit\windows-restriction-lineage-20261022-019\p3-manual-keypad-windows-005\result.json`. The fixture prefilled the text; this case qualifies actual native keypad navigation, autosave and restart, not manually typed text or IME. Case 004 separately covers manual text and composing. Physical keyboard identity and rendered-pixel certification remain outside this evidence. Full P3 and release acceptance remain open.
