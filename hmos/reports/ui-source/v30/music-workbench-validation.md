# v30 音乐协调与实际源组合验证

Date: 2026-10-09. Scope: `MusicWorkbench` actual product composition and stored real Native DTOs. This file does not qualify a new device run, AVPlayer codec, native Store execution or complete Windows/HMOS parity.

`music-workbench-final-result.json` records **65/65 PASS**, exit 0, **11,586.324 ms**, and **19 actually read repository inputs**, all with identical byte lengths and SHA256 before/after the run. The three suites are 37 coordinator composition checks, 24 MusicLibrary checks and 4 stored Native DTO checks. Log SHA256: `63968bcd7d474b7969819116ff6f8f897596d822a8356c2f60508dddaed318ee`.

The composition executes actual `MusicWorkbench`, `MusicLibrary`, `MusicFiles`, `MusicPlayback`, `PlatformMusicPlayer`, `Workbench.sendRaw/importFd/exportFd` admission queue and `EditorInputHash`. Its OHOS SDK file/picker/crypto and Native/AVPlayer providers are controlled seams. It also consumes the unchanged complete Store-produced fixture `v29/music-store-fixture.json`, SHA256 `da6d8df4f50f59555a4064ab29c9b302ce29b85a843deb03f2f5c9eb0d83b9b2`, proving actual DTO acceptance without generating a replacement fixture. The test source tracer records actual bytes read, repeated-read identity and exact post-run identity; it does not infer inputs from imports alone.

## Product identity

| File | Bytes | SHA256 |
| --- | ---: | --- |
| `entry/src/main/ets/model/MusicWorkbench.ets` | 27,626 | `30b643cd380a5f4a54297c17406dce8bbd55373a9aa09cdcf6c9255348152974` |
| `entry/src/main/ets/model/MusicLibrary.ets` | 45,655 | `c293488d9912f6efde7689e4c256bc0cfccfe153e82a822619ecbb291037c41f` |
| `tool/music-workbench-model.test.cjs` | 29,510 | `fd5532b03fc4f0795f62ed8ba588a0260695702df895f4a631da76b57103023c` |
| `tool/music-workbench-test-harness.cjs` | 14,442 | `52dd117cba4be5960ff09b13e6f0cc8e948b572c0a48708178d9d8f7db6ec2b6` |

The only MusicLibrary change here imports `MusicPlaybackIdentity` and makes `MusicLibraryTrackIdentity` derive from it. All original fields/defaults, serialization and library behavior remain the same. This provides nominal type compatibility required by the actual ArkTS compiler. Library's own 28 checks are included in the final run above.

## Public API and behavior

`new MusicWorkbench(context, hooks)` uses function-property hooks `owned`, `foreground`, `owner`, `changed` and `beforePlay`. Its public methods are `view`, `load`, `action`, `reorder`, `background`, `foreground`, `pauseForMedia` and `dispose`.

`load` is read only: it restores the actual selected ID and complete spool metadata without writing selected, opening a player or autoplaying. Initial idle controls permit import and the first explicit play/selection. New transports and writes require a complete qualified foreground library. An already playing exact owned player can always be explicitly paused independently of library/lyrics/policy Unknown.

The complete UI action contract is `add/select/toggle/previous/next/seek/remove/move/lyrics_toggle/lyrics_import/lyrics_view/lyrics_close/read_retry/mutation_retry/mutation_discard_known/begin_retry/import_inspect/import_continue/import_reconcile/import_finish/spool_resume/spool_start/retry_io/cleanup_cache/retry_close`. Lyrics toggle applies an explicit 0/1 target; repeating the same target creates no new mutation. A row not present in the actual ordered playlist is rejected before a negative policy index can select a different track.

User selection and EOF navigation use actual Library admission before actual Files export. A selected existing stable track creates no redundant CAS; reorder and removal of another track preserve its FD/player/position. Rust policy replies are proposals, while player state comes only from actual adapter acknowledgement. Seek passes clamped milliseconds to the player. `pauseForMedia` requires an actual paused/prepared/completed/idle state and rejects failed, loading, closing, cleanup failure or an unacknowledged pause; it never claims that a failed pause stopped audio.

Removing the current track closes its old resource before the explicitly authorized replacement. If remove was committed but its reply or refresh was lost, a later qualified fixed mutation/read retry checks the old stable source. A retired or missing source is closed without automatically opening the replacement. Release failure preserves the original resource and explicit close retry.

Import selection allows up to 20 grants. Each whole-file preparation is bound to a unique fixed request; any Unknown stops the batch and preserves the captured spool/original plan. Metadata and the exact FD request are fsynced by Files before FD dispatch. Only a matching qualified Ready receipt admits spool release and plan finish. A request-bearing recovered spool is inspected using its exact saved literal. A blank sidecar displays all actual Native Pending entries whose complete request metadata match; it requires an explicit candidate choice and never guesses an ID. Only an unissued cache with no matching Native Pending can explicitly start a new request.

Partly deleted Ready sources are not displayed as complete import sources. The process retains the original registered cleanup handle and qualified Ready plan through `retry_io/recover`; explicit finish retries the original cleanup and does not reimport FD bytes. Fixed unissued begin retry also keeps the same operation/CAS bytes after a lost begin response; it is explicit, never automatic.

Local lyrics are read in full. A rejected 60,000-byte UTF8 lyric retains the full original text instead of cropping to Native's 49,152-byte limit. Unsubmitted text retains its original track ID/title, and another viewer cannot relabel it. Cached full lyrics remain viewable under Unknown. Actual Rust parsed lines are cached once per qualified read; player position updates select the footer line without a Native Store read per callback. Footer behavior follows the actual Flutter source for pre-first timestamp (`♪ title`), an empty timed line (`♪`), untimed first line (` · 无时间轴`) and known empty lyrics (` · 暂无歌词`). An unqualified nonempty lyric read is shown as pending, not asserted empty.

## Remaining boundaries and preserved failures

The begin request is not separately persisted before Native registration. An app crash between preparation/registration/FD-sidecar save relies on exact actual Native Pending matching. A crash before Native registration has no durable fixed begin intent; this gap remains OPEN and must not be advertised as complete pre-begin recovery.

Cleanup ownership for a partly released source is process local. Unknown leftovers after process restart remain diagnostic data and are not blindly deleted or admitted as complete sources. Unsaved lyric text is held by the current page coordinator; persistence across app restart is not qualified. Standard audio is the current scope; encrypted/online import, cover/tag extraction and full format/codec coverage remain outside these checks.

`music-workbench-source-integration-a1-*` preserves a 63/64 failure: its older assertion expected a release-started source to remain in the complete-spool list. The new Files behavior correctly removes it from that list while retaining the exact cleanup owner. `source-integration-a2-*` changes that assertion and adds the full partial metadata cleanup/recover/finish test, yielding 65/65 with exact inputs. `final-*` additionally asserts original unsaved track ID/title and is the current evidence. These runs do not replace Root's separately owned full SDK/model/package/device acceptance reports.
