# Owned Windows named-pipe I/O 001

Shared host/guest platform facade, independent of Tokio/Core/grants. Dependencies: std and
windows-sys0.61.2. The only unsafe code in this new platform slice is the audited Windows
FFI and ownership boundary here; consumers retain forbid(unsafe_code). This crate itself
does not approve networking or authenticate native sessions.

Host: `Pipe::create_private(locator)`, `begin_connect`, `poll(Connect)`, `client_pid`.
Guest: `Pipe::open_client(locator)` is one CreateFileW attempt, not reconnect/retry. Host
must create/listen before offering the locator. A busy/error result must not cause unbounded
busy-wait; session driver uses its original deadline and rejects failed binding. Guest SQOS
is explicitly PRESENT+IDENTIFICATION, with no unnecessary server impersonation capability.

Both: `begin_read(1..32772)` / `begin_write(Vec<u8>)` issue at most one of each; `poll(Kind)`
returns None only for actual GetOverlappedResult ERROR_IO_INCOMPLETE, otherwise Completed
contains OS bytes/error. `Started.pending` records ERROR_IO_PENDING at issue, not sustained
backpressure. `has_operation` means unreaped slot, including immediately completed operations;
it is not evidence of OS pending. Partial writes require new owned operation for the tail.
Read data is returned only after completion. EOF/disconnect remain explicit OS error facts.

Each operation owns Box<UnsafeCell<OVERLAPPED>>, separate noninherited event, and stable
Box<[UnsafeCell<u8>]> memory. Only raw pointers reach Windows while pending; ordinary buffer
reads occur after completion. Pipe is single-owner Send, not Sync. Moving it cannot move
OS-referenced storage. No consumer receives a raw handle or OVERLAPPED pointer.

`cancel_all` rejects all future issue and requests CancelIoEx. ERROR_NOT_FOUND is not a
completion receipt. Continue polling connect/read/write individually or call blocking
`cancel_and_reap`. Expected completed errors include995(cancelled),109(broken pipe),232/233,
234,38,1236; unexpected API errors retain operation and handle as unconfirmed. A failed
initiation is separately known not to own pending I/O. Any error must remain visible.

**Pipe belongs on a dedicated owned-I/O thread.** Blocking cancel_and_reap and Drop must not
run on UI/control dispatch or a Tokio async worker. Driver ControlCancel first records
acceptance and signals that thread; cleanup waits separately for real completed/reaped facts
and thread join. If completion does not arrive by the cleanup observation budget, retain the
thread/resources and report unconfirmed; do not fabricate RequestClosed/Released. Drop is
only a memory-safety fallback: it requests cancellation and waits. If API failure still
leaves uncertain operations, it intentionally retains their allocations/events and the
original handle via ManuallyDrop, never a fallible clone. This exceptional process-lifetime
leak is NOT successful closure or join and has not been fault-injected in producer tests.

Creation uses first-instance, overlapped byte mode, remote rejection, one instance, requested
1024-byte inbound/outbound buffers, explicit protected DACL(current user SID + SYSTEM only)
and noninherited handle. File-all-access is explicit; Windows maps generic-all to file-all
for pipe objects. `security_sddl` reads the actual kernel DACL; `expected_private_sddl`
builds the comparable expected current-identity descriptor. `buffer_info` reports actual
buffer sizes; requested size is not an unconditional quota guarantee. A pipe locator/PID
alone is not identity: higher driver must match observed client PID to held child and validate
session/epoch/schema/grant/channel nonce. Same-user adversary/handle delegation isolation
and process-tree containment are not claimed.

Four substantive real Windows tests plus one helper entry cover: actual protected DACL and
noninherit flag/first-instance, pending connect cancellation completed995, known child PID,
duplex bytes, client/server endpoint disconnection109, and an8KiB write to a stopped reader
remaining IO_INCOMPLETE at three samples25ms apart while independent stdio control works.
The latter also cancels/reaps simultaneous read+write as995. Fixed pipe buffers are1024 in
the measured test. This is platform qualification only: not the native HTTP/credit/Core
chain, not full16KiB application-window proof, not remote rejection runtime attack testing,
not full host-death/crash/fault recovery, not all Windows versions. A helper's actual OS exit
and stdout EOF are observed; no all-process inventory is performed.
