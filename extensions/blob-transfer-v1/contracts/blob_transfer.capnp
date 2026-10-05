@0xf2b61e7ad7349c05;
# Independent bounded byte-transfer metadata. No storage/import/grant authority.
enum ReceiptStatus { accepted @0; existing @1; }
struct Descriptor { totalLength @0 :UInt64; wholeSha256 @1 :Data; }
struct Chunk { sequence @0 :UInt64; offset @1 :UInt64; payload @2 :Data; chunkSha256 @3 :Data; }
struct Receipt { requestDigest @0 :Data; sequence @1 :UInt64; offset @2 :UInt64; length @3 :UInt64; chunkSha256 @4 :Data; status @5 :ReceiptStatus; }
struct End { totalLength @0 :UInt64; wholeSha256 @1 :Data; }
struct Frame {
  version @0 :UInt16;
  schemaSha256 @1 :Data;
  transferEpoch @2 :Data;
  objectRef @3 :Data;
  operationId @4 :Data;
  union { descriptor @5 :Descriptor; chunk @6 :Chunk; receipt @7 :Receipt; end @8 :End; }
}
