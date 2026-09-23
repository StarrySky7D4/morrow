@0x953e78ccbe0e8474;
# Runtime UI contract v1. Descriptions and action IDs confer no authority.
enum Kind { column @0; row @1; text @2; button @3; textInput @4; toggle @5; }
enum Tone { normal @0; muted @1; emphasis @2; }
enum EventKind { activate @0; editText @1; setToggle @2; }
struct Node {
  id @0 :Text;
  parent @1 :Text;
  kind @2 :Kind;
  label @3 :Text;
  text @4 :Text;
  action @5 :Text;
  enabled @6 :Bool = true;
  checked @7 :Bool;
  maxBytes @8 :UInt32;
  tone @9 :Tone;
}
struct Document { version @0 :UInt16; schemaDigest @1 :Data; nodes @2 :List(Node); }
struct Event {
  version @0 :UInt16;
  schemaDigest @1 :Data;
  view @2 :Text;
  generation @3 :UInt64;
  revision @4 :UInt64;
  serial @5 :UInt64;
  node @6 :Text;
  action @7 :Text;
  kind @8 :EventKind;
  text @9 :Text;
  checked @10 :Bool;
}
