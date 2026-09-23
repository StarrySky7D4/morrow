@0xd4a692fd706c815e;
# Fixed dependency call v1. Names and bytes carry no host authority.
# The trusted host resolves the declared slot against its current approved registry.
struct Request {
  callId @0 :Text;
  slot @1 :Text;
  input @2 :Data; # 1..65536 bytes; current frozen shared objects cannot be empty.
}
struct Response {
  callId @0 :Text;
  requestSha256 @1 :Data; # SHA-256 of the complete original serialized request frame.
  outputType @2 :Text;
  output @3 :Data; # 0..65536 bytes. Empty is a valid successful output, not an error.
}
struct Message {
  version @0 :UInt16;
  schemaDigest @1 :Data; # SHA-256 of this schema after CRLF -> LF normalization.
  union {
    request @2 :Request;
    response @3 :Response;
  }
}
