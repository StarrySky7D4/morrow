@0xc6bd9374f1a28e05;
# Independent directory observation codec. No path, authority or traversal grant.
# Shared immutable Data pointers are allowed; aggregate bounds count every field.
enum NameEncoding { unknown @0; utf8 @1; utf16Le @2; }
enum EntryKind { unknown @0; file @1; directory @2; other @3; }
struct Entry {
  entryId @0 :Data;
  name @1 :Data;
  encoding @2 :NameEncoding;
  kind @3 :EntryKind;
  hasLogicalLength @4 :Bool;
  logicalLength @5 :UInt64;
}
struct Page {
  version @0 :UInt16;
  schemaSha256 @1 :Data;
  selectionEpoch @2 :Data;
  pageSequence @3 :UInt64;
  entries @4 :List(Entry);
  terminal @5 :Bool;
}
