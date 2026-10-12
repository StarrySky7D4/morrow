@0xefd6bdf04e961a31;
# Private trusted UI profile. No authority, admission, claim, endpoint or Start.
enum Operation { unsupported @0; attach @1; open @2; submit @3; status @4; read @5; markLost @6; detach @7; admitWriter @8; closeWriter @9; }
enum Action { unsupported @0; list @1; create @2; snapshot @3; openWriter @4; append @5; }
enum Status { unsupported @0; unavailable @1; busy @2; invalid @3; unknown @4; limit @5; attached @6; submitted @7; pending @8; opened @9; reply @10; observed @11; detached @12; writerAdmitted @13; }
enum ResultKind { none @0; session @1; sessions @2; snapshot @3; rejected @4; }
struct Event { eventId @0 :Text; body @1 :Data; }
struct StoredEvent { sequence @0 :UInt64; eventId @1 :Text; body @2 :Data; }
struct SessionInfo {
 sessionId @0 :Text; parent @1 :Text; parentTail @2 :UInt64;
 epoch @3 :UInt64; tail @4 :UInt64; floor @5 :UInt64; archived @6 :Bool;
 revision @7 :UInt64; checkpointTail @8 :UInt64; checkpointSha256 @9 :Data;
 checkpointSealed @10 :Bool; parentCheckpointSha256 @11 :Data;
}
struct Request {
 version @0 :UInt16; digest @1 :Data; id @2 :Data; task @3 :Data;
 attachment @4 :UInt64; operation @5 :Operation; submission @6 :UInt64;
 writerSlot @7 :UInt64; generation @8 :UInt64; action @9 :Action;
 sessionId @10 :Text; parent @11 :Text; parentTail @12 :UInt64;
 after @13 :UInt64; limit @14 :UInt32; epoch @15 :UInt64;
 expectedTail @16 :UInt64; events @17 :List(Event);
}
struct Reply {
 version @0 :UInt16; digest @1 :Data; id @2 :Data; task @3 :Data;
 attachment @4 :UInt64; operation @5 :Operation; requestSha256 @6 :Data;
 status @7 :Status; submission @8 :UInt64; writerSlot @9 :UInt64;
 generation @10 :UInt64; leaseLive @11 :Bool; deliveryUnknown @12 :Bool;
 canonicalReply @13 :Data; canonicalRequestSha256 @14 :Data;
 resultKind @15 :ResultKind; rejectionCode @16 :UInt16;
 sessions @17 :List(SessionInfo); events @18 :List(StoredEvent);
 gap @19 :Bool; checkpoint @20 :Data;
}
