@0xbcd03e704b3e82a1;
# Separate from the frozen V1 workbench protocol. All operations are pure;
# the owner must bind the result to its original card/revision before committing.
enum Action { migrate @0; setCompletion @1; rename @2; reorder @3; setStage @4;
 completeAllAndSetStage @5; add @6; remove @7; project @8; }
struct Request {
 version @0 :UInt16; digest @1 :Data; action @2 :Action;
 cardId @3 :Text; title @4 :Text; revision @5 :UInt64;
 baseRevision @6 :UInt64; sourceDigest @7 :Data; properties @8 :Data;
 taskId @9 :Text; text @10 :Text; complete @11 :Bool; order @12 :List(Text);
}
struct Task { id @0 :Text; text @1 :Text; completion @2 :UInt16; }
struct Response {
 version @0 :UInt16; digest @1 :Data; properties @2 :Data;
 stage @3 :Text; complete @4 :UInt32; incomplete @5 :UInt32;
 ambiguous @6 :UInt32; tasks @7 :List(Task);
}
