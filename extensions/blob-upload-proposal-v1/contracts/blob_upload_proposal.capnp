@0xc2f288d85c55bcaa;
# Untrusted plugin data for a trusted operation review. No permission or Store.
# Exact bounded encoding; original task/upload/frame contracts remain unchanged.
struct Proposal {
  version @0 :UInt16;
  reserved @1 :UInt16;
  reserved2 @2 :UInt32;
  schemaSha256 @3 :Data;
  label @4 :Text;
  byteLength @5 :UInt64;
  wholeSha256 @6 :Data;
  producerInput @7 :Data;
}
# Trusted host assigns fresh frame identities only after reviewing the proposal.
# This envelope is correlation data, never evidence of approval or a live grant.
struct BoundInput {
  version @0 :UInt16;
  reserved @1 :UInt16;
  reserved2 @2 :UInt32;
  schemaSha256 @3 :Data;
  frameSchemaSha256 @4 :Data;
  proposalDigest @5 :Data;
  proposal @6 :Data;
  descriptor @7 :Data;
}
