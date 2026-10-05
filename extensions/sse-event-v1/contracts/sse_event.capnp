@0xce8d74f331a560c9;
# Independent trusted source payload, carried in the existing opaque channel frame.
# provider id and retry are metadata only; no resume/reconnect authority is encoded.
struct Event {
  version @0 :UInt16;
  schemaSha256 @1 :Data;
  data @2 :Text;
  event @3 :Text;
  id @4 :Text;
  hasRetry @5 :Bool;
  retry @6 :UInt64;
}
