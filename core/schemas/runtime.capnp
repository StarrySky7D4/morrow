@0xbfd1347ca4879e62;
# Versioned internal messages, not a persistence container or capability grant.
struct Request {
  protocolVersion @0 :UInt16;
  operationId @1 :Text;
  runtimeDigest @4 :Data;
  contentDigest @5 :Data;
  union {
    unsupported @2 :Void;
    renameCard @3 :RenameCard;
  }
}
struct RenameCard {
  cardId @0 :Text;
  expectedRevision @1 :UInt64;
  title @2 :Text;
}
# Summaries are a read projection. They must never replace a stored Card.
struct CardSummary {
  protocolVersion @0 :UInt16;
  cardId @1 :Text;
  typeId @2 :Text;
  formatVersion @3 :UInt32;
  revision @4 :UInt64;
  title @5 :Text;
  previewText @6 :Text;
}
