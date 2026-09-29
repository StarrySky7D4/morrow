@0xe504b95058e15e18;
# SOLE AUTHORITATIVE SCHEMA. Experimental wire major 1, revision 1.
# Exact raw source SHA-256 is mandatory on every frame. No implicit defaults.
# Qualification transport only; no RPC capability here conveys OS authority.

enum ClockDomain { unspecified @0; monotonicMillis @1; unixMillis @2; }
struct Deadline { domain @0 :ClockDomain; clockId @1 :Text; value @2 :UInt64; }
struct ResourceRef {
  namespace @0 :Text;
  id @1 :Text;
  revision @2 :UInt64;
  byteLength @3 :UInt64;
  sha256 @4 :Data;
}
enum ErrorCode {
  unspecified @0; invalid @1; unsupportedVersion @2; schemaMismatch @3;
  denied @4; limit @5; conflict @6; staleEpoch @7; unavailable @8;
  commitUnknown @9; storageFull @10; storageBusy @11; integrity @12;
}
struct ErrorEnvelope {
  code @0 :ErrorCode;
  stage @1 :Text;
  operationId @2 :Text;
  attemptId @3 :Text;
  recovery @4 :Text;
  diagnosticId @5 :Text;
}
struct Identity {
  pluginId @0 :Text;
  packageDigest @1 :Data;
  artifactDigest @2 :Data;
  platform @3 :Text;
  accountEpoch @4 :UInt64;
}
struct NativeSession {
  union { hello @0 :Identity; drain @1 :Void; observeExit @2 :Void; }
}
struct StreamOpen {
  destination @0 :ResourceRef;
  requestBytes @1 :UInt64;
  requestDigest @2 :Data;
  deadline @3 :Deadline;
}
struct WriteChunk { offset @0 :UInt64; bytes @1 :Data; }
struct ReadChunk { expectedOffset @0 :UInt64; maxBytes @1 :UInt32; }
struct Stream {
  attemptId @0 :Text;
  union {
    open @1 :StreamOpen;
    writeChunk @2 :WriteChunk;
    commitRequest @3 :Void;
    read @4 :ReadChunk;
    inspect @5 :Void;
    cancel @6 :Void;
    close @7 :Void;
  }
}
struct AgentEvent {
  turnId @0 :Text;
  itemId @1 :Text;
  partId @2 :Text;
  attemptId @3 :Text;
  semanticKind @4 :Text; # Opaque plugin-owned label, never interpreted by host.
  sourceDigest @5 :Data;
  payload @6 :Data; # Qualification small inline payload, bounded to 4096 bytes.
}
struct AppendBatch {
  producerSequence @0 :UInt64;
  expectedTail @1 :UInt64;
  events @2 :List(AgentEvent);
}
struct Event {
  writerEpoch @0 :UInt64;
  union { openWriter @1 :Void; appendBatch @2 :AppendBatch; readAfter @3 :UInt64; }
}
struct ToolIntent {
  rootOperation @0 :Text;
  attemptId @1 :Text;
  inputDigest @2 :Data;
  toolSchemaDigest @3 :Data;
  executorArtifact @4 :Data;
  source @5 :ResourceRef;
}
enum ToolState { unspecified @0; proposed @1; approved @2; claimed @3; reported @4; unknown @5; }
struct ToolReport {
  state @0 :ToolState;
  exitCode @1 :Int32;
  outputClosed @2 :Bool;
  outputDigest @3 :Data;
}
struct Tool {
  operationId @0 :Text;
  union {
    propose @1 :ToolIntent;
    claim @2 :Data; # Test-only opaque fixture permit; not a production signature.
    report @3 :ToolReport;
    inspect @4 :Void;
  }
}
enum StreamState { unspecified @0; prepared @1; committed @2; streaming @3; eof @4; cancelled @5; closed @6; }
struct NativeReceipt {
  qualificationOnly @0 :Bool;
  capabilityBits @1 :UInt32; # 1 native, 2 stream, 4 event, 8 tool, only fake operations.
  closingUnconfirmed @2 :Bool;
}
struct StreamReceipt {
  state @0 :StreamState;
  transportSequence @1 :UInt64;
  offset @2 :UInt64;
  bytes @3 :Data;
  requestFixed @4 :Bool;
  # eof is a transport fact; it does not represent a model or agent terminal event.
}
struct EventReceipt {
  writerEpoch @0 :UInt64;
  durableSequence @1 :UInt64; # Simulated durability only; qualificationOnly must be true.
  replayed @2 :Bool;
  events @3 :List(AgentEvent);
}
struct ToolReceipt {
  state @0 :ToolState;
  execute @1 :Bool; # Only the first valid claim returns true; fake never spawns anything.
  exitCode @2 :Int32;
  outputClosed @3 :Bool;
}
struct Reply {
  qualificationOnly @0 :Bool;
  union {
    nativeSession @1 :NativeReceipt;
    stream @2 :StreamReceipt;
    event @3 :EventReceipt;
    tool @4 :ToolReceipt;
    error @5 :ErrorEnvelope;
  }
}
struct Frame {
  major @0 :UInt16;
  revision @1 :UInt16;
  schemaDigest @2 :Data;
  requestId @3 :UInt64;
  sessionId @4 :Text;
  instanceEpoch @5 :UInt64;
  union {
    nativeSession @6 :NativeSession;
    stream @7 :Stream;
    event @8 :Event;
    tool @9 :Tool;
    reply @10 :Reply;
  }
}
