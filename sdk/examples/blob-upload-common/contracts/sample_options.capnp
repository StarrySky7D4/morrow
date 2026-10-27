@0x9737ba9bb33a408f;
# Example business input, not an SDK permission or source-selection contract.
struct Options {
  version @0 :UInt16;
  fill @1 :UInt8;
  reserved @2 :UInt8;
  reserved2 @3 :UInt32;
  byteLength @4 :UInt64;
  schemaSha256 @5 :Data;
}
