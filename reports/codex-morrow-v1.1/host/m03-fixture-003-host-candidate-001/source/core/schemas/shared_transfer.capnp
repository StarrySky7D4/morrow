@0x9e7b98eea3ecf6de;
# Private trusted parent/child bootstrap v1. No field is a host capability.
# The handle value only identifies a pre-delivered read-only handle in the intended child.
struct Offer {
  transfer @0 :UInt64;
  remoteHandle @1 :UInt64;
  descriptor @2 :Data; # Fixed shared_object v1 message, independently checked.
}
struct Reply {
  transfer @0 :UInt64;
  descriptor @1 :Data;
  payload @2 :Data; # Qualification copies at most 64 KiB; not content commit data.
  writeRejected @3 :Bool; # Observed result, not an authority or security guarantee.
}
struct Message {
  version @0 :UInt16;
  schemaDigest @1 :Data;
  union {
    offer @2 :Offer;
    reply @3 :Reply;
  }
}
