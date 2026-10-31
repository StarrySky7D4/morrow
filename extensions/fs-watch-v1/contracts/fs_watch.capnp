@0xf114ca43af66b51f;
# Independent finite directory-change observations. Names are not authority.
# Rename old/new names remain separate observations, not a mutation receipt.
# A terminal batch or its ACK is not evidence of native resource closure.

struct Change {
  kind @0 :UInt16;
  reserved @1 :UInt16;
  reserved2 @2 :UInt32;
  nameUtf16Le @3 :Data;
}

struct Batch {
  version @0 :UInt16;
  end @1 :UInt16;
  reserved @2 :UInt32;
  sequence @3 :UInt64;
  schemaSha256 @4 :Data;
  watchEpoch @5 :Data;
  changes @6 :List(Change);
}

