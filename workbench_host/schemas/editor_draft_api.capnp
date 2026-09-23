@0xc6a2a773ba351ddd;
# Trusted desktop editor draft wire format. Persistence is host-owned protobuf.
struct TextValue {
  text @0 :Text;
  selectionBase @1 :Int32;
  selectionExtent @2 :Int32;
  affinity @3 :UInt16;
  directional @4 :Bool;
  composingStart @5 :Int32;
  composingEnd @6 :Int32;
}
struct Values {
  title @0 :TextValue;
  description @1 :TextValue;
  hypothesis @2 :TextValue;
  conclusion @3 :TextValue;
  todos @4 :TextValue;
  category @5 :Text;
  stage @6 :Text;
}
struct AssetSelection {
  origin @0 :UInt16;
  assetId @1 :Text;
  aliases @2 :List(Text);
}
struct WriteRequest {
  version @0 :UInt16;
  digest @1 :Data;
  cardId @2 :Text;
  draftId @3 :Text;
  operation @4 :Text;
  expectedGeneration @5 :UInt64;
  sourceRevision @6 :UInt64;
  predecessorOperation @7 :Text;
  predecessorDigest @8 :Data;
  values @9 :Values;
  assets @10 :List(AssetSelection);
  sourceKind @11 :UInt16;
}
struct ParentLink {
  parentDraftId @0 :Text;
  parentGeneration @1 :UInt64;
  parentSaveOperation @2 :Text;
  parentRequestSha256 @3 :Data;
  committedOperation @4 :Text;
  committedSha256 @5 :Data;
  childOperation @6 :Text;
}
struct ParentRetirement {
  childDraftId @0 :Text;
  childOperation @1 :Text;
  operation @2 :Text;
  parentGeneration @3 :UInt64;
}
struct HandoffRequest {
  version @0 :UInt16;
  digest @1 :Data;
  request @2 :WriteRequest;
  parentLink @3 :ParentLink;
}
struct StoredAsset {
  selection @0 :AssetSelection;
  name @1 :Text;
  mediaType @2 :Text;
  bytes @3 :UInt64;
  sha256 @4 :Data;
}
struct Record {
  request @0 :WriteRequest;
  generation @1 :UInt64;
  active @2 :Bool;
  currentGeneration @3 :UInt64;
  currentActive @4 :Bool;
  repeated @5 :Bool;
  sourceFormat @6 :UInt32;
  sourceRevision @7 :UInt64;
  sourceSha256 @8 :Data;
  predecessorRevision @9 :UInt64;
  predecessorSha256 @10 :Data;
  assets @11 :List(StoredAsset);
  parentLink @12 :ParentLink;
  retirement @13 :ParentRetirement;
  requestSha256 @14 :Data;
}
struct Summary {
  cardId @0 :Text;
  draftId @1 :Text;
  generation @2 :UInt64;
  active @3 :Bool;
}
struct Lineage {
  cardId @0 :Text;
  childDraftId @1 :Text;
  childGeneration @2 :UInt64;
  childActive @3 :Bool;
  parentGeneration @4 :UInt64;
  parentActive @5 :Bool;
  parentLink @6 :ParentLink;
  cursor @7 :Text;
}
struct ImportedAsset {
  id @0 :Text;
  name @1 :Text;
  kind @2 :Text;
  bytes @3 :UInt64;
}
enum ResultKind { absent @0; record @1; list @2; imported @3; exported @4; lineages @5; }
struct Envelope {
  version @0 :UInt16;
  digest @1 :Data;
  kind @2 :ResultKind;
  cardId @3 :Text;
  draftId @4 :Text;
  operation @5 :Text;
  expectedGeneration @6 :UInt64;
  record @7 :Record;
  summaries @8 :List(Summary);
  asset @9 :ImportedAsset;
  lineages @10 :List(Lineage);
  nextCursor @11 :Text;
  requestCursor @12 :Text;
  requestLimit @13 :UInt32;
}
