@0xe24d37e98ab14c61;
# Independent directory request v1. References are host-issued names, not grants.
# No path, native handle, DirectorySession serial, authority boolean or clock.
# Flat exact layouts: future fields require a new protocol identity.
struct Request {
  version @0 :UInt16;
  action @1 :UInt16;
  reserved @2 :UInt32;
  schemaSha256 @3 :Data;
  nominationRef @4 :Data;
  nonce @5 :Data;
  selectionEpoch @6 :Data;
  pageSequence @7 :UInt64;
  afterEntryId @8 :Data;
}
struct Response {
  version @0 :UInt16;
  action @1 :UInt16;
  status @2 :UInt16;
  pageVersion @3 :UInt16;
  schemaSha256 @4 :Data;
  pageSchemaSha256 @5 :Data;
  nominationRef @6 :Data;
  nonce @7 :Data;
  requestDigest @8 :Data;
  selectionEpoch @9 :Data;
  pageSequence @10 :UInt64;
  entries @11 :UInt32;
  reserved @12 :UInt32;
  metadataBytes @13 :UInt64;
  page @14 :Data;
}
