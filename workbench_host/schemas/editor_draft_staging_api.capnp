@0xc50edbd4ec19a483;
# Private trusted UI channel. Paths are transport-only, never persisted proposals.
struct ImportRequest {
  version @0 :UInt16; digest @1 :Data;
  cardId @2 :Text; draftId @3 :Text; operation @4 :Text;
  expectedGeneration @5 :UInt64;
  name @6 :Text; kind @7 :Text; bytes @8 :UInt64; sha256 @9 :Data;
}
enum Phase { pending @0; ready @1; retired @2; }
struct Record {
  request @0 :ImportRequest; assetId @1 :Text; phase @2 :Phase;
  currentActive @3 :Bool; bytesRetained @4 :Bool; stagingRevision @5 :UInt64;
  repeated @6 :Bool; currentGeneration @7 :UInt64; mainActive @8 :Bool;
}
enum ResultKind { absent @0; record @1; list @2; exported @3; reconciled @4; decision @5; decisions @6; scopes @7; }
struct Envelope {
  version @0 :UInt16; digest @1 :Data; kind @2 :ResultKind;
  cardId @3 :Text; draftId @4 :Text; operation @5 :Text;
  expectedGeneration @6 :UInt64; importOperation @7 :Text;
  currentGeneration @8 :UInt64; mainActive @9 :Bool; stagingRevision @10 :UInt64;
  record @11 :Record; records @12 :List(Record);
  exportSha256 @13 :Data; exportBytes @14 :UInt64;
  decision @15 :Decision; decisions @16 :List(Decision);
  nextCursor @17 :Text; requestCursor @18 :Text; requestLimit @19 :UInt32;
  scopes @20 :List(Scope);
}

# A decision is durable metadata; neither reading nor restoring it executes it.
enum DecisionStatus { pending @0; committed @1; cancelled @2; conflict @3; }
struct Decision {
  request @0 :ImportRequest;
  operation @1 :Text;
  expectedGeneration @2 :UInt64;
  status @3 :DecisionStatus;
  currentGeneration @4 :UInt64;
  mainActive @5 :Bool;
  stagingRevision @6 :UInt64;
  decisionRevision @7 :UInt64;
  committedRevision @8 :UInt64;
}

# Global recovery discovery includes inactive draft identities.
struct Scope { cardId @0 :Text; draftId @1 :Text; }
