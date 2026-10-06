@0xdd03b9f5c17db590;
# Sole agent-content-v1 experimental authority, version1 revision1.
# Exact raw schema and current Core runtime/content digests identify every frame.
# IDs, references and proposals confer no authority. No Store or native handle is serialized.

struct ContentRef {
  cardId @0 :Text;
  revision @1 :UInt64;
  totalLength @2 :UInt64;
  bodySha256 @3 :Data;
}
struct Query { cards @0 :List(Text); }
struct ReadRef {
  reference @0 :ContentRef;
  offset @1 :UInt64;
  length @2 :UInt32;
}
struct ProposeMutation {
  reference @0 :ContentRef;
  command @1 :Data; # One unmodified Core EditContent request. Never an implicit commit.
}
struct InspectOperation { cardId @0 :Text; operationId @1 :Text; }
struct Request {
  version @0 :UInt16;
  revision @1 :UInt16;
  schemaSha256 @2 :Data;
  runtimeDigest @3 :Data;
  contentDigest @4 :Data;
  requestId @5 :Text;
  union {
    query @6 :Query;
    readRef @7 :ReadRef;
    proposeMutation @8 :ProposeMutation;
    inspectOperation @9 :InspectOperation;
  }
  coreRuntimeVersion @10 :UInt16;
}
struct Proposed {
  operationId @0 :Text;
  proposalSha256 @1 :Data; # SHA256 of the complete original outer Request bytes.
}
enum Failure {
  denied @0; notFound @1; revisionConflict @2; operationConflict @3;
  capacity @4; busy @5; storage @6; commitUnknown @7; limit @8;
}
struct Reply {
  version @0 :UInt16;
  revision @1 :UInt16;
  schemaSha256 @2 :Data;
  runtimeDigest @3 :Data;
  contentDigest @4 :Data;
  requestId @5 :Text;
  requestSha256 @6 :Data;
  union {
    query @7 :List(Data); # Ordered Core Summary/Rejected replies, one per nominated card.
    readRef @8 :Data; # One Core ContentChunk/Rejected reply.
    proposed @9 :Proposed;
    operation @10 :Data; # One Core OperationResult/Rejected reply.
    rejected @11 :Failure;
  }
  coreRuntimeVersion @12 :UInt16;
}
