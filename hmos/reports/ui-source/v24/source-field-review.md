# v24 ordinary-field restoration selection admission

Date: 2026-10-07. Scope: one admission-order repair in actual `Index.leaseSelectionChanged`, its independent actual-method test harness and this report. No Todo model/component, EditorDraft, EditorBusiness, Rust, shared field harness, version, SDK build, device or Git operation was changed/performed by this worker. The overall Flutter/HMOS goal remains OPEN.

## Actual reference sources

Fresh local reads of `build/io-safety-refactor` and `build/win-cloud-20261005` found the same source after CRLF→LF normalization. Worktree metadata still resolves their heads to `925fb8ca6563dcb7de38db8fbdf6b005f9ad5420` and `e83cdf1d3506b001d07ffceaaa9f98b688bcc150`; the actual disk bytes below, including the reference working trees, are the read provenance.

| Source | io-safety raw SHA256 | win-cloud / LF SHA256 |
| --- | --- | --- |
| `lib/main.dart` | `6203B7465F320DC8463DFACB5387EA8B437EC3EC1458CCA7E215E44B8F65CB1E` | `2CB2A519E31AC982D3A8638EB7DE95FE63D5421ED3D1B6ACDA507CD142169F06` |
| `lib/plugins/editor_draft_session.dart` | `422A4AC7C7CACE0115CC50290E027FE63348C6B8273E89A659C24C5302015E7E` | `107CEE0DBFE8E8FD232B66EEA92002077120ABAB0BA6FB67033FA0F3A7693AF8` |

Flutter `main.dart:5693–5708` assigns all initial controller text before freezing `_lastFieldValues` and attaching listeners. `_observeDraft` at 5210–5221 then compares the full controller `TextEditingValue`; later selection/affinity/direction changes revoke stale business consumption through `_editGeneration`. `editor_draft_session.dart:22–29` compares full text, UTF16 selection, affinity, direction and composition, while its snapshot also preserves ordered asset provenance. These sources do not justify treating a bare invalid construction range as a complete new draft, or replacing raw with a matching visible string.

Flutter's listener is not itself focus-filtered: a later legitimate full controller value can change while unfocused. The repaired HMOS gate applies to bare SDK range callbacks; explicit programmatic full-value capture continues through its existing routes. This report does not claim every controller/lifecycle behavior now matches Flutter.

## Defect and minimal repair

Read-before-repair `Index.ets` SHA256 was `34DDECA5A375328ADB1A7EC53DE7F8E5D1A90804CD8634FAA85672D7A588160C`, 285,733 bytes. The original `leaseSelectionChanged` (1078–1084) checked the lease, then range validity, before delegating to `draftSelectionChanged`. Invalid ranges went straight to `leaseUncaptured`, which stores the original event, increments input epoch and blocks flush/cleanup. The focused-field/restore checks in the delegated method therefore came too late for invalid initialization callbacks.

The final [actual Index source](../../../entry/src/main/ets/pages/Index.ets) now checks `ownsEditorView(owner)` **and** `draftFocusedFields.has(name)` before validating or retaining a range. It adds two explanatory comments and the single focus predicate; `draftSelectionChanged`, text/preview capture, restoration timers and retirement protocol are unchanged.

- Unfocused initialization ranges, valid or invalid, create neither a raw mutation nor a new missing-capture event, including while `draftRestoreInput` is true.
- A genuinely focused invalid range still reaches `leaseUncaptured` with its original JSON, even during restoration, attachment work, retirement or an unknown retirement outcome. There is no blanket restore/disabled escape that discards such an event.
- Focused valid ranges still follow the existing complete-value/retirement paths. Valid reverse UTF16 ranges preserve text, affinity, direction, composition and confirmed attachment selections.
- Blur changes the admission state for later callbacks. It does not erase the event already delivered while focused, and it does not prove the SDK callback queue has drained.
- The lease owner/revocation fences remain first. Old/revoked callbacks cannot borrow the current field's focus. No existing Unknown is cleared because visible text/count/range appears correct.

## Fresh actual-method tests

[index-editor-field-selection-tests.log](index-editor-field-selection-tests.log): **28/28 PASS**, zero failures/skips, 3,467.7145ms. The original 20 cases remain, with eight new bounded cases:

1. All four ordinary fields' valid/invalid/unfocused construction ranges before and during restore leave original raw, epoch and pins intact; unchanged flush consumes no new journal generation.
2. Existing Unknown event bytes and blocking state survive subsequent unfocused initialization callbacks.
3. Focused invalid events remain exact for all four fields in active, restore, attachment-working, retiring and retirement-Unknown states.
4. A real focused reverse selection is written with full Unicode text, original composition, affinity/direction, other fields and confirmed pins.
5. Real blur excludes later unfocused ranges without erasing the preceding captured invalid event.
6. Old or revoked lease callbacks cannot alter a replacement editor through range, focus or text/preview routes.
7. Disabled/retiring text callbacks preserve exact input/preview independently of selection focus; invalid preview offsets retain the event and do not invent a full value.
8. A focused valid retirement selection is retained with its original text/range locally, without obtaining a write to the possibly retired raw scope.

The harness extracts fresh verbatim Index methods and loads the actual ETS field/draft/paste models through installed SDK TypeScript 4.9.5. This update includes the actual restore/focus/blur methods in the extraction, and initializes the real `retirementInput`/`editorFocusIntent` state needed by those methods. It does not replace the owner/focus/range admission logic. Worker/service replies and controllable SDK callbacks remain synthetic; native Unicode16, ArkUI event ordering and product SDK compilation are separate qualifications.

## Frozen identities and device boundary

| Final file | Bytes | SHA256 |
| --- | ---: | --- |
| `pages/Index.ets` | 285,929 | `AFF77EB7F1C90E53DDF5D64DD89FB3A037DAB98AC0B08AE0382090A2055EFEAF` |
| `tool/index-editor-field-integration.test.cjs` | 36,105 | `20500F9DDC3AE8FC88BBA196E16EE2B32E39B4028D946559FF294684E2513412` |
| `index-editor-field-selection-tests.log` (raw disk bytes) | 3,188 | `8F128771E9864184FA7F08D0481E288F49388AF857C7B7F2C03E7B757304362F` |

The existing [v23 report](../v23/todo-capture-review.md), its 88/95 logs and submitted checkpoint `7f07d074` are not rewritten by this work. The prior 3116 candidate's restoration failure and unexecuted keep stage remain as recorded; they do not prove the missing event's actual field/kind/range. The root agent's separately frozen 7f diagnostic candidate must have its own source/package identity and trace. It cannot be attributed to the modified Index above.

As of this source freeze, this v24 repair has **NOT_RUN** product HAP/installation/device restore/keep acceptance. Exact field/kind/original-range traces are still required to link an actual device failure to this corrected path. Model PASS does not certify restored-editor close, raw replay/recovery, lifecycle drain or protected-production authority.
