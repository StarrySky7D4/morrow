# v29 MusicLibrary foundation audit — frozen 2026-10-09

This checkpoint adds an independent music library coordinator and strict native music DTOs. It does **not** connect the Index music page, demonstrate an audible decoded song, or qualify any device/codec. Root owns publication, SDK/native package checks and the later page integration. No Index, MusicFiles, MusicPlayback, platform player, native, old tests, Git or device state was modified by this worker.

## Frozen implementation and source requirements

`hmos/entry/src/main/ets/model/MusicLibrary.ets` is 45,568 bytes, SHA256 `7317B45A6E853FF78BD0FE44D39B3B8957DA3DAB4CA16E014903AAC70095DF36`. It has no Card/Draft, file path, FD, AVPlayer or platform I/O dependency. Music source URIs are opaque `file:///morrow-music/<track_id>` identities; they are never opened by this model.

The actual Flutter comparison is `build/win-cloud-20261005/lib/music/`, rather than an assumed mock panel:

| Source evidence | Required behavior and checkpoint boundary |
|---|---|
| `audio_formats.dart:1`, `:73` | Nineteen standard audio extensions and 150 MiB audio bound are represented in import request qualification. Listing an extension is not platform decoder support. Protected/encrypted container imports remain OPEN. |
| `music_controller.dart:286`, `:308` | Complete target-specific lyrics, original source metadata, durable track ordering, selected index and lyric visibility. Library reads actual metadata and persists lyrics/visibility through independent CAS operations. Covers, embedded metadata, online lyrics and automatic sidecar matching are not delivered here. |
| `music_controller.dart:474`, `:568` | Selection and wrapped next/previous refer to actual stable tracks. Library exports verified Ready identities and explicit durable select admission; platform playback belongs to the separately owned player. Local `adjacent` is an identity helper. Actual native `policy` is a proposal, not AVPlayer success. |
| `music_controller.dart:570` | Reorder preserves the selected track and source instead of reopening transport. Library holds exact complete Ready order and applies native CAS; source identity is track/import operation/length/hash, while current library revision is acquire CAS. |
| `music_controller.dart:596` | Removal freezes target/order while awaiting policy and preserves the other selected track. Library freezes the original operation/CAS and validates target and actual receipt; disposal or owner changes cannot install a late reply. Root still has to connect current-track removal to player disposal/selection. |
| `music_panel.dart:61`, `:133`, `:209`, `:507` | Real multi-file import, local lyrics picker, full lyrics display and stable row reordering are UI obligations. These UI flows are not connected in this checkpoint. Flutter picker permits local lyrics files up to 1 MiB; this native foundation rejects complete UTF-8 lyrics over 49,152 bytes or parsed lyrics over 1,024 lines rather than truncating them. |

## Actual native protocol and safeguards

`parseNativeMusicReply` at line 214 requires the complete native schema, exact typed keys, canonical decimal string numbers and lowercase digest values. It rejects missing `music`, malformed Unicode, foreign source URIs, contradictory Ready retention, duplicate order, oversized replies, and fake read commits. Successful ordinary reads have top effect `not_committed` and an empty receipt revision. Historical operation revision is distinct from actual current library revision.

`load`/`readPages` at lines 317/332 read every entry lexically in pages of 16, including Pending and retained Retired entries. Headers must retain one revision/order/selection/visibility. Only a complete Ready record set matching the actual playlist order is installed. A rejected final page is validated as a candidate and cannot poison the partial aggregate; explicit retry preserves the original current-page literal. An actual absent library is the native successful revision `0` empty DTO; a missing/failed DTO is never converted into that result.

`identity`/`adjacent` at lines 363/370 qualify actual Ready records, import operation, retained bytes, length and digest. Pending or Retired records never grant playback. A metadata-only library revision change does not replace stable blob identity. `claim`/`guard` at lines 294/299 freeze owner epoch per attempt; late owner/epoch/dispose results are rejected while the fixed request remains available for explicit reconciliation.

`importAction` at line 499 holds exact begin, inspect, reconcile and native-produced FD import literals. A committed Pending receipt proves a durable plan, not Ready bytes. A local recovered original literal first requires native inspection. Retained Pending can become qualified Ready through explicit native reconcile without reading a replacement file. FD import is only an explicit confirmed-Pending action through the supplied hook. Complete Ready qualification checks original literal and digest, request tuple, actual retained bytes, and the native internal Ready receipt operation. Unknown retains the plan and never releases or automatically reimports it. `finishImport` is allowed only after qualified Ready.

`mutate` at line 555 freezes operation bytes and CAS for select/remove/reorder/lyrics/visibility. Every dispatch begins with effect Unknown; a prior known no-write failure cannot authorize discarding a later attempt whose receipt is lost. Only a new valid known no-write error enables explicit discard. Confirmed writes refresh the library through readonly requests. No auto write replay is used. `readLyrics` verifies complete source byte length, original target identity, parsed lines and active position. `playbackPolicy` consumes the actual Rust policy response and checks its action/state contract; it does not perform transport.

Intended host wiring is `send = workbench.sendRaw` using the existing host admission tail; `importFile(originalLiteral, request)` binds the actual prepared file through `MusicFiles.importPrepared` and the same tail's `importFd`. The model never owns a second native queue, fabricated file receipt or path. Root supplies an owner epoch hook and the actual Files/Playback lifecycle; these are interfaces, not an assertion of page integration.

## Independent review and validation

Session peer independently reviewed the source. It found and this worker corrected two real retry defects: stale known no-write effect surviving a later Unknown write attempt, and failed final page being appended before complete aggregate validation. Actual-source regressions prove both exact fixed retry paths. The peer's second readonly review found no additional blocker in this bounded Library review. It did not run this worker's tests or modify files.

A narrow readonly native review covered `music_bridge.rs`: fixed Pending history and exact request qualification (`original`, line 255), actual retained owner/blob identity (line 266), missing bytes returning readonly inspection instead of FD replay (`ready`, line 377), Retired preventing reactivation, and independent library routing/privacy (`lib.rs:295`, `:376`, music envelope guards at `music_bridge.rs:427`). No native source/build was changed or run by this worker.

Final command:

```powershell
& 'C:\Program Files\Huawei\DevEco Studio\tools\node\node.exe' hmos/tool/run-music-library.cjs
```

Result: exit 0, **28/28 tests PASS**, 0 failures/skips/cancellations, **512.5916 ms**, across two test files. There are 11 explicit repository inputs with equal before/after bytes and hashes; 10 were observed as actual subprocess reads/modules, with 0 missing or unexpected reads. The remaining input is the runner itself. The existing actual ETS loader also eagerly loads EditorFieldPolicy and EditorDraft; those are listed as observed loader inputs, not a new music business dependency. SDK TypeScript transpilation in a VM is not ArkTS SDK compilation.

Final evidence:

- `music-library-final.log` is the final test output.
- `music-library-result.json` contains exact source identities before/after, tool command, counts, observed reads and log hash.
- `music-library-source-reads.jsonl` contains observed per-process repository reads.
- `music-library-worker-freeze.json` lists this worker's exact submit files and hashes. Native fixtures are separately owned inputs.

The unchanged actual isolated native Store fixture `music-store-fixture.json` is 65,127 bytes, SHA256 `DA6D8DF4F50F59555A4064AB29C9B302CE29B85A843DEB03F2F5C9EB0D83B9B2`. Tests consume 22 original full music Reply DTOs without adding fields. Coordinator tests consume actual 16+2 pages (18 records: 2 Ready, 15 Pending, 1 retained Retired), actual current track, retained Pending inspection/reconciliation, and actual seek/next/restore proposals. Those fixture bytes are synthetic data from actual native Store operations; they are not a decoded music, codec, protected container, audible or device proof. Stage1 original fixture `B60E2AE4…` remains separately pinned and unchanged.

Controlled receiver tests cover ownership, exact request timing, fixed literal retry, CAS rejection, malformed replies, import qualification, full UTF-8 lyrics, stable selection, order/removal and budgets. This receiver is explicitly not Engine/Store/crash/policy implementation. Native Store/crash evidence is owned by the native worker, not inferred from these Node cases.

Stages are preserved: stage1 18/22 (test helper/cross-realm mistakes), stage2 21/22 (test used Seek against an actual Toggle fixture), stage3 22/22 (initial matching contract), stage4 24/24 (two retry regressions), stage5 28/28 (expanded actual final fixture). They are historical evidence, not the final frozen-source run. Stage2 was corrected to the actual Toggle request and strict action response qualification; no native reply was altered to create success.

## Remaining limits

Index music UI, real file picker/recovery integration, actual platform audible playback, device resume/background behavior and rendered Flutter UI parity remain OPEN. Root's SDK and aggregate tests have separate evidence and are not claimed here. This checkpoint has no merge, release or device acceptance claim.

The current native foundation has 512 total durable entries **including Pending/Retired**, 512 MiB aggregate declared owned audio, 150 MiB per audio, complete reply limit 512 KiB, and lyrics 49,152 UTF-8 bytes/1,024 parsed lines. Retired source pins remain; deletion is not blob GC, and GC needs a separate durable release design. This is a bounded first native library, not Flutter's unbounded acceptance. Root replaced the earlier proposed music 64 MiB host restriction with an independent 150 MiB music spool; attachment limits do not authorize a smaller music bound. Source lengths/hashes cannot qualify audio decoding.
