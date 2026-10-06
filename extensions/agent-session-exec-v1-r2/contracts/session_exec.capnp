@0xd9d0d7c2cc464d13;
# Independent agent-session-exec-v1, version 1 revision 2.
# Canonical-only frames bind exact schema and current Core identities.
# Opaque events are durable model data. No event, ID or wire permit creates host authority.
# Trusted approval, revocation and execution-domain admission are local host operations.

struct Event { eventId @0 :Text; body @1 :Data; }
struct Environment { name @0 :Text; value @1 :Text; }
struct Intent {
  operationId @0 :Text; program @1 :Text; argv @2 :List(Text);
  cwd @3 :Text; env @4 :List(Environment); input @5 :Data;
  executionDomain @6 :Text; maxRuntimeMs @7 :UInt64; artifactSha256 @8 :Data;
}
struct ExecutionFacts {
  hasExitCode @0 :Bool; exitCode @1 :Int32; outputClosed @2 :Bool;
  stdoutSha256 @3 :Data; stderrSha256 @4 :Data;
  stdoutBytes @5 :UInt64; stderrBytes @6 :UInt64;
}
struct Create { sessionId @0 :Text; hasParent @1 :Bool; parent @2 :Text; parentTail @3 :UInt64; }
struct Snapshot { sessionId @0 :Text; after @1 :UInt64; limit @2 :UInt32; }
struct OpenWriter { sessionId @0 :Text; expectedEpoch @1 :UInt64; }
struct Append { sessionId @0 :Text; epoch @1 :UInt64; expectedTail @2 :UInt64; events @3 :List(Event); }
struct Checkpoint { sessionId @0 :Text; epoch @1 :UInt64; expectedTail @2 :UInt64; state @3 :Data; }
struct Archive { sessionId @0 :Text; epoch @1 :UInt64; expectedTail @2 :UInt64; }
struct Propose { sessionId @0 :Text; intent @1 :Intent; }
struct Claim { operationId @0 :Text; permit @1 :Data; }
struct Report { operationId @0 :Text; claim @1 :Data; facts @2 :ExecutionFacts; }
struct Inspect { operationId @0 :Text; }
struct Request {
  version @0 :UInt16; revision @1 :UInt16; schemaSha256 @2 :Data;
  runtimeDigest @3 :Data; contentDigest @4 :Data;
  coreRuntimeVersion @5 :UInt16; requestId @6 :Text;
  union {
    create @7 :Create; list @8 :Void; snapshot @9 :Snapshot;
    openWriter @10 :OpenWriter; append @11 :Append; checkpoint @12 :Checkpoint;
    archive @13 :Archive; propose @14 :Propose; claim @15 :Claim;
    report @16 :Report; inspect @17 :Inspect;
  }
  generation @18 :UInt64;
}
struct SessionInfo {
  sessionId @0 :Text; hasParent @1 :Bool; parent @2 :Text; parentTail @3 :UInt64;
  epoch @4 :UInt64; tail @5 :UInt64; floor @6 :UInt64; archived @7 :Bool;
  revision @8 :UInt64; checkpointTail @9 :UInt64; checkpointSha256 @10 :Data;
  checkpointSealed @11 :Bool; parentCheckpointSha256 @12 :Data;
}
struct StoredEvent { sequence @0 :UInt64; event @1 :Event; }
struct SessionSnapshot {
  info @0 :SessionInfo; events @1 :List(StoredEvent); gap @2 :Bool; checkpoint @3 :Data;
}
enum ToolPhase { proposed @0; approved @1; revoked @2; dispatchUnknown @3; reported @4; }
struct ToolInfo {
  operationId @0 :Text; sessionId @1 :Text; intentSha256 @2 :Data;
  phase @3 :ToolPhase; hasFacts @4 :Bool; facts @5 :ExecutionFacts;
  hasObservation @6 :Bool; observation @7 :ExecutionFacts; observationRevision @8 :UInt64;
}
struct Claimed { info @0 :ToolInfo; intent @1 :Intent; claim @2 :Data; }
enum Failure {
  invalid @0; contract @1; limit @2; correlation @3; denied @4;
  conflict @5; notFound @6; commitUnknown @7; storage @8;
}
struct Reply {
  version @0 :UInt16; revision @1 :UInt16; schemaSha256 @2 :Data;
  runtimeDigest @3 :Data; contentDigest @4 :Data;
  coreRuntimeVersion @5 :UInt16; requestId @6 :Text; requestSha256 @7 :Data;
  union {
    session @8 :SessionInfo; sessions @9 :List(SessionInfo); snapshot @10 :SessionSnapshot;
    tool @11 :ToolInfo; claimed @12 :Claimed; rejected @13 :Failure;
  }
  generation @14 :UInt64;
}
