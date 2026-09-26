@0xaac5af872ec1d917;
# Experimental IO v1 contract. Admission is not a resource grant or backend support.
# Exact schema identity is required; this is not a frozen SDK compatibility profile.
# References are opaque host-issued bytes, scoped to the real instance and generation.
struct ReadChunk {
  reference @0 :Data;
  offset @1 :UInt64;
  limit @2 :UInt32;
}
struct WriteChunk {
  reference @0 :Data;
  offset @1 :UInt64;
  bytes @2 :Data;
}
struct Header {
  name @0 :Text;
  value @1 :Data;
}
struct HttpRequest {
  endpoint @0 :Data;
  method @1 :Text;
  relativeTarget @2 :Text;
  headers @3 :List(Header);
  body @4 :Data;
  credential @5 :Data;
}
struct FileMutation {
  target @0 :Data;
  name @1 :Text;
  expectedIdentity @2 :Data;
}
struct Publication {
  listener @0 :Data;
  handler @1 :Text;
  methods @2 :List(Text);
  path @3 :Text;
  remotePolicy @4 :Data;
}
struct ServiceReply {
  request @0 :Data;
  status @1 :UInt16;
  headers @2 :List(Header);
  body @3 :Data;
}
struct Submission {
  # Stable operation identity, not a request/call ID or authorization handle.
  operationId @0 :Data;
  deadlineMs @1 :UInt64;
  union {
    fileRead @2 :Data;
    fileList @3 :Data;
    fileCreate @4 :FileMutation;
    fileReplace @5 :FileMutation;
    fileDelete @6 :FileMutation;
    httpRequest @7 :HttpRequest;
    httpListen @8 :Data;
    httpPublish @9 :Publication;
    httpUnpublish @10 :Data;
    webSocketConnect @11 :HttpRequest;
    serviceAccept @12 :Data;
    serviceReply @13 :ServiceReply;
  }
}
struct Request {
  version @0 :UInt16;
  schemaSha256 @1 :Data;
  callId @2 :UInt64;
  union {
    submit @3 :Submission;
    poll @4 :Data;
    read @5 :ReadChunk;
    write @6 :WriteChunk;
    finish @7 :Data;
    cancel @8 :Data;
    queryOperation @9 :Data;
  }
}
enum Status {
  invalid @0;
  accepted @1;
  pending @2;
  completed @3;
  denied @4;
  revoked @5;
  expired @6;
  unsupported @7;
  quota @8;
  notFound @9;
  conflict @10;
  cancelled @11;
  outcomeUnknown @12;
  evidenceUnavailable @13;
  failed @14;
}
struct Response {
  version @0 :UInt16;
  schemaSha256 @1 :Data;
  callId @2 :UInt64;
  requestSha256 @3 :Data;
  status @4 :Status;
  reference @5 :Data;
  bytes @6 :Data;
  offset @7 :UInt64;
  eof @8 :Bool;
  httpStatus @9 :UInt16;
  headers @10 :List(Header);
}
