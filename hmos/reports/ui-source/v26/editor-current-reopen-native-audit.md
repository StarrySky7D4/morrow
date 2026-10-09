# v26 current V2 and immutable draft history — native scope

2026-10-09. This worker implemented the native foundation for current-revision V2 body editing and read-only draft operation history. Seven native source/test inputs are frozen in `editor-handoff-native-inputs.json`; the post-default-suite capture `editor-handoff-native-inputs-after.json` is byte identical. This is a branch source checkpoint, not full Flutter/HMOS acceptance.

## Actual contracts

`current_v2` is an independent typed mode in the existing schema-1 editor Submission, save, inspect, prepare/issue journal and durable transport. It has marker mode byte 4. Its business action is `edit`, `todos` is empty, and `continuation` is null. The publication's complete todos text must also be empty under the existing full-value equality and no-composition gates. Title, description, hypothesis, conclusion and selected attachments may change. Category/stage must equal the complete actual source; their original protobuf bytes are preserved, as are favorite, current TaskIds, task text/completion/order, retired IDs, migration metadata and unrelated task/property/outer extensions. It does not replace current V2 tasks with LF labels.

Authority comes from the actual complete V2 Card source and exact raw publication. `prepare_intent` compares that source with the current Store Card; the one Core versioned business transaction enforces the full source CAS again after issue. Inherited or copied old create/continuation markers grant no current authority. A change after issue rejects without rebasing. The previous `edit` and `continued_todos` contracts are not relaxed; mode 4 cannot become an owned LF continuation baseline. Exact original wire and operation history remain required on retry/inspection.

`draft_read_history` uses existing request fields:

```json
{"action":"draft_read_history","id":"card-id","draft_id":"draft-id","draft_operation":"exact-operation","generation":"2"}
```

It accepts a positive canonical u64 generation, validates identities, rejects other non-default route fields, reads the actual immutable operation Slot, and compares its exact historical generation. It returns one complete existing DraftView with `repeated:true`, true current-journal `current_generation/current_active`, `effect:"not_committed"` and empty `receipt_revision`. The complete reply is bounded to the unchanged 512 KiB limit. Read failure neither deletes raw data nor becomes a write grant. Callers must separately validate their fixed plan proof, source, hash and selected pin inventory before adoption. Reading retired history grants no export, ordinary discard, write or reactivation authority.

## Closed owned-todos assessment

An unchanged closed successful create/continued context needs no new native ownership contract. Read the original immutable intent and registered inspection wire; strictly qualify its actual original command/result. Require `live_matches:true` plus equality of actual current Card ID, revision and complete source with the accepted historical result. A fresh ordinary current-source raw publication can then submit existing `continued_todos` with the exact root request and baseline receipt. The actual closed-saved-exact Store test reopens the database, proves this equality, makes a new publication and continuation, preserves surviving task IDs, and keeps the old parent inactive. A subsequent original inspection remains historical and reports the new live revision separately.

Favorite/category/TaskId changes break this equality. They cannot be repaired by a marker or old baseline. The new task-preserving current mode provides body editing at the actual new revision instead; owned LF restoration and full task-edit equivalence are not thereby proven.

## Final exact-source checks

| Evidence | Result and scope |
| --- | --- |
| `rust-tests-final.log` | Complete default command exit 0: 187 library PASS /0 FAIL /16 conditional ignored /89.09s; attachment binary 3 PASS /5.65s; self-check 0 and rustdoc 0 PASS. Default checks completed before the feature build, avoiding the prior shared-target rustdoc conflict. |
| `current-v2-dto-exact-final.log` | Exact current-v2 actual Store exporter 1 PASS /0.28s. The final sample changes title and body after favorite/category/rename/completion. |
| `reopen-history-dto-exact-final.log` | Exact immutable-history actual Store exporter 1 PASS /0.39s. Includes actual S1 publication, different S2 parent, first and advanced current child, fixed retirement, pre/closed native views and original strict inspection. |
| `current-v2-crash-final.log` | 1 PASS /7 actual subprocess Core crash boundaries /1.78s /exit 0, with exact fixed retry and surviving original tasks after reopen. |

The ten new default groups cover current revision after metadata/rename/completion/reorder/removal with surviving task IDs, independent source CAS after intent issue and stale owned baseline refusal, exact schema/metadata/publication rejection, complete task/asset/outer unknown bytes plus actual attachment export, event-capacity Unknown and fixed original retry after reopen, immutable S2 history with true active/retired/advanced-child current flags, strict history shape/generation/identity refusal, oversized complete history reply refusal without deleting raw, actual retired-parent pin metadata without export authority, and unchanged closed-owned context continuation. The fault test uses explicit host-only `morrow-core/fault-injection`; it is not an OHOS or power-loss guarantee. The old handoff transaction implementation is unchanged, so v25's 48 vectors are historical and were not rerun or counted as new v26 fault proof.

## Exact actual Store DTO artifacts

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `editor-current-v2-store-fixture.json` | 38,806 | `50F9037B3A235CFF72E1423BF4CE5F30CF6B655C243373FA7F6B273BB926B301` |
| `editor-reopen-history-store-fixture.json` | 75,959 | `4191624D7B2894ABC17A3874EA3005249B450E6EE270B9C880EA2F04530415DB` |

The current fixture carries actual current source generation 5, immutable raw publication, exact mode-4 request/proof, the five issued native parts and registered outer save, actual generation-6 save/reopened inspect, and the original create's `live_matches:false` inspection. The history fixture carries original publication S1 and full fixed parent S2 separately, actual first/current child replies, active/retired parent history, exact retirement history, original first child with advanced current flags, current child read, actual original inspect, seven native parts before/after close, and unchanged fixed action literals. Missing optional native `intent_next_after` on draft actions remains absent; the fixture does not manufacture cursor fields.

## Historical evidence retained separately

`current-v2-focused-stage1.log` records test API compile errors; stage2 records a negative helper trying to publish stale source before its intended business rejection. These were corrected without weakening production gates. Other focused logs precede the final explicit-body-change test helper and are not the final frozen suite. `current-v2-dto-final.log` is a historical failed wide-filter command: the new exporter passed, but it accidentally also selected the old exporter without its explicit output environment. The exact final exporters above are the scoped success proof. The first successful full command and first unchanged-body fixture are retained as `rust-tests-before-explicit-body-change.log`, `current-v2-dto-exact-before-body-change.log` and `editor-current-v2-store-fixture-before-body-change.json`; they do not qualify the current test hash.

## Remaining OPEN

Root owns final ABI/SDK/package adoption and branch publication. This worker changed no ETS or Index and performed no Git mutation, NDK/SDK build, installation or device action. UI integration, body-kind handling, complete owned-todos reopen/current-child recovery flow, actual IME lease/callback draining, rendered UI, signed HAP, ARM64 device execution, HUKS/protected storage/audit, full task editing and Flutter/Windows product acceptance remain OPEN. Host Store/model success does not close these boundaries. All old wires, Unknown states, quota/pin rules and separate transaction semantics remain in place.
