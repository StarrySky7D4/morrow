# Blob upload request candidate

This independent Cap'n Proto contract wraps the existing `blob-transfer-v1`
frames. It preserves all existing task, IO, channel and blob schemas. Its current
implementation includes the request/response codec, a bounded Rust client state
machine, checked C ABI owners and move-only C++ owners. Core now accepts an explicit isolated package declaration and an independent
Manager approval ceiling. A dedicated Runner now admits exactly the two task
imports and `morrow_blob_upload_v1.call`, with a fixed 65,536-byte request and
4,096-byte response ceiling. Original live approval binds the Manager, instance,
Store, operation, deadline and cumulative charges once. Generic callback/task
routes remain denied. The dedicated original-owner worker now stages, inspects
and releases through the original Store with live commit checks. **Protected
product approval/recovery remain open.** Actual three-language guest evidence403
is recorded below, including16 MiB through an ordinary original test owner. Protected product approval and recovery remain required work.

The host must separately approve and nominate one exact original operation.
The nomination name, frame identities, hashes and receipts confer no permission.
The adapter must retain the original manager, instance, Store, absolute deadline,
revocation and resource accounting. FileRead/FileCreate or an ordinary channel
grant cannot substitute for this approval. No path, database address, native
handle, grant, clock or caller-selected lease is carried on the wire.

Requests are Open, Push, Inspect and Release. Push embeds an unchanged original
Descriptor, Chunk or End frame. Guest-supplied Receipt frames are rejected.
Replies bind the exact original request digest, nomination, nonce and action.
They contain a state, the original chunk receipt, or a bounded error. A chunk
receipt is only a byte receipt. Only Staged reports that the original backend
committed the exact object. Even Staged does not mean a business attachment was
committed, a worker returned, resources were reclaimed or release gates passed.

An unknown transport/delivery result requires explicit recovery under the
original authority; no automatic retransmission or reconstruction is provided.
Inspect of a previously unseen operation cannot restore a sender or its hash
state. Absent is an observation, never fresh permission. Release is separate from
task cancellation and actual worker return. The host must preserve original
durable release history and must not make released identities reusable.

## Limits and ownership

Requests remain within 65,536 bytes; their embedded frames are at most 64,512
bytes, leaving envelope space without enlarging a legacy call. Existing 60 KiB
chunks fit. Responses are at most 4,096 bytes and embedded chunk receipts at most
1,024 bytes. Objects remain at most 16 MiB. Flat layouts, both schema digests,
versions, reserved fields, action/phase combinations, trailing messages and
capabilities are checked before an owned request is constructed. Nested frame
bytes retain their original encoding and digest.

The Rust client defaults to 1,024 calls and 64 MiB cumulative request/response
wire; trusted callers select budgets within the hard ceilings of 4,096 calls and
64 MiB wire. Only one original request may be pending. Unique nonces, actual admitted
wire charges and call counts are retained through failure or release. Resident
nonce metadata is bounded by the call ceiling; there is no complete-object
buffer. Decoder/caller allocations and callback-owned state are separate from
wire accounting. Returned request/response views borrow their owning container.

`call_once_with` invokes one non-reentrant synchronous callback. Its buffers may
not be retained. A failure, panic, invalid response, foreign response or lost
result seals Unknown, retains the original pending request and forbids another
call. A valid explicit OutcomeUnknown reply is likewise terminal. No helper
state proves that the host stopped or rolled back an operation.

The actual original-owner and three-language guest evidence is recorded below.
Protected product approval/recovery and full SDK acceptance remain required work.


## Native C and C++ scope

Headers in `include/` use `mbu_` symbols and the independent namespace
`morrow::blob_upload_request_v1`. Exact input version/size and zero unused fields
are required. Output prefixes, slots and lengths remain unchanged on errors;
already admitted charges and Unknown state can remain. Checked ranges prevent
overlap with the accessed live helper storage. Native callers must still supply
valid aligned memory and unique live handles; these helpers are not pointer
isolation. Views borrow their owner. Callbacks are explicit, synchronous and
non-reentrant, cannot retain buffers or unwind across the C ABI. Ordinary
allocator failure is not promised recoverable.

C++ request/response/client owners are move-only. Encoding swaps a temporary
vector only on success. `client::initialize` refuses a second initialization;
there is no hidden reset of pending requests, cumulative fees or Unknown state.
Dropping a native helper is not host cancellation or proof of worker return.

Qualification368 binds the original cold Rust build364 (19 methods and strict
library Clippy), actual MSVC C/C++ programs366 and Clang C/C++ programs367. Each C
program passed eight scenarios including empty, small and 16 MiB/274-chunk
transfer, guarded undersized inputs, output preservation, exact response binding,
budgets and no replay after Unknown. Each C++ program passed ownership, vector,
correlation, move and callback checks. Compiler executions share scenarios and
are not additive product coverage. Tool setup/parser failures364/365/366 remain
preserved. The Staged responses are fixtures: these programs do not access a
Store, public Wasm import, protected Session, approval UI or VM.


## Package and approval ceiling

Persistent Protobuf Manifest field22 carries the independent declaration: both
schema digests, the complete unique registered handler set, object/call/wire
ceilings. It requires guest ABI2 and only transform-handlers-v1 plus
blob-upload-request-v1. Content/IO/channel/dependency/mutation combinations are
rejected. Unknown, duplicate or noncanonical declaration semantics are rejected.
All internal upload messages remain Cap'n Proto; storage metadata is Protobuf.

Registry v3 persists one exact-package upload ceiling separately from content
and IO approval. Older v1/v2 snapshots cannot carry it. Ordinary v2 publication
remains unchanged until upload approval is used; v3 does not downgrade after
revocation. A different package digest never inherits upload approval. Manager
validates first, then invalidates prior live controls before changing approval.
CommitUnknown blocks even a no-op until the original persistent state is reopened.
This ceiling does not nominate an object, Store, owner, deadline or live grant.

Qualification372 binds core369 (876 passing methods,19 ignored, including11 new
declaration/registry methods), runtime370 (56 passing, including3 new Manager/
preparation methods), strict library Clippy for both and15 original controls371.
The initially zero-selected controls target370 receives no credit. At that
qualification live binding and Runner dispatch were still open. The following
section records subsequent progress; protected product approval/recovery and
real Rust/C/C++ upload guests remain open.


## Dedicated import and original live approval

Qualification381 verifies five private continuation methods (121 unrelated unit
methods filtered),83 runtime regressions and86 core methods with two ignored child-harness entries.
Strict library Clippy passed for core/runtime. The production Workbench cold
library check passed; it is not product execution. All57 frozen SDK inputs remain
unchanged. Runtime380 retained the failed377 programs and reused its dependency
cache only with compiler receipts proving the modified runtime and every selected
test program recompiled; it does not claim a cold target.

Approval is issued once per original managed instance and is not recreated by
dropping it. Calls and wire charges are cumulative. Revocation, expiry, loss of
the original receiver/Manager/Store and observed Store substitution deny further
use. An invalid empty-object hash is rejected before consuming that claim.
At qualification381 these checks did not establish durable guest upload or a
protected product owner. The subsequent original-worker evidence follows below;
protected product approval/recovery and actual Rust/C/C++ upload guests remain open.


## Original upload worker and Store dispatch391

`UploadWorker::spawn_owned` moves the complete original Manager/runtime owner,
managed instance and one-shot approval to one native thread for one upload task.
The independent upload handler checks its registered task types and limits;
ordinary pure transform execution remains forbidden. Polling is nonblocking,
the original join returns the owner, and failed disconnection retains the exact
instance. Cancellation or Drop does not prove thread termination.

Open, Push, Inspect and Release use the independent Cap'n Proto protocol. The
producer permanently seals this bounded request stream on a verified End while
keeping that exact End call suspended. End is answered Staged only after the
original Store transaction commits. This is not guest termination or temporary
unavailability treated as EOF. Later Push is refused; a later failure cannot
roll back a committed End. Chunk acknowledgments remain byte-validation facts.

Live guards veto access and final Store commit; source failure and precommit
revocation roll back. CommitUnknown, postcommit delivery failure and unread
result cancellation retain the original history without replay. Observed Store
substitution prevents foreign cleanup. Native verification reserves a separate
128 MiB cumulative host-work ceiling before storage calls; short Inspect/Release
messages pay for full payload verification. These charges never refund.

Qualification388 passes 96 selected runtime methods, including13 actual WAT
guest/owner/Store methods; five private continuation methods also pass, with121
unrelated methods filtered. Core passes12 dedicated-handler methods. Both core
and runtime strict library Clippy pass. The preceding guard qualification384,
reviewed without replay by386, passed882 full-core methods with19 ignored; that
earlier source and its overlapping counts remain separate. Workbench390 compiles
the current production library after the one-package lock update; no product
process was run. Reused dependency caches are explicit, changed source compiler
receipts are retained, and all57 frozen SDK inputs match.

Failure387 records the wrong pure-handler admission route before its dedicated
replacement. No upload was admitted in that failed run. Metadata preflight389
requested an uncached non-Windows dependency;390 instead checks the actual locked
Windows library offline. Neither original failed record was overwritten.

This evidence is not Rust/C/C++ upload-guest SDK qualification, protected product
execution, product approval/recovery UI, full P2/P3, SDK freeze or release.


## Actual Rust/C/C++ upload guests403

The three independently compiled Wasm guests now use only the task read/complete
and fixed upload call imports. C and C++ share one Rust codec archive/allocator;
C++ exercises the move-only client and response wrappers. Inputs and replies use
existing Cap'n Proto messages. No WASI or trusted Store code is linked into guests.
The bounded example producer generates byte42 chunks; it is not arbitrary file IO.

Qualification395 ran empty,150017-byte and16 MiB objects for each language through
the actual original UploadWorker and Store. All nine successful cases verify exact
bytes/SHA256, observe the known existing stage without re-uploading, release under
new explicit original-owner approval, join all27 workers and reopen durable history.
16 MiB uses274 chunks,278 host calls,17076048 wire bytes and84525056 native work bytes.

Real SHA256 and codec work exceeded the legacy100 million fuel ceiling. The isolated
upload profile now permits an explicitly declared and host-approved cumulative
ceiling of8 billion. Ordinary profiles retain100 million; defaults retain20 million.
Package/host limits intersect and no resume refills fuel. Actual16 MiB used about
6.03 billion Rust and6.63 billion C/C++ units. Absolute approval deadlines, revocation,
memory/call/wire/native-work ceilings remain independent. Pure computation is still
bounded by interpreter fuel, not an immediate cancellation interrupt.

The low-budget negative case correctly stopped with Limits,1323 remaining units,
six calls and no staged record.395's arbitrary test-only assertion that the remainder
must be below1000 was wrong: the interpreter charges blocks.396 changed only that
assertion, reproduced the entire395 source digest with the old test bytes, and reran
only the failed method. Its actual closure/rollback passed; the nine prior successes
were not replayed.396 then caught a new profile-test compile error: Package has no
Clone implementation.398 corrected only that test to decode the original archive,
then its broader test Clippy found an existing complex function-pointer type.
400 names that test type, proves the full-source delta and runs the remaining
regressions. Its checks all pass, but its wrapper incorrectly expects121 filtered
private methods instead of123 with fault-injection enabled.402 reviews the logs
without replaying passed tests and compiles the Windows production library.
All failures392/394/395/396/398/400 remain recorded; production code did not change
after395.

Core package regressions52, related runtime methods97 and private continuations5
pass. The123 unrelated private methods and9 success methods filtered by396 are not
additional tests. Strict Clippy and the Windows production library check pass;
the latter is compilation only. All57 frozen inputs remain unchanged. Targets and
temporary files stay under D:/build_cache; no VM or host-policy actions occurred.

Protected product owner, approval/recovery UI, watch/rename, remaining Windows P2/P3,
full SDK26/G04/SDK_FREEZE, source and release acceptance remain open.
