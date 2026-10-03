@0xc1a5e7db03a5b193;
# Optional native WS payload v1. This is not a new base SDK import or permission.
# The original channel remains the owner/credit/ACK carrier for these bytes.
enum Kind {
  text @0;
  binary @1;
  ping @2;
  pong @3;
  close @4;
}
struct WsMessage {
  version @0 :UInt16;
  schemaSha256 @1 :Data;
  kind @2 :Kind;
  payload @3 :Data;
  hasCloseCode @4 :Bool;
  closeCode @5 :UInt16;
}
