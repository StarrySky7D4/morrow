@0xb78e4904fc0a8d63;
# Inbound service v1. Principal is an authenticated host fact, never guest authority.
struct Header {
  name @0 :Text;
  value @1 :Data;
}
struct Request {
  version @0 :UInt16;
  schemaSha256 @1 :Data;
  callId @2 :UInt64;
  service @3 :Text;
  handler @4 :Text;
  principal @5 :Text;
  method @6 :Text;
  target @7 :Text;
  headers @8 :List(Header);
  body @9 :Data;
}
struct Response {
  version @0 :UInt16;
  schemaSha256 @1 :Data;
  callId @2 :UInt64;
  requestSha256 @3 :Data;
  status @4 :UInt16;
  headers @5 :List(Header);
  body @6 :Data;
}
