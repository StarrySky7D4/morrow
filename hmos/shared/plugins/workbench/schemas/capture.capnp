@0xbda8896c5fddabf9;
# Host parses platform HTML/XML into an inert tree. The guest owns conversion.
struct Node { parent @0 :UInt16; tag @1 :Text; text @2 :Text; href @3 :Text; src @4 :Text; alt @5 :Text; style @6 :Text; columns @7 :UInt16; rows @8 :UInt16; index @9 :UInt16; formula @10 :Text; }
struct Request { version @0 :UInt16; digest @1 :Data; format @2 :Text; source @3 :Text; nodes @4 :List(Node); }
struct Response { version @0 :UInt16; digest @1 :Data; markdown @2 :Text; warnings @3 :List(Text); }
