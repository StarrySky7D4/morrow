@0xbfd1347ca4879e62;
# Versioned internal messages, not a persistence container or capability grant.
struct Request {
  protocolVersion @0 :UInt16;
  operationId @1 :Text;
  runtimeDigest @4 :Data;
  contentDigest @5 :Data;
  union {
    unsupported @2 :Void;
    renameCard @3 :RenameCard;
    readSummary @6 :Text;
    queryOperation @7 :QueryOperation;
    readAttachment @8 :ReadAttachment;
  }
}
struct RenameCard {
  cardId @0 :Text;
  expectedRevision @1 :UInt64;
  title @2 :Text;
}
# Summaries are a read projection. They must never replace a stored Card.
struct CardSummary {
  protocolVersion @0 :UInt16;
  cardId @1 :Text;
  typeId @2 :Text;
  formatVersion @3 :UInt32;
  revision @4 :UInt64;
  title @5 :Text;
  previewText @6 :Text;
}

# Replies are runtime projections/results, never persistence records or grants.
struct Response {
  protocolVersion @0 :UInt16;
  requestId @1 :Text;
  runtimeDigest @2 :Data;
  contentDigest @3 :Data;
  union {
    unsupported @4 :Void;
    renamed @5 :CommitReceipt;
    summary @6 :CardSummary;
    rejected @7 :Failure;
    operationResult @8 :OperationResult;
    attachmentChunk @9 :AttachmentChunk;
  }
}
struct CommitReceipt {
  operationId @0 :Text;
  cardId @1 :Text;
  revision @2 :UInt64;
  contentSha256 @3 :Data;
  eventId @4 :Text;
}
enum Failure {
  denied @0;
  notFound @1;
  revisionConflict @2;
  operationConflict @3;
  capacity @4;
  busy @5;
  storage @6;
  commitUnknown @7;
  limit @8;
}

struct QueryOperation {
  cardId @0 :Text;
  operationId @1 :Text;
}
struct OperationResult {
  cardId @0 :Text;
  operationId @1 :Text;
  union {
    # Absence is only the current scoped snapshot, never proof of no in-flight commit.
    absentSnapshot @2 :Void;
    locallyCommitted @3 :CommitReceipt;
  }
}

struct ReadAttachment {
  cardId @0 :Text;
  attachmentId @1 :Text;
  expectedRevision @2 :UInt64;
  offset @3 :UInt64;
  length @4 :UInt32;
}
struct AttachmentChunk {
  cardId @0 :Text;
  attachmentId @1 :Text;
  revision @2 :UInt64;
  offset @3 :UInt64;
  totalLength @4 :UInt64;
  contentSha256 @5 :Data;
  bytes @6 :Data;
}
