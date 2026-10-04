# Experimental finite card-change metadata source

`changes-metadata-v1` is a new opt-in native-library source profile for a finite,
explicitly approved set of card-change notifications. It supports incremental
indexing and cache invalidation. It is not a live forever watcher, Cloud sync,
historical content reader or production Workbench binding.

This implementation uses the existing Events channel and its original durable
ACKs. It adds no guest import, legacy schema field, persistent table or Store
migration. Store schema remains 24. The existing SDK327, frozen compatibility
originals, pins, old channel/content mixing gates and SDK source distribution stay
unchanged. The extension lives outside that frozen SDK.

## What is disclosed

Each notification contains the independent profile digest, scope digest, current
window identity, card ID, operation ID, revision and SHA256 of the whole
`Card.encode()` value. Its strict fixed encoding is defined in the
[Core-owned specification](../core/schemas/changes_metadata_v1.wire), mirrored
byte-for-byte in the [extension](../extensions/changes-metadata-v1/README.md).

The payload has a 150-byte fixed header and two original UTF-8 identifiers, each
1..256 bytes, for a maximum 662 bytes. Unknown versions/digests, malformed IDs,
zero scope/window/revision, inconsistent lengths and trailing bytes are refused.
The profile digest is SHA256 of the normative file's exact ASCII/LF bytes:
`07fc0dc4c48eb17f85309207a441d6a07f3fb8e06aab5e2ce337a4b3c98f9089`.

No body, title, preview, attachment, filesystem path, SQL, global sequence,
internal upper bound, scan count or unselected object ID is sent. Metadata-only
does **not** mean zero information about content: the full-card digest reveals
comparisons/equality and can validate guesses about low-entropy content. Real
application approval must explain this fingerprint disclosure as well as the
selected objects and historical starting point. This phase exercises only
synthetic content and approvals.

The full-card digest is not `ContentChunk.body_sha256`. To refresh current content,
use a separate ordinary content call with its own explicit authorization and
revision checks. A changed current revision can conflict; this notification does
not grant access to the historical body or make a later read a snapshot of it.

## Package and source approval are separate

A new package requires both `changes-metadata-v1` and the original `channel-v1`.
It is ABI2, Events-only, nonduplex and has empty content capabilities; IO,
dependency, service and mutation combinations are refused. An old host rejects
the unknown new feature before running the guest. Old valid packages retain their
original admission rules. Package preparation itself grants no source authority.

A trusted host must explicitly issue
[`ChangesApproval`](../plugin_runtime/src/channel/changes.rs) for one exact
receiver, package digest, Manager revision/selection, HostRuntime, connection,
Control, original Store, complete fixed card set, starting point, read budget and
finite expiry. A hash or an existing `ContentAuthorization` is not this approval.
Existing content probes, when supplied, only restrict it and cap its lifetime.
Grant creation here is a product API design; it does not automatically authorize
real user data, enable an account or install a recurring permission.

`ChangesApproval::issue` validates and materializes the bounded same-Store window.
`bind` consumes that exact batch and installs the dedicated guard before starting
its producer. The resulting `ChangesSource` exposes the original broker and a
restriction-only revocation handle. Revoking any selected card closes the whole
minimal source; it does not silently skip that card and call the feed complete.

Source checks cover preparation, payload retrieval/decode, producer release,
Receive and final durable ACK. They remain active while the guest makes no calls.
Drain/disconnect, original owner destruction, cancellation, original expiry,
Manager mutation, package/instance replacement and observed Store substitution
reject the old source. An observed Store mismatch permanently revokes it; swapping
back cannot revive it. Holding an old Store/batch alive does not make it the
HostRuntime's current Store. Final guards use bounded non-reentrant observations,
not another Store/queue/clock mutex acquisition.

## Finite work and ownership

The reader uses permanent `operations` / `operation_events`, committed atomically
with the card. The pending outbox is not a feed because audit sealing removes it.
A fixed private high-water mark bounds each window. Candidate ranges are bounded
before object filtering, and only selected kind-0 card events load payloads.

Current hard ceilings are 32 cards, 4096 candidate events per window (including a
resumed anchor), 64 candidates per page, 64 retained notifications, 64 KiB retained
canonical metadata, 32 MiB total container bytes and 32 MiB declared decoded bytes.
Page container/decoded ceilings are independently limited to 32 MiB. The original
single-commit decoded limit remains 16 MiB. Window duration defaults to 30 seconds
and cannot exceed 60 seconds; caller/instance/restriction limits can narrow it.
Original channel message, byte, request, deadline and ledger limits also apply.

These are logical workload/read/retained-data quotas, not a proof of exact peak
allocator memory. Oversized or malformed windows fail rather than silently return
partial success. An empty selected page is not EOF until the fixed window ends.
No Store borrow or read transaction survives materialization into the owned small
batch, and none is held while waiting for a guest ACK.

An ACK confirms the original frame/cursor was durably recorded. It does not confirm
an application business effect, response delivery, all future changes, or OS
worker join. Source completion and actual cleanup/join are separate observations.
A Revoked/Expired terminal cause is not a general proof that an uncertain ACK did
not commit; inspect original durable history when the outcome is uncertain.

## Reopening is a newly approved window

`HistoryStart::Beginning` is an explicit historical starting-point decision for
the fixed set. `HistoryStart::LatestAck` does not accept a guest cursor as authority:

1. The trusted host selects the old subscription and source epoch in this Store
2. The current checkpoint and its exact latest original ACK receipt must agree
3. Frame/request/response, profile, cursor, epoch and scope are strictly verified
4. The scope is recomputed for the new exact receiver package and complete card set
5. The original card/operation is looked up in the permanent log and its revision
   and whole-card digest must match
6. Fresh approval creates a new finite window, new reference and new epoch

No old grant, original hidden upper bound, private unacknowledged scan position or
live source is restored. Missing ACK/anchor, foreign logical Store, changed package/
set/profile, malformed correlation or absent current approval rejects. A window
with no ACKed selected event has no public durable progress anchor. Choosing an
earlier backfill is a different explicit decision, not seamless resume.

The scope uses an existing durable Store identity only as a domain-separated
logical identity. Database copies can retain it. This is not physical-file
attestation, anti-clone detection or rollback-lineage proof. The existing global
4096 ACK-receipt limit is unchanged; a fresh epoch does not refund capacity. No
journal GC, rolling retention or indefinite subscription is introduced.

## Independent developer extension

The [extension README](../extensions/changes-metadata-v1/README.md) describes the
Rust codec, C ABI/C++ wrapper, shared vectors, explicit package builder and
reproducible three-language guest preparation. Guests use one original
`morrow.channel.directory.v1` Events endpoint, enforce profile/epoch/cursor and
constant scope, and use original receive/ACK transport. Their result is a bounded
count of ACKed notifications, not a content change or resource-join certificate.
The new C/C++ build uses one combined Rust archive, not two Rust runtimes.

This phase exposes a native library adapter and developer extension. Workbench
profile discovery, catalog/GUI integration and production owner binding were not
extended. Existing discovery flags remain unchanged and do not advertise or
authorize this new source. Static `prepared` is not evidence that an installed
Workbench can start it; a host integration must explicitly supply the new source
approval/binding route.

Linux ordinary disposable-Store end-to-end tests and Core protected temporary-SQL
tests are separate evidence. `ProtectedStoreSlice` still cannot be passed through
the complete HostRuntime/channel path. No test constructor or raw Store fallback
was made public. Windows/macOS/product-owner/GUI qualification remains separate.
See [G06 validation](../reports/reconstruction-2026-10-04/changes-metadata-validation.md)
for exact source, artifact, test and preserved-failure scope.
