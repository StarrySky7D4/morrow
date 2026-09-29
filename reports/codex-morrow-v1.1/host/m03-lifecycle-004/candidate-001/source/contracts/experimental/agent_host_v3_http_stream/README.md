# Native HTTP stream wire: major 3, revision 1

This is the sole M03 Cap'n Proto contract candidate. It does not extend old v2 in place.
Schema/codec vectors can be reviewed before either host or plugin runtime exists. No native
runtime READY or permission is implied by successful decoding. All values are little endian
where an explicit integer byte representation is described below.

## Framing, identity and lanes

Four-byte LE payload length, then exactly one unpacked Capnp message. Payload 8..32768 bytes,
multiple of 8; total frame <=32772. No packed encoding, trailing bytes or second message.
Reader traversal <=16384 words, nesting <=8. A partial frame gets at most500ms and never more
than the original session deadline. Any partial-write cancellation closes the lane.

Every frame binds major3/revision1, raw schema SHA256, session, epoch, actual child PID,
nonce32, artifact32, executionConfig32, original operationId(1..256 bytes), attempt=1 and
capabilities=3. Bits mean supported own-session and HTTP proposal protocols; neither bit
grants HTTP. Separate HTTP approval is mandatory. Parent nonce and pipe nonce are different.
All unknown kinds/enums, reserved!=0 and bad lengths fail closed. `same_admission` compares
only the stable tuple, so runtime MUST also check generation, remaining/budget echo,
direction/lane, code, payload state and strict sequence. Decode alone checks none of that
cross-message authority. Schema hash is exact file bytes, including comments/newlines.

Control lane: inherited guest stdin/stdout, stderr diagnostics. Challenge sequence0;
Hello sequence1 exactly echoes admission (except kind/sequence); Welcome echoes1. Subsequent
guest control sequence increments by1, no gaps/replay; Query/Close/Prepare/Commit/Credit/Cancel
requests have code0 and echo initial Challenge remainingMs/requestBudget/generation.
Host direct responses echo request sequence; spontaneous notifications have sequence0.
Host response remainingMs is a current monotonic observation, never permission to reset time.
Host active generation1 becomes2 when cancellation/revocation is applied; guest stale tuples
do not grant activity. At most128 guest control requests per session, independently of bytes.

Data lane: one host-created duplex local named pipe, host owns the only server. DataOffer on
control carries locator<=240, random nonce32, maxChunkBytes<=8192, creditLimit=16384. Guest
DataBind is data-direction sequence1, echoes Channel in full and original admission. Host
checks actual GetNamedPipeClientProcessId against its held child process handle BEFORE data
permission; DataBound is host data sequence1 with exact Channel. Each direction increments
its own data sequence by1. Only RequestChunk guest→host and BodyChunk host→guest follow.
The control sequence is independent. DataOffer/Bind must never be accepted as a filesystem
path supplied by an unapproved guest. One connection only; no reconnect/replay/regrant.

Platform requirements remain explicit first-instance/remote-reject/noninherit/SID+SYSTEM
DACL, and owned OVERLAPPED/event/buffer completion including connect/read/write. The codec
does not implement these. Closing/drop/cancel request alone is not completion proof.

## Proposal, trusted approval and exactly one Commit

1. After native Welcome and data binding, guest control HttpPrepare contains exact method,
   full absolute target, ordered raw header pairs, bodyBytes<=32768, bodySha256 and
   responseLimitBytes in1..65536. Native request policy is loopback-only, no credentials,
   precise URL/method and required Core-header allowlist, all independently host-checked.
2. RequestChunk offset starts0, contiguous, each1..8192, sums exactlybodyBytes. Zero-body
   proposal completes at Prepare. Chunks do no DNS/connect/HTTP and imply no Core Prepared.
3. Host emits HttpProposed sequence0 with random proposalRef32, its own body/request hashes,
   bodyBytes, requested responseLimitBytes, sendBudget0, empty httpGrantRef/endpointRef.
   Guest matching hashes never approve anything. Proposal input cannot subsequently change.
4. Trusted operator independently inspects original bytes, then explicitly approve_http
   with proposalRef/expected request hash and approved limits. Host emits HttpApproved
   sequence0, with host-issued grant32/endpoint32, budget1 and approved response limit<=the
   request limit and host64KiB cap. Tightening that limit is an explicit approval constraint;
   requestSha256 still names the original request with its requested limit. Approval binds
   the full Decision including approved limit, plus parent and endpoint policy.
5. HttpCommit carries an exact copy of Approved Decision (code0), only once. The runtime
   uses original live native authority, Core HTTP validation/materials/Prepared and unique
   durable Unknown claim, then consumes one SendTicket at the owner effect fence. Replayed
   Commit cannot send, reconnect cannot revive, Unknown permits only Inspect/ReconcileOnly.
   Host replies State echoing Commit when accepted; ResponseHead is later sequence0.

Transport hash preimage is implemented by `request_digest`: literal UTF8/ASCII domain bytes
`Morrow/native-http-proposal/v1\0` (final byte00), session u64, epoch u64, operation field,
attempt u64, method field, absoluteTarget field, header count u32, each name field/value
field in order, requested responseLimitBytes u32, body field. A field is u32 byte length plus
exact bytes; Text uses UTF8. There is no JSON parse/re-encode, lowercasing or sorting in this
hash. Body hash is SHA256 body alone. Core request hash is SHA256 Core encode_http_submit
material and is a third, different hash; it belongs to host evidence, not guest permission.

Core `io::Header.value` is already Vec<u8> and permits valid obs-text. Correction to frozen
transport README: the old String restriction belongs to network_node::HttpRequest/managed
conversion, not Core. A new native adapter can use Core raw bytes → RawHttpRequest without
introducing String conversion. Still run Core validate_http_submission and native allowlist;
never delete/lossily replace fields. Legal raw bytes do not bypass no-credential policy.

## Delivery, absolute credit and consumption classes

ResponseHead preserves status100..599, duplicate header pairs and raw values, actual checked
remote address. Total headers<=8192 bytes including name+value+4 per pair, <=32 entries;
each name<=128 ASCII bytes, value<=8192. HTTP grammar is validated by HTTP/core policy, not
by treating this codec's structural bound as authorization. 3xx/4xx/5xx are not automatic
retries or semantic success. No redirect, proxy, credentials, decompression, WS/fallback.

Guest HttpCredit begins with consumedOffset0 and the requested window(0..16384), maxChunk
1..8192. A zero window pauses new issuance. CreditState echoes it with host Progress.
Each update is an absolute snapshot; counters must individually be monotonic, and
parserYielded + drainDiscarded + cancelDiscarded + errorConsumed = consumedOffset. Duplicate
snapshot is idempotent; it does not add credit. Conflicting/decreasing classifications reject.
`parserYielded` means handed to parser, not model success; `drainDiscarded` is bounded tail
discard after business supervisor chooses drain; `errorConsumed` is non2xx diagnostic bytes;
`cancelDiscarded` is late bytes discarded after cancel, never new work. Classes are disjoint
parts of a consumed byte prefix, not four independently refillable windows.

Host requires consumedOffset<=issuedOffset, not necessarily <=its observed osCompletedOffset:
a peer may read/ACK before host observes overlapped completion. Authorized end is capped
`min(approved_response_limit, consumedOffset + windowBytes)`; bytes already reserved/issued
retain their accounting if window shrinks, and no new reservation may exceed the new end.
After cancel, only zero-window ACK for already issued bytes can update cleanup diagnostics;
it must not start a read, extend the deadline or grant new write. Request/response offsets
count body bytes, not Capnp framing. At most two8KiB application queued chunks plus one
OS write operation; the pending write occupies the SAME outstanding credit reservation.

When received bytes equal the approved response cap and all delivery is consumed, the host
must still allow one bounded terminal poll under the original deadline to discover actual
HTTP EOF. It requires no positive body credit and delivers no new bytes: any excess byte
is Limit/Unknown. Reaching the length cap alone is never EOF. Native credit integration must
test exact-limit+EOF and limit+1, beyond the transport-only tests. A spontaneous HttpTerminal
may precede data delivery; plugin cannot return byte-stream EOF until its contiguous received
prefix reaches the final received offset and all prior body frames have been accounted for.

Host body offset0, contiguous chunks<=agreed maxChunk; sequences are data-lane sequence.
receivedOffset counts HTTP bytes accepted under quota (not all rejected/network-card bytes),
reservedOffset counts scheduled body, issuedOffset counts complete logical chunks whose
first OS write has been authorized, osCompletedOffset counts complete frame-body boundaries
whose entire physical frame finished; partial frame byte counts are separate OS diagnostics.
No claim of byte retraction: already issued bytes may arrive after CancelAccepted. Consumer
must apply generation/cancel state before exposing cached chunks as new business events.

ParserDetached/Completed are plugin business states, absent from host authority protocol.
The plugin RequestTask retains the lease. Detached pauses new demand pending actual Core
terminal; completed permits bounded drain with the same operation/credit/deadline; errors
or user cancellation issue HttpCancel. Core incomplete(interrupted) may be Completed with
end_turn=false; the host has no SSE knowledge. Neither model terminal upgrades HTTP evidence.

## Independent status dimensions and cleanup

HttpCancel is guest control with no body. Host persists original HTTP revocation before
applying it and reports CancelAccepted/Progress with persisted/applied flags. External host
revoke can remain application-pending. Acknowledgement rejects new issue at the owner fence;
it does not retract ongoing DNS/connect/send/OS data. Code25 indicates requested stop;
16 protocol,17 identity,18 sequence,19 revoked,20 expired,21 quota,22 unsupported,
23 handshake,24 disconnected,26 transport,27 storage/commit unknown,28 proposal/hash,
29 not approved,30 duplicate dispatch. Errors after boundary retain Unknown unless full
response was already stored/Observed. Never downgrade an existing Observed on later IPC error.

Progress has separate intent/network/owner, error, HTTP status/EOF, responseMaterialStored,
offsets, revocation, workerStarted/Joined, connect/read/writeReaped, dataClosed, child exit,
stdout/stderr EOF, requestClosed and ownerReleased. Reaped means there is no outstanding
operation of that type (including the never-issued case); workerJoined is true only when a
started worker's JoinHandle resolved. Internal reqwest tasks/remote rollback are not covered.

HttpTerminal sequence0 reports a transport/evidence transition, which may precede delivery
or owner cleanup. Observed requires actual full HTTP EOF and complete durable materials,
never a partial response. It can coexist with later delivery cancellation/parser error.
Unknown can coexist with plugin business Completed if drain failed. CancelAccepted can
coexist with live worker/unreleased owner.

RequestClosed sequence0 is the live guest's REQUEST cleanup receipt: worker if started is
joined, data pipe connect/read/write operations all reaped and dataClosed. It requires no
child exit/stdout EOF/stderr EOF/owner release. Guest can then send Close and exit. On normal
success, host sends it only after the plugin consumed/ACKed all body and cleanup completed;
on cancellation, unACKed issued bytes remain explicit, not silently marked consumed.
Whole-session Released is a trusted operator inspection result after real child exit, dual
EOF and durable owner release; there is NO guest Released message. The guest must never wait
for its own OS exit in order to exit. If control is broken, only operator cleanup facts are
available; a missing guest receipt is not success. Losing the guest always triggers cleanup.

Tests/vectors prove structural encoding and hash binding only. Runtime cross-message states,
pipe authentication, durable approvals, claim/revoke races and end-to-end success await the
new host/plugin candidates and independent joint execution. Old001/002 and product gates
remain unchanged. This is not production CLI authentication or an OS sandbox.
