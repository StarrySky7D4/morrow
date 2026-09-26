@0xe876c94d3a125b09;
# Opt-in service-resources-v1 metadata. References never confer authority.
struct Endpoint {
  reference @0 :Text;
  credential @1 :Data;
  methods @2 :List(Text);
  maxRequestBytes @3 :UInt64;
  maxResponseBytes @4 :UInt64;
  timeoutMs @5 :UInt64;
  responseFrameLimit @6 :UInt64;
}
struct Directory {
  version @0 :UInt16;
  schemaSha256 @1 :Data;
  scopeSha256 @2 :Data;
  endpoints @3 :List(Endpoint);
}
