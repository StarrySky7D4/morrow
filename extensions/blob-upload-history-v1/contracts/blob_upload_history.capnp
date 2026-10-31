@0xa8d3af6aebc7b32d;

# Read-only, explicitly selected historical data. No live owner or retry token.
enum Outcome {
  notObserved @0;
  absent @1;
  staged @2;
  released @3;
  error @4;
}
enum Execution {
  notObserved @0;
  returned @1;
  rejected @2;
}
enum Stage {
  absent @0;
  staged @1;
  released @2;
}
struct Packet {
  version @0 :UInt16;
  schemaSha256 @1 :Data;
  queryNonce @2 :Data;
  submission @3 :Data;
  packageId @4 :Text;
  packageDigest @5 :Data;
  registryRevision @6 :UInt64;
  operationId @7 :Text;
  byteLength @8 :UInt64;
  wholeSha256 @9 :Data;
  handler @10 :Text;
  inputSha256 @11 :Data;
  requestBindingSha256 @12 :Data;
  reservedAt @13 :Int64;
  revision @14 :UInt16;
  outcome @15 :Outcome;
  errorCode @16 :UInt16;
  originalUnknown @17 :Bool;
  calls @18 :UInt32;
  wireBytes @19 :UInt64;
  nativeWorkBytes @20 :UInt64;
  execution @21 :Execution;
  exitCode @22 :Int32;
  executionError @23 :UInt16;
  hostCalls @24 :UInt32;
  observedAt @25 :Int64;
  acknowledgedAt @26 :Int64;
  currentStage @27 :Stage;
}
