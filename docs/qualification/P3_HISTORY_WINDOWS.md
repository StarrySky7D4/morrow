# Windows draft history and retirement, 2026-10-23

Result: **NATIVE_WINDOWS_FORMAL_DRAFT_LIFECYCLE_SCOPED_PASS** for six independent actual Windows clients. Editable restart and full P3 remain open; source/release are not accepted.

- Frozen whole-source digest: `f5ba873268fe7a9e97114cfadda073fc079e7a5205087ef325605722b8ce3f80`; base commit `f8ef33ec091a82d68f3c2745dde950f8762d8878`. The before/after source manifests are identical. Documentation written after this run is newer than the frozen snapshot.
- Evidence: `D:\build_cache\_audit\windows-restriction-lineage-20261022-019\p3-history-windows-003/result.json`; binding: `D:\build_cache\_audit\windows-restriction-lineage-20261022-019/p3-history-native-binding.json`.
- All clients used the same original host, workbench package and theme bytes. Each owned visible native window matched its original PID; the installed Dart kernel matched the assembled kernel. Native executable/engine/kernel hashes are retained per phase.
- Test input was injected through Flutter's test framework. This is actual Windows engine/host execution, not physical keyboard/clipboard/IME or a rendered-pixel inspection.

| Phase | Actual PID | Seconds | Result |
|---|---:|---:|---|
| seed | 21752 | 23.203 | PASS |
| recover | 23424 | 24.25 | PASS |
| retire | 10548 | 25.047 | PASS |
| archive | 24472 | 22.844 | PASS |
| history | 20308 | 22.735 | PASS |
| live-retire | 20500 | 31.25 | PASS |

The formal blank-title editor preserves all raw field metadata and exact pinned attachments/capture evidence. Restart review/export reads original durable pins without reopening source aliases or rerunning plugins. Explicit retirement retains original identities and receipts. The live-retirement phase completes 18 original-owner retirements with capture/import closure and no business operation.

The history UI displays localized field labels and bounded title/preview text. It can inspect/export retired records and explicitly archive an exact generation/request hash. The original journal, audited operation history, parent-child lineage and retirement markers remain unchanged. Repeated archive only returns the original receipt and never releases admission twice. Old identities and saved business operations cannot be revived by archive.

Admission limits remain 16 active and 256 unarchived draft identities; archived identities have a separate 4096-record/64 MiB current snapshot plus marker budget. This is not a bound on all database pages or historical transactions. Import/decision/proposal long-term maintenance and capacity remain separate unfinished work.

Archive requires actual original import reconcile/prune and closure of the original capture scope. The public import phase projects Retired for an inactive main draft even when its Pending/Ready entry still holds quota or bytes; it is not physical retirement proof. An initial test caught this distinction and remains retained in `p3-history-002`. Corrected actual host tests passed 22/22, including Pending/Ready retained bytes before and after restart, capture-scope refusal, 256-identity capacity and duplicate archive. Handoff tests passed 11/11, including failed child/parent writes and inactive/archived lineage. Codec tests passed 24/24; analysis and generated binding/localization checks passed.

Native attempt001 failed before application launch at the immutable same-version theme guard. Explicit retained-package pins now preserve existing theme/workbench bytes while compiling new guest sources separately. Retained guest bytes are not certified as outputs of the current guest source. Attempt002 built but could not start its engine because CMake copied a D-cache junction as an invalid NT-path symlink. The install helper now resolves the root junction at installation and copies real asset contents within the owned bundle. Synthetic install/reinstall and this complete native run verify the repair; both original failures remain preserved.

Remaining: fresh-authority editable restart, the complete S1/S2/reply-loss/fault/UI-change matrix, and import/decision/proposal maintenance. P2 full refusal capacity and Windows SDK26/G04/FREEZE remain open. VM count remains four. No historical Unknown owner was contacted or replayed, and no push or release was performed.
