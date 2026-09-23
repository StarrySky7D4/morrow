@0xdde8d56cce9278ea;
# First-party business contract, independently versioned from privileged runtime.
struct Asset {
 id @0 :Text; name @1 :Text; kind @2 :Text; bytes @3 :UInt64;
}
struct Idea {
 id @0 :Text; title @1 :Text; description @2 :Text; category @3 :Text;
 stage @4 :Text; hypothesis @5 :Text; conclusion @6 :Text;
 favorite @7 :Bool; todos @8 :List(Text); completed @9 :List(Text);
 assets @10 :List(Asset); icon @11 :UInt16; color @12 :UInt32;
 deleted @13 :Bool; deletedAt @14 :UInt64;
}
enum Action {
 create @0; edit @1; favorite @2; todo @3; stage @4; toProject @5;
 delete @6; restore @7; query @8;
}
struct Request {
 version @0 :UInt16; digest @1 :Data; action @2 :Action;
 current @3 :Idea; proposed @4 :Idea;
 text @5 :Text; flag @6 :Bool; nowMs @7 :UInt64;
 ideas @8 :List(Idea); section @9 :Text; filter @10 :Text; sort @11 :Text;
}
struct Response {
 version @0 :UInt16; digest @1 :Data; idea @2 :Idea; ids @3 :List(Text);
}
