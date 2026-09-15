@0xb7e41c09a6d02f13;
# IO runtime frame v1. References are host-issued; guests cannot construct grants.
# Schema identity is SHA-256 of this file after CRLF -> LF normalization.

enum Kind {
  fileRead @0;
  fileList @1;
  fileCreate @2;
  fileReplace @3;
  fileDelete @4;
  httpGet @5;
  httpSend @6;
  credentialUse @7;
}

enum Status {
  ok @0;
  denied @1;
  revoked @2;
  expired @3;
  unsupported @4;
  invalidPath @5;
  quota @6;
  notFound @7;
  conflict @8;
  pending @9;
  cancelled @10;
  outcomeUnknown @11;
  evidenceUnavailable @12;
  unsupportedPlatform @13;
}

struct Request {
  version @0 :UInt16;
  schemaDigest @1 :Data;
  callId @2 :Text;
  resourceRef @3 :Data; # Host random token. Guessing it confers no authority.
  kind @4 :Kind;
  offset @5 :UInt64;
  length @6 :UInt32; # 0 means "use declared default chunk", never "unbounded".
  path @7 :Text; # Relative UTF-8 path for directory mode only; empty for selected files.
}

struct Response {
  version @0 :UInt16;
  schemaDigest @1 :Data;
  callId @2 :Text;
  requestSha256 @3 :Data;
  status @4 :Status;
  eof @5 :Bool;
  payload @6 :Data; # 0..65536. Status != ok must use empty or a short diagnostic.
}

struct Message {
  version @0 :UInt16;
  schemaDigest @1 :Data;
  union {
    request @2 :Request;
    response @3 :Response;
  }
}
