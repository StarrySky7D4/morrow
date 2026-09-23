@0x9cf064981b7f23a1;
# Frozen mixed V1/V2 query boundary. Pure filtering or sorting; no writes.
enum Action { filter @0; sort @1; }
struct Conditions {
 section @0 :Text; filter @1 :Text; text @2 :Text; sort @3 :Text;
}
struct Candidate {
 id @0 :Text; title @1 :Text; formatVersion @2 :UInt32; properties @3 :Data;
}
struct SortKey {
 id @0 :Text; title @1 :Text; favorite @2 :Bool;
}
struct Request {
 version @0 :UInt16; digest @1 :Data; action @2 :Action;
 conditions @3 :Conditions; candidates @4 :List(Candidate);
 sort @5 :Text; keys @6 :List(SortKey);
}
struct Response {
 version @0 :UInt16; digest @1 :Data; ids @2 :List(Text);
}
