@0xcb930c9ce39482f1;
# Sole native-session wire authority, major2 revision1 (withdrawn non-Capnp candidate is not revision history).
# Transport: uint32 little-endian byte length, then one unpacked Cap'n Proto message.
# Payload 8..2048 bytes, multiple of8. No packed encoding, trailing bytes or second message.
# Private anonymous pipes: child stdin=host, stdout=client; stderr diagnostics, not protocol.
enum Kind {
  challenge @0; hello @1; welcome @2; query @3;
  state @4; denied @5; stop @6; close @7;
}
struct NativeSession {
  major @0 :UInt16;
  revision @1 :UInt16;
  kind @2 :Kind;
  sequence @3 :UInt64; # Challenge/Stop=0; Hello=1, client then strictly +1. Replies echo request.
  session @4 :UInt64; # Host issued, nonzero.
  instanceEpoch @5 :UInt64;
  revocationGeneration @6 :UInt64; # Host starts1, revoke2; never reset by retry.
  childPid @7 :UInt32; # Host-observed launch PID; client echo is not authentication.
  code @8 :UInt32; # Requests0; Welcome/State phase; Denied/Stop error.
  remainingMs @9 :UInt64; # Host monotonic observation; request echoes initial Challenge.
  nonce @10 :Data; # Exactly32 host-random bytes, scoped to inherited child pipe endpoints.
  schemaSha256 @11 :Data; # Exactly32, SHA256 raw bytes of this schema.
  artifactSha256 @12 :Data; # Exactly32, trusted prelaunch hash, image-load race remains.
  executionConfigSha256 @13 :Data; # Exactly32, host calculated launch binding.
  requestBudget @14 :UInt64; # Request echoes initial Challenge; response reports remaining.
  capabilities @15 :UInt64; # Only bit0 read own session supported. No data-plane capability.
  reserved @16 :UInt64; # Must be0.
}
# Hello: exact Challenge echo except kind=hello, sequence1, code0.
# Query/Close: initial Challenge echo except kind and strictly increasing sequence, code0.
# Host checks authority independently; no pluginId/approved field grants anything.
# Close -> host Stop(seq0,code25), closes child stdin, bounded exit/EOF observation.
# Trusted revoke -> generation2 acknowledgement outside client queue, future requests Denied19.
# Revocation does not renew TTL or grant scope. Partial cancelled response closes channel.
# Phases: Preparing1 Active2 Revoked3 Closing4 ClosingUnconfirmed5 Released6.
# Errors: Protocol16 Identity17 Sequence18 Revoked19 Expired20 Quota21 Unsupported22 Handshake23 Disconnected24 HostStop25.
# Unsupported auth/content/network/writer/execution never fall back to a default backend.
