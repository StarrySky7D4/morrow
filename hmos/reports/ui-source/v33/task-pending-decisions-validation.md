# v33 pending task explicit decision

Status: **IMPLEMENTED / ACTUAL_SOURCE_CONTROLLED_HOST_PASS**. Full SDK and task device acceptance are **NOT_RUN by this agent**. Branch baseline was `codex/ArkTsUI`, `ada95e0f119b3d3c3273b8e5bf6da031d3b10c10`; no Git operation, Native change, SDK build or device operation was performed by this task.

The frozen task Index now presents help_outline for completion `2` and two explicit choices, **确认已完成 / 确认未完成**, in both the detail view and existing-card editor. The pending explanation and both choices use the actual Flutter source phrases already present in all nine locales. Ordinary `0/1` tasks retain a Checkbox using its actual bool callback. Pending tasks have no ordinary checkbox or inferred direction. Both contexts also have menus with both pending directions; the editor retains rename, move up/down and remove. The pending choices use a wrapping Flex below the task so the two directions remain available at narrow widths; actual width/rendering remains device validation work.

The existing MorrowIcons font renders the Flutter `Icons.help_outline` codepoint `0xe30b`. The font agent separately checked the actual local Flutter source and all three Unicode cmap subtables of the unchanged OTF. This task proof executes event callbacks and methods; it does not render that glyph.

## State invariants actually implemented

1. A fresh decision belongs to the original rendered card snapshot (`id`, source bytes, revision), actual TaskId, original task text/completion, same detail route or editor view lease, foreground page and input epoch. There must be one actual target card, unique nonempty task IDs and a complete known `v2` DTO. Completion `2` requires an explicit bool; other unknown completion states, legacy cards and invalid bool values cannot dispatch. No name/count/index inference or local optimistic completion is used.
2. Buttons, menu callbacks and the helper share the actual write blockers. Existing pending wire, business uncertainty, field/file/paste activity, dirty body or rename, raw fork, retiring/restoring/retired/conflicted/disposed/paused draft and missing original input prevent a new task decision. The editor's complete original raw input passes actual `flushDraft` before the actual Workbench queue admits the command; the original snapshot/owner/epoch and write gate are checked again after that await.
3. Once admitted, the existing `pending` literal holds the same actual `task_toggle` command: original card/source, TaskId, bool, operation and time. Native exceptions, ambiguous effect, malformed success/current DTO or invalid receipt retain both that literal and the original task witness. Repeated choices cannot replace the pending operation. Explicit retry sends the exact original bytes.
4. Only task_toggle captures a per-send foreground response boundary (selected/detail/editor route, view lease, actual draft, attachment identity and epoch). A late result after a field/lifecycle/ownership change neither installs its cards nor clears the original pending wire. Explicit retry can establish a fresh response boundary for that same original request. An unrelated selected/detail card cannot become the old request's new target. A neutral foreground page can explicitly reconcile the original target; it does not mint a new command.
5. A task success needs strict `ok=true`, empty error, `effect=committed`, a positive canonical u64 receipt, complete current card/task DTOs and one target card. The target source must have advanced from the original source, and receipt must advance the original revision. If receipt equals current revision, the exact original TaskId must have the chosen state while other TaskIds/text/order/completion remain unchanged. If receipt is historical and less than the current revision, the Native latest view is accepted, including later decisions, task removal or card deletion. Receipt greater than current revision is rejected. Revisions use canonical decimal length/lexical comparison without JS Number conversion.
6. A failed reply releases the task request only with `ok=false`, a nonempty error, `effect=not_committed` and empty receipt. It leaves task completion and original raw text intact. Other replies retain the original uncertainty. Native/Core continues to validate the full original operation; there is no new Core action or parallel task store.
7. The editor's remove-menu confirmation also binds its original snapshot, task, owner and epoch through the actual confirmation callback. Existing rename/reorder/remove functionality remains wired to the original methods.

## Actual execution evidence

[task-decisions-a1-result.json](task-decisions-a1-result.json): **126/126 PASS**, exit 0, fail/cancelled/skipped/todo 0. This comprises 18 task decision tests and 108 existing business, history/recovery, field, todo and raw-fork tests across six files. The qualified run occurred `2026-10-09T04:34:53.268Z`–`2026-10-09T04:35:34.522Z`.

The harness extracts actual private Index methods with SDK TypeScript AST boundaries and executes the actual ETS models and Workbench queue. New task tests also extract and execute the actual normal Checkbox callback and both actual pending-button callbacks from the product Builder source, and execute actual generated menu actions. This is callback/method execution, not ArkUI component rendering or framework event delivery. Native task/CAS/history replies, timers and platform lifecycle/confirmation delivery are controlled seams.

Cases include both true/false directions in both contexts; same-name different TaskIds decided independently; normal 0/1 checkbox directions; stale card/source/revision/text/TaskId/route/owner/epoch; write blockers; retirement/restoration/disposal/source/input changes during real raw flush; page/foreground/editor/draft/attachment/selection/raw-input changes during actual Workbench await; replacement-card versus neutral explicit retry; actual rename/reorder/remove callbacks and delayed remove confirmation; Unknown before and after controlled commit with literal retry; malformed receipts/DTOs; no-commit input retention; historical current views with removed/deleted tasks; canonical u64 values above JS safe integer and near u64 MAX.

The runner froze **52 repository inputs**, observed the exact bytes of **51 actual repository reads**, and qualified all of them. All source bytes, model filenames and the actual Node/TypeScript runtime remained identical before/after; missing reads, unqualified reads and read drift are empty. All models are included because the actual shared/recovery harness preloads their source bytes. The runner itself is the one parent input not loaded by test workers.

- [task-decisions-a1-inputs-before.json](task-decisions-a1-inputs-before.json): exact before identities and runtime.
- [task-decisions-a1-inputs-after.json](task-decisions-a1-inputs-after.json): exact after identities and actual observed reads.
- [task-decisions-a1-tests.log](task-decisions-a1-tests.log): actual TAP stdout/stderr, SHA256 `5A0450584E4BD64BA88BEF0B33FC71DEA453F7A3EC5299F09DCF3BCEF1078D13`.
- [task-decisions-a1-trace.jsonl](task-decisions-a1-trace.jsonl): actual file reads, SHA256 `A77E79A646DCE19154E8EDC1DAB942B94CBDF37E89A7FFE05267B8EFD449EADD`.
- [task-decisions-exploratory-runs.json](task-decisions-exploratory-runs.json): separately retained exploratory failures and repairs; those direct runs had no source/runtime manifests and do not provide frozen qualification.

Runtime: actual `C:\Program Files\Huawei\DevEco Studio\tools\node\node.exe`, v24.14.1; bundled TypeScript 4.9.5-r4. Actual executable/compiler/package bytes are in both manifests. The immutable runner refuses an existing evidence label. Repeat on future integrated sources with a new label.

## Task freeze and reference identities

| File | Bytes | SHA256 |
| --- | ---: | --- |
| Index.ets before task implementation | 338531 | 0930A469A4DD92E98DB914E40FB464763A7FF0A0C4A5678E325F72A62684D773 |
| Index.ets task freeze | 350258 | 619592C0E8483199EA1CA92AB48FA6EED9EFA25BE37051E368060E6410786664 |
| index-business-test-harness.cjs | 35438 | F60692A8A53BEA4875CCC698DF3526A9E3D726E3D7B69F3627C2102765E1863F |
| index-task-decision-integration.test.cjs | 20063 | F17FFDEF54509D0DB36E220C02AF2D3B4D24B3279A51F65FAEEDFD23FD3235E2 |
| index-task-source-trace.cjs | 1558 | E791C8EB7A71ECD449916F8A2540C10D228C308C425E18780CC29882FD127268 |
| v33/run-task-decisions.cjs | 6191 | 577E69DB9206EADDCBD4DB695A75CE14449EDAEEF6F74090E45BB0D12E9B3104 |
| Workbench.ets unchanged | 2502 | 71582E668921776554E48F0469E1F6B323728DB076082FD22ED5AE7E4BB0B423 |
| Native lib.rs readonly reference | 64423 | 1CC8E44723FC69965777C0C502EBFBB88C4561BF89CD163D51E684AB1E2CA938 |
| shared tasks_v2.rs readonly reference | 16389 | 43849B828B2C6F4605EA8FE677CBB4E95A64FCFB68EEC4639346556E9947C70D |
| win-cloud/versioned_task_panel.dart readonly reference | 21175 | F97EC18EC42B4F6F33963EB0EB5DA15A137F2588F6E4915C7DB99F7E6D40C9A3 |
| materialicons.otf unchanged readonly reference | 1645184 | D9865B671A09D683D13A863089D8825E0F61A37696CE5D7D448BC8023AA62453 |

Index was handed back to Root after this run. Later appearance integration changes its full hash; this evidence must not qualify those later bytes. Pure Rim/Music/material/appearance preference functions and Native Rust were outside this task's changes. Nine-locale lookup has separate font-agent evidence; its earlier whole-Index manifest is not the final integrated Index proof.

## Remaining boundaries

Root still owns the full SDK/build and immutable HAP/device validation for the final integrated v33. A real pending-TaskId fixture, both visible direction controls and actual clicks in detail/editor, narrow-width wrapping, disabled/Unknown rendering, real foreground return and the new help glyph require actual device evidence. Native Store execution and crash/restart survival of generic task pending state are not established here. The implementation deliberately retains same-process original uncertainty; it does not introduce a durable task intent journal or claim full Windows/HMOS parity. Root's v32/dev22 style observations do not qualify the new v33 task functionality.
