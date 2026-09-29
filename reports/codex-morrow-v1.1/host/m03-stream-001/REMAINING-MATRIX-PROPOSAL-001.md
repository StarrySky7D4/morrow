# Remaining M03 runtime matrix: minimal extension proposal

This is a proposal only. No additional POST, executable change or runtime invocation is
authorized by this document. Keep host003/plugin002 and all completed/failed batches fixed.
Joint is already repeating the first positive scenario; do not duplicate it here.
Every case gets a fresh profile, operation, parent grant, HTTP approval and evidence directory.
No retry of an Unknown operation. Keep handshake6000ms within the original approval TTL10000ms.

| Priority / case | Minimal new fixture work | Required independent observations |
|---|---|---|
| 1. HTTP error, one POST | Harness-only variants for complete 401 and 503 response bodies; same frozen host003/plugin002. Replace positive-only assertions with separate Core/error/evidence/cleanup assertions. | One POST each; no retry or auth recovery; actual Core error; complete HTTP body can still become Observed, with HTTP status retained. RequestClosed/exit/owner release checked independently. A Core error must not be mislabeled transport Unknown if full material was committed. |
| 1. Redirect denial | Harness-only 307 with Location to a second synthetic loopback sink. | Original POST count1, target sink count0; no redirect/fallback. Record actual handling of full original response separately from model success. |
| 1. Exact cap and real EOF | Harness-only valid SSE Completed plus bounded comment tail padded to exactly65536 response bytes, then wait for terminal read before chunked EOF. Separate fresh cap+1 case. | At exact cap, no Observed/RequestClosed merely from length or final ACK; only actual EOF can close. All offsets and classification totals equal65536 after EOF. Extra byte gives quota failure/Unknown unless an actual full permitted response already existed; never hide/retry excess. Record the real terminal poll/EOF evidence; do not infer it from body length. |
| 1. Early disconnect | Harness closes connection mid-SSE/UTF8 without completed HTTP body, after one POST. | Real Core error, no fake Completed, durable Unknown, no second dispatch; cancelled/joined worker and pipe/owner cleanup remain separately visible. |
| 2. Actual host revoke suppresses queued business output | New separately frozen plugin fixture mode: allow one real Core event to queue/reserve, hold its final consumer delivery behind a trusted test barrier, then release after host's applied revoke acknowledgment. Harness drives existing host revoke. Do not replace actual Core with injected ResponseEvent. | Already delivered events remain facts. Events whose shared delivery gate loses to cancellation are never emitted afterward. Record enqueue/reserve/delivery/revoke ordering from a common harness monotonic clock and the existing admission generation. Native consumed attempt cannot retry; server may have received the first request and stays Unknown without full material. |
| 2. Native credit and actual pending WriteFile cancellation | New separately frozen plugin fixture mode with explicit pipe-reader pause acknowledgment, distinct from pausing the parser/consumer. Harness server supplies enough bounded data; keep native buffers1024, credit16384, chunks<=8192 and queue2. Revoke over independent control while one actual write remains pending. | Prove the same overlapped write has three ERROR_IO_INCOMPLETE observations at least25ms apart after the pause barrier, and control cancellation reaches the owner independently. Credit exhaustion alone is not OS backpressure. If the fixed window cannot reach this state, mark case unqualified; do not enlarge buffers/credit/response caps to force success. Reap each outstanding operation and join workers before RequestClosed; child exit/dual EOF before owner release. |
| 2. Cross-lane ACK and late data | Reuse the new controlled pipe mode to expose already-issued tail before OS completion observation is drained, and to deliver a previously issued frame after control revoke. New adversarial codec peer only for wrong-offset/positive-credit negative frames, not as a replacement for real Core positive acceptance. | Host ACK check uses actual issued fence, never queued/reserved bytes. Per-lane sequence and full admission tuple hold. Original-generation zero-window issued-tail ACK may clean up; positive window, duplicate Commit, wrong identity/sequence or unissued offset is rejected. Late bytes after cancel are classified cancel-discarded, never parser/business output. |

## Shared fixture/report changes

Extend the append-only harness into explicit named cases with expectation fields rather than
changing frozen input at runtime. Pin each new script and fixture executable before launch.
Capture terminal final independently of operator command success, preserving the original
failure reason, stderr bytes/text policy, process liveness and separate owner cleanup facts.
Server records exact original approved request bytes and count, all emitted response bytes,
EOF/disconnect and barrier timestamps. Never turn a failed expectation into an automatic rerun.

HTTP status, Core terminal, durable IO phase, HTTP EOF, parser/drain/error/cancel byte totals,
RequestClosed, native/guest worker joins and whole owner release are separate result dimensions.
Use existing frozen candidates for harness-only cases. New barrier behavior needs a new plugin
candidate and targeted local review before a real host test; it must not change approval,
deadline, framing, window or effect-fence rules. Host changes are needed only if these cases
expose a concrete defect. OS partial control-write fault injection remains a distinct gap.

Smallest useful next tranche after joint positive acceptance: one503, one307+zero-hit sink,
exact-cap EOF and cap+1, each independently authorized once. This is a proposed ordering,
not authority to run four POSTs or a complete M03 qualification claim.
