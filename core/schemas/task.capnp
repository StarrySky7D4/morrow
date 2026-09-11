@0xc37bfe813a120b94;
# Runtime task profile v1. IDs are correlation only; authority is bound by the host.
struct Invocation {
  version @0 :UInt16;
  schemaDigest @1 :Data;
  taskId @2 :Text;
  command @3 :Data; # Frozen runtime v6 request, not a grant.
}
struct Completion {
  version @0 :UInt16;
  schemaDigest @1 :Data;
  taskId @2 :Text;
  inputDigest @3 :Data; # SHA-256 of exact invocation bytes.
  response @4 :Data; # Must match the actual core response for this task.
}
