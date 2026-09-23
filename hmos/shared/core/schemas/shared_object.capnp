@0xc37ec9ea2a338ff5;
# Runtime-only immutable object metadata v1. This descriptor never conveys authority.
# Segment offsets refer to logical object bytes, not addresses or mapping offsets.
struct Segment {
  offset @0 :UInt64;
  length @1 :UInt64;
}
struct Descriptor {
  version @0 :UInt16;
  schemaDigest @1 :Data;
  arena @2 :UInt64;
  object @3 :UInt64;
  generation @4 :UInt64;
  length @5 :UInt64;
  sha256 @6 :Data;
  segments @7 :List(Segment);
}
