@0xcfec678aba212219;
# Frozen cards-edit v2 guest boundary. Each request is a pure projection.
enum Action { edit @0; setFavorite @1; setCategory @2; delete @3; restore @4; }
struct Asset { id @0 :Text; name @1 :Text; kind @2 :Text; bytes @3 :UInt64; }
struct Request {
 version @0 :UInt16;
 digest @1 :Data;
 action @2 :Action;
 cardId @3 :Text;
 title @4 :Text;
 properties @5 :Data;
 editTitle @6 :Text;
 description @7 :Text;
 hypothesis @8 :Text;
 conclusion @9 :Text;
 icon @10 :UInt16;
 color @11 :UInt32;
 assets @12 :List(Asset);
 favorite @13 :Bool;
 category @14 :Text;
 stage @15 :Text;
 nowMs @16 :UInt64;
}
struct Response {
 version @0 :UInt16;
 digest @1 :Data;
 title @2 :Text;
 properties @3 :Data;
}
