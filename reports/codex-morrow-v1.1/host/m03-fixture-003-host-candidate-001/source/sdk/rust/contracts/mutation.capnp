@0xa2a94473fe3480db;
# Experimental independent guest mutation profile; not an IO v1 replacement.
# Valid frames are not grants. The owner binds references to the live instance,
# generation, reviewed target, approved content and current permission.
# No host import is enabled by merely compiling this schema.
const version :UInt16 = 1;
const maxFrameBytes :UInt32 = 131072;
const maxChunkBytes :UInt32 = 61440;
const maxContentBytes :UInt64 = 16777216;
const maxOperationBytes :UInt32 = 256;
const maxDeadlineMs :UInt32 = 30000;

enum Kind {
  invalid @0;
  prepareCreate @1;
  prepareDelete @2;
  chunk @3;
  commit @4;
  execute @5;
  query @6;
  cancelPlan @7;
  release @8;
}
enum Status {
  invalid @0;
  completed @1;
  denied @2;
  revoked @3;
  expired @4;
  unsupported @5;
  quota @6;
  notFound @7;
  conflict @8;
  cancelled @9;
  outcomeUnknown @10;
  failed @11;
}
enum Phase {
  none @0;
  absent @1;
  prepared @2;
  outcomeUnknown @3;
  observed @4;
  cancelledBeforeDispatch @5;
}
enum Effect {
  unspecified @0;
  osSucceeded @1;
  osRejected @2;
}
struct Create {
  contentLength @0 :UInt64;
  contentSha256 @1 :Data;
}
struct Chunk {
  offset @0 :UInt64;
  bytes @1 :Data;
}
struct Request {
  version @0 :UInt16;
  schemaSha256 @1 :Data;
  callId @2 :UInt64;
  # Opaque host-issued selection lease, exactly 32 nonzero bytes.
  reference @3 :Data;
  # Exact command retry identity; never automatically regenerated/replayed.
  submission @4 :Data;
  operationId @5 :Text;
  deadlineMs @6 :UInt32;
  union {
    prepareCreate @7 :Create;
    prepareDelete @8 :Void;
    chunk @9 :Chunk;
    commit @10 :Void;
    execute @11 :Void;
    query @12 :Void;
    cancelPlan @13 :Void;
    release @14 :Void;
  }
}
struct Response {
  version @0 :UInt16;
  schemaSha256 @1 :Data;
  callId @2 :UInt64;
  reference @3 :Data;
  submission @4 :Data;
  operationId @5 :Text;
  kind @6 :Kind;
  status @7 :Status;
  phase @8 :Phase;
  effect @9 :Effect;
  stagedBytes @10 :UInt64;
  durableContent @11 :Bool;
}
# One response resumes one paused guest import. No accepted/pending result is
# implied. Failure/timeout never proves rollback; query original history under
# fresh live authority. Release frees selection state, not history or worker.
