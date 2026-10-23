# Formal Windows receipt-loss checks, 2026-10-23

Five actual Windows clients passed formal NewIdeaDialog Save and explicit original-operation inspection after withheld business, Prepare, Complete, child-update and parent-retirement replies. Full P3 remains open.

Frozen source: `670fb6c3c06e1eef040b2bb3efdf30488c59e954f7666c5704455b68dd962106`; commit `f8ef33ec091a82d68f3c2745dde950f8762d8878`. Before/after inventories match. Evidence: `D:\build_cache\_audit\windows-restriction-lineage-20261022-019\p3-fault-windows-001/result.json`; binding: `D:\build_cache\_audit\windows-restriction-lineage-20261022-019/p3-fault-native-binding.json`.

| Withheld original reply | Actual PID | Seconds |
|---|---:|---:|
| business | 19684 | 24.266 |
| prepare | 24924 | 24.578 |
| complete | 23828 | 24.329 |
| child | 23924 | 24.532 |
| retire | 12016 | 24.203 |

Each fault adapter first waits for the real host effect, then withholds its acknowledgement. It neither generates successful receipts nor changes host transactions. Input arriving during S1 and after the failure stays in the formal editor. Prepare-close and ordinary host-close are refused while the attempt is unresolved. The same Save control explicitly inspects the retained original. Every case submits business once, calls Prepare/Complete/Retire once and acknowledges once. The child-update case inspects its lost original and uses one distinct later operation for newer S2. The child contains the latest raw selection/composition and original pinned attachment; the retired parent's late write is rejected. A fresh actual host verifies exact durable child bytes, business revision/commit evidence and attachment contents after the selected source file was deleted.

These are framework-input tests in actual Windows windows. Physical input and rendered pixels are not certified. Retained guest package bytes remain separately pinned. This slice covers lost replies after successful writes; it does not prove failed retirement transactions, crash recovery from double-active state, real disk exhaustion, all UI/workspace changes or full maintenance capacity. P2 and Windows SDK gates remain open.
