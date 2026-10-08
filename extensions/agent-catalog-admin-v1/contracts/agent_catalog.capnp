@0xf23adfe79a10c15a;

# Private native UI management. No Core object, grant or live handle crosses this wire.
enum Action {
  state @0;
  inspect @1;
  install @2;
  baseSelect @3;
  baseEnable @4;
  wrapperSelect @5;
  approve @6;
  wrapperEnable @7;
  remove @8;
  page @9;
}
enum Status {
  ok @0;
  invalid @1;
  conflict @2;
  denied @3;
  notFound @4;
  limit @5;
  storage @6;
  unknown @7;
  busy @8;
  recoveryRequired @9;
  ownerUnavailable @10;
  unsupported @11;
}
enum BodyKind { none @0; review @1; page @2; }
struct Revisions { catalog @0 :UInt64; manager @1 :UInt64; }
struct Approval {
  sessionBits @0 :UInt16;
  processBits @1 :UInt16;
  sessions @2 :List(Text);
  domain @3 :Text;
}
struct Review {
  id @0 :Text;
  version @1 :Text;
  fullSha256 @2 :Data;
  baseSha256 @3 :Data;
  sessionSchema @4 :Data;
  processSchema @5 :Data;
  sessionBits @6 :UInt16;
  processBits @7 :UInt16;
  sessions @8 :List(Text);
  domain @9 :Text;
}
struct Entry {
  review @0 :Review;
  selected @1 :Bool;
  enabled @2 :Bool;
  approval @3 :Approval;
  baseSelected @4 :Bool;
  baseEnabled @5 :Bool;
}
struct Request {
  version @0 :UInt16;
  digest @1 :Data;
  action @2 :Action;
  id @3 :Data;
  revisions @4 :Revisions;
  path @5 :Text;
  packageId @6 :Text;
  fullSha256 @7 :Data;
  enabled @8 :Bool;
  approval @9 :Approval;
  cursor @10 :Text;
  limit @11 :UInt16;
}
struct Reply {
  version @0 :UInt16;
  digest @1 :Data;
  id @2 :Data;
  requestSha256 @3 :Data;
  action @4 :Action;
  status @5 :Status;
  revisions @6 :Revisions;
  kind @7 :BodyKind;
  review @8 :Review;
  entries @9 :List(Entry);
  next @10 :Text;
}
