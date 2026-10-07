# v24 — durable original business session foundation

Status: standalone implementation frozen; 30/30 actual-source model checks pass, including two complete actual Store fixture parser cases. This report concerns only the new standalone `EditorBusinessSession.ets` and its actual-source model harness. It does not change the published v21/v22/v23 evidence or qualify the product goal.

## Implemented contract

The session wraps the unchanged, strict `EditorBusinessCoordinator`. Fresh create/edit proposals still require complete exact publication values, no active composition, the real field-policy response, and a frozen owner/input epoch. `parentReady` is a mandatory caller hook: the exact parent/publication queue has been paused with no in-flight write, Unknown or conflict. Fresh prepare/issue/save checks this gate. The session neither resumes an old queue nor updates any live strings on an ACK. The caller must retain and independently capture newer S2 values.

The explicit sequence is:

1. `prepare(...)` freezes the actual business model; it sends nothing.
2. `prepareIntent()` durably requests the original complete Submission under one prepare operation. A valid metadata ACK establishes immutable intent proof, never business commitment.
3. `issueIntent()` sends only `{intent,expected_generation:'1'}`. The native transaction generates and retains both transport literals; no double escaped save/inspect payload is submitted in this call.
4. `loadTransport()` explicitly reads the `save` and `inspect` parts. Their complete envelopes must contain the identical original inner request, proof and expected revision. Empty, mixed, oversized or changed parts block dispatch.
5. `save()` invokes the actual business model. Its immutable original no-intent save string is mapped to the native persisted registered save literal and sent byte-for-byte. The model's original inspect string is likewise fixed to the returned native inspector literal. Equivalent JSON is not regenerated for transport.

Every missing/lost/malformed mutation ACK retains the original action and operation. `retryPrepare`, `retryIssue`, `retryCancel`, `retryRead`, `retrySave` and `retryInspect` are explicit; they preserve the relevant original wire. A first definitive no-write rejection can remain known; an uncertain retry cannot downgrade the original Unknown. A native read failure never causes a business send using an invented plan. A native business `committed` effect is retained as a conservative fact even if its full DTO then fails validation; this hint does **not** qualify a historical receipt or authorize consuming input. Only the actual unchanged business model validates and qualifies that receipt.

There is a distinct issued-before-first-send boundary. If issue is already durable but its ACK is pending while the same owner captures S2 (including changed selection/composition) or loses ownership, the valid issue ACK is retained and fresh save is blocked. The issued intent cannot be cancelled as prepared. The user can explicitly restore the same proof with five read-only parts and then call `retrySave()` to settle the immutable original S1 using its exact registered literal, even when the old `isExact`/`parentReady` hooks are false. No business request is sent during restoration, no S2 is consumed, and the old parent queue is not resumed. The future Index must provide an explicit original-request verification/continuation entry for this state; the fresh-save refusal is not a terminal replacement for that product flow.

`cancelPrepared` supports only `cancel_prepared`, with one fixed independent Close literal. It is refused locally after known issuance/commitment, and native generation-1 CAS remains authoritative. Issuance phase alone proves no business commit. Empty unissued save/inspect parts prove neither cancellation nor a business absence. No `saved_exact` close is implemented here.

## Restart and identity validation

`discover` is read-only, with a bounded page of at most 16 complete summaries, duplicate/card/cursor checks and domain identity verification. It does not claim a globally atomic multi-page snapshot. `restore(proof)` reads all five explicit parts: Submission, immutable publication, save, inspect, close. It requires one stable proof/card/business operation/expected revision/current phase across those reads, validates every full part, and restores the actual business coordinator as uncertain without sending any mutation. A failed restoration retains the immutable ref with the caller; explicit restoration of that same ref is a safe read-only retry.

The intent ID domain frames `[card_id,business_operation]`; the issue operation domain frames `[intent_id,prepare_operation]`. Each uses its exact NUL-terminated native v1 domain and UTF-8 lengths as little-endian u64. The first proof generation remains `'1'`, while current issued/closed state is generation `'2'`. All counters remain canonical decimal u64 strings, including values above 2^53 and the full maximum receipt revision. Current phase is never confused with an immutable historical requested phase. The issue identity is present in prepared, issued and closed views.

The original Submission SHA is checked over its literal complete UTF-8 bytes. Full publication scope/source, five TextValues, selection/composition, category/stage, ordered assets, complete pin metadata/aliases and relationships are retained. Current publication diagnostics may have advanced or retired since that immutable active publication; these current diagnostics are not substituted for its original tuple. Subsequent publication reads are compared against the complete frozen record. On initial restart, the complete native immutable publication and prepared protobuf digest are inputs from the trusted development producer. ETS cannot reconstruct the canonical intent protobuf digest from its JSON View and does not invent a protected proof from that digest. Historical business assets/source/hash and all publication tuples are independently checked by the unchanged business model before qualification.

All original, prepare, close, returned registered transport, and complete response JSON are preflighted at 512 KiB of UTF-8 bytes. JSON string escaping and extra envelope fields count. No oversized response is truncated. An opaque prepared-record digest, a phase, or a current card snapshot cannot replace a complete original request or historical result.

An own qualified create/continued receipt can produce a new constant-root/latest-baseline `continued_todos` proposal through the actual business model, provided there is a real exact source-kind-0 publication. A kind-1 raw fork cannot enter this path. This foundation does not produce that business source handoff.

## Evidence and boundary

- [Final actual-source Node run](editor-business-session-issued-recovery-model-tests.log) passes **30/30, zero failures/skipped**, 5061.8498 ms, against fresh `EditorBusinessSession`, `EditorBusiness`, `EditorDraft` and `EditorFieldPolicy` modules. The [earlier 29/29 run](editor-business-session-final-model-tests.log) is preserved. UTF-8/crypto and the synthetic intent receiver are controlled providers. The receiver intentionally returns registered transport with different JSON key order/whitespace to prove literal mapping, rather than equal semantic JSON being silently regenerated. All six inputs listed below matched byte-for-byte before and after this final run. Production remained the same `792E…` bytes across both runs.
- Covered: early composition/raw/field admission, parent queue gate, first no-write vs uncertain prepare, malformed proof/current metadata, issue Unknown, missing/changed full parts, exact native transport dispatch, owner/epoch changes, late selection/composition, monotonic business effect, exact historical assets, five-part restart and current publication retirement, prepared cancellation CAS/Unknown, pagination, full UTF-8 budgets, own source-0 continuation and full u64 receipt revision.
- [Actual Store fixture](editor-intent-store-fixture.json), producer `fresh actual Store`, and its [explicit exporter run](editor-intent-dto-fixture-tests.log), **1 PASS**, come from native `editor_intent::tests::actual_store_intent_dto_fixture`. Five issued and five closed parts are complete real native `editor_intent_read` Replies, including their actual effects, receipt revisions, intent arrays and cursors; no synthetic header is added. Two final ETS tests consume these actual five-part sequences through the unchanged business model. They validate the exact full request/publication/proof/domain identity, original persisted native save/inspect/Close strings, real historical Card source/content hash and canonical publication hash. The issued context performs an explicit inspect and a separate explicit same-original save retry; the cancelled context remains read-only and has no business transport. The fixture exporter is the Store authority, while this parser harness has a controlled receiver. Neither is native transport, SDK or device proof, nor this worker's independent Store crash qualification.
- API26 SDK qualification of this new module: **NOT_RUN by this worker**. Node transpilation is not ArkTS SDK compilation.
- Index/UI integration: **NOT_IMPLEMENTED** for this session. Native adoption, product build, device input/save/keep/close loop, source-0 handoff, `saved_exact` intent closure, protected producer/handoff and complete Flutter parity: **OPEN**. Existing raw fork/source conflict behavior is not silently rebased.

## Frozen input identities and reproduction

| Input | Bytes | SHA-256 |
|---|---:|---|
| New `EditorBusinessSession.ets` | 32,782 | `792E95BC45D5D589A08810B4A3BED24FA0C6229612FB38FDA18ABD12A9821FBD` |
| Unchanged `EditorBusiness.ets` | 26,767 | `6469D1F2AF4E2DA6B9066B52917E658BC13ECECB4EF2A4E6B92FBB4AD0D996CF` |
| Unchanged `EditorDraft.ets` | 29,143 | `A3AE1E99D198FCBA3CDEF741A6F138612AD16979BE42C7A8809003F00D04F965` |
| Unchanged `EditorFieldPolicy.ets` | 6,929 | `10C3162E77A947335ACF0C3DF2E464C80195D0AC2BBC2922BBB09C0A8A626842` |
| New `editor-business-session-model.test.cjs` | 38,644 | `F64477A60AF77857D090AC17E606CF13E9EAA3442FF9CCFBF4E9F41C60661540` |
| Native actual Store `editor-intent-store-fixture.json` | 39,256 | `9C8BD71EE6C42AD965A129973C5943F62A38261EEF99310FA3D12AF99A9999BB` |

From the repository root, with the existing Huawei Node/TypeScript environment:

```powershell
& 'C:/Program Files/Huawei/DevEco Studio/tools/node/node.exe' --test hmos/tool/editor-business-session-model.test.cjs
```

`HMOS_TYPESCRIPT_PATH` / `HMOS_TYPESCRIPT` can select the installed TypeScript provider; `HMOS_EDITOR_INTENT_FIXTURE` can select the complete actual Store fixture. The default is the checked-in v24 fixture above. No mutation is issued by restoration or discovery; the controlled receiver dispatches a business request only on an explicit test save/inspect/retry.

Author: `/root/dev19_delivery_docs`, 2026-10-07 UTC. These new production/test/report/log files are frozen for parent review. This worker did not operate Git, a device, native archives or the product HAP. New source-0 handoff, UI wiring and complete product acceptance remain required follow-up work.
