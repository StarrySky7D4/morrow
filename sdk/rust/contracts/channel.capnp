@0xe8c75af428314b92;
# Independently versioned channel-v1. All IDs/epochs are 32 bytes; no reference grants authority.
enum Kind { byteStream @0; events @1; }
enum Status {
  ready @0; frame @1; acked @2; accepted @3; idle @4; closed @5;
  closingUnconfirmed @6; revoked @7; expired @8; limit @9;
  invalid @10; unknown @11; unsupported @12;
}
struct Budget {
  maxChannels @0 :UInt32;
  maxFrameBytes @1 :UInt32;
  maxBytes @2 :UInt64;
  maxMessages @3 :UInt64;
  maxRequests @4 :UInt64;
  maxDurationMs @5 :UInt64;
}
struct Endpoint {
  reference @0 :Data;
  sourceEpoch @1 :Data;
  kind @2 :Kind;
  budget @3 :Budget;
}
struct Directory {
  version @0 :UInt32;
  schemaSha256 @1 :Data;
  scopeSha256 @2 :Data;
  channels @3 :List(Endpoint);
}
struct Frame {
  sequence @0 :UInt64;
  sourceEpoch @1 :Data;
  bytes @2 :Data;
  cursor @3 :Data;
}
struct Receive { lastAcked @0 :UInt64; creditBytes @1 :UInt32; }
struct Ack { sequence @0 :UInt64; frameSha256 @1 :Data; cursor @2 :Data; }
struct Send { sequence @0 :UInt64; bytes @1 :Data; }
struct Request {
  version @0 :UInt32;
  schemaSha256 @1 :Data;
  callId @2 :Data;
  reference @3 :Data;
  sourceEpoch @4 :Data;
  union { receive @5 :Receive; ack @6 :Ack; send @7 :Send; close @8 :Void; query @9 :Void; }
}
struct Response {
  version @0 :UInt32;
  schemaSha256 @1 :Data;
  callId @2 :Data;
  requestSha256 @3 :Data;
  reference @4 :Data;
  sourceEpoch @5 :Data;
  status @6 :Status;
  frame @7 :Frame;
  lastAcked @8 :UInt64;
  acceptedSequence @9 :UInt64;
  # Only the owning runtime's actual resource reaper can assert this.
  resourceReclaimed @10 :Bool;
}
