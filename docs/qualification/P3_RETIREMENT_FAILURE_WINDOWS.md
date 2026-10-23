# Formal parent retirement failure and restart, 2026-10-23

Three independent native Windows clients passed actual parent-retirement transaction failure, double-active restart recovery and next-process verification. Full P3 remains open.

Frozen source: `c12d29f3185666a620de6fba4fb862c926663e22ed7e3f0bedd51e9d08dfe666`; commit `f8ef33ec091a82d68f3c2745dde950f8762d8878`. Source manifests match. Evidence: `D:\build_cache\_audit\windows-restriction-lineage-20261022-019\p3-retirement-windows-001/result.json`; binding: `D:\build_cache\_audit\windows-restriction-lineage-20261022-019/p3-retirement-native-binding.json`.

| Phase | Original PID | Seconds |
|---|---:|---:|
| fail | 23940 | 24.625 |
| recover | 7272 | 25.922 |
| verify | 8020 | 23.594 |

The first formal Save commits S1 once while later S2 remains in the editor. An externally installed SQLite trigger rejects UPDATE only for that exact parent journal after the child is durable. The actual typed retirement failure reaches the UI, both parent and child remain active, both export their exact original attachment bytes and normal prepare-close/host-close refuse. The test then deliberately closes its original transport and observes that original host's exit0. This injected disconnection is not normal workspace close, retirement acceptance or a successful business replay.

The next client removes only the exact original test trigger, verifies both immutable snapshots and uses the formal handoff menu to retire the parent with the original recorded retirement operation. Its saved-draft menu then resumes the child using a fresh editor, preserves raw selection/composition and attachment pins, and saves later input. A third client reads the exact saved request and exports the attachment. All three observe business revision1 and the original commit; recovery never resubmits business.

Framework input runs in actual Windows engine windows; physical input/rendered pixels remain unqualified. Guest packages retain separately pinned historical bytes. Remaining P3 cells include real disk/capacity exhaustion, all workspace/preferences/fault/asset-reselection paths and long-term maintenance. P2 and Windows SDK26/G04/FREEZE remain open. No historical Unknown owner or VM was contacted.
