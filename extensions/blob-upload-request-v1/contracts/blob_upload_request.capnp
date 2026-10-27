@0xedb509929d84d0d4;
# Independent upload request candidate. A nomination is a name, never authority.
# Exact layouts; old task/channel/blob frame contracts are unchanged.
# No path, Store identifier, native handle, grant, clock or caller-selected lease.
struct Request {
  version @0 :UInt16;
  action @1 :UInt16;
  reserved @2 :UInt32;
  schemaSha256 @3 :Data;
  frameSchemaSha256 @4 :Data;
  nominationRef @5 :Data;
  nonce @6 :Data;
  frame @7 :Data;
}
struct Response {
  version @0 :UInt16;
  action @1 :UInt16;
  kind @2 :UInt16;
  status @3 :UInt16;
  schemaSha256 @4 :Data;
  frameSchemaSha256 @5 :Data;
  nominationRef @6 :Data;
  nonce @7 :Data;
  requestDigest @8 :Data;
  transferEpoch @9 :Data;
  objectRef @10 :Data;
  operationId @11 :Data;
  wholeSha256 @12 :Data;
  byteLength @13 :UInt64;
  receivedBytes @14 :UInt64;
  nextSequence @15 :UInt64;
  phase @16 :UInt16;
  reserved @17 :UInt16;
  reserved2 @18 :UInt32;
  receipt @19 :Data;
}
